//! storage 行为测试（按域分组：事件/分析队列/基础设施/实体/wiki/待办杂项）。
//! 原为 mod.rs 内联 mod tests 整体外移，后按 600 行上限二次分组；只测公共 API
//! + 少量 pub(crate) 辅助。共享辅助 temporary_database/entity_page/wiki_draft 一并在此。

use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

// 公共测试辅助（原文件 helpers 原样平移，仅提升可见性给分组子文件）。
pub(super) fn temporary_database() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "elsewhen-storage-test-{}.db",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

pub(super) fn entity_page(store: &Store, slug: &str, kind: &str, title: &str) {
    store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: slug.to_string(),
                kind: kind.to_string(),
                title: title.to_string(),
                summary: String::new(),
                content_md: format!("# {title}"),
                tags: vec![],
                source_event_ids: vec![],
                status: "active".to_string(),
                reason: "test".to_string(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
}

// ── M1：人工编辑保护（human_edited_at + ContentPolicy + opinion）──────────

fn wiki_draft(slug: &str, kind: &str, content_md: &str) -> WikiPageDraft {
    WikiPageDraft {
        slug: slug.to_string(),
        kind: kind.to_string(),
        title: slug.to_string(),
        summary: "s".to_string(),
        content_md: content_md.to_string(),
        tags: vec![],
        source_event_ids: vec![],
        status: "active".to_string(),
        reason: "test".to_string(),
        source_url: None,
    }
}

mod events;
mod analysis_jobs;
mod infra;
mod entities;
mod todos_misc;
mod digest_content;
mod wiki_pages;
