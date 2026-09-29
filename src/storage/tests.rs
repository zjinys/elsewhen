//! storage 行为测试（从 mod.rs 内联 mod tests 整体外移，只测公共 API + 少量
//! pub(crate) 辅助）。私有辅助 temporary_database/entity_page/wiki_draft 一并在此。

use super::*;
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

fn temporary_database() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "elsewhen-storage-test-{}.db",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn persists_and_lists_raw_events() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store.insert_event(NewEvent::now("完成最小 MVP")).unwrap();
    let events = store.list_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].raw_text, "完成最小 MVP");
    let _ = std::fs::remove_file(path);
}

#[test]
fn analysis_job_stats_cover_every_queue_status() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.analysis_job_stats().unwrap(),
        AnalysisJobStats::default()
    );

    for (index, status) in ["pending", "running", "retry", "succeeded", "failed"]
        .into_iter()
        .enumerate()
    {
        let event_id = store
            .insert_event(NewEvent::now(&format!("queue status {index}")))
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE analysis_jobs SET status = ?1 WHERE event_id = ?2",
                params![status, event_id],
            )
            .unwrap();
    }

    assert_eq!(
        store.analysis_job_stats().unwrap(),
        AnalysisJobStats {
            pending: 1,
            running: 1,
            retry: 1,
            succeeded: 1,
            failed: 1,
        }
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn input_record_is_idempotent_and_links_routed_objects() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let first = store
        .create_input_record(" 今天完成了支付模块 ", "main_input", Some("request-1"))
        .unwrap();
    let repeated = store
        .create_input_record("不同文本也不能重复创建", "main_input", Some("request-1"))
        .unwrap();
    assert_eq!(first.id, repeated.id);
    assert_eq!(repeated.raw_text, "今天完成了支付模块");
    assert_eq!(repeated.route_status, "pending");

    let event_id = store.insert_event(NewEvent::now(&first.raw_text)).unwrap();
    let routed = store
        .update_input_route(&first.id, "routed", Some(&event_id), None, None, None)
        .unwrap();
    assert_eq!(routed.event_id.as_deref(), Some(event_id.as_str()));
    assert_eq!(routed.route_status, "routed");
    assert!(store
        .update_input_route(&first.id, "unknown", None, None, None, None)
        .is_err());

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn submit_input_as_event_is_atomic_and_idempotent() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let first = store
        .submit_input_as_event("今天完成统一输入", "main_input", Some("submit-1"))
        .unwrap();
    let repeated = store
        .submit_input_as_event("不会产生第二条", "main_input", Some("submit-1"))
        .unwrap();
    assert_eq!(first.id, repeated.id);
    assert_eq!(first.route_status, "routed");
    assert!(first.event_id.is_some());
    assert_eq!(store.list_events().unwrap().len(), 1);
    assert_eq!(store.analysis_job_stats().unwrap().pending, 1);

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn submit_conversation_input_links_one_message_and_one_event() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let conversation_id = store.create_conversation(None, None).unwrap();
    let first = store
        .submit_conversation_input(
            &conversation_id,
            "今天完成主循环接线",
            Some("conv-submit-1"),
        )
        .unwrap();
    let repeated = store
        .submit_conversation_input(&conversation_id, "重复", Some("conv-submit-1"))
        .unwrap();
    assert_eq!(first.id, repeated.id);
    assert!(first.event_id.is_some());
    assert!(first.message_id.is_some());
    assert_eq!(store.list_events().unwrap().len(), 1);
    assert_eq!(store.list_messages(&conversation_id).unwrap().len(), 1);
    assert_eq!(store.analysis_job_stats().unwrap().pending, 1);

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn idempotency_conflict_error_message_matches_column() {
    // P2-3 兜底分支依赖错误串识别幂等冲突；用裸 SQL 触发唯一冲突验证
    // SQLite 实际报「UNIQUE constraint failed: input_records.idempotency_key」
    // （纯列索引不报索引名），catch 分支的匹配串必须与此一致才有机会命中。
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    store
        .connection
        .execute(
            "INSERT INTO input_records
             (id,raw_text,source,route_status,idempotency_key,created_at,updated_at)
             VALUES (?1,'第一次','main_input','pending',?2,?3,?3)",
            params![Uuid::new_v4().to_string(), "dup-key", now],
        )
        .unwrap();
    let err = store
        .connection
        .execute(
            "INSERT INTO input_records
             (id,raw_text,source,route_status,idempotency_key,created_at,updated_at)
             VALUES (?1,'第二次','main_input','pending',?2,?3,?3)",
            params![Uuid::new_v4().to_string(), "dup-key", now],
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("input_records.idempotency_key"),
        "错误串应含幂等键列：{err}"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn single_active_config_conflict_error_message_matches_column() {
    // P2-2 兜底分支依赖识别「同一时刻两条 is_active=1」冲突；裸 SQL 触发
    // 部分唯一索引冲突，确认错误串按列报（同 index 的名字不出现）。
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    let insert = |name: &str| {
        store
            .connection
            .execute(
                "INSERT INTO ai_provider_configs
                 (id,name,provider_type,base_url,model,api_key_source,is_active,temperature,max_tokens,created_at,updated_at)
                 VALUES (?1,?2,'openai','http://x','gpt','env',1,0.7,NULL,?3,?3)",
                params![Uuid::new_v4().to_string(), name, now],
            )
            .map(|_| ())
    };
    insert("cfg-a").unwrap();
    let err = insert("cfg-b").unwrap_err();
    assert!(
        err.to_string().contains("ai_provider_configs.is_active"),
        "错误串应含激活标记列：{err}"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn daily_entries_unify_legacy_capture_and_conversation_events() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store.insert_event(NewEvent::now("历史事件")).unwrap();
    store
        .submit_input_as_event("Capture 事件", "capture", None)
        .unwrap();
    let conversation_id = store.create_conversation(None, None).unwrap();
    store
        .submit_conversation_input(&conversation_id, "对话事件", None)
        .unwrap();

    let entries = store
        .daily_entries(chrono::Local::now().date_naive())
        .unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.input_id.is_some())
            .count(),
        2
    );
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.message_id.is_some())
            .count(),
        1
    );
    assert!(entries.iter().any(|entry| entry.raw_text == "历史事件"));

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn daily_reviews_are_versioned_and_require_same_day_sources() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_id = store
        .insert_event(NewEvent::now("今天完成总结契约"))
        .unwrap();
    let today = chrono::Local::now().date_naive();

    assert!(store
        .save_daily_review(today, "daily-review-v1", "{}", &[])
        .is_err());
    assert!(store
        .save_daily_review(
            today - chrono::Duration::days(1),
            "daily-review-v1",
            "{}",
            std::slice::from_ref(&event_id),
        )
        .is_err());

    let first = store
        .save_daily_review(
            today,
            "daily-review-v1",
            r#"{"summary":"第一版"}"#,
            std::slice::from_ref(&event_id),
        )
        .unwrap();
    let second = store
        .save_daily_review(
            today,
            "daily-review-v2",
            r#"{"summary":"第二版"}"#,
            std::slice::from_ref(&event_id),
        )
        .unwrap();
    assert_ne!(first, second);

    let latest = store.latest_daily_review(today).unwrap().unwrap();
    assert_eq!(latest.id, second);
    assert_eq!(latest.prompt_version, "daily-review-v2");
    assert_eq!(latest.source_event_ids, vec![event_id]);
    assert_eq!(store.list_events().unwrap().len(), 1);

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn entity_facts_are_idempotent_and_raise_confidence() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_id = store
        .insert_event(NewEvent::now("项目进入测试阶段"))
        .unwrap();
    let first = store
        .upsert_entity_fact(
            "project",
            "elsewhen",
            "进入测试阶段",
            chrono::Utc::now().to_rfc3339().as_str(),
            2,
            &event_id,
        )
        .unwrap();
    let second = store
        .upsert_entity_fact(
            "project",
            "elsewhen",
            "进入测试阶段",
            &first.occurred_at,
            4,
            &event_id,
        )
        .unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.confidence, 4);
    assert_eq!(
        store
            .list_entity_facts("project", "elsewhen")
            .unwrap()
            .len(),
        1
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn deleting_entity_fact_keeps_source_event() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_id = store
        .insert_event(crate::event::NewEvent::now("原始事实"))
        .unwrap();
    let fact = store
        .upsert_entity_fact(
            "person",
            "person/张三",
            "负责项目",
            "2026-01-01T00:00:00Z",
            3,
            &event_id,
        )
        .unwrap();
    assert!(store.delete_entity_fact(&fact.id).unwrap());
    assert!(store
        .list_entity_facts("person", "person/张三")
        .unwrap()
        .is_empty());
    assert_eq!(store.list_events().unwrap().len(), 1);
    let _ = std::fs::remove_file(path);
}

