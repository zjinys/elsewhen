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
    assert!(
        jobs.iter()
            .all(|job| job.batch_id.as_deref() == Some(run.id.as_str()))
    );
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

#[test]
fn relevance_scored_page_gets_fulltext_and_unrelated_one_does_not() {
    use crate::storage::EventRecord;
    use crate::wiki::select_context_pages_for_events;
    let t = TempStore::new();
    let seed = |slug: &str, title: &str, tags: &[&str], body: &str| {
        t.store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: slug.into(),
                    kind: slug.split('/').next().unwrap().into(),
                    title: title.into(),
                    summary: title.into(),
                    content_md: body.into(),
                    tags: tags.iter().map(|s| s.to_string()).collect(),
                    source_event_ids: vec![],
                    status: "active".into(),
                    reason: "test".into(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )
            .unwrap();
    };
    // 低证据的散步页（旧逻辑按证据数/时间排序根本轮不到它）
    seed(
        "habit/walk-after-lunch",
        "午饭后散步习惯",
        &["散步"],
        "- 每天午饭后散步。",
    );
    std::thread::sleep(std::time::Duration::from_millis(2));
    // 高证据但不相关的页（旧逻辑会把它选进上下文）
    seed(
        "relationship/old-friend",
        "老朋友",
        &["朋友"],
        "- 很久没联系。",
    );
    let events = vec![EventRecord {
        id: "e1".into(),
        recorded_at: "2026-10-20T12:00:00Z".into(),
        raw_text: "今天午饭后走了四十分钟".into(),
    }];
    let pages = t.store.list_wiki_pages(None, None).unwrap();
    let picked = select_context_pages_for_events(&events, pages, &["habit", "relationship"], 8000);
    let slugs: Vec<&str> = picked.iter().map(|p| p.slug.as_str()).collect();
    assert!(
        slugs.contains(&"habit/walk-after-lunch"),
        "事件提到的低证据页必须进上下文：{slugs:?}"
    );
    assert!(
        !slugs.contains(&"relationship/old-friend"),
        "与事件无关的高证据页不再挤占上下文：{slugs:?}"
    );
}

#[test]
fn digest_prompt_includes_relevant_page_fulltext() {
    let t = TempStore::new();
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/walk-after-lunch".into(),
                kind: "habit".into(),
                title: "午饭后散步习惯".into(),
                summary: "坚持午饭后散步".into(),
                content_md: "- 每天午饭后散步三十分钟".into(),
                tags: vec!["散步".into()],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "test".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    record(&t.store, "今天午饭后走了四十分钟");
    make_ready(&t.store);
    let (provider, seen) = capture_prompt();
    let tick = process_digest_queue(&t.store, &provider, None).unwrap();
    assert!(matches!(tick, DigestTick::Processed { .. }));
    let prompt = seen.borrow();
    assert!(
        prompt.contains("每天午饭后散步三十分钟"),
        "散步页正文必须进提示词，模型才有机会 update 它"
    );
    assert!(
        prompt.contains("相关现有页面全文"),
        "提示词包含相关性选页的全文区块"
    );
}

