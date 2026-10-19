use super::*;
use uuid::Uuid;

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
    assert!(
        store
            .update_input_route(&first.id, "unknown", None, None, None, None)
            .is_err()
    );

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
    assert!(
        store
            .events_on_date(today - chrono::Duration::days(10))
            .unwrap()
            .is_empty()
    );
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
    assert!(
        store
            .list_entity_facts("topic", "topic/回答")
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .pending_actions_for_conversation(&conversation_id)
            .unwrap()
            .is_empty()
    );
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