#[test]
fn entity_aliases_are_idempotent_and_scoped() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .add_entity_alias("project", "project/acme", "ACME")
        .unwrap();
    store
        .add_entity_alias("project", "project/acme", "ACME")
        .unwrap();
    assert_eq!(
        store
            .list_entity_aliases("project", "project/acme")
            .unwrap(),
        vec!["ACME"]
    );
    assert!(store
        .list_entity_aliases("project", "project/other")
        .unwrap()
        .is_empty());
    let _ = std::fs::remove_file(path);
}

#[test]
fn events_on_date_uses_local_day_boundaries() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let today = chrono::Local::now().date_naive();
    let yesterday = today - chrono::Duration::days(1);
    let now = chrono::Utc::now();
    store
        .insert_event(NewEvent {
            raw_text: "今天的事件",
            occurred_at: now,
            recorded_at: now,
            source: "test",
        })
        .unwrap();
    store
        .insert_event(NewEvent {
            raw_text: "昨天的事件",
            occurred_at: now - chrono::Duration::days(1),
            recorded_at: now - chrono::Duration::days(1),
            source: "test",
        })
        .unwrap();
    let today_events = store.events_on_date(today).unwrap();
    assert_eq!(today_events.len(), 1);
    assert_eq!(today_events[0].raw_text, "今天的事件");
    let yesterday_events = store.events_on_date(yesterday).unwrap();
    assert_eq!(yesterday_events.len(), 1);
    assert_eq!(yesterday_events[0].raw_text, "昨天的事件");
    assert!(store
        .events_on_date(today - chrono::Duration::days(10))
        .unwrap()
        .is_empty());
    let _ = std::fs::remove_file(path);
}

#[test]
fn raw_text_cannot_be_changed() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let id = store.insert_event(NewEvent::now("原始事实")).unwrap();
    let result = store
        .connection
        .execute("UPDATE events SET raw_text = '被覆盖' WHERE id = ?1", [id]);
    assert!(result.is_err());
    let _ = std::fs::remove_file(path);
}

#[test]
fn event_creation_enqueues_one_analysis_job() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_id = store.insert_event(NewEvent::now("等待后台理解")).unwrap();
    let count: i64 = store
        .connection
        .query_row(
            "SELECT count(*) FROM analysis_jobs WHERE event_id = ?1 AND status = 'pending'",
            [event_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    let _ = std::fs::remove_file(path);
}

#[test]
fn failed_analysis_keeps_raw_event_and_schedules_retry() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .insert_event(NewEvent::now("AI 失败也不能丢"))
        .unwrap();
    let job = store.claim_analysis_job().unwrap().unwrap();
    store.fail_analysis(&job, "provider unavailable").unwrap();
    let status: String = store
        .connection
        .query_row(
            "SELECT status FROM analysis_jobs WHERE id = ?1",
            [&job.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "retry");
    assert_eq!(store.list_events().unwrap()[0].raw_text, "AI 失败也不能丢");
    let _ = std::fs::remove_file(path);
}

#[test]
fn recover_interrupted_analysis_jobs_requeues_running_work() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store.insert_event(NewEvent::now("恢复中的任务")).unwrap();
    let job = store.claim_analysis_job().unwrap().unwrap();
    assert_eq!(store.analysis_job_stats().unwrap().running, 1);
    assert_eq!(store.recover_interrupted_analysis_jobs().unwrap(), 1);
    assert_eq!(store.analysis_job_stats().unwrap().retry, 1);
    assert!(store.claim_analysis_job().unwrap().is_some());
    let _ = std::fs::remove_file(path);
    drop(job);
}

