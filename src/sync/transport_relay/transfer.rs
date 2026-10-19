//! RelayClient 收发面：send/fetch_page/fetch/ack/status/presence/peers 与 HTTP 辅助。

use super::*;

impl RelayClient {
    /// ciphertext_b64 must already be sealed by the end-to-end protocol layer.
    pub fn send(
        &self,
        recipient_mb: &str,
        sender_mb: &str,
        sender_cred: &[u8; 32],
        seq_hint: i64,
        ciphertext_b64: &str,
        expires_at_ms: i64,
    ) -> Result<String> {
        validate_mailbox(recipient_mb)?;
        validate_mailbox(sender_mb)?;
        validate_integer(seq_hint)?;
        validate_integer(expires_at_ms)?;
        ensure!(
            ciphertext_b64.len() <= MAX_ENVELOPE_BYTES.div_ceil(3) * 4,
            "envelope too large"
        );
        let (ts, nonce) = request_stamp();
        let hash = sha256_hex(ciphertext_b64.as_bytes());
        let sig = hmac_sign_hex(
            sender_cred,
            &[
                "relay-v2-send",
                sender_mb,
                recipient_mb,
                &seq_hint.to_string(),
                &expires_at_ms.to_string(),
                &ts.to_string(),
                &nonce,
                &hash,
            ],
        );
        let result: serde_json::Value = self.post(
            "send",
            &serde_json::json!({
                "version": VERSION, "mailbox_key": recipient_mb, "sender_mb": sender_mb,
                "seq_hint": seq_hint, "ciphertext": ciphertext_b64, "expires_at": expires_at_ms,
                "ts": ts, "nonce": nonce, "sig": sig,
            }),
        )?;
        let id = result
            .get("id")
            .and_then(|value| value.as_str())
            .context("relay response missing mail id")?;
        validate_cursor(id)?;
        Ok(id.to_string())
    }

    pub fn fetch_page(
        &self,
        mailbox: &str,
        owner_cred: &[u8; 32],
        after: &str,
        limit: usize,
    ) -> Result<RelayPage> {
        validate_mailbox(mailbox)?;
        validate_cursor(after)?;
        ensure!((1..=100).contains(&limit), "fetch limit must be 1..=100");
        let (ts, nonce) = request_stamp();
        let sig = sign(
            owner_cred,
            "fetch",
            &[mailbox, after, &limit.to_string()],
            ts,
            &nonce,
        );
        let page: RelayPage = self.get(
            "fetch",
            &[
                ("version", VERSION.to_string()),
                ("mailbox_key", mailbox.to_string()),
                ("after", after.to_string()),
                ("limit", limit.to_string()),
                ("ts", ts.to_string()),
                ("nonce", nonce),
                ("sig", sig),
            ],
        )?;
        ensure!(
            page.envelopes.len() <= limit,
            "relay response exceeds page limit"
        );
        let mut last = after.parse::<i64>().context("invalid cursor")?;
        for envelope in &page.envelopes {
            validate_cursor(&envelope.id)?;
            validate_mailbox(&envelope.sender_mb)?;
            let next = envelope.id.parse::<i64>()?;
            ensure!(next > last, "relay response is not strictly ordered");
            ensure!(
                envelope.ciphertext.len() <= MAX_ENVELOPE_BYTES.div_ceil(3) * 4,
                "relay response envelope too large"
            );
            last = next;
        }
        let expected = page
            .envelopes
            .last()
            .map(|e| e.id.as_str())
            .unwrap_or(after);
        ensure!(
            page.next_cursor == expected,
            "relay response cursor does not match page"
        );
        ensure!(
            !page.envelopes.is_empty() || !page.has_more,
            "relay returned an empty nonterminal page"
        );
        Ok(page)
    }

    pub fn fetch(
        &self,
        mailbox: &str,
        owner_cred: &[u8; 32],
        after: &str,
        limit: usize,
    ) -> Result<Vec<RelayEnvelope>> {
        Ok(self
            .fetch_page(mailbox, owner_cred, after, limit)?
            .envelopes)
    }

    /// Only call after authenticated payloads and application receipts are durable.
    pub fn ack(&self, mailbox: &str, owner_cred: &[u8; 32], ids: &[String]) -> Result<usize> {
        validate_mailbox(mailbox)?;
        ensure!(
            !ids.is_empty() && ids.len() <= 100,
            "ack ids must be 1..=100"
        );
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            validate_cursor(id)?;
            ensure!(
                id.parse::<i64>()? > 0 && seen.insert(id),
                "invalid or duplicate mail id"
            );
        }
        let (ts, nonce) = request_stamp();
        let sig = sign(owner_cred, "ack", &[mailbox, &ids.join(",")], ts, &nonce);
        let result: serde_json::Value = self.post(
            "ack",
            &serde_json::json!({
                "version": VERSION, "mailbox_key": mailbox, "ids": ids,
                "ts": ts, "nonce": nonce, "sig": sig,
            }),
        )?;
        let deleted = integer_field(&result, "deleted")? as usize;
        ensure!(
            deleted <= ids.len(),
            "relay returned an invalid deletion count"
        );
        Ok(deleted)
    }

    pub fn status(&self, mailbox: &str, owner_cred: &[u8; 32]) -> Result<MailboxStatus> {
        validate_mailbox(mailbox)?;
        let (ts, nonce) = request_stamp();
        let sig = sign(owner_cred, "status", &[mailbox], ts, &nonce);
        self.get(
            "status",
            &[
                ("version", VERSION.to_string()),
                ("mailbox_key", mailbox.to_string()),
                ("ts", ts.to_string()),
                ("nonce", nonce),
                ("sig", sig),
            ],
        )
    }

    pub fn presence(
        &self,
        _mailbox: &str,
        _cred: &[u8; 32],
        _endpoint: &str,
        _ttl_ms: i64,
    ) -> Result<i64> {
        bail!("discovery is not supported in relay v2")
    }

    pub fn peers(
        &self,
        _mailbox: &str,
        _cred: &[u8; 32],
        _want: &[String],
    ) -> Result<Vec<PeerPresence>> {
        bail!("discovery is not supported in relay v2")
    }

    pub(crate) fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T> {
        let response = self
            .http
            .post(format!("{}/{path}", self.base_url))
            .json(body)
            .send()
            .context("relay POST failed")?;
        response_json(response)
    }
    pub(crate) fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        let response = self
            .http
            .get(format!("{}/{path}", self.base_url))
            .query(query)
            .send()
            .context("relay GET failed")?;
        response_json(response)
    }
}
