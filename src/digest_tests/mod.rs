//! 知识消化队列（FR-PES-004 阶段 1）集成测试：临时库 + 脚本化 provider。
//!
//! 覆盖旧时间游标的漏处理场景（积压超上限、同时间戳、迟到、调用期间新增），
//! 以及整批原子性、证据去重、退避/冷却回队、中断恢复、人工编辑保护、
//! 可记录性过滤与运行日志。

use crate::ai::memory::ContextMessage;
use crate::ai::provider::{AiProvider, AiReply};
use crate::ai::tool::ToolSpec;
use crate::event::NewEvent;
use crate::storage::{ContentPolicy, DIGEST_MAX_ATTEMPTS, Store, WikiPageDraft};
use crate::wiki::{DigestTick, process_digest_queue};
use anyhow::Result;
use chrono::{Duration, Utc};
use std::cell::Cell;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// 公共测试辅助（原文件平移，pub(super) 供分组子文件复用）。
pub(super) fn temporary_database() -> PathBuf {
    std::env::temp_dir().join(format!(
        "elsewhen-digest-test-{}-{}.db",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

struct TempStore {
    path: PathBuf,
    store: Store,
}

impl TempStore {
    fn new() -> Self {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        Self { path, store }
    }
}

impl Drop for TempStore {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.path.with_extension("db-wal"));
        let _ = std::fs::remove_file(self.path.with_extension("db-shm"));
    }
}

/// 以用户提示词为输入的脚本化 provider。
struct ScriptedProvider<F: Fn(&str) -> Result<String>>(F);

impl<F: Fn(&str) -> Result<String>> AiProvider for ScriptedProvider<F> {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        let user = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.clone())
            .unwrap_or_default();
        (self.0)(&user).map(AiReply::text)
    }
}

/// 提示词里本批事件的数量（按 `[N] ` 行计）。
fn batch_size(prompt: &str) -> usize {
    prompt
        .lines()
        .take_while(|line| !line.starts_with("当前 wiki 索引"))
        .filter(|line| line.starts_with('[') && line.contains("] "))
        .count()
}

/// 把本批全部事件归到同一页的提议。
fn habit_page_for_all(prompt: &str) -> String {
    let ids: Vec<String> = (1..=batch_size(prompt))
        .map(|n| format!("\"{n}\""))
        .collect();
    format!(
        r#"[{{"op":"update","kind":"habit","slug":"habit/morning-run","title":"晨跑","summary":"坚持晨跑","content":"- 晨跑","tags":[],"source_event_ids":[{}]}}]"#,
        ids.join(",")
    )
}

/// 模拟事件分析已完成（可指定可记录性），并把消化任务调到可领取。
fn mark_analyzed(store: &Store, event_id: &str, recordable: bool) {
    let kind = if recordable { "event" } else { "chitchat" };
    store
        .connection
        .execute(
            "INSERT INTO event_analyses (id,event_id,prompt_version,result_json,created_at)
             VALUES (lower(hex(randomblob(16))),?1,'event-analysis',?2,?3)",
            rusqlite::params![
                event_id,
                format!(r#"{{"recordable":{recordable},"kind":"{kind}"}}"#),
                Utc::now().to_rfc3339()
            ],
        )
        .unwrap();
    store
        .connection
        .execute(
            "UPDATE analysis_jobs SET status='succeeded' WHERE event_id=?1",
            [event_id],
        )
        .unwrap();
}

pub(super) fn make_ready(store: &Store) {
    store
        .connection
        .execute(
            "UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00+00:00'
             WHERE status IN ('pending','retry','failed')",
            [],
        )
        .unwrap();
}

pub(super) fn record(store: &Store, text: &str) -> String {
    let id = store.insert_event(NewEvent::now(text)).unwrap();
    mark_analyzed(store, &id, true);
    id
}

pub(super) fn job_status(store: &Store, event_id: &str) -> (String, i64, i64) {
    store
        .connection
        .query_row(
            "SELECT status, attempts, failed_rounds FROM knowledge_digest_jobs WHERE event_id=?1",
            [event_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap()
}

pub(super) fn drain(store: &Store, provider: &dyn AiProvider) -> Vec<DigestTick> {
    let mut ticks = Vec::new();
    for _ in 0..50 {
        let tick = process_digest_queue(store, provider, Some("stub")).unwrap();
        let idle = tick == DigestTick::Idle;
        ticks.push(tick);
        if idle {
            break;
        }
    }
    ticks
}

// ── 提案 A/B（知识页持续更新，2026-10-20）：相关性选页 + 同义新建改写 ──

/// 捕获发给模型的本批用户提示词，供断言上下文选页结果。
fn capture_prompt() -> (
    ScriptedProvider<impl Fn(&str) -> Result<String>>,
    std::rc::Rc<std::cell::RefCell<String>>,
) {
    let seen = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    let sink = seen.clone();
    (
        ScriptedProvider(move |prompt: &str| {
            sink.borrow_mut().push_str(prompt);
            Ok(habit_page_for_all(prompt))
        }),
        seen,
    )
}

// ── P2 后台再汇总任务 + P3 关系网（提案 2026-10-19，2026-10-20）──

fn draft(slug: &str, kind: &str, title: &str, content: &str, tags: Vec<String>) -> WikiPageDraft {
    WikiPageDraft {
        slug: slug.into(),
        kind: kind.into(),
        title: title.into(),
        summary: String::new(),
        content_md: content.into(),
        tags,
        source_event_ids: vec![],
        status: "active".into(),
        reason: "test".into(),
        source_url: None,
    }
}

mod digest_queue;

mod page_relevance;
mod reconsolidation;

mod mention_relations;
