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
    /// blob 数据面已落盘字节（relay 自己算的真实占盘）。
    /// 老 relay 不返回这两个字段，缺失按 0 处理（`/status` 的旧字段语义不变）。
    #[serde(default)]
    pub blob_bytes: i64,
    #[serde(default)]
    pub blobs: i64,
    /// relay 自报的能力清单（当前是 `['blob','drop']`）。
    ///
    /// 老 relay 缺这个字段 → 空 vec → 被当成「没有 blob 数据面」：这是**故意**
    /// 的保守默认（review 2026-10-25 M2），因为默认「有」会让发送端对着一个
    /// 没有 `/blob/*` 路由的 relay 上传，每拍都 404。
    #[serde(default)]
    pub capabilities: Vec<String>,
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

#[derive(Clone)]
pub struct RelayClient {
    base_url: String,
    http: reqwest::blocking::Client,
}

mod account;
mod blob;
mod crypto;
#[cfg(test)]
mod tests;
mod transfer;

// `BlobOpen`/`BlobWindow` 只有测试用的内存数据面（`sync::mem_blob`）需要直接
// 命名；生产构建里不会被提到，所以给这条 re-export 关掉 unused 警告。
#[allow(unused_imports)]
pub(crate) use blob::{
    BlobOpen, BlobSink, BlobSource, BlobState, BlobWindow, RelaySink, RelaySource, WINDOW_CHUNKS,
    blob_capability, set_blob_capability,
};
pub(crate) use crypto::random_hex;
pub(crate) use crypto::{
    RelayBlobUnavailable, RelayHttpError, RelayQuotaExceeded, bytes_to_hex, derive_key,
    hmac_sign_hex, integer_field, request_stamp, response_json, sha256_hex, sign, validate_cursor,
    validate_integer, validate_mailbox,
};
