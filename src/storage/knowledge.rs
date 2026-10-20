//! Exact source versions, reviewable compilation and knowledge provenance.
use super::{ContentPolicy, Store, WIKI_PAGE_COLS, WikiPage, WikiPageDraft, map_wiki_page};
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
/// 「已否定」列表的行（负知识一等公民，提案 2026-10-19）：身份 + 否定
/// 原因；title/slug 来自共享该 identity 的首个页面，仅为展示面。
pub mod core;
mod issues;
mod proposals;
mod sources;

pub use core::{
    KnowledgeIssue, KnowledgeMetadata, KnowledgeProposal, RejectedSourceRow, SourceSnapshot,
};
pub(crate) use core::{
    SNAPSHOT_SELECT, backfill_sources_once, capture_source_on, content_hash, snapshot_row,
    source_change_preview, source_identity,
};
