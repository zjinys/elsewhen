use crate::storage::Store;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_database() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "elsewhen-conversation-test-{}.db",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn create_conversation_returns_id() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let id = store.create_conversation(Some("Test conversation"), None).unwrap();

    assert!(!id.is_empty());
    let _ = std::fs::remove_file(path);
}

#[test]
fn list_conversations_shows_created_conversations() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    store.create_conversation(Some("First chat"), None).unwrap();
    store.create_conversation(Some("Second chat"), None).unwrap();

    let conversations = store.list_conversations().unwrap();

    assert_eq!(conversations.len(), 2);
    assert_eq!(conversations[0].title, Some("Second chat".to_string())); // Most recent first
    assert_eq!(conversations[1].title, Some("First chat".to_string()));
    let _ = std::fs::remove_file(path);
}

#[test]
fn send_message_creates_message_in_conversation() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let conv_id = store.create_conversation(None, None).unwrap();
    let msg_id = store.send_message(&conv_id, "user", "Hello world", None).unwrap();

    assert!(!msg_id.is_empty());

    let messages = store.list_messages(&conv_id).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[0].content, "Hello world");
    let _ = std::fs::remove_file(path);
}

#[test]
fn list_messages_returns_in_chronological_order() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let conv_id = store.create_conversation(Some("Chat"), None).unwrap();

    store.send_message(&conv_id, "user", "First message", None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    store.send_message(&conv_id, "assistant", "Second message", None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    store.send_message(&conv_id, "user", "Third message", None).unwrap();

    let messages = store.list_messages(&conv_id).unwrap();

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].content, "First message");
    assert_eq!(messages[1].content, "Second message");
    assert_eq!(messages[2].content, "Third message");
    let _ = std::fs::remove_file(path);
}

#[test]
fn conversation_updated_at_changes_on_message() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let conv_id = store.create_conversation(Some("Test"), None).unwrap();
    let initial = store.get_conversation(&conv_id).unwrap().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(10));
    store.send_message(&conv_id, "user", "New message", None).unwrap();

    let updated = store.get_conversation(&conv_id).unwrap().unwrap();

    assert_ne!(initial.updated_at, updated.updated_at);
    let _ = std::fs::remove_file(path);
}

#[test]
fn message_count_reflects_actual_messages() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let conv_id = store.create_conversation(Some("Counter test"), None).unwrap();

    store.send_message(&conv_id, "user", "Message 1", None).unwrap();
    store.send_message(&conv_id, "assistant", "Message 2", None).unwrap();
    store.send_message(&conv_id, "user", "Message 3", None).unwrap();

    let conv = store.get_conversation(&conv_id).unwrap().unwrap();

    assert_eq!(conv.message_count, 3);
    let _ = std::fs::remove_file(path);
}

#[test]
fn last_message_preview_shows_latest_content() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let conv_id = store.create_conversation(Some("Preview test"), None).unwrap();

    store.send_message(&conv_id, "user", "First", None).unwrap();
    store.send_message(&conv_id, "assistant", "Latest message content", None).unwrap();

    let conv = store.get_conversation(&conv_id).unwrap().unwrap();

    assert_eq!(conv.last_message_preview, Some("Latest message content".to_string()));
    let _ = std::fs::remove_file(path);
}

#[test]
fn invalid_role_is_rejected() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let conv_id = store.create_conversation(None, None).unwrap();
    let result = store.send_message(&conv_id, "invalid_role", "Test", None);

    assert!(result.is_err());
    let _ = std::fs::remove_file(path);
}

#[test]
fn get_nonexistent_conversation_returns_none() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let result = store.get_conversation("nonexistent-id").unwrap();

    assert!(result.is_none());
    let _ = std::fs::remove_file(path);
}

#[test]
fn record_token_usage_and_aggregate_daily() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let conv_id = store.create_conversation(Some("usage test"), None).unwrap();

    // 三次调用：两次带会话，一次不带（如批处理）
    store.record_token_usage(Some(&conv_id), 100, 20, 120, Some("gpt-4o")).unwrap();
    store.record_token_usage(Some(&conv_id), 200, 30, 230, Some("gpt-4o")).unwrap();
    store.record_token_usage(None, 300, 40, 340, Some("ollama")).unwrap();

    let daily = store.daily_token_usage(7).unwrap();
    assert_eq!(daily.len(), 1, "三次调用都发生在今天，应聚合成一行");
    let day = &daily[0];
    assert_eq!(day.total_tokens, 690);
    assert_eq!(day.prompt_tokens, 600);
    assert_eq!(day.completion_tokens, 90);
    assert_eq!(day.call_count, 3);
    // 当日分组，日期格式应为 YYYY-MM-DD
    assert_eq!(day.date.len(), 10);

    let _ = std::fs::remove_file(path);
}

#[test]
fn daily_token_usage_empty_without_records() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let daily = store.daily_token_usage(7).unwrap();
    assert!(daily.is_empty());
    let _ = std::fs::remove_file(path);
}
