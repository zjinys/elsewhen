//! 工作流记录存取（从 `Store` 抽出）。
//!
//! 规则（rules，pending → active 确认门）、个人待办（todos）、人物关系（relations）、
//! AI 写类工具的待确认动作（pending_actions：草拟 → 用户确认 → 执行/拒绝）、
//! 应用元数据（app_meta）与全量分析列表。

use anyhow::Result;
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use super::adapter::AnalysisSummary;
use super::{
    PendingAction, Relation, RelationDraft, RuleStatus, RuleSummary, Store, Todo, TodoStatus,
};

mod meta;
mod pending;
mod relations;
mod rules;
mod todos;
