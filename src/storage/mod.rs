mod adapter;
pub(crate) mod approvals;
pub(crate) mod chat_page_tasks;
pub(crate) mod chat_queue;
pub(crate) mod chat_runs;
pub(crate) mod chat_summaries;
pub(crate) mod chat_supplements;
pub(crate) mod companion;
mod conversations;
mod digest;
mod entities;
mod events;
mod goals;
mod habit_nudges;
pub(crate) mod knowledge;
mod mcp_servers;
mod new_migrations;
pub mod page_marks;
mod provider;
mod provider_keys;
pub use mcp_servers::McpServerConfig;
pub(crate) use page_marks::PageMark;

mod records;
pub mod relations;
mod reminders;
mod review;
pub(crate) mod rule_triggers;
pub(crate) mod skills;
pub(crate) mod sync;
mod web_search;
mod wiki;
pub(crate) mod wiki_images;

// 唯一跨模块复用的 review 行投影（R4 报告读面 DTO 组装用）。
pub(crate) use review::DeviationReportRow;

use crate::event::{EventSummary, NewEvent};
use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

// Schema initialization is process-scoped, not API-call-scoped.  API entry
// points still open short-lived connections, but only the first connection for
// a database path performs migrations/backfills/checkpointing.
static INITIALIZED_DATABASES: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

/// 后台 worker 路径的写冲突重试：`SQLITE_BUSY` 时短暂退避，其余错误直接上抛。
/// 仅用于「失败可安全重试、下个 tick 也会重来」的幂等写操作（队列回收/领取）；
/// 前台用户操作不要用——长等待应由 busy_timeout 与明确报错呈现。
pub(crate) fn with_busy_retry<T>(mut op: impl FnMut() -> Result<T>) -> Result<T> {
    const BACKOFF_MS: [u64; 2] = [150, 400];
    for (attempt, backoff) in BACKOFF_MS.iter().enumerate() {
        match op() {
            Err(error) if is_busy_error(&error) => {
                std::thread::sleep(std::time::Duration::from_millis(*backoff));
                if attempt + 1 == BACKOFF_MS.len() {
                    return Err(error);
                }
            }
            result => return result,
        }
    }
    op()
}

fn is_busy_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<rusqlite::Error>(),
            Some(rusqlite::Error::SqliteFailure(ffi, _))
                if ffi.code == rusqlite::ErrorCode::DatabaseBusy
        )
    })
}

pub use adapter::{
    AiProviderConfig, AiProviderConfigRow, AnalysisJob, AnalysisSummary, StorageAdapter,
};
pub use digest::{
    DIGEST_FAILED_COOLDOWN_SECS, DIGEST_MAX_ATTEMPTS, DIGEST_SETTLE_SECS, DIGEST_VERSION,
    DigestBatch, DigestBatchOutcome, DigestJobRow, DigestJobStats, DigestRunRow,
};
pub use goals::{Goal, GoalPhase, GoalStatus, MAX_ACTIVE_GOALS};
pub use knowledge::{
    KnowledgeIssue, KnowledgeMetadata, KnowledgeProposal, RejectedSourceRow, SourceSnapshot,
};
pub use reminders::DueReminder;
pub use web_search::{DohMode, NetworkSettings, WebSearchConfig};

// Conversation and Message summary structs
mod as_adapter;
mod dto;

pub use dto::*;

#[cfg(test)]
mod tests;
