//! 事件 / 输入记录 / 分析队列存取（从 `Store` 抽出）。核心写路径。
//!
//! - insert_event：唯一提交边界，单事务写入 event + analysis_job（+ input_record）。
//!   原始事件不可变由 DB trigger 强制，分析失败不阻塞落盘（进 retry）。
//! - input_records：统一输入关联层，幂等键 INSERT OR IGNORE + 回读（原子）。
//! - analysis_jobs：claim/complete/fail + 指数退避 available_at，重启恢复 running→retry。

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use uuid::Uuid;

use super::adapter::AnalysisJob;
use super::digest::enqueue_digest_job_on;
use super::{
    AnalysisJobStats, DailyEntry, DailyReviewRecord, EventAnalysisDetail,
    EventRecordabilityDecision, EventSummary, InputRecord, InsightSummary, NewEvent,
    PageGuidanceItem, Store, map_input_record,
};
use super::{EventEmotionItem, parse_json_string_array};

/// Shared raw-event write used by capture and authenticated replication inside
/// their caller-owned transaction. Job scheduling is a separate local concern.
mod analysis;
mod capture;
mod guidance;
mod query;

pub(crate) use capture::insert_raw_event_on;
pub(crate) use guidance::local_day_bounds;