#[test]
fn event_analysis_detail_exposes_queue_result_and_error() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_id = store.insert_event(NewEvent::now("查看分析详情")).unwrap();
    let pending = store.event_analysis_detail(&event_id).unwrap().unwrap();
    assert_eq!(pending.job_status, "pending");
    assert_eq!(pending.attempts, 0);
    assert!(pending.result_json.is_none());

    let job = store.claim_analysis_job().unwrap().unwrap();
    store.fail_analysis(&job, "invalid schema").unwrap();
    let retry = store.event_analysis_detail(&event_id).unwrap().unwrap();
    assert_eq!(retry.job_status, "retry");
    assert_eq!(retry.last_error.as_deref(), Some("invalid schema"));

    store
        .complete_analysis(
            &job,
            "event-analysis-v1",
            r#"{"schema_version":"event-analysis-v1"}"#,
        )
        .unwrap();
    let succeeded = store.event_analysis_detail(&event_id).unwrap().unwrap();
    assert_eq!(succeeded.job_status, "succeeded");
    assert_eq!(
        succeeded.prompt_version.as_deref(),
        Some("event-analysis-v1")
    );
    assert!(succeeded.result_json.is_some());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn manual_recordability_is_append_only_and_preserves_raw_event_and_message() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let conversation_id = store.create_conversation(None, None).unwrap();
    let input = store
        .submit_conversation_input(
            &conversation_id,
            "你这个回答情绪价值不够",
            Some("recordability-1"),
        )
        .unwrap();
    let event_id = input.event_id.clone().unwrap();
    let message_id = input.message_id.clone().unwrap();
    store
        .upsert_entity_fact(
            "topic",
            "topic/回答",
            "评价：不满意",
            "2026-09-21",
            2,
            &event_id,
        )
        .unwrap();
    store
        .create_pending_action(
            &conversation_id,
            "propose_people_relations",
            &format!(r#"{{"source_event_id":"{event_id}"}}"#),
        )
        .unwrap();

    let first = store
        .set_event_recordability(&event_id, false, "manual-ui")
        .unwrap();
    assert!(!first.recordable);
    assert_eq!(first.kind, "discussion");
    assert!(store
        .list_entity_facts("topic", "topic/回答")
        .unwrap()
        .is_empty());
    assert!(store
        .pending_actions_for_conversation(&conversation_id)
        .unwrap()
        .is_empty());
    assert_eq!(
        store
            .get_conversation(&conversation_id)
            .unwrap()
            .unwrap()
            .tag
            .as_deref(),
        Some("discussion")
    );
    assert_eq!(
        store.list_events().unwrap()[0].raw_text,
        "你这个回答情绪价值不够"
    );
    assert_eq!(
        store.list_messages(&conversation_id).unwrap()[0].id,
        message_id
    );

    let second = store
        .set_event_recordability(&event_id, true, "manual-ui")
        .unwrap();
    assert!(second.recordable);
    let decision_count: i64 = store
        .connection
        .query_row(
            "SELECT count(*) FROM event_recordability_decisions WHERE event_id=?1",
            [&event_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(decision_count, 2);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn reanalysis_requeues_without_deleting_previous_analysis() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_id = store.insert_event(NewEvent::now("存量重新分析")).unwrap();
    let job = store.claim_analysis_job().unwrap().unwrap();
    store.complete_analysis(&job, "event-analysis", r#"{"schema_version":"event-analysis","recordable":true,"kind":"event","event_type":"note","confidence":0.8,"summary":"旧结果","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[]}"#).unwrap();
    assert!(store.requeue_event_analysis(&event_id).unwrap());
    let detail = store.event_analysis_detail(&event_id).unwrap().unwrap();
    assert_eq!(detail.job_status, "pending");
    assert!(detail.result_json.unwrap().contains("旧结果"));
    assert_eq!(
        store
            .connection
            .query_row(
                "SELECT count(*) FROM event_analyses WHERE event_id=?1",
                [&event_id],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn provider_config_is_fully_persisted() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .upsert_ai_provider_config("https://example.test/v1", "test-model", "secret-value")
        .unwrap();
    let provider = store.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(provider.base_url, "https://example.test/v1");
    assert_eq!(provider.model, "test-model");
    assert_eq!(provider.api_key_source, "database");
    assert_eq!(provider.api_key, "secret-value");
    let _ = std::fs::remove_file(path);
}

#[test]
fn concurrent_connections_can_insert_events() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let handles = (0..8)
        .map(|index| {
            let connection = store.clone();
            std::thread::spawn(move || {
                connection
                    .insert_event(NewEvent::now(&format!("并发事件 {index}")))
                    .unwrap();
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(store.list_events().unwrap().len(), 8);
    let _ = std::fs::remove_file(path);
}

#[test]
fn update_todo_edits_fields_and_clears_optionals() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let todo = store
        .create_todo(
            "跟进付款",
            "normal",
            Some("2026-09-20"),
            None,
            None,
            Some("原始说明"),
        )
        .unwrap();

    // 编辑：标题/说明/优先级/截止都改
    store
        .update_todo(
            &todo.id,
            "跟进双链路付款",
            Some("已和张玮对齐时间"),
            Some("high"),
            Some("2026-09-18"),
        )
        .unwrap();
    let updated = store.list_todos(None).unwrap();
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0].title, "跟进双链路付款");
    assert_eq!(updated[0].note.as_deref(), Some("已和张玮对齐时间"));
    assert_eq!(updated[0].priority, "high");
    assert_eq!(updated[0].due_at.as_deref(), Some("2026-09-18"));
    assert_eq!(updated[0].status, TodoStatus::Open, "编辑不改状态");

    // 清除可选字段：传 None
    store
        .update_todo(&todo.id, "跟进双链路付款", None, None, None)
        .unwrap();
    let cleared = store.list_todos(None).unwrap();
    assert!(cleared[0].note.is_none());
    assert!(cleared[0].due_at.is_none());
    assert_eq!(cleared[0].priority, "normal");

    // 空标题拒绝
    assert!(store
        .update_todo(&todo.id, "   ", None, None, None)
        .is_err());
    let _ = std::fs::remove_file(path);
}

#[test]
fn work_item_link_is_lazy_idempotent_and_keeps_completed_history() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let todo = store
        .create_todo(
            "讨论发布策略",
            "high",
            Some("2026-09-30"),
            None,
            None,
            Some("先收集约束"),
        )
        .unwrap();

    // 创建待办不会隐式制造知识页；页面升级后关联可回溯。
    assert!(todo.related_wiki_slug.is_none());
    store
        .set_todo_related_wiki_slug(&todo.id, "topic/discuss-release")
        .unwrap();
    let linked = store.list_todos(None).unwrap();
    assert_eq!(
        linked[0].related_wiki_slug.as_deref(),
        Some("topic/discuss-release")
    );

    // 重复升级只覆盖同一关联，不产生第二条待办或改变其字段。
    store
        .set_todo_related_wiki_slug(&todo.id, "topic/discuss-release")
        .unwrap();
    assert_eq!(store.list_todos(None).unwrap().len(), 1);
    store
        .update_todo_status(&todo.id, TodoStatus::Done)
        .unwrap();
    let completed = store.list_todos(None).unwrap();
    assert_eq!(completed[0].status, TodoStatus::Done);
    assert_eq!(
        completed[0].related_wiki_slug.as_deref(),
        Some("topic/discuss-release")
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn get_todo_includes_archived_history_for_on_demand_migration() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let todo = store
        .create_todo("归档后仍可讨论", "normal", None, None, None, None)
        .unwrap();
    store
        .update_todo_status(&todo.id, TodoStatus::Archived)
        .unwrap();
    let found = store.get_todo(&todo.id).unwrap().unwrap();
    assert_eq!(found.status, TodoStatus::Archived);
    assert!(store.list_todos(None).unwrap().is_empty());
    let _ = std::fs::remove_file(path);
}

#[test]
fn find_wiki_page_by_tag_exact_matches_json_array_member() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    // tag 精确落在 JSON 数组的一个成员里（P2-5 直查替代全表扫描）
    store
        .upsert_wiki_page(
            &wiki_draft("topic/target", "topic", "命中页"),
            ContentPolicy::Always,
        )
        .unwrap();
    let target = store.get_wiki_page("topic/target").unwrap().unwrap();
    store
        .update_wiki_tags(&target.slug, &["work-item-id:abc".to_string()])
        .unwrap();
    let found = store
        .find_wiki_page_by_tag("work-item-id:abc")
        .unwrap()
        .unwrap();
    assert_eq!(found.slug, "topic/target");

    // 不存在的 tag 返回 None
    assert!(store
        .find_wiki_page_by_tag("work-item-id:nope")
        .unwrap()
        .is_none());

    let _ = std::fs::remove_file(path);
}

#[test]
fn relations_upsert_dedupe_and_list_both_directions() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let rel = RelationDraft {
        from_slug: "person/张三".to_string(),
        from_kind: "person".to_string(),
        to_slug: "kb-双链路付款".to_string(),
        to_kind: "project".to_string(),
        relation: "负责".to_string(),
        note: Some("主导该项目".to_string()),
        confidence: 3,
        source_conversation_id: Some("conv-1".to_string()),
        source_event_id: None,
    };
    store.upsert_relation(&rel).unwrap();
    // 同一条重复写入：去重为 1 条，指向同一页双向都能查到
    store.upsert_relation(&rel).unwrap();
    let from_person = store.list_relations_for_page("person/张三").unwrap();
    let from_project = store.list_relations_for_page("kb-双链路付款").unwrap();
    assert_eq!(from_person.len(), 1);
    assert_eq!(from_project.len(), 1);
    assert_eq!(from_person[0].relation, "负责");
    assert_eq!(from_person[0].to_slug, "kb-双链路付款");
    assert_eq!(store.list_relations().unwrap().len(), 1);

    // 删除
    assert!(store.delete_relation(&from_person[0].id).unwrap());
    assert!(store.list_relations().unwrap().is_empty());
    let _ = std::fs::remove_file(path);
}

#[test]
fn rename_wiki_page_moves_slug_relations_and_chat() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let draft = WikiPageDraft {
        slug: "topic/付款流程".to_string(),
        kind: "topic".to_string(),
        title: "付款流程".to_string(),
        summary: "付款流程（由人物关系确认时自动建档）".to_string(),
        content_md: "# 付款流程\n\n跟进付款相关事宜。".to_string(),
        tags: vec!["付款".to_string()],
        source_event_ids: vec![],
        status: "active".to_string(),
        reason: "test".to_string(),
        source_url: None,
    };
    store
        .upsert_wiki_page(&draft, ContentPolicy::Always)
        .unwrap();
    store
        .upsert_relation(&RelationDraft {
            from_slug: "person/谭俊".to_string(),
            from_kind: "person".to_string(),
            to_slug: "topic/付款流程".to_string(),
            to_kind: "topic".to_string(),
            relation: "跟进".to_string(),
            note: Some("处理付款事宜".to_string()),
            confidence: 3,
            source_conversation_id: Some("conv-1".to_string()),
            source_event_id: None,
        })
        .unwrap();
    store
        .create_wiki_chat_conversation("topic/付款流程", "[知识页] 付款流程")
        .unwrap();
    let before_revisions = store.list_wiki_revisions("topic/付款流程").unwrap().len();

    // 改名：标题 + slug 一起换，关系与会话迁移
    let outcome = store
        .rename_wiki_page("topic/付款流程", "fpso111 尾款", "项目真名更正")
        .unwrap();
    assert!(outcome.changed);
    assert_eq!(outcome.new_slug, "topic/fpso111-尾款");
    assert_eq!(outcome.relations_moved, 1);
    assert_eq!(outcome.chats_moved, 1);
    assert!(store.get_wiki_page("topic/付款流程").unwrap().is_none());
    let renamed = store.get_wiki_page("topic/fpso111-尾款").unwrap().unwrap();
    assert_eq!(renamed.title, "fpso111 尾款");
    assert!(
        renamed.summary.contains("fpso111 尾款"),
        "summary 里的旧名应被替换: {}",
        renamed.summary
    );
    // 关系引用已指向新 slug
    let rels = store.list_relations().unwrap();
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0].to_slug, "topic/fpso111-尾款");
    // 页内聊天会话已迁移
    assert!(store
        .find_wiki_chat_conversation("topic/fpso111-尾款")
        .unwrap()
        .is_some());
    assert!(store
        .find_wiki_chat_conversation("topic/付款流程")
        .unwrap()
        .is_none());
    // 修订历史多了一条重命名记录
    assert_eq!(
        store
            .list_wiki_revisions("topic/fpso111-尾款")
            .unwrap()
            .len(),
        before_revisions + 1
    );

    // 标题没变：changed=false，零写操作
    let outcome = store
        .rename_wiki_page("topic/fpso111-尾款", "fpso111 尾款", "x")
        .unwrap();
    assert!(!outcome.changed);

    // 页面不存在：报错并提示可能已改名
    let err = store
        .rename_wiki_page("topic/付款流程", "x", "y")
        .unwrap_err();
    assert!(err.to_string().contains("知识页不存在"), "{err}");

    // 无前缀页（如来源页）：只改标题，slug 不动
    let src_draft = WikiPageDraft {
        slug: "tweet-123".to_string(),
        kind: "source".to_string(),
        title: "旧标题".to_string(),
        summary: "s".to_string(),
        content_md: "c".to_string(),
        tags: vec![],
        source_event_ids: vec![],
        status: "active".to_string(),
        reason: "test".to_string(),
        source_url: None,
    };
    store
        .upsert_wiki_page(&src_draft, ContentPolicy::Always)
        .unwrap();
    let outcome = store
        .rename_wiki_page("tweet-123", "新标题", "更正")
        .unwrap();
    assert!(outcome.changed);
    assert_eq!(outcome.new_slug, "tweet-123", "来源页 slug 应保持不变");
    assert_eq!(
        store.get_wiki_page("tweet-123").unwrap().unwrap().title,
        "新标题"
    );
    let _ = std::fs::remove_file(path);
}

