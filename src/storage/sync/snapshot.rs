//! Initial, authenticated checkpoints for a fresh staging database. A pinned WAL
//! read view is streamed as bounded typed records, never as an attached SQLite
//! database. Checkpoints are attested by the explicitly paired source; they do
//! not claim to contain independent historical device signatures.

use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::digest::{Context as DigestContext, SHA256};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::storage::Store;
use crate::sync::{
    crypto::{self, ContentKey, DeviceIdentity, EncryptedEnvelope, EnvelopeHeader},
    protocol::{
        ConversationRecord, EventRecord, HabitNudgeRecord, InputRecord, MessageRecord,
        ProviderBundle, SyncOperation, validate_id,
    },
};

pub(crate) use super::hash;

mod apply;
mod apply_rest;
mod decode;
mod exporter;
mod import;
mod records;

pub(crate) use decode::{FrameDecoder, domain_table, hex_to_vec, validate_json_arg};
pub(crate) use exporter::{chunk_header, hex, valid_hash};
pub(crate) use records::*;

#[cfg(test)]
mod tests;
