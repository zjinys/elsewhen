//! 盲中继客户端：与自托管 Cloudflare Worker（relay/src/worker.ts）通信。
//!
//! 设计文档：docs/notes/proposed/architecture/2026-09-30-multi-device-sync.md §6.3
//!
//! 职责边界：
//! - 本模块只负责"信封的密封与投递"：端到端加密（XChaCha20-Poly1305，待 binding.rs
//!   提供共享密钥后启用；当前载荷为调用方传入的不透明字节）、HMAC 签名、HTTP 收发。
//! - 本模块不知道载荷里是什么（增量行 / 回执 / 全量快照），那是 sync 协议层的事。
//! - 中继只能验签（HMAC），看不到明文；mailbox_key 是 relay_cred 的派生哈希，
//!   中继不接触明文 device_id。
//!
//! 签名规范（必须与 worker.ts 逐字节一致，relay/test/crypto.test.mjs 锁定）：
//!   send:  HMAC(cred, "relay-send"|sender_mb|recipient_mb|seq_hint|
//!                expires_at_ms|ts|nonce|SHA256(ciphertext_b64))
//!   fetch: HMAC(cred, "relay-fetch"|mailbox_key|after|limit|ts|nonce)
//!   ack:   HMAC(cred, "relay-ack"|mailbox_key|ids_csv|ts|nonce)
//!   status:HMAC(cred, "relay-status"|mailbox_key|ts|nonce)
//!   register: HMAC(new_cred, "relay-register"|sender_mb|recipient_mb|expires_at)

use anyhow::{Context, Result, anyhow, bail};
use ring::hmac;
use serde::{Deserialize, Serialize};

/// 签名时间戳容差（与 worker.ts SKEW_MS 一致）。
const _SKEW_MS: i64 = 5 * 60 * 1000;

/// 邮箱条目（中继返回；密文对客户端同样不透明，由 sync 协议层解密）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayEnvelope {
    pub id: String,
    pub sender_mb: String,
    pub seq_hint: i64,
    pub ciphertext: String, // base64 端到端密文
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

/// 对端在线状态（发现接口返回）。endpoint_blob 是端到端密文，需 binding 层解密。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerPresence {
    pub mailbox_key: String,
    pub endpoint_blob: String, // base64 端到端密文（含 addr/port/device_id）
    pub updated_at: i64,
}

/// 中继客户端。一个实例代表"我作为某邮箱的取件者"或"我作为发件人"。
/// relay_cred 与 mailbox_key 的派生在 binding.rs（绑定流程）完成，这里只消费。
pub struct RelayClient {
    base_url: String,
    http: reqwest::blocking::Client,
}