fn entity_page(store: &Store, slug: &str, kind: &str, title: &str) {
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

#[test]
fn entity_merge_and_undo_preserve_sources_and_restore_rows() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    entity_page(&store, "person/张伟-a", "person", "张伟（设计）");
    entity_page(&store, "person/张伟", "person", "张伟");
    entity_page(&store, "project/付款", "project", "付款项目");
    let event_id = store
        .insert_event(NewEvent::now("张伟负责付款联调"))
        .unwrap();
    let fact = store
        .upsert_entity_fact(
            "person",
            "person/张伟-a",
            "职责：付款联调",
            "2026-09-21",
            4,
            &event_id,
        )
        .unwrap();
    store
        .add_entity_alias("person", "person/张伟-a", "设计张伟")
        .unwrap();
    let relation = store
        .upsert_relation(&RelationDraft {
            from_slug: "person/张伟-a".into(),
            from_kind: "person".into(),
            to_slug: "project/付款".into(),
            to_kind: "project".into(),
            relation: "负责".into(),
            note: None,
            confidence: 4,
            source_conversation_id: None,
            source_event_id: Some(event_id.clone()),
        })
        .unwrap();
    let todo = store
        .create_todo(
            "确认付款联调",
            "high",
            None,
            Some(&event_id),
            Some("person/张伟-a"),
            None,
        )
        .unwrap();

    assert!(store
        .merge_entity("person", "person/张伟-a", "person/张伟")
        .unwrap());
    assert_eq!(
        store
            .get_wiki_page("person/张伟-a")
            .unwrap()
            .unwrap()
            .status,
        "merged"
    );
    assert_eq!(
        store.list_entity_facts("person", "person/张伟").unwrap()[0].id,
        fact.id
    );
    assert_eq!(
        store.list_relations_for_page("person/张伟").unwrap()[0].id,
        relation.id
    );
    assert_eq!(store.list_events().unwrap().len(), 1);
    assert_eq!(
        store.list_todos(None).unwrap()[0]
            .related_wiki_slug
            .as_deref(),
        Some("person/张伟")
    );

    assert!(store.undo_entity_merge("person/张伟-a").unwrap());
    assert_eq!(
        store
            .get_wiki_page("person/张伟-a")
            .unwrap()
            .unwrap()
            .status,
        "active"
    );
    assert_eq!(
        store.list_entity_facts("person", "person/张伟-a").unwrap()[0].id,
        fact.id
    );
    assert_eq!(
        store
            .list_entity_aliases("person", "person/张伟-a")
            .unwrap(),
        vec!["设计张伟"]
    );
    assert_eq!(
        store.list_relations_for_page("person/张伟-a").unwrap()[0].id,
        relation.id
    );
    assert_eq!(store.list_events().unwrap()[0].raw_text, "张伟负责付款联调");
    let restored_todo = store
        .list_todos(None)
        .unwrap()
        .into_iter()
        .find(|item| item.id == todo.id)
        .unwrap();
    assert_eq!(
        restored_todo.related_wiki_slug.as_deref(),
        Some("person/张伟-a")
    );
    assert!(store
        .merge_entity("person", "person/张伟-a", "person/张伟")
        .unwrap());
    assert!(store.undo_entity_merge("person/张伟-a").unwrap());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn entity_merge_deduplicates_and_undo_restores_source_copies() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    entity_page(&store, "topic/旧", "topic", "旧主题");
    entity_page(&store, "topic/新", "topic", "新主题");
    let event_id = store.insert_event(NewEvent::now("共同事实")).unwrap();
    store
        .upsert_entity_fact(
            "topic",
            "topic/旧",
            "状态：进行中",
            "2026-09-21",
            3,
            &event_id,
        )
        .unwrap();
    store
        .upsert_entity_fact(
            "topic",
            "topic/新",
            "状态：进行中",
            "2026-09-21",
            5,
            &event_id,
        )
        .unwrap();
    store
        .add_entity_alias("topic", "topic/旧", "共同别名")
        .unwrap();
    store
        .add_entity_alias("topic", "topic/新", "共同别名")
        .unwrap();
    store.merge_entity("topic", "topic/旧", "topic/新").unwrap();
    assert_eq!(
        store.list_entity_facts("topic", "topic/新").unwrap().len(),
        1
    );
    assert_eq!(
        store
            .list_entity_aliases("topic", "topic/新")
            .unwrap()
            .len(),
        1
    );
    store.undo_entity_merge("topic/旧").unwrap();
    assert_eq!(
        store.list_entity_facts("topic", "topic/旧").unwrap().len(),
        1
    );
    assert_eq!(
        store.list_entity_facts("topic", "topic/新").unwrap().len(),
        1
    );
    assert_eq!(
        store.list_entity_aliases("topic", "topic/旧").unwrap(),
        vec!["共同别名"]
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn entity_merge_rejects_invalid_targets_and_undo_is_atomic_after_changes() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    entity_page(&store, "person/a", "person", "A");
    entity_page(&store, "person/b", "person", "B");
    entity_page(&store, "project/b", "project", "B项目");
    assert!(store.merge_entity("person", "person/a", "missing").is_err());
    assert!(store
        .merge_entity("person", "person/a", "project/b")
        .is_err());
    let event_id = store.insert_event(NewEvent::now("A事实")).unwrap();
    let fact = store
        .upsert_entity_fact(
            "person",
            "person/a",
            "团队：支付",
            "2026-09-21",
            3,
            &event_id,
        )
        .unwrap();
    store
        .merge_entity("person", "person/a", "person/b")
        .unwrap();
    store
        .connection
        .execute(
            "UPDATE entity_facts SET confidence=5 WHERE id=?1",
            [&fact.id],
        )
        .unwrap();
    assert!(store.undo_entity_merge("person/a").is_err());
    assert_eq!(
        store.get_wiki_page("person/a").unwrap().unwrap().status,
        "merged"
    );
    assert!(store
        .list_entity_facts("person", "person/a")
        .unwrap()
        .is_empty());
    assert_eq!(
        store.list_entity_facts("person", "person/b").unwrap()[0].confidence,
        5
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

/// `entity_merges` 对 `(entity_kind, source_slug)` **不设** UNIQUE。
///
/// 早期结构带这个唯一约束，导致同一个源实体第二次合并时插入直接失败。
/// 约束是历史迁移拆表去掉的，现在固化在 `migrations/01-baseline/up.sql` 里，所以这个测试盯住
/// 「约束没被加回来」+「审计快照仍在」两件事。
#[test]
fn entity_merges_allows_repeated_merge_of_same_source_and_keeps_audit() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let schema: String = store
        .connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='entity_merges'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!schema
        .replace(' ', "")
        .contains("UNIQUE(entity_kind,source_slug)"));

    // 同一源实体合并到两个不同目标，都应成功
    store
        .connection
        .execute_batch(
            "INSERT INTO entity_merges (id,entity_kind,source_slug,target_slug,created_at)
               VALUES ('m1','person','person/a','person/b','2026-09-21');
             INSERT INTO entity_merges (id,entity_kind,source_slug,target_slug,created_at)
               VALUES ('m2','person','person/a','person/c','2026-09-22');
             INSERT INTO entity_merge_snapshots (merge_id,table_name,row_id,payload)
               VALUES ('m1','entity_aliases','a1','{}');
             INSERT INTO entity_merge_snapshots (merge_id,table_name,row_id,payload)
               VALUES ('m2','entity_aliases','a1','{}');",
        )
        .unwrap();

    for (table, expected) in [("entity_merges", 2), ("entity_merge_snapshots", 2)] {
        let n: i64 = store
            .connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, expected, "{table} 行数不对");
    }
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn find_wiki_page_by_title_matches_exact_ignoring_case_and_trim() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "person/张伟".to_string(),
                kind: "person".to_string(),
                title: "张伟".to_string(),
                summary: "简介".to_string(),
                content_md: "内容".to_string(),
                tags: vec![],
                source_event_ids: vec![],
                status: "active".to_string(),
                reason: "test".to_string(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    assert!(store.find_wiki_page_by_title(" 张伟 ").unwrap().is_some());
    assert!(store
        .find_wiki_page_by_title("不存在的人")
        .unwrap()
        .is_none());
    let _ = std::fs::remove_file(path);
}

#[test]
fn list_conversations_excludes_wiki_page_chats() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let normal = store.create_conversation(Some("普通对话"), None).unwrap();
    let wiki_chat = store
        .create_wiki_chat_conversation("person/张伟", "处理本页")
        .unwrap();
    // 知识页聊天有 wiki_page_slug 关联
    let conv = store.get_conversation(&wiki_chat).unwrap().unwrap();
    assert_eq!(conv.wiki_page_slug.as_deref(), Some("person/张伟"));

    let listed = store.list_conversations().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, normal);
    assert!(listed.iter().all(|c| c.wiki_page_slug.is_none()));

    // 归档列表同样不出现知识页聊天
    store.set_conversation_archived(&normal, true).unwrap();
    let archived = store.list_archived_conversations().unwrap();
    assert!(archived.iter().all(|c| c.wiki_page_slug.is_none()));
    assert_eq!(archived.len(), 1);
    let _ = std::fs::remove_file(path);
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

