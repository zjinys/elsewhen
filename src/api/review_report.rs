//! 周期报告管道 + 偏差检测（R3，FR-PES-005-04）。
//!
//! 分层：报告数字来自本地 SQL 实时聚合（R2 聚合层同一实现），AI 只做
//! 「统计与目标之间有没有值得说的偏差」的语义判定——统计本地算，偏差
//! 才用 AI（FR-PES-005 不变式 1）。
//!
//! 触达语义（对 Q4 裁决与验收场景「进程一周未开 → 启动后补做本周报告，
//! 历史空窗不补」的调和）：每个 tick 保证**最近一个完整周期**有报告
//! （若非空且缺失则补做），生成时刻的下限是周期结束后的第一个 08:00；
//! 只补这一个，更早的空窗不追补。UNIQUE(period, period_type) 让补做可以
//! 在任意 tick 安全重试。
//!
//! 生成与检测独立状态：报告先落库（可读），检测失败或 Provider 未配置
//! 都只是把 `deviations_json` 留在 NULL（「尚未检测」，合法状态），退避
//! 重试由同一条 5s timer worker 承担——与知识消化同模式。

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use uuid::Uuid;

use super::*;

/// 报告结构版本：统计字段演进时递增，消费方（R4 呈现）按版本解析。
pub(crate) const REPORT_SCHEMA_VERSION: &str = "deviation-report-v1";

/// 偏差类型枚举（closed set，逐字校验；R4 呈现做中文化映射）。
pub(crate) const DEVIATION_TYPES: [&str; 4] = ["no_progress", "stalled", "conflict", "scope_drift"];
/// 严重度枚举。
pub(crate) const SEVERITIES: [&str; 3] = ["low", "medium", "high"];
/// 检测退避基数与上限（连续失败时指数退避，防每 5s 打一次 provider）。
const DETECT_BACKOFF_BASE_MS: i64 = 5 * 60 * 1000;
const DETECT_BACKOFF_MAX_MS: i64 = 60 * 60 * 1000;

// ── 窗口计算 ─────────────────────────────────────────────────────────

/// 报告窗口：period = 周期末日（周=周日、月=月末），本地日界含两端。
mod build;
mod detect;
mod schedule;
mod store;

#[cfg(test)]
pub(crate) use build::ReportGoalSnapshot;
pub(crate) use build::{PeriodReport, validate_deviation_candidates};
pub(crate) use build::{build_deviation_prompt, build_period_report};
#[cfg(test)]
pub(crate) use build::{fmt_date, top_groups};
pub use detect::{PeriodReportTickResult, tick_periodic_reports};
#[cfg(test)]
pub(crate) use detect::{detect_deviation, finish_run, start_run};
pub(crate) use schedule::{due_reports, report_window};
#[cfg(test)]
pub(crate) use schedule::{latest_complete_month, latest_complete_week};
pub use store::{
    DeviationItemDto, DeviationReportDto, dismiss_deviation, list_deviation_reports,
    mark_deviation_report_read,
};

#[cfg(test)]
#[path = "review_report_tests/mod.rs"]
mod tests;
