//! 协议工具：整数/游标校验、签名域分离、HMAC/HKDF 派生、hex 编码与错误类型。

use super::*;

#[derive(Debug)]
pub(crate) struct RelayHttpError(pub(crate) reqwest::StatusCode);
impl std::fmt::Display for RelayHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "relay HTTP status {}", self.0)
    }
}
impl std::error::Error for RelayHttpError {}

/// 流式计算文件 sha256（60MB 量级，不整读进内存）。
pub(crate) fn apk_sha256_hex(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).context("open apk for hashing")?;
    let mut ctx = ring::digest::Context::new(&SHA256);
    let mut buf = [0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        ctx.update(&buf[..n]);
    }
    Ok(bytes_to_hex(ctx.finish().as_ref()))
}

pub(crate) fn response_json<T: DeserializeOwned>(
    response: reqwest::blocking::Response,
) -> Result<T> {
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