impl RelayClient {
    pub fn new(base_url: &str) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_string();
        if !base_url.starts_with("https://")
            && !base_url.starts_with("http://127.0.0.1")
            && !base_url.starts_with("http://localhost")
        {
            bail!("relay url must be https (http only allowed for local dev)");
        }
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("build relay http client")?;
        Ok(Self { base_url, http })
    }

    pub fn health(&self) -> Result<bool> {
        let resp = self.http.get(format!("{}/health", self.base_url)).send()?;
        Ok(resp.status().is_success())
    }

    /// 派生 mailbox_key：HMAC(relay_cred, "relay-mailbox")。
    /// 中继只见过这个哈希，没见过明文 device_id。
    pub fn mailbox_key(relay_cred: &[u8; 32]) -> String {
        hmac_sign_hex(relay_cred, &["relay-mailbox"])
    }

    /// 从共享密钥派生 relay_cred：HMAC(shared_key, "relay-auth")。
    /// 鉴权与保密分离：中继持 cred 只能验签，推不出 shared_key。
    pub fn derive_relay_cred(shared_key: &[u8; 32]) -> [u8; 32] {
        let mut out = [0u8; 32];
        let hex = hmac_sign_hex(shared_key, &["relay-auth"]);
        out.copy_from_slice(&hex_to_bytes(&hex).expect("hmac output is 32 bytes"));
        out
    }

    /// 首次绑定时向收件人邮箱注册发件凭证（仅当尚无有效凭证时中继才接受）。
    /// sig 用申报的新 cred 自签，证明申报者持有该 cred。
    pub fn register_credential(
        &self,
        recipient_mb: &str,
        sender_mb: &str,
        cred: &[u8; 32],
        expires_at_ms: i64,
    ) -> Result<()> {
        let cred_hex = bytes_to_hex(cred);
        let sig = hmac_sign_hex(
            cred,
            &[
                "relay-register",
                sender_mb,
                recipient_mb,
                &expires_at_ms.to_string(),
            ],
        );
        let body = serde_json::json!({
            "mailbox_key": recipient_mb,
            "sender_mb": sender_mb,
            "cred_hex": cred_hex,
            "expires_at": expires_at_ms,
            "sig": sig,
        });
        let resp = self
            .http
            .post(format!("{}/register", self.base_url))
            .json(&body)
            .send()
            .context("relay register request")?;
        if !resp.status().is_success() {
            bail!(
                "relay register failed: {} {}",
                resp.status(),
                resp.text().unwrap_or_default()
            );
        }
        Ok(())
    }

    /// 发送端到端密文到收件人邮箱。ciphertext 由 sync 协议层加密好（base64）。
    pub fn send(
        &self,
        recipient_mb: &str,
        sender_mb: &str,
        cred: &[u8; 32],
        seq_hint: i64,
        ciphertext_b64: &str,
        expires_at_ms: i64,
    ) -> Result<String> {
        let ts = now_ms();
        let nonce = random_hex(16);
        let payload_hash = sha256_hex(ciphertext_b64.as_bytes());
        let sig = hmac_sign_hex(
            cred,
            &[
                "relay-send",
                sender_mb,
                recipient_mb,
                &seq_hint.to_string(),
                &expires_at_ms.to_string(),
                &ts.to_string(),
                &nonce,
                &payload_hash,
            ],
        );
        let body = serde_json::json!({
            "mailbox_key": recipient_mb,
            "sender_mb": sender_mb,
            "seq_hint": seq_hint,
            "ciphertext": ciphertext_b64,
            "expires_at": expires_at_ms,
            "ts": ts,
            "nonce": nonce,
            "sig": sig,
        });
        let resp = self
            .http
            .post(format!("{}/send", self.base_url))
            .json(&body)
            .send()
            .context("relay send request")?;
        if !resp.status().is_success() {
            bail!(
                "relay send failed: {} {}",
                resp.status(),
                resp.text().unwrap_or_default()
            );
        }
        let v: serde_json::Value = resp.json()?;
        v.get("id")
            .and_then(|x| x.as_str())
            .map(str::to_string)
            .ok_or_else(|| anyhow!("relay send response missing id"))
    }

    /// 游标取件：返回 id > after 的信封（按 id 升序）。
    pub fn fetch(
        &self,
        mailbox_key: &str,
        cred: &[u8; 32],
        after: &str,
        limit: usize,
    ) -> Result<Vec<RelayEnvelope>> {
        let ts = now_ms();
        let nonce = random_hex(16);
        let limit = limit.min(100);
        let sig = hmac_sign_hex(
            cred,
            &[
                "relay-fetch",
                mailbox_key,
                after,
                &limit.to_string(),
                &ts.to_string(),
                &nonce,
            ],
        );
        let resp = self
            .http
            .get(format!("{}/fetch", self.base_url))
            .query(&[
                ("mailbox_key", mailbox_key),
                ("recipient_mb", mailbox_key), // 取件者自证（cred 注册在 self→self）
                ("after", after),
                ("limit", &limit.to_string()),
                ("ts", &ts.to_string()),
                ("nonce", &nonce),
                ("sig", &sig),
            ])
            .send()
            .context("relay fetch request")?;
        if !resp.status().is_success() {
            bail!(
                "relay fetch failed: {} {}",
                resp.status(),
                resp.text().unwrap_or_default()
            );
        }
        let v: serde_json::Value = resp.json()?;
        let envelopes = v.get("envelopes").cloned().unwrap_or(serde_json::json!([]));
        Ok(serde_json::from_value(envelopes)?)
    }

    /// 确认已处理，中继删除对应信封。
    pub fn ack(&self, mailbox_key: &str, cred: &[u8; 32], ids: &[String]) -> Result<usize> {
        if ids.is_empty() || ids.len() > 100 {
            bail!("ack ids must be 1..=100");
        }
        let ts = now_ms();
        let nonce = random_hex(16);
        let ids_csv = ids.join(",");
        let sig = hmac_sign_hex(
            cred,
            &["relay-ack", mailbox_key, &ids_csv, &ts.to_string(), &nonce],
        );
        let body = serde_json::json!({
            "mailbox_key": mailbox_key,
            "recipient_mb": mailbox_key,
            "ids": ids,
            "ts": ts,
            "nonce": nonce,
            "sig": sig,
        });
        let resp = self
            .http
            .post(format!("{}/ack", self.base_url))
            .json(&body)
            .send()
            .context("relay ack request")?;
        if !resp.status().is_success() {
            bail!(
                "relay ack failed: {} {}",
                resp.status(),
                resp.text().unwrap_or_default()
            );
        }
        let v: serde_json::Value = resp.json()?;
        Ok(v.get("deleted").and_then(|x| x.as_u64()).unwrap_or(0) as usize)
    }

    /// 心跳：上报本设备在线 + 端到端加密的端点信息。
    /// endpoint_blob 由 binding/transport_p2p 层用共享密钥加密（含 addr/port/device_id），
    /// 中继只存盲索引，不可读。
    pub fn presence(
        &self,
        mailbox_key: &str,
        cred: &[u8; 32],
        endpoint_blob_b64: &str,
        ttl_ms: i64,
    ) -> Result<i64> {
        let ts = now_ms();
        let nonce = random_hex(16);
        let endpoint_hash = sha256_hex(endpoint_blob_b64.as_bytes());
        let sig = hmac_sign_hex(
            cred,
            &[
                "relay-presence",
                mailbox_key,
                &endpoint_hash,
                &ttl_ms.to_string(),
                &ts.to_string(),
                &nonce,
            ],
        );
        let body = serde_json::json!({
            "mailbox_key": mailbox_key,
            "endpoint_blob": endpoint_blob_b64,
            "ttl_ms": ttl_ms,
            "ts": ts,
            "nonce": nonce,
            "sig": sig,
        });
        let resp = self
            .http
            .post(format!("{}/presence", self.base_url))
            .json(&body)
            .send()
            .context("relay presence request")?;
        if !resp.status().is_success() {
            bail!(
                "relay presence failed: {} {}",
                resp.status(),
                resp.text().unwrap_or_default()
            );
        }
        let v: serde_json::Value = resp.json()?;
        Ok(v.get("ttl_ms").and_then(|x| x.as_i64()).unwrap_or(ttl_ms))
    }

    /// 发现：列出本设备可见的在线对端（含加密端点，中继不可读）。
    /// want 为空则返回全部已绑定且在线的对端。
    pub fn peers(
        &self,
        mailbox_key: &str,
        cred: &[u8; 32],
        want: &[String],
    ) -> Result<Vec<PeerPresence>> {
        let ts = now_ms();
        let nonce = random_hex(16);
        let sig = hmac_sign_hex(cred, &["relay-peers", mailbox_key, &ts.to_string(), &nonce]);
        let want_csv = want.join(",");
        let mut query = vec![
            ("mailbox_key", mailbox_key.to_string()),
            ("ts", ts.to_string()),
            ("nonce", nonce),
            ("sig", sig),
        ];
        if !want_csv.is_empty() {
            query.push(("want", want_csv));
        }
        let resp = self
            .http
            .get(format!("{}/peers", self.base_url))
            .query(&query)
            .send()
            .context("relay peers request")?;
        if !resp.status().is_success() {
            bail!(
                "relay peers failed: {} {}",
                resp.status(),
                resp.text().unwrap_or_default()
            );
        }
        let v: serde_json::Value = resp.json()?;
        let peers = v.get("peers").cloned().unwrap_or(serde_json::json!([]));
        Ok(serde_json::from_value(peers)?)
    }

    pub fn status(&self, mailbox_key: &str, cred: &[u8; 32]) -> Result<MailboxStatus> {
        let ts = now_ms();
        let nonce = random_hex(16);
        let sig = hmac_sign_hex(
            cred,
            &["relay-status", mailbox_key, &ts.to_string(), &nonce],
        );
        let resp = self
            .http
            .get(format!("{}/status", self.base_url))
            .query(&[
                ("mailbox_key", mailbox_key),
                ("recipient_mb", mailbox_key),
                ("ts", &ts.to_string()),
                ("nonce", &nonce),
                ("sig", &sig),
            ])
            .send()
            .context("relay status request")?;
        if !resp.status().is_success() {
            bail!(
                "relay status failed: {} {}",
                resp.status(),
                resp.text().unwrap_or_default()
            );
        }
        Ok(resp.json()?)
    }
}