/// 粘贴文本导入的笔记页：`note-` 前缀 + `kind='note'` + `area='imported'`。
///
/// 旧结构靠 v29 迁移回填存量页的 kind；现在没有版本链了，这个不变量改由
/// `save_text_page` 的写入路径直接保证，所以测试也跟着移到写入侧。
#[test]
fn imported_note_page_gets_note_prefix_kind_and_imported_area() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let page = crate::wiki::save_text_page(
        "第一行标题\n\n正文内容",
        None,
        &["随手".to_string()],
        &store,
    )
    .unwrap();

    assert!(
        page.slug.starts_with("note-"),
        "笔记页 slug 应带 note- 前缀，实际 {}",
        page.slug
    );
    assert_eq!(page.kind, "note", "笔记页 kind 应为 note");
    assert_eq!(page.area, "imported", "note- 前缀应归入 imported 分区");
    // 人工编辑时间与观点列存在且初始为空（v29 引入的两列）
    assert_eq!(page.human_edited_at, None);
    assert_eq!(page.opinion, None);
    assert!(page.tags.contains(&"note".to_string()), "note 锚点标签");

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn save_wiki_page_content_sets_human_edited_at_and_rejects_material() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .upsert_wiki_page(
            &wiki_draft("person/张三", "person", "AI 原始内容"),
            ContentPolicy::Always,
        )
        .unwrap();

    // 正常路径：保存正文 → human_edited_at 置位 + revision 原因带 [human]
    let saved = store
        .save_wiki_page_content("person/张三", "人类修改后的正文", "修正职位", None)
        .unwrap();
    assert!(saved.human_edited_at.is_some());
    assert_eq!(saved.content_md, "人类修改后的正文");
    let (_, revised_content, reason) = store
        .list_wiki_revisions("person/张三")
        .unwrap()
        .first()
        .cloned()
        .unwrap();
    assert_eq!(revised_content, "人类修改后的正文");
    assert!(
        reason.starts_with("[human]"),
        "reason 应带 [human] 前缀: {reason}"
    );
    let log = store.list_wiki_log(5).unwrap();
    assert!(log.iter().any(|(_, e)| e.contains("人工编辑正文")));

    // 空正文拒绝
    assert!(store
        .save_wiki_page_content("person/张三", "   ", "清空", None)
        .is_err());

    // 素材页（source/note）只读拒绝
    store
        .upsert_wiki_page(
            &wiki_draft("tweet-1", "source", "素材内容"),
            ContentPolicy::Always,
        )
        .unwrap();
    assert!(store
        .save_wiki_page_content("tweet-1", "改素材", "不该允许", None)
        .is_err());
    store
        .upsert_wiki_page(
            &wiki_draft("note-x", "note", "笔记内容"),
            ContentPolicy::Always,
        )
        .unwrap();
    assert!(store
        .save_wiki_page_content("note-x", "改笔记", "不该允许", None)
        .is_err());

    // 超过 64k 字符拒绝
    let big = "长".repeat(65537);
    assert!(store
        .save_wiki_page_content("person/张三", &big, "超大", None)
        .is_err());

    let _ = std::fs::remove_file(path);
}

