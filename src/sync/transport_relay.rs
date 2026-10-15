//! Relay protocol v2: owner-authorized sender grants and bounded HTTP transport.
//!
//! Payload encryption belongs to the sync protocol layer. Relay credentials are
//! not content keys: the relay can authenticate (and forge) HMACs, but must never
//! receive the end-to-end encryption key. Each device needs a distinct owner key.

use anyhow::{Context, Result, anyhow, bail, ensure};
use ring::hmac;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::Read;

const VERSION: u8 = 2;
const MAX_RESPONSE_BYTES: u64 = 17 * 1024 * 1024;
const MAX_ENVELOPE_BYTES: usize = 8 * 1024 * 1024;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayEnvelope {
    pub id: String,
    pub sender_mb: String,
    pub seq_hint: i64,
    pub ciphertext: String,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxStatus {
    pub bytes: i64,
    pub count: i64,
    pub quota: i64,
    pub earliest_expiry: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderStatus {
    pub revision: i64,
    pub active: bool,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayPage {
    pub envelopes: Vec<RelayEnvelope>,
    pub next_cursor: String,
    pub has_more: bool,
}

// Kept for source compatibility; discovery is deliberately disabled in v2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerPresence {
    pub mailbox_key: String,
    pub endpoint_blob: String,
    pub updated_at: i64,
}

pub struct RelayClient {
    base_url: String,
    http: reqwest::blocking::Client,
}

impl RelayClient {
    pub fn new(base_url: &str) -> Result<Self> {
        let url = reqwest::Url::parse(base_url).context("parse relay URL")?;
        let local = url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        ensure!(
            url.scheme() == "https" || (url.scheme() == "http" && local),
            "relay URL must use HTTPS (HTTP only for loopback development)"
        );
        ensure!(
            url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "relay URL must not contain credentials, query or fragment"
        );
        let http = crate::http_proxy::with_env_proxy(reqwest::blocking::Client::builder())
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("build relay HTTP client")?;
        Ok(Self {
            base_url: url.as_str().trim_end_matches('/').to_string(),
            http,
        })
    }

    pub fn health(&self) -> Result<bool> {
        let response = self.http.get(format!("{}/health", self.base_url)).send()?;
        if !response.status().is_success() {
            return Ok(false);
        }
        let body: serde_json::Value = response_json(response)?;
        Ok(
            body.get("version").and_then(|v| v.as_u64()) == Some(VERSION.into())
                && body.get("ok").and_then(|v| v.as_bool()) == Some(true),
        )
    }

    /// Initial mailbox identity. Persist it across subsequent owner-key rotation.
    /// Owner keys must be generated independently per device, never pair-shared.
    pub fn mailbox_key(owner_cred: &[u8; 32]) -> String {
        hmac_sign_hex(owner_cred, &["relay-mailbox"])
    }

    /// Legacy derivation, not a device identity or end-to-end encryption key.
    pub fn derive_relay_cred(shared_key: &[u8; 32]) -> [u8; 32] {
        derive_key(shared_key, &["relay-auth"])
    }

    /// Recipient-scoped sender capability; does not grant mailbox owner access.
    pub fn derive_sender_credential(
        shared_key: &[u8; 32],
        sender_mb: &str,
        recipient_mb: &str,
    ) -> Result<[u8; 32]> {
        validate_mailbox(sender_mb)?;
        validate_mailbox(recipient_mb)?;
        ensure!(
            sender_mb != recipient_mb,
            "sender and recipient must differ"
        );
        Ok(derive_key(
            shared_key,
            &["relay-v2-sender", sender_mb, recipient_mb],
        ))
    }

    pub fn create_mailbox(&self, owner_cred: &[u8; 32]) -> Result<String> {
        let mailbox = Self::mailbox_key(owner_cred);
        let (ts, nonce) = request_stamp();
        let sig = sign(owner_cred, "create", &[&mailbox], ts, &nonce);
        let _: serde_json::Value = self.post(
            "create",
            &serde_json::json!({
                "version": VERSION, "mailbox_key": mailbox,
                "owner_cred_hex": bytes_to_hex(owner_cred), "ts": ts,
                "nonce": nonce, "sig": sig,
            }),
        )?;
        Ok(mailbox)
    }

    /// v1 self-registration is unsafe and cannot grant sender access in v2.
    /// Kept as an explicit error rather than silently interpreting owner keys.
    pub fn register_credential(
        &self,
        _recipient_mb: &str,
        _sender_mb: &str,
        _cred: &[u8; 32],
        _expires_at_ms: i64,
    ) -> Result<()> {
        bail!(
            "relay v1 registration removed; use create_mailbox and owner-authorized register_sender"
        )
    }

    /// Create, renew or rotate a sender grant at an explicitly observed revision.
    /// Revision 0 means no prior grant; revoked grants retain their revisions.
    /// Retry lost responses with the same credential, expiry and expected revision;
    /// the Worker accepts only the exact post-state, never a newer conflicting grant.
    pub fn register_sender(
        &self,
        mailbox: &str,
        owner_cred: &[u8; 32],
        sender_mb: &str,
        sender_cred: &[u8; 32],
        expires_at_ms: i64,
        expected_revision: i64,
    ) -> Result<i64> {
        validate_mailbox(mailbox)?;
        validate_mailbox(sender_mb)?;
        ensure!(
            sender_mb != mailbox,
            "sender must differ from mailbox owner"
        );
        ensure!(
            sender_cred != owner_cred,
            "sender credential must differ from owner credential"
        );
        validate_integer(expected_revision)?;
        validate_integer(expires_at_ms)?;
        let (ts, nonce) = request_stamp();
        let credential = bytes_to_hex(sender_cred);
        let sig = sign(
            owner_cred,
            "register",
            &[
                mailbox,
                sender_mb,
                &credential,
                &expires_at_ms.to_string(),
                &expected_revision.to_string(),
            ],
            ts,
            &nonce,
        );
        let result: serde_json::Value = self.post(
            "register",
            &serde_json::json!({
                "version": VERSION, "mailbox_key": mailbox, "sender_mb": sender_mb,
                "cred_hex": credential, "expires_at": expires_at_ms,
                "expected_revision": expected_revision, "ts": ts, "nonce": nonce, "sig": sig,
            }),
        )?;
        integer_field(&result, "revision")
    }

    pub fn sender_status(
        &self,
        mailbox: &str,
        owner_cred: &[u8; 32],
        sender_mb: &str,
    ) -> Result<SenderStatus> {
        validate_mailbox(mailbox)?;
        validate_mailbox(sender_mb)?;
        let (ts, nonce) = request_stamp();
        let sig = sign(owner_cred, "sender", &[mailbox, sender_mb], ts, &nonce);
        self.get(
            "sender",
            &[
                ("version", VERSION.to_string()),
                ("mailbox_key", mailbox.to_string()),
                ("sender_mb", sender_mb.to_string()),
                ("ts", ts.to_string()),
                ("nonce", nonce),
                ("sig", sig),
            ],
        )
    }

    pub fn revoke_sender(
        &self,
        mailbox: &str,
        owner_cred: &[u8; 32],
        sender_mb: &str,
        expected_revision: i64,
    ) -> Result<i64> {
        validate_mailbox(mailbox)?;
        validate_mailbox(sender_mb)?;
        validate_integer(expected_revision)?;
        ensure!(expected_revision > 0, "sender revision must be positive");
        let (ts, nonce) = request_stamp();
        let sig = sign(
            owner_cred,
            "revoke",
            &[mailbox, sender_mb, &expected_revision.to_string()],
            ts,
            &nonce,
        );
        let result: serde_json::Value = self.post(
            "revoke",
            &serde_json::json!({
                "version": VERSION, "mailbox_key": mailbox, "sender_mb": sender_mb,
                "expected_revision": expected_revision, "ts": ts, "nonce": nonce, "sig": sig,
            }),
        )?;
        integer_field(&result, "revision")
    }

    /// The mailbox address stays stable; old owner keys stop authorizing requests.
    pub fn rotate_owner(
        &self,
        mailbox: &str,
        owner_cred: &[u8; 32],
        new_cred: &[u8; 32],
    ) -> Result<i64> {
        validate_mailbox(mailbox)?;
        ensure!(owner_cred != new_cred, "new owner credential must differ");
        let credential = bytes_to_hex(new_cred);
        let (ts, nonce) = request_stamp();
        let sig = sign(owner_cred, "rotate", &[mailbox, &credential], ts, &nonce);
        let result: serde_json::Value = self.post(
            "rotate",
            &serde_json::json!({
                "version": VERSION, "mailbox_key": mailbox, "new_cred_hex": credential,
                "ts": ts, "nonce": nonce, "sig": sig,
            }),
        )?;
        integer_field(&result, "owner_generation")
    }

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

    fn post<T: DeserializeOwned>(&self, path: &str, body: &serde_json::Value) -> Result<T> {
        let response = self
            .http
            .post(format!("{}/{path}", self.base_url))
            .json(body)
            .send()
            .context("relay POST failed")?;
        response_json(response)
    }
    fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        let response = self
            .http
            .get(format!("{}/{path}", self.base_url))
            .query(query)
            .send()
            .context("relay GET failed")?;
        response_json(response)
    }
}

#[derive(Debug)]
pub(crate) struct RelayHttpError(pub(crate) reqwest::StatusCode);
impl std::fmt::Display for RelayHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "relay HTTP status {}", self.0)
    }
}
impl std::error::Error for RelayHttpError {}