// ---------------------------------------------------------------------------
// 加密原语（ring）：HMAC-SHA256 / SHA-256，hex 编码
// ---------------------------------------------------------------------------

fn hmac_sign_hex(key: &[u8; 32], parts: &[&str]) -> String {
    let k = hmac::Key::new(hmac::HMAC_SHA256, key);
    let tag = hmac::sign(&k, parts.join("|").as_bytes());
    bytes_to_hex(tag.as_ref())
}

fn sha256_hex(data: &[u8]) -> String {
    use ring::digest;
    bytes_to_hex(digest::digest(&digest::SHA256, data).as_ref())
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_to_bytes(hex: &str) -> Result<Vec<u8>> {
    if hex.len() % 2 != 0 {
        bail!("odd-length hex");
    }
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|e| anyhow!(e)))
        .collect()
}

fn random_hex(bytes: usize) -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let rng = SystemRandom::new();
    let mut buf = vec![0u8; bytes];
    rng.fill(&mut buf).expect("system rng");
    bytes_to_hex(&buf)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ---------------------------------------------------------------------------
// 测试：锁定与 worker.ts 的签名格式一致性（relay/test/crypto.test.mjs 同款向量）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const CRED_HEX: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn cred() -> [u8; 32] {
        let v = hex_to_bytes(CRED_HEX).unwrap();
        let mut out = [0u8; 32];
        out.copy_from_slice(&v);
        out
    }

    #[test]
    fn hmac_format_matches_worker_spec() {
        // 与 relay/test/crypto.test.mjs 的"签名消息格式"测试同一向量：
        // HMAC(cred, "relay-send|s|r|1|100|50|n|h")
        let sig = hmac_sign_hex(
            &cred(),
            &["relay-send", "s", "r", "1", "100", "50", "n", "h"],
        );
        // node crypto 计算结果（同一输入）：
        //   createHmac("sha256", Buffer.from(CRED_HEX,"hex")).update("relay-send|s|r|1|100|50|n|h").digest("hex")
        assert_eq!(sig.len(), 64);
        // 黄金值由 node 预先算出，防止 Rust 端与 TS 端各自漂移：
        assert_eq!(
            sig,
            node_golden_relay_send_sig(),
            "rust hmac must match node hmac for the same input"
        );
    }

    fn node_golden_relay_send_sig() -> &'static str {
        // 由以下 node 一行命令生成（cred = 'a'*64）：
        //   node -e 'const c=require("crypto");console.log(c.createHmac("sha256",Buffer.from("a".repeat(64),"hex")).update("relay-send|s|r|1|100|50|n|h").digest("hex"))'
        "67523c09e2e4d4f2fe185d7a8f4c3e30544fbeae7830f59739d89c3c666add0b"
    }

    #[test]
    fn derive_cred_and_mailbox_key_are_stable() {
        let shared = [7u8; 32];
        let cred1 = RelayClient::derive_relay_cred(&shared);
        let cred2 = RelayClient::derive_relay_cred(&shared);
        assert_eq!(cred1, cred2, "derivation must be deterministic");
        assert_ne!(cred1, shared, "cred must differ from shared key");

        let mb1 = RelayClient::mailbox_key(&cred1);
        assert_eq!(mb1.len(), 64);
        assert!(mb1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn rejects_non_https_url() {
        assert!(RelayClient::new("http://evil.example.com").is_err());
        assert!(RelayClient::new("https://relay.example.com").is_ok());
        assert!(RelayClient::new("http://localhost:8787").is_ok()); // wrangler dev
    }

    #[test]
    fn hex_roundtrip() {
        let bytes = [0xde, 0xad, 0xbe, 0xef];
        let hex = bytes_to_hex(&bytes);
        assert_eq!(hex, "deadbeef");
        assert_eq!(hex_to_bytes(&hex).unwrap(), bytes);
    }
}
