use super::temp_db;
use super::*;

#[test]
fn record_event_writes_directly() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let registry = ToolRegistry::default();
    let call = ToolCall::new(
        "record_event",
        json!({"raw_text": "昨天和张三沟通了网站改版"}),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(result.content.contains("已保存事件"), "{}", result.content);
    // 直接写入了 events 真源
    assert_eq!(store.list_events().unwrap().len(), 1);
    // 不产生待确认动作
    assert!(
        store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty()
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn record_event_does_not_duplicate_unified_conversation_input() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    store
        .submit_conversation_input(&conv, "今天整理了树莓派板子", Some("input-1"))
        .unwrap();
    let registry = ToolRegistry::default();
    let call = ToolCall::new("record_event", json!({"raw_text": "今天整理了树莓派板子"}));

    let result = dispatch(&call, &registry, &store, &conv);

    assert!(
        result.content.contains("无需重复记录"),
        "{}",
        result.content
    );
    assert_eq!(store.list_events().unwrap().len(), 1);
    assert_eq!(store.analysis_job_stats().unwrap().pending, 1);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn execute_record_event_writes_real_source() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let action_id = store
        .create_pending_action(
            &conv,
            "record_event",
            &json!({"raw_text": "去健身房练了背部"}).to_string(),
        )
        .unwrap();
    let pa = store.pending_actions_for_conversation(&conv).unwrap();
    let summary = execute_pending_action(&store, &pa[0]).unwrap();
    assert!(summary.contains("已保存"), "{summary}");
    assert_eq!(store.list_events().unwrap().len(), 1);
    let _ = action_id;
    drop(store);
    let _ = std::fs::remove_file(path);
}