fn response_json<T: DeserializeOwned>(response: reqwest::blocking::Response) -> Result<T> {
    // A compromised relay may return secrets in an error body. Never echo it.
    if !response.status().is_success() {
        return Err(RelayHttpError(response.status()).into());
    }
    if let Some(length) = response.content_length() {
        ensure!(length <= MAX_RESPONSE_BYTES, "relay response too large");
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .context("read relay response")?;
    ensure!(
        bytes.len() as u64 <= MAX_RESPONSE_BYTES,
        "relay response too large"
    );
    // serde errors may contain server-supplied values; keep them out of diagnostics.
    serde_json::from_slice(&bytes).map_err(|_| anyhow!("invalid relay response"))
}
fn integer_field(value: &serde_json::Value, name: &str) -> Result<i64> {
    let result = value
        .get(name)
        .and_then(|v| v.as_i64())
        .context("relay response missing numeric field")?;
    validate_integer(result)?;
    Ok(result)
}
fn validate_integer(value: i64) -> Result<()> {
    ensure!(
        (0..=MAX_SAFE_INTEGER).contains(&value),
        "integer outside relay wire range"
    );
    Ok(())
}
fn validate_mailbox(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "invalid relay mailbox"
    );
    Ok(())
}
fn validate_cursor(value: &str) -> Result<()> {
    ensure!(
        value == "0" || (value.len() == 16 && value.bytes().all(|c| c.is_ascii_digit())),
        "invalid relay cursor"
    );
    validate_integer(value.parse::<i64>().context("invalid relay cursor")?)
}
fn request_stamp() -> (i64, String) {
    (chrono::Utc::now().timestamp_millis(), random_hex(16))
}
fn sign(key: &[u8; 32], domain: &str, parts: &[&str], ts: i64, nonce: &str) -> String {
    let domain = format!("relay-v2-{domain}");
    let ts = ts.to_string();
    let mut fields = vec![domain.as_str()];
    fields.extend_from_slice(parts);
    fields.extend([ts.as_str(), nonce]);
    hmac_sign_hex(key, &fields)
}
fn hmac_sign_hex(key: &[u8; 32], parts: &[&str]) -> String {
    bytes_to_hex(
        hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, key),
            parts.join("|").as_bytes(),
        )
        .as_ref(),
    )
}
fn derive_key(key: &[u8; 32], parts: &[&str]) -> [u8; 32] {
    let tag = hmac::sign(
        &hmac::Key::new(hmac::HMAC_SHA256, key),
        parts.join("|").as_bytes(),
    );
    tag.as_ref().try_into().expect("SHA256 has 32 bytes")
}
fn sha256_hex(bytes: &[u8]) -> String {
    bytes_to_hex(ring::digest::digest(&ring::digest::SHA256, bytes).as_ref())
}
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn random_hex(length: usize) -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = vec![0; length];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("system random source");
    bytes_to_hex(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_validation_rejects_prefix_tricks_and_credentials() {
        for url in [
            "http://localhost.evil.invalid",
            "http://127.0.0.1.evil.invalid",
            "http://localhost@evil.invalid",
            "https://user:secret@relay.invalid",
            "https://relay.invalid?key=secret",
            "https://relay.invalid#fragment",
        ] {
            assert!(RelayClient::new(url).is_err(), "{url}");
        }
        for url in [
            "https://relay.invalid",
            "http://localhost:8787",
            "http://127.0.0.1:8787",
            "http://[::1]:8787",
        ] {
            assert!(RelayClient::new(url).is_ok(), "{url}");
        }
    }

    #[test]
    fn sender_capability_is_direction_and_recipient_bound() {
        let a = RelayClient::mailbox_key(&[1; 32]);
        let b = RelayClient::mailbox_key(&[2; 32]);
        let c = RelayClient::mailbox_key(&[3; 32]);
        assert_ne!(a, b);
        let ab = RelayClient::derive_sender_credential(&[9; 32], &a, &b).unwrap();
        assert_ne!(
            ab,
            RelayClient::derive_sender_credential(&[9; 32], &b, &a).unwrap()
        );
        assert_ne!(
            ab,
            RelayClient::derive_sender_credential(&[9; 32], &a, &c).unwrap()
        );
        assert!(RelayClient::derive_sender_credential(&[9; 32], &a, &a).is_err());
    }

    #[test]
    fn wire_numbers_and_cursors_are_bounded() {
        assert!(validate_integer(MAX_SAFE_INTEGER).is_ok());
        assert!(validate_integer(MAX_SAFE_INTEGER + 1).is_err());
        assert!(validate_cursor("0").is_ok());
        assert!(validate_cursor("0000000000000001").is_ok());
        for value in ["1", "-1", "1|nonce", "9999999999999999", ""] {
            assert!(validate_cursor(value).is_err());
        }
    }

    #[test]
    fn signatures_are_domain_separated_from_v1() {
        assert_ne!(
            hmac_sign_hex(&[7; 32], &["relay-send", "a"]),
            hmac_sign_hex(&[7; 32], &["relay-v2-send", "a"])
        );
        assert_eq!(
            sign(&[7; 32], "status", &["mailbox"], 123, "nonce"),
            hmac_sign_hex(&[7; 32], &["relay-v2-status", "mailbox", "123", "nonce"])
        );
    }
}
