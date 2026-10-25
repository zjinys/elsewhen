//! Versioned domain operations. Wire data never names a SQLite table or SQL statement.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_OPERATION_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_BATCH_OPERATIONS: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeReceipt {
    pub dataset_id: String,
    pub acknowledging_device_id: String,
    pub target_device_id: String,
    pub message_id: String,
    pub payload_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationReceipt {
    pub dataset_id: String,
    pub acknowledging_device_id: String,
    pub target_device_id: String,
    pub op_id: String,
    pub payload_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncOperation {
    pub protocol_version: u32,
    pub dataset_id: String,
    pub origin_device_id: String,
    pub origin_stream_id: String,
    pub origin_sequence: i64,
    pub logical_clock: i64,
    pub op_id: String,
    pub mutation: SyncMutation,
}

impl SyncOperation {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_OPERATION_BYTES,
            "sync_operation_too_large"
        );
        let operation: Self = serde_json::from_slice(bytes)?;
        operation.validate()?;
        Ok(operation)
    }

    /// Tolerant identity-only header parse for quarantine. Reads only the
    /// fields needed to key an isolated row (no version/structural validation),
    /// so an unknown-version op can be recorded without aborting the receive.
    pub(crate) fn identity_header(bytes: &[u8]) -> Result<(String, String, String, i64, String)> {
        ensure!(
            bytes.len() <= MAX_OPERATION_BYTES,
            "sync_operation_too_large"
        );
        let value: serde_json::Value = serde_json::from_slice(bytes)?;
        let get_str = |k: &str| {
            value
                .get(k)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        let dataset = get_str("dataset_id").context("sync_operation_header_invalid")?;
        let device = get_str("origin_device_id").context("sync_operation_header_invalid")?;
        let stream = get_str("origin_stream_id").context("sync_operation_header_invalid")?;
        let seq = value
            .get("origin_sequence")
            .and_then(serde_json::Value::as_i64)
            .context("sync_operation_header_invalid")?;
        let op_id = get_str("op_id").context("sync_operation_header_invalid")?;
        Ok((dataset, device, stream, seq, op_id))
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.protocol_version == PROTOCOL_VERSION,
            "sync_protocol_unsupported"
        );
        for id in [
            &self.dataset_id,
            &self.origin_device_id,
            &self.origin_stream_id,
        ] {
            validate_id(id)?;
        }
        ensure!(self.origin_sequence > 0, "sync_sequence_invalid");
        ensure!(
            (1..=9_007_199_254_740_991).contains(&self.logical_clock),
            "sync_logical_clock_invalid"
        );
        ensure!(
            self.op_id == format!("{}:{}", self.origin_stream_id, self.origin_sequence),
            "sync_operation_identity_invalid"
        );
        self.mutation.validate()
    }
}

pub(crate) fn validate_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
        "sync_identity_invalid"
    );
    Ok(())
}

mod mutation;
mod records;

pub use mutation::SyncMutation;
pub use records::{
    AnalysisRecord, ConversationRecord, EventRecord, GoalRecord, HabitNudgeRecord,
    HabitPageLinkRecord, InputRecord, KnowledgeReadingStateRecord, KnowledgeReviewRecord,
    MessageRecord, ProviderBundle, ProviderProfile, RuleRecord, TodoRecord, WikiPageRecord,
    WikiRevisionRecord, analysis_task_id,
};