#[test]
fn save_wiki_page_content_optimistic_lock_rejects_stale_write() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .upsert_wiki_page(
            &wiki_draft("person/王五", "person", "AI 原始内容"),
            ContentPolicy::Always,
        )
        .unwrap();

    // 加载时刻的快照 updated_at
    let loaded_at = store
        .get_wiki_page("person/王五")
        .unwrap()
        .unwrap()
        .updated_at;

    // 模拟编辑期间后台 digest 写回：updated_at 变化（直接改库绕过守卫）
    store
        .connection
        .execute(
            "UPDATE wiki_pages SET content_md='digest 新内容', updated_at='2099-01-01T00:00:00Z' WHERE slug='person/王五'",
            [],
        )
        .unwrap();

    // 持旧快照保存 → 冲突拒绝，且不落库
    let err = store
        .save_wiki_page_content("person/王五", "人工修改", "修正", Some(&loaded_at))
        .unwrap_err();
    assert!(err.to_string().contains("编辑冲突"), "应报编辑冲突: {err}");
    assert_eq!(
        store
            .get_wiki_page("person/王五")
            .unwrap()
            .unwrap()
            .content_md,
        "digest 新内容",
        "冲突时不得覆盖后台写入"
    );

    // 持当前快照保存 → 正常通过
    let current_at = store
        .get_wiki_page("person/王五")
        .unwrap()
        .unwrap()
        .updated_at;
    store
        .save_wiki_page_content("person/王五", "人工修改", "修正", Some(&current_at))
        .unwrap();
    assert_eq!(
        store
            .get_wiki_page("person/王五")
            .unwrap()
            .unwrap()
            .content_md,
        "人工修改"
    );

    // 模拟 Dart 往返格式漂移（毫秒精度 + "Z" 后缀）：同一时刻应判定一致。
    // 注意：上面保存成功后 updated_at 已变，需重读当前值再做格式变换。
    let fresh_at = store
        .get_wiki_page("person/王五")
        .unwrap()
        .unwrap()
        .updated_at;
    let reformatted = format!(
        "{}Z",
        chrono::DateTime::parse_from_rfc3339(&fresh_at)
            .unwrap()
            .to_utc()
            .format("%Y-%m-%dT%H:%M:%S%.3f")
    );
    store
        .save_wiki_page_content(
            "person/王五",
            "格式漂移仍应通过",
            "修正",
            Some(&reformatted),
        )
        .unwrap();

    // 不传 expected（None）→ 跳过校验（兼容旧调用方）
    store
        .save_wiki_page_content("person/王五", "无锁保存", "修正", None)
        .unwrap();

    let _ = std::fs::remove_file(path);
}

