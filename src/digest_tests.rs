//! 知识消化队列（FR-PES-004 阶段 1）集成测试：临时库 + 脚本化 provider。
//!
//! 覆盖旧时间游标的漏处理场景（积压超上限、同时间戳、迟到、调用期间新增），
//! 以及整批原子性、证据去重、退避/冷却回队、中断恢复、人工编辑保护、
//! 可记录性过滤与运行日志。

use crate::ai::memory::ContextMessage;
use crate::ai::provider::{AiProvider, AiReply};
use crate::ai::tool::ToolSpec;
use crate::event::NewEvent;
use crate::storage::{ContentPolicy, Store, WikiPageDraft, DIGEST_MAX_ATTEMPTS};
use crate::wiki::{process_digest_queue, DigestTick};
use anyhow::Result;
use chrono::{Duration, Utc};
use std::cell::Cell;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_database() -> PathBuf {
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

fn make_ready(store: &Store) {
    store
        .connection
        .execute(
            "UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00+00:00'
             WHERE status IN ('pending','retry','failed')",
            [],
        )
        .unwrap();
}

fn record(store: &Store, text: &str) -> String {
    let id = store.insert_event(NewEvent::now(text)).unwrap();
    mark_analyzed(store, &id, true);
    id
}

fn job_status(store: &Store, event_id: &str) -> (String, i64, i64) {
    store
        .connection
        .query_row(
            "SELECT status, attempts, failed_rounds FROM knowledge_digest_jobs WHERE event_id=?1",
            [event_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap()
}

fn drain(store: &Store, provider: &dyn AiProvider) -> Vec<DigestTick> {
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

#[test]
fn new_event_waits_for_settle_window_then_digests() {
    let t = TempStore::new();
    let id = record(&t.store, "今天晨跑 5 公里");
    let provider = ScriptedProvider(|p: &str| Ok(habit_page_for_all(p)));
    assert_eq!(
        process_digest_queue(&t.store, &provider, None).unwrap(),
        DigestTick::Idle,
        "沉淀窗内不领取"
    );
    make_ready(&t.store);
    let tick = process_digest_queue(&t.store, &provider, None).unwrap();
    assert!(matches!(tick, DigestTick::Processed { events: 1, .. }));
    assert_eq!(job_status(&t.store, &id).0, "succeeded");
    let page = t.store.get_wiki_page("habit/morning-run").unwrap().unwrap();
    assert_eq!(page.source_event_ids, vec![id]);
    assert_eq!(page.evidence_count, 1);
}

#[test]
fn backlog_beyond_old_limit_same_timestamp_and_late_events_are_all_digested() {
    let t = TempStore::new();
    let base = Utc::now() - Duration::days(30);
    let mut ids = Vec::new();
    // 120 条：同一时间戳 40 条 + 递增 60 条 + 20 条「迟到」（记录时间早于已处理的）
    for i in 0..120 {
        let at = match i {
            0..=39 => base,
            40..=99 => base + Duration::minutes(i as i64),
            _ => base - Duration::days(3),
        };
        let text = format!("晨跑记录 {i}");
        let id = t
            .store
            .insert_event(NewEvent {
                raw_text: &text,
                occurred_at: at,
                recorded_at: at,
                source: "capture",
            })
            .unwrap();
        mark_analyzed(&t.store, &id, true);
        ids.push(id);
        if i == 99 {
            // 前 100 条先消化完，再来 20 条迟到事件
            make_ready(&t.store);
            drain(
                &t.store,
                &ScriptedProvider(|p: &str| Ok(habit_page_for_all(p))),
            );
        }
    }
    make_ready(&t.store);
    drain(
        &t.store,
        &ScriptedProvider(|p: &str| Ok(habit_page_for_all(p))),
    );

    let stats = t.store.digest_job_stats().unwrap();
    assert_eq!(stats.succeeded, 120, "{stats:?}");
    assert_eq!(
        stats.pending + stats.retry + stats.running + stats.failed,
        0
    );
    let page = t.store.get_wiki_page("habit/morning-run").unwrap().unwrap();
    assert_eq!(page.evidence_count, 120, "每个事件恰计一次证据");
}

#[test]
fn retry_and_redelivery_do_not_double_count_evidence() {
    let t = TempStore::new();
    let id = record(&t.store, "晨跑");
    make_ready(&t.store);
    let fail_once = Cell::new(true);
    let provider = ScriptedProvider(|p: &str| {
        if fail_once.replace(false) {
            anyhow::bail!("simulated timeout")
        }
        Ok(habit_page_for_all(p))
    });
    assert!(matches!(
        process_digest_queue(&t.store, &provider, None).unwrap(),
        DigestTick::Failed { .. }
    ));
    assert_eq!(job_status(&t.store, &id).0, "retry");
    make_ready(&t.store);
    drain(&t.store, &provider);
    assert_eq!(job_status(&t.store, &id).0, "succeeded");

    // 重复投递：同一事件任务被重新置为 pending 再消化一次
    t.store
        .connection
        .execute(
            "UPDATE knowledge_digest_jobs SET status='pending' WHERE event_id=?1",
            [&id],
        )
        .unwrap();
    make_ready(&t.store);
    drain(&t.store, &provider);
    let page = t.store.get_wiki_page("habit/morning-run").unwrap().unwrap();
    assert_eq!(page.evidence_count, 1);
    assert_eq!(page.source_event_ids, vec![id]);
}

#[test]
fn invalid_proposals_roll_back_whole_batch() {
    let replies = [
        "这不是 JSON",
        // 一条合法 + 一条越界编号：整批不写
        r#"[{"kind":"habit","slug":"habit/a","title":"A","content":"- a","source_event_ids":["1"]},
            {"kind":"habit","slug":"habit/b","title":"B","content":"- b","source_event_ids":["99"]}]"#,
        // 空来源
        r#"[{"kind":"habit","slug":"habit/a","title":"A","content":"- a","source_event_ids":[]}]"#,
        // 非法 kind
        r#"[{"kind":"nonsense","slug":"habit/a","title":"A","content":"- a","source_event_ids":["1"]}]"#,
    ];
    for reply in replies {
        let t = TempStore::new();
        let id = record(&t.store, "晨跑");
        make_ready(&t.store);
        let log_before = t.store.list_wiki_log(100).unwrap().len();
        let provider = ScriptedProvider(|_: &str| Ok(reply.to_string()));
        let tick = process_digest_queue(&t.store, &provider, None).unwrap();
        assert!(
            matches!(tick, DigestTick::Failed { events: 1, .. }),
            "{reply}"
        );
        assert!(
            t.store.get_wiki_page("habit/a").unwrap().is_none(),
            "{reply}"
        );
        assert_eq!(t.store.list_wiki_log(100).unwrap().len(), log_before);
        assert_eq!(job_status(&t.store, &id).0, "retry");
        let runs = t.store.list_digest_runs(10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "failed");
        assert!(runs[0].error.is_some());
        assert!(
            !runs[0].error.as_deref().unwrap().contains("晨跑"),
            "运行日志不记录事件原文"
        );
    }
}

#[test]
fn exhausted_attempts_cool_down_then_requeue_automatically() {
    let t = TempStore::new();
    let id = record(&t.store, "晨跑");
    let provider = ScriptedProvider(|_: &str| anyhow::bail!("provider down"));
    for _ in 0..DIGEST_MAX_ATTEMPTS {
        make_ready(&t.store);
        assert!(matches!(
            process_digest_queue(&t.store, &provider, None).unwrap(),
            DigestTick::Failed { .. }
        ));
    }
    let (status, attempts, rounds) = job_status(&t.store, &id);
    assert_eq!(
        (status.as_str(), attempts, rounds),
        ("failed", DIGEST_MAX_ATTEMPTS, 0)
    );
    let stats = t.store.digest_job_stats().unwrap();
    assert_eq!(stats.failed, 1);
    assert!(stats.last_error.unwrap().contains("provider down"));

    // 冷却期内不回队
    t.store.settle_digest_queue().unwrap();
    assert_eq!(job_status(&t.store, &id).0, "failed");
    // 冷却期满：自动回队，本轮尝试次数清零
    make_ready(&t.store);
    t.store.settle_digest_queue().unwrap();
    assert_eq!(job_status(&t.store, &id), ("retry".to_string(), 0, 1));
    let ok = ScriptedProvider(|p: &str| Ok(habit_page_for_all(p)));
    make_ready(&t.store);
    drain(&t.store, &ok);
    assert_eq!(job_status(&t.store, &id).0, "succeeded");
}

#[test]
fn interrupted_batch_is_recovered() {
    let t = TempStore::new();
    let id = record(&t.store, "晨跑");
    make_ready(&t.store);
    let batch = t.store.claim_digest_batch(20, 6000, None).unwrap().unwrap();
    assert_eq!(batch.events.len(), 1);
    assert_eq!(job_status(&t.store, &id).0, "running");
    // 进程退出后重启
    assert_eq!(t.store.recover_interrupted_digest_jobs().unwrap(), 1);
    assert_eq!(job_status(&t.store, &id).0, "retry");
    assert_eq!(t.store.list_digest_runs(10).unwrap()[0].status, "failed");
    drain(
        &t.store,
        &ScriptedProvider(|p: &str| Ok(habit_page_for_all(p))),
    );
    assert_eq!(job_status(&t.store, &id).0, "succeeded");
}

#[test]
fn human_edited_page_keeps_content_and_accumulates_evidence() {
    let t = TempStore::new();
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/morning-run".into(),
                kind: "habit".into(),
                title: "晨跑".into(),
                summary: "".into(),
                content_md: "- 人工写的正文".into(),
                tags: vec![],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "test".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    t.store
        .connection
        .execute(
            "UPDATE wiki_pages SET human_edited_at=?1 WHERE slug='habit/morning-run'",
            [Utc::now().to_rfc3339()],
        )
        .unwrap();
    let id = record(&t.store, "晨跑");
    make_ready(&t.store);
    let tick = process_digest_queue(
        &t.store,
        &ScriptedProvider(|p: &str| Ok(habit_page_for_all(p))),
        None,
    )
    .unwrap();
    let DigestTick::Processed { outcome, .. } = &tick else {
        panic!("{tick:?}")
    };
    assert_eq!(outcome.protected, vec!["habit/morning-run".to_string()]);
    let page = t.store.get_wiki_page("habit/morning-run").unwrap().unwrap();
    assert_eq!(page.content_md, "- 人工写的正文");
    assert_eq!(page.source_event_ids, vec![id]);
}

#[test]
fn non_recordable_events_are_skipped_until_marked_recordable() {
    let t = TempStore::new();
    let id = t.store.insert_event(NewEvent::now("哈哈好的")).unwrap();
    mark_analyzed(&t.store, &id, false);
    make_ready(&t.store);
    let provider = ScriptedProvider(|p: &str| Ok(habit_page_for_all(p)));
    assert_eq!(
        process_digest_queue(&t.store, &provider, None).unwrap(),
        DigestTick::Idle
    );
    assert_eq!(job_status(&t.store, &id).0, "skipped");
    assert!(
        t.store.list_digest_runs(10).unwrap().is_empty(),
        "空闲 tick 不写日志"
    );

    t.store.set_event_recordability(&id, true, "test").unwrap();
    assert_eq!(job_status(&t.store, &id).0, "pending");
    make_ready(&t.store);
    drain(&t.store, &provider);
    assert_eq!(job_status(&t.store, &id).0, "succeeded");
}

#[test]
fn digest_waits_for_analysis_to_finish() {
    let t = TempStore::new();
    let id = t.store.insert_event(NewEvent::now("晨跑")).unwrap();
    make_ready(&t.store);
    let provider = ScriptedProvider(|p: &str| Ok(habit_page_for_all(p)));
    assert_eq!(
        process_digest_queue(&t.store, &provider, None).unwrap(),
        DigestTick::Idle,
        "分析未终态时等待"
    );
    assert_eq!(job_status(&t.store, &id).0, "pending");
    // 分析最终失败：原始事件仍是真源，照常消化
    t.store
        .connection
        .execute(
            "UPDATE analysis_jobs SET status='failed' WHERE event_id=?1",
            [&id],
        )
        .unwrap();
    drain(&t.store, &provider);
    assert_eq!(job_status(&t.store, &id).0, "succeeded");
}

#[test]
fn events_recorded_during_model_call_are_not_lost() {
    let t = TempStore::new();
    let first = record(&t.store, "晨跑一");
    make_ready(&t.store);
    let path = t.path.clone();
    let late = std::cell::RefCell::new(None::<String>);
    let provider = ScriptedProvider(|p: &str| {
        if late.borrow().is_none() {
            // 模型调用期间另一个连接写入新事件（GUI 同时在记录）
            let other = Store::open(&path).unwrap();
            let id = other.insert_event(NewEvent::now("晨跑二")).unwrap();
            mark_analyzed(&other, &id, true);
            *late.borrow_mut() = Some(id);
        }
        Ok(habit_page_for_all(p))
    });
    assert!(matches!(
        process_digest_queue(&t.store, &provider, None).unwrap(),
        DigestTick::Processed { events: 1, .. }
    ));
    let late_id = late.borrow().clone().unwrap();
    assert_eq!(job_status(&t.store, &first).0, "succeeded");
    assert_eq!(
        job_status(&t.store, &late_id).0,
        "pending",
        "新事件留给下一批"
    );
    make_ready(&t.store);
    drain(&t.store, &provider);
    assert_eq!(job_status(&t.store, &late_id).0, "succeeded");
    let page = t.store.get_wiki_page("habit/morning-run").unwrap().unwrap();
    assert_eq!(page.evidence_count, 2);
}

#[test]
fn successful_run_is_logged_without_raw_text() {
    let t = TempStore::new();
    record(&t.store, "晨跑一");
    record(&t.store, "晨跑二");
    make_ready(&t.store);
    drain(
        &t.store,
        &ScriptedProvider(|p: &str| Ok(habit_page_for_all(p))),
    );
    let runs = t.store.list_digest_runs(10).unwrap();
    assert_eq!(runs.len(), 1, "一批一条日志，空闲 tick 不写");
    let run = &runs[0];
    assert_eq!(run.status, "succeeded");
    assert_eq!(run.event_count, 2);
    assert_eq!(run.created_slugs, vec!["habit/morning-run".to_string()]);
    assert!(run.duration_ms.is_some());
    assert_eq!(run.model.as_deref(), Some("stub"));
    assert!(run.error.is_none());
    let stats = t.store.digest_job_stats().unwrap();
    assert_eq!(stats.succeeded, 2);
    assert!(stats.last_success_at.is_some());
    let jobs = t.store.list_digest_jobs(Some("succeeded"), 10).unwrap();
    assert_eq!(jobs.len(), 2);
    assert!(jobs
        .iter()
        .all(|job| job.batch_id.as_deref() == Some(run.id.as_str())));
}

#[test]
fn every_event_is_enqueued_once_at_insert_and_open_never_duplicates() {
    let t = TempStore::new();
    t.store.insert_event(NewEvent::now("一")).unwrap();
    t.store.insert_event(NewEvent::now("二")).unwrap();
    // 无沉淀窗：任务应立即可领取
    let ready: i64 = t
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_digest_jobs WHERE available_at<=?1",
            [Utc::now().to_rfc3339()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(ready, 0, "刚落库的事件应留沉淀窗，不立即可领取");
    assert_eq!(
        t.store.digest_job_stats().unwrap().pending,
        2,
        "事件一落库就入队"
    );
    make_ready(&t.store);
    assert_eq!(
        t.store.digest_job_stats().unwrap().pending,
        2,
        "快进沉淀窗后仍在队列里"
    );

    // 重复开库不得重复入队，也不得把已入队的丢掉
    let reopened = Store::open(&t.path).unwrap();
    assert_eq!(reopened.digest_job_stats().unwrap().pending, 2);
    drop(reopened);
    let again = Store::open(&t.path).unwrap();
    assert_eq!(
        again.digest_job_stats().unwrap().pending,
        2,
        "重复 open 不重复入队"
    );
}

#[test]
fn no_provider_config_means_no_digest_provider() {
    let t = TempStore::new();
    record(&t.store, "晨跑");
    assert!(crate::wiki::digest_provider(&t.store).unwrap().is_none());
    assert_eq!(
        t.store.digest_job_stats().unwrap().pending,
        1,
        "事件照常入队等待"
    );
}
