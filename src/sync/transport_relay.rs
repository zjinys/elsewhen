//! Relay protocol v2: owner-authorized sender grants and bounded HTTP transport.
//!
//! Payload encryption belongs to the sync protocol layer. Relay credentials are
//! not content keys: the relay can authenticate (and forge) HMACs, but must never
//! receive the end-to-end encryption key. Each device needs a distinct owner key.

use anyhow::{Context, Result, anyhow, bail, ensure};
use ring::digest::SHA256;
use ring::hmac;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const VERSION: u8 = 2;
const MAX_RESPONSE_BYTES: u64 = 17 * 1024 * 1024;
const MAX_ENVELOPE_BYTES: usize = 8 * 1024 * 1024;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
/// 升级 APK 体积上限（当前产物 ~60MB；为后续增长留余量）。
const RELEASE_APK_MAX_BYTES: u64 = 128 * 1024 * 1024;

/// 应用升级发布清单（发布方写入 R2 的 release/manifest.json）。清单由
/// 发布脚本（Node 侧）生成，字段是 JS 惯例 camelCase。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifest {
    pub version: String,
    pub build_hash: String,
    pub build_seq: i64,
    pub changelog: String,
    pub apk_sha256: String,
    pub apk_size: i64,
    pub published_at: i64,
}

/// 7-64 位小写十六进制（git 短 hash…全 hash 都合法）。
fn is_short_hash(value: &str) -> bool {
    (7..=64).contains(&value.len())
        && value
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

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

mod account;
mod crypto;
#[cfg(test)]
mod tests;
mod transfer;

#[cfg(test)]
pub(crate) use crypto::random_hex;
pub(crate) use crypto::{
    RelayHttpError, apk_sha256_hex, bytes_to_hex, derive_key, hmac_sign_hex, integer_field,
    request_stamp, response_json, sha256_hex, sign, validate_cursor, validate_integer,
    validate_mailbox,
};