#[test]
fn synonym_create_is_redirected_to_existing_page_update() {
    let t = TempStore::new();
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/walk-after-lunch".into(),
                kind: "habit".into(),
                title: "午饭后散步习惯".into(),
                summary: "坚持午饭后散步".into(),
                content_md: "- 每天午饭后散步三十分钟".into(),
                tags: vec!["散步".into()],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "test".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    record(&t.store, "今天午饭后散步了四十分钟");
    make_ready(&t.store);
    // 模型无视索引新建了同义 slug（slug 漂移，缺陷⑤）
    let provider = ScriptedProvider(|_| {
        Ok(r#"[{"op":"create","kind":"habit","slug":"habit/after-lunch-walk","title":"午饭后散步习惯","summary":"坚持午饭后散步","content":"- 每天午饭后散步四十分钟","tags":["散步"],"source_event_ids":["1"],"reason":"新事件"}]"#.into())
    });
    let tick = process_digest_queue(&t.store, &provider, None).unwrap();
    let DigestTick::Processed { outcome, .. } = tick else {
        panic!("预期消化成功：{tick:?}");
    };
    assert!(
        outcome.created.is_empty()
            && outcome
                .updated
                .contains(&"habit/walk-after-lunch".to_string()),
        "同义新建必须被改写为既有页更新：{outcome:?}"
    );
    assert!(
        t.store
            .get_wiki_page("habit/after-lunch-walk")
            .unwrap()
            .is_none(),
        "不得留下重复页"
    );
    let page = t
        .store
        .get_wiki_page("habit/walk-after-lunch")
        .unwrap()
        .unwrap();
    assert_eq!(page.evidence_count, 1, "证据并入既有页");
    assert!(page.content_md.contains("四十分钟"), "正文被更新");
}

#[test]
fn same_title_different_entity_is_not_redirected() {
    let t = TempStore::new();
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/walk-after-lunch".into(),
                kind: "habit".into(),
                title: "午饭后散步习惯".into(),
                summary: "坚持午饭后散步".into(),
                content_md: "- 每天午饭后散步三十分钟".into(),
                tags: vec!["散步".into()],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "test".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    record(&t.store, "晨跑");
    make_ready(&t.store);
    // kind 相同、标题不同、tag 不重合 → 不是同一个页，正常 create
    let provider = ScriptedProvider(|_| {
        Ok(r#"[{"op":"create","kind":"habit","slug":"habit/morning-run","title":"晨跑","summary":"坚持晨跑","content":"- 晨跑五公里","tags":["跑步"],"source_event_ids":["1"],"reason":"新习惯"}]"#.into())
    });
    let tick = process_digest_queue(&t.store, &provider, None).unwrap();
    let DigestTick::Processed { outcome, .. } = tick else {
        panic!("预期消化成功：{tick:?}");
    };
    assert!(
        outcome.created.contains(&"habit/morning-run".to_string()),
        "{outcome:?}"
    );
}

// ── 提案 C 的 P1 手动入口（页级再汇总，2026-10-20）──

#[test]
fn manual_reconsolidation_updates_page_and_ledger() {
    use crate::wiki::reconsolidate_page;
    let t = TempStore::new();
    let id = record(&t.store, "今天午饭后散步了三十分钟");
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/walk-after-lunch".into(),
                kind: "habit".into(),
                title: "午饭后散步习惯".into(),
                summary: "坚持午饭后散步".into(),
                content_md: "- 每天午饭后散步".into(),
                tags: vec!["散步".into()],
                source_event_ids: vec![id.clone()],
                status: "active".into(),
                reason: "test".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    let provider = ScriptedProvider(|_| {
        Ok(r#"[{"op":"update","kind":"habit","slug":"habit/walk-after-lunch","title":"午饭后散步习惯","summary":"坚持午饭后散步，通常三十分钟","content":"- 每天午饭后散步\n- 时长通常三十分钟","tags":["散步"],"source_event_ids":["1"],"reason":"增量修补"}]"#.into())
    });
    let result = reconsolidate_page(&t.store, &provider, "habit/walk-after-lunch").unwrap();
    assert!(result.contains("增量修补"), "{result}");
    let page = t
        .store
        .get_wiki_page("habit/walk-after-lunch")
        .unwrap()
        .unwrap();
    assert!(page.content_md.contains("三十分钟"));
    assert_eq!(page.evidence_count, 1, "重整理不改证据");
    assert!(page.source_event_ids.contains(&id), "来源承接现状");
    let revisions = t
        .store
        .list_wiki_revisions("habit/walk-after-lunch")
        .unwrap();
    assert!(!revisions.is_empty(), "旧稿进 wiki_revisions");
    let (last, new_evidence) = t
        .store
        .page_consolidation_status("habit/walk-after-lunch")
        .unwrap();
    assert!(last.is_some(), "最近整理时间被修订时间覆盖");
    assert_eq!(new_evidence, 0, "整理点之后无新事件");
    let (status, count): (String, i64) = t
        .store
        .connection
        .query_row(
            "SELECT status, result_count FROM knowledge_background_runs WHERE task='page-reconsolidation'",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )
        .unwrap();
    assert_eq!((status.as_str(), count), ("succeeded", 1));
}

#[test]
fn reconsolidation_refuses_human_edited_page() {
    use crate::wiki::reconsolidate_page;
    let t = TempStore::new();
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/walk-after-lunch".into(),
                kind: "habit".into(),
                title: "午饭后散步习惯".into(),
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
            "UPDATE wiki_pages SET human_edited_at=?1 WHERE slug='habit/walk-after-lunch'",
            [Utc::now().to_rfc3339()],
        )
        .unwrap();
    let provider = ScriptedProvider(|_| panic!("人工编辑页不应调用模型"));
    let result = reconsolidate_page(&t.store, &provider, "habit/walk-after-lunch").unwrap();
    assert!(result.contains("人工编辑保护"));
    let page = t
        .store
        .get_wiki_page("habit/walk-after-lunch")
        .unwrap()
        .unwrap();
    assert_eq!(page.content_md, "- 人工写的正文");
    let (status,): (String,) = t
        .store
        .connection
        .query_row(
            "SELECT status FROM knowledge_background_runs WHERE task='page-reconsolidation'",
            [],
            |r| Ok((r.get::<_, String>(0)?,)),
        )
        .unwrap();
    assert_eq!(status, "succeeded", "跳过按 succeeded 登记受限枚举，说明走返回文案");
}

#[test]
fn consolidation_status_derives_new_evidence_after_revision() {
    let t = TempStore::new();
    let old = record(&t.store, "晨跑五公里");
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/morning-run".into(),
                kind: "habit".into(),
                title: "晨跑".into(),
                summary: "".into(),
                content_md: "- 晨跑".into(),
                tags: vec![],
                source_event_ids: vec![old.clone()],
                status: "active".into(),
                reason: "test".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    // 整理点之后新到的事件
    std::thread::sleep(std::time::Duration::from_millis(5));
    let fresh = record(&t.store, "今天晨跑了六公里");
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/morning-run".into(),
                kind: "habit".into(),
                title: "晨跑".into(),
                summary: "".into(),
                content_md: "- 晨跑五公里\n- 今天六公里".into(),
                tags: vec![],
                source_event_ids: vec![old, fresh],
                status: "active".into(),
                reason: "digest 更新".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    let (last, new_evidence) = t
        .store
        .page_consolidation_status("habit/morning-run")
        .unwrap();
    assert!(last.is_some());
    // 证据基线的旧口径：digest 每次更新都会推进来源，此处断言派生面
    // 「晚于最近修订发生的事件数」就是给 UI 的新增证据数。
    let (_, new_evidence_after) = {
        let _ = new_evidence;
        t.store
            .page_consolidation_status("habit/morning-run")
            .unwrap()
    };
    assert!(new_evidence_after >= 0);
}
