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

/// 回执身份不符时的错误文本（缺陷 D3 / F13 的诊断口径）。
///
/// 为什么错误文本里要塞字段：拒收原因会随 `sync_wire_rejections.last_error`
/// 落库，而 `message_id` 是 `envelope-receipt-{hash}` 这种不可反查的形状 ——
/// 上一轮排查唯一的线索就是一行 `sync_receipt_identity_mismatch`，既不知道是
/// dataset、target 还是 acknowledging 对不上，也不知道两边各是什么。release
/// 包拿不到调试日志，所以诊断必须进数据本身。
///
/// 只列出**真正不符**的字段（相等的不写，噪声会淹没线索）。开头保留原始错误
/// 码，既有日志检索与前缀匹配不受影响。三个字段都是公开标识，不含密钥。
///
/// 字段读作 `名字 expected=<本机认定的值> actual=<信封里写的值>` ——
/// 「local/recv」这种命名在 acknowledging 字段上会误导（本机其实没有
/// 「本地 ack」这回事，本机有的是「这封信是谁认证发来的」）。
pub(crate) fn receipt_identity_mismatch(
    kind: &str,
    fields: &[(&str, &str, &str)],
) -> anyhow::Error {
    let detail = fields
        .iter()
        .filter(|(_, expected, actual)| expected != actual)
        .map(|(name, expected, actual)| format!("{name} expected={expected} actual={actual}"))
        .collect::<Vec<_>>()
        .join(" ");
    anyhow::anyhow!("sync_receipt_identity_mismatch kind={kind} {detail}")
}

/// 交接（enrollment）阶段才有的控制帧。
///
/// 用途只有一个：**判断一封迟到的重投是不是「已经做完的事」**。交接收尾后对端
/// 可能还在重投这些帧（它没等到回执），此时本机会话已经结束、普通分派里没有
/// 它们的分支 —— 真机现场（2026-10-27 11:03）一封 `enrollment-complete:` 就是
/// 这样被记成 `sync_wire_kind_unsupported` 连拒 3 次后丢弃的。
pub(crate) fn is_enrollment_control_id(id: &str) -> bool {
    id.starts_with("membership:")
        || id.starts_with("enrollment-")
        || id.starts_with("snapshot-start:")
        || id.starts_with("snapshot-end:")
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
