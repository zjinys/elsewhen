//! 知识库（wiki）存取（从 `Store` 抽出）。
//!
//! wiki_pages（含 area 分区 / based_on 派生 / content_type）+ wiki_revisions
//! （每次正文变更落版本）+ wiki_log（操作流水）。合并/改名经 record_wiki_revision
//! 记录历史，slug 唯一性由 unique_slug 类逻辑保证。

use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use super::{
    ContentPolicy, EventRecord, RenameWikiOutcome, Store, WIKI_PAGE_COLS, WIKI_PAGE_META_COLS,
    WikiPage, WikiPageDraft, WikiPageMeta, WikiUpsertOutcome, map_wiki_page, map_wiki_page_meta,
};

mod content;
mod export;
mod lifecycle;
mod modify;
mod queries;