#[test]
fn digest_preserve_respects_human_edited_and_material_pages() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_one = store.insert_event(NewEvent::now("与李四讨论方案")).unwrap();
    let event_two = store.insert_event(NewEvent::now("李四确认实施步骤")).unwrap();

    // ① 人工编辑过的档案页：PreserveHumanEdits → 正文不动、证据照累、protected=true
    store
        .upsert_wiki_page(
            &wiki_draft("person/李四", "person", "AI 初稿"),
            ContentPolicy::Always,
        )
        .unwrap();
    store
        .save_wiki_page_content("person/李四", "人工定稿", "人工修正", None)
        .unwrap();
    let outcome = store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "person/李四".to_string(),
                kind: "person".to_string(),
                title: "李四（AI 想改名）".to_string(),
                summary: "AI 摘要".to_string(),
                content_md: "AI 想覆盖的新内容".to_string(),
                tags: vec!["ai".to_string()],
                source_event_ids: vec![event_one.clone(), event_two.clone()],
                status: "active".to_string(),
                reason: "digest".to_string(),
                source_url: None,
            },
            ContentPolicy::PreserveHumanEdits,
        )
        .unwrap();
    assert!(outcome.protected, "人工编辑页应被保护");
    let page = outcome.page;
    assert_eq!(page.content_md, "人工定稿", "正文不可被 digest 覆盖");
    assert_eq!(page.title, "person/李四", "标题不可被 digest 覆盖");
    assert_eq!(page.tags, Vec::<String>::new(), "tags 不可被 digest 覆盖");
    assert_eq!(
        page.source_event_ids,
        vec![event_one, event_two],
        "但证据应并集"
    );
    assert_eq!(page.evidence_count, 2);

    // ② 未人工编辑的档案页：PreserveHumanEdits → 正常整篇覆盖
    store
        .upsert_wiki_page(
            &wiki_draft("topic/新主题", "topic", "AI 第一版"),
            ContentPolicy::Always,
        )
        .unwrap();
    let outcome2 = store
        .upsert_wiki_page(
            &wiki_draft("topic/新主题", "topic", "AI 第二版"),
            ContentPolicy::PreserveHumanEdits,
        )
        .unwrap();
    assert!(!outcome2.protected);
    assert_eq!(outcome2.page.content_md, "AI 第二版");

    // ③ 素材页：PreserveHumanEdits → 永不覆盖（采集快照只读）
    store
        .upsert_wiki_page(
            &wiki_draft("tweet-9", "source", "原始素材"),
            ContentPolicy::Always,
        )
        .unwrap();
    let outcome3 = store
        .upsert_wiki_page(
            &wiki_draft("tweet-9", "source", "新素材内容"),
            ContentPolicy::PreserveHumanEdits,
        )
        .unwrap();
    assert!(outcome3.protected);
    assert_eq!(outcome3.page.content_md, "原始素材", "素材正文对 AI 只读");

    // ④ Always 策略 = 素材导入流程：允许刷新素材内容（仅所有者可写）
    let outcome4 = store
        .upsert_wiki_page(
            &wiki_draft("tweet-9", "source", "导入流程刷新"),
            ContentPolicy::Always,
        )
        .unwrap();
    assert!(!outcome4.protected);
    assert_eq!(outcome4.page.content_md, "导入流程刷新");

    // ⑤ Always 策略 = 确认制修订（save_wiki_revision：AI 草拟 → 用户确认后才落库）：
    //    人工编辑页也可被覆盖——这是文档承诺的修订通道，区别于 digest 的静默覆盖
    let outcome5 = store
        .upsert_wiki_page(
            &wiki_draft("person/李四", "person", "用户确认后的修订版本"),
            ContentPolicy::Always,
        )
        .unwrap();
    assert!(!outcome5.protected);
    assert_eq!(outcome5.page.content_md, "用户确认后的修订版本");
    assert!(
        outcome5.page.human_edited_at.is_some(),
        "确认制修订不触碰 human_edited_at 列：曾被人动过的事实开关永久保留"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn set_wiki_opinion_only_on_material_and_audits() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .upsert_wiki_page(
            &wiki_draft("tweet-8", "source", "素材"),
            ContentPolicy::Always,
        )
        .unwrap();

    // 认可
    let p = store.set_wiki_opinion("tweet-8", Some("endorse")).unwrap();
    assert_eq!(p.opinion.as_deref(), Some("endorse"));

    // 不认可
    let p = store.set_wiki_opinion("tweet-8", Some("reject")).unwrap();
    assert_eq!(p.opinion.as_deref(), Some("reject"));

    // 清空（未表态）
    let p = store.set_wiki_opinion("tweet-8", None).unwrap();
    assert_eq!(p.opinion, None);

    // 非法值拒绝
    assert!(store.set_wiki_opinion("tweet-8", Some("meh")).is_err());

    // 非素材页拒绝（即使人工编辑过也一样）
    store
        .upsert_wiki_page(
            &wiki_draft("person/王五", "person", "x"),
            ContentPolicy::Always,
        )
        .unwrap();
    assert!(store
        .set_wiki_opinion("person/王五", Some("endorse"))
        .is_err());

    // 审计日志
    let log = store.list_wiki_log(10).unwrap();
    assert!(log.iter().any(|(_, e)| e.contains("素材评价")));

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_project_path_validates_and_backfills_legacy_pages() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    // 存量页：只有正文快照、没有 source_url
    let dir_a = std::env::temp_dir().join("elsewhen-proj-a");
    let dir_b = std::env::temp_dir().join("elsewhen-proj-b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();
    let content = format!(
        "# 资产档案：A\n\n## 资产边界\n- 项目目录：`{}`\n",
        dir_a.display()
    );
    store
        .upsert_wiki_page(
            &wiki_draft("project/a", "project", &content),
            ContentPolicy::Always,
        )
        .unwrap();
    // 新 open 触发幂等回填
    drop(store);
    let store = Store::open(&path).unwrap();
    let page = store.get_wiki_page("project/a").unwrap().unwrap();
    let url = page.source_url.clone().unwrap();
    assert!(url.starts_with("file://"));
    // 回填按正文原样记录（不做 canonicalize，目录可能已搬走）
    assert_eq!(crate::wiki::file_url_to_path(&url).unwrap(), dir_a);
    // 二次 open 不再重复写 updated_at（幂等无副作用）
    let updated_before = page.updated_at.clone();
    drop(store);
    let store = Store::open(&path).unwrap();
    let page2 = store.get_wiki_page("project/a").unwrap().unwrap();
    assert_eq!(page2.updated_at, updated_before);
    // 改路径：新目录必须存在
    assert!(store
        .update_project_path("project/a", "/definitely/not/here")
        .is_err());
    let moved = store
        .update_project_path("project/a", &dir_b.display().to_string())
        .unwrap();
    let new_url = moved.source_url.clone().unwrap();
    assert!(new_url.starts_with("file://"));
    assert_eq!(
        crate::wiki::file_url_to_path(&new_url).unwrap(),
        dir_b.canonicalize().unwrap()
    );
    // 正文快照行同步更新
    assert!(moved.content_md.contains(&dir_b.display().to_string()));
    assert!(!moved.content_md.contains(&dir_a.display().to_string()));
    // 非项目页拒绝
    store
        .upsert_wiki_page(
            &wiki_draft("person/王五", "person", "x"),
            ContentPolicy::Always,
        )
        .unwrap();
    assert!(store
        .update_project_path("person/王五", &dir_b.display().to_string())
        .is_err());
    std::fs::remove_dir_all(&dir_a).ok();
    std::fs::remove_dir_all(&dir_b).ok();
    let _ = std::fs::remove_file(path);
}

#[test]
fn digest_protection_does_not_record_revisions_or_opinion_changes() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event = store.insert_event(NewEvent::now("与赵六确认方案")).unwrap();
    store
        .upsert_wiki_page(
            &wiki_draft("person/赵六", "person", "v1 AI"),
            ContentPolicy::Always,
        )
        .unwrap();
    store
        .save_wiki_page_content("person/赵六", "v1 人工", "修正", None)
        .unwrap();
    let revs_before = store.list_wiki_revisions("person/赵六").unwrap().len();

    // 受保护写回：不追加 revision（内容没变，纯证据累加）
    let outcome = store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "person/赵六".to_string(),
                kind: "person".to_string(),
                title: "赵六".to_string(),
                summary: "s".to_string(),
                content_md: "AI 想覆盖".to_string(),
                tags: vec![],
                source_event_ids: vec![event],
                status: "active".to_string(),
                reason: "digest".to_string(),
                source_url: None,
            },
            ContentPolicy::PreserveHumanEdits,
        )
        .unwrap();
    assert!(outcome.protected);
    let revs_after = store.list_wiki_revisions("person/赵六").unwrap().len();
    assert_eq!(revs_before, revs_after, "保护降级写回不应追加 revision");

    // opinion / human_edited_at 不受 upsert 影响
    store
        .upsert_wiki_page(
            &wiki_draft("tweet-7", "source", "素材"),
            ContentPolicy::Always,
        )
        .unwrap();
    store.set_wiki_opinion("tweet-7", Some("endorse")).unwrap();
    let before = store.get_wiki_page("tweet-7").unwrap().unwrap();
    assert!(before.opinion.is_some());
    store
        .upsert_wiki_page(
            &wiki_draft("tweet-7", "source", "改不了"),
            ContentPolicy::PreserveHumanEdits,
        )
        .unwrap();
    let after = store.get_wiki_page("tweet-7").unwrap().unwrap();
    assert_eq!(
        after.opinion,
        Some("endorse".to_string()),
        "评价不被 digest 清掉"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn message_ordering_is_deterministic_on_same_timestamp() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let conversation_id = store.create_conversation(None, None).unwrap();
    // 两条消息刻意用完全相同的 created_at（旧实现没有 id tiebreak，
    // 同毫秒时 SQLite 返回顺序不确定，跨调用会翻转）。
    let same_ts = "2026-09-25T10:00:00.000000000Z";
    for (index, (id, content)) in [("msg-tie-2", "b"), ("msg-tie-1", "a")]
        .into_iter()
        .enumerate()
    {
        store
            .connection
            .execute(
                "INSERT INTO messages (id, conversation_id, parent_message_id, role, content, created_at)
                 VALUES (?1, ?2, NULL, 'user', ?3, ?4)",
                params![id, conversation_id, content, same_ts],
            )
            .unwrap();
    }
    // list_messages / get_child_messages 都应按 id ASC 破平，顺序稳定。
    let messages = store.list_messages(&conversation_id).unwrap();
    let ids: Vec<&str> = messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["msg-tie-1", "msg-tie-2"],
        "同时间戳消息按 id ASC 稳定排序"
    );
    let child = store.get_child_messages("msg-tie-1").unwrap();
    assert!(child.is_empty());
    let _ = std::fs::remove_file(path);
}

#[test]
fn wiki_pages_schema_exposes_human_edited_at_and_opinion() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    // v29 引入的这两列已固化进 migrations/01-baseline/up.sql，读取路径不得因缺列而报错
    let columns = {
        let mut statement = store
            .connection
            .prepare("PRAGMA table_info(wiki_pages)")
            .unwrap();
        statement
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    for expected in ["human_edited_at", "opinion"] {
        assert!(
            columns.iter().any(|name| name == expected),
            "wiki_pages 缺 {expected} 列，现有列：{columns:?}"
        );
    }
    store
        .upsert_wiki_page(
            &wiki_draft("topic/subject", "topic", "x"),
            ContentPolicy::Always,
        )
        .unwrap();
    let page = store.get_wiki_page("topic/subject").unwrap().unwrap();
    assert_eq!(page.human_edited_at, None);
    assert_eq!(page.opinion, None);
    drop(store);
    let _ = std::fs::remove_file(path);
}
