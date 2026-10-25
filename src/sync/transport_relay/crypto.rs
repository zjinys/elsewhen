//! 协议工具：整数/游标校验、签名域分离、HMAC/HKDF 派生、hex 编码与错误类型。

use super::*;

/// relay 明确表示「信箱配额打满」（HTTP 507）。这不是"这次没送到"，而是
/// "在腾出空间之前再发也没用"：把它当普通失败每拍重试，正是把配额越撞越死、
/// 让 outbox 反复洗掉 relay_id 的放大器。单独成型，让发送侧能停止本拍发送、
/// 退避并让配对面板说明原因。
#[derive(Debug)]
pub(crate) struct RelayQuotaExceeded;

impl std::fmt::Display for RelayQuotaExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "relay mailbox quota exceeded")
    }
}
impl std::error::Error for RelayQuotaExceeded {}

/// relay 说「这份 blob 我不存在」（HTTP 404）：要么分片被配额/TTL 收掉了，
/// 要么这台 relay 根本没有 blob 数据面（`/blob/open` 是路由级 404）。
///
/// 为什么必须从 `RelayHttpError` 里单列（review 2026-10-25 N1）：404 与 5xx
/// 的正确反应完全不同 —— 5xx 是「等一等再发同一片」，404 是「从第 0 片重新
/// 开一份」。混在 `Transport` 里意味着本该重传的错要等十分钟退避，而重传
/// 本来就只是花点带宽。注：`/blob/state` 不会 404（回 `200 + present:[]`），
/// 所以只有 `open` / `put` 的 404 落到这里。
#[derive(Debug)]
pub(crate) struct RelayBlobUnavailable;

impl std::fmt::Display for RelayBlobUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "relay blob is gone or unsupported")
    }
}
impl std::error::Error for RelayBlobUnavailable {}

#[derive(Debug)]
pub(crate) struct RelayHttpError(pub(crate) reqwest::StatusCode);
impl std::fmt::Display for RelayHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "relay HTTP status {}", self.0)
    }
}
impl std::error::Error for RelayHttpError {}

pub(crate) fn response_json<T: DeserializeOwned>(
    response: reqwest::blocking::Response,
) -> Result<T> {
    // A compromised relay may return secrets in an error body. Never echo it.
    if !response.status().is_success() {
        if response.status() == reqwest::StatusCode::INSUFFICIENT_STORAGE {
            return Err(RelayQuotaExceeded.into());
        }
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
pub(crate) fn integer_field(value: &serde_json::Value, name: &str) -> Result<i64> {
    let result = value
        .get(name)
        .and_then(|v| v.as_i64())
        .context("relay response missing numeric field")?;
    validate_integer(result)?;
    Ok(result)
}
pub(crate) fn validate_integer(value: i64) -> Result<()> {
    ensure!(
        (0..=MAX_SAFE_INTEGER).contains(&value),
        "integer outside relay wire range"
    );
    Ok(())
}
pub(crate) fn validate_mailbox(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "invalid relay mailbox"
    );
    Ok(())
}
pub(crate) fn validate_cursor(value: &str) -> Result<()> {
    ensure!(
        value == "0" || (value.len() == 16 && value.bytes().all(|c| c.is_ascii_digit())),
        "invalid relay cursor"
    );
    validate_integer(value.parse::<i64>().context("invalid relay cursor")?)
}
pub(crate) fn request_stamp() -> (i64, String) {
    (chrono::Utc::now().timestamp_millis(), random_hex(16))
}
pub(crate) fn sign(key: &[u8; 32], domain: &str, parts: &[&str], ts: i64, nonce: &str) -> String {
    let domain = format!("relay-v2-{domain}");
    let ts = ts.to_string();
    let mut fields = vec![domain.as_str()];
    fields.extend_from_slice(parts);
    fields.extend([ts.as_str(), nonce]);
    hmac_sign_hex(key, &fields)
}
pub(crate) fn hmac_sign_hex(key: &[u8; 32], parts: &[&str]) -> String {
    bytes_to_hex(
        hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, key),
            parts.join("|").as_bytes(),
        )
        .as_ref(),
    )
}
pub(crate) fn derive_key(key: &[u8; 32], parts: &[&str]) -> [u8; 32] {
    let tag = hmac::sign(
        &hmac::Key::new(hmac::HMAC_SHA256, key),
        parts.join("|").as_bytes(),
    );
    tag.as_ref().try_into().expect("SHA256 has 32 bytes")
}
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    bytes_to_hex(ring::digest::digest(&ring::digest::SHA256, bytes).as_ref())
}
pub(crate) fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
pub(crate) fn random_hex(length: usize) -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = vec![0; length];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("system random source");
    bytes_to_hex(&bytes)
}
