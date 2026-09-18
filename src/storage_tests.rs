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

    let id = store
        .create_conversation(Some("Test conversation"), None)
        .unwrap();

    assert!(!id.is_empty());
    let _ = std::fs::remove_file(path);
}

#[test]
fn list_conversations_shows_created_conversations() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    store.create_conversation(Some("First chat"), None).unwrap();
    store
        .create_conversation(Some("Second chat"), None)
        .unwrap();

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
    let msg_id = store
        .send_message(&conv_id, "user", "Hello world", None)
        .unwrap();

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

    store
        .send_message(&conv_id, "user", "First message", None)
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    store
        .send_message(&conv_id, "assistant", "Second message", None)
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    store
        .send_message(&conv_id, "user", "Third message", None)
        .unwrap();

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
    store
        .send_message(&conv_id, "user", "New message", None)
        .unwrap();

    let updated = store.get_conversation(&conv_id).unwrap().unwrap();

    assert_ne!(initial.updated_at, updated.updated_at);
    let _ = std::fs::remove_file(path);
}

#[test]
fn message_count_reflects_actual_messages() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let conv_id = store
        .create_conversation(Some("Counter test"), None)
        .unwrap();

    store
        .send_message(&conv_id, "user", "Message 1", None)
        .unwrap();
    store
        .send_message(&conv_id, "assistant", "Message 2", None)
        .unwrap();
    store
        .send_message(&conv_id, "user", "Message 3", None)
        .unwrap();

    let conv = store.get_conversation(&conv_id).unwrap().unwrap();

    assert_eq!(conv.message_count, 3);
    let _ = std::fs::remove_file(path);
}

#[test]
fn last_message_preview_shows_latest_content() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let conv_id = store
        .create_conversation(Some("Preview test"), None)
        .unwrap();

    store.send_message(&conv_id, "user", "First", None).unwrap();
    store
        .send_message(&conv_id, "assistant", "Latest message content", None)
        .unwrap();

    let conv = store.get_conversation(&conv_id).unwrap().unwrap();

    assert_eq!(
        conv.last_message_preview,
        Some("Latest message content".to_string())
    );
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
    store
        .record_token_usage(Some(&conv_id), 100, 20, 120, Some("gpt-4o"))
        .unwrap();
    store
        .record_token_usage(Some(&conv_id), 200, 30, 230, Some("gpt-4o"))
        .unwrap();
    store
        .record_token_usage(None, 300, 40, 340, Some("ollama"))
        .unwrap();

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

#[test]
fn rules_lifecycle_active_pending_promote_discard_delete() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    use crate::storage::RuleStatus;

    // 初始为空
    assert!(store.list_rules(None, None).unwrap().is_empty());

    // 手动新增 → active
    let active_id = store
        .add_rule(
            "和大型企业的人沟通重要事项必须留痕",
            RuleStatus::Active,
            None,
        )
        .unwrap();
    let active_list = store.list_active_rules().unwrap();
    assert_eq!(active_list.len(), 1);
    assert_eq!(active_list[0].id, active_id);
    assert_eq!(active_list[0].status, RuleStatus::Active);

    // AI 提议 → pending，不进入 active 列表（归属指定会话）
    store
        .add_rule(
            "生成内容的证据需要留底",
            RuleStatus::Pending,
            Some("conv-a"),
        )
        .unwrap();
    // 另一个会话的 pending 不属于本会话，不应被本会话看到/转正
    store
        .add_rule("别会话的规则", RuleStatus::Pending, Some("conv-b"))
        .unwrap();
    assert_eq!(
        store.list_active_rules().unwrap().len(),
        1,
        "pending 不应出现在 active 列表"
    );
    assert_eq!(
        store
            .list_rules(Some(RuleStatus::Pending), None)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        store
            .list_rules(Some(RuleStatus::Pending), Some("conv-a"))
            .unwrap()
            .len(),
        1,
        "应按会话隔离 pending"
    );

    // 用户确认 → 只 promote 本会话的 pending（conv-b 的保留）
    assert_eq!(store.promote_pending_rules("conv-a").unwrap(), 1);
    assert_eq!(store.list_active_rules().unwrap().len(), 2);
    assert_eq!(
        store
            .list_rules(Some(RuleStatus::Pending), None)
            .unwrap()
            .len(),
        1,
        "conv-b 的 pending 不应被误转正"
    );

    // 用户拒绝 → discard 只清本会话
    store
        .add_rule("这条会被丢弃", RuleStatus::Pending, Some("conv-a"))
        .unwrap();
    assert_eq!(store.discard_pending_rules("conv-a").unwrap(), 1);
    assert_eq!(
        store.list_rules(None, None).unwrap().len(),
        3,
        "conv-b 的 pending 应保留"
    );

    // 删除
    assert!(store.delete_rule(&active_id).unwrap());
    assert!(
        !store.delete_rule(&active_id).unwrap(),
        "重复删除应返回 false"
    );
    assert_eq!(store.list_rules(None, None).unwrap().len(), 2);

    let _ = std::fs::remove_file(path);
}

#[test]
fn recent_user_messages_spans_conversations() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let c1 = store.create_conversation(Some("对话一"), None).unwrap();
    let c2 = store.create_conversation(Some("对话二"), None).unwrap();

    store
        .send_message(
            &c1,
            "user",
            "昨天给海油服的张玮沟通了双链路付款的事情",
            None,
        )
        .unwrap();
    store
        .send_message(&c2, "user", "问了问张玮是否有和太极沟通", None)
        .unwrap();
    // assistant 消息不应出现在结果里
    store
        .send_message(&c1, "assistant", "这是 AI 回复，不算", None)
        .unwrap();

    let recent = store.recent_user_messages(10, 160).unwrap();
    assert_eq!(recent.len(), 2, "只应返回 user 消息");
    // 最新的在前：c2 的消息应排第一
    assert!(
        recent[0].content.contains("太极"),
        "最晚消息应在前: {:?}",
        recent
    );
    assert!(recent[1].content.contains("张玮"));
    assert!(
        recent.iter().all(|m| !m.content.contains("AI 回复")),
        "assistant 消息不应混入"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn recent_user_messages_respects_limit_and_truncation() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let c = store.create_conversation(Some("limit"), None).unwrap();
    for i in 0..5 {
        store
            .send_message(
                &c,
                "user",
                &format!("消息编号第{i}条，内容比较长用于测试截断行为"),
                None,
            )
            .unwrap();
    }

    let few = store.recent_user_messages(2, 160).unwrap();
    assert_eq!(few.len(), 2, "limit 应生效");

    let truncated = store.recent_user_messages(2, 4).unwrap();
    assert!(
        truncated[0].content.chars().count() <= 4,
        "应按 max_chars 截断: {}",
        truncated[0].content
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn provider_multi_config_single_active() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    // 第一个配置自动激活
    let a = store
        .save_ai_provider_config(
            None,
            "openai-main",
            "openai-compatible",
            "https://api.openai.com/v1",
            "gpt-4o",
            "key-a",
            0.7,
            None,
        )
        .unwrap();
    // 第二个配置默认不激活
    let b = store
        .save_ai_provider_config(
            None,
            "ollama-local",
            "ollama",
            "http://localhost:11434",
            "qwen2.5",
            "key-b",
            0.3,
            Some(4096),
        )
        .unwrap();
    let list = store.list_ai_provider_configs().unwrap();
    assert_eq!(list.len(), 2);
    assert!(
        list.iter().any(|p| p.id == a && p.is_active),
        "第一个配置应自动激活"
    );
    assert!(
        !list.iter().any(|p| p.id == b && p.is_active),
        "新配置默认不应激活"
    );

    // 切换激活：有且仅有一个激活项
    store.set_active_ai_provider_config(&b).unwrap();
    let active = store.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(active.id, b);
    assert_eq!(active.temperature, 0.3);
    assert_eq!(active.max_tokens, Some(4096));
    let actives = store
        .list_ai_provider_configs()
        .unwrap()
        .into_iter()
        .filter(|p| p.is_active)
        .collect::<Vec<_>>();
    assert_eq!(actives.len(), 1, "必须只有一个激活项");
    assert_eq!(actives[0].id, b);

    // 编辑配置：api_key 传空串应保留原 key，其余字段更新
    store
        .save_ai_provider_config(
            Some(&b),
            "ollama-local",
            "ollama",
            "http://localhost:11434",
            "qwen3",
            "",
            0.2,
            None,
        )
        .unwrap();
    let refreshed = store.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(refreshed.api_key, "key-b", "空串应保留原 key");
    assert_eq!(refreshed.model, "qwen3");
    assert_eq!(refreshed.temperature, 0.2);

    // 删除激活项 → 剩余配置自动接管激活
    store.delete_ai_provider_config(&b).unwrap();
    let list = store.list_ai_provider_configs().unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].is_active, "删除激活项后应自动接管激活");
    assert_eq!(list[0].id, a);

    // 同名配置冲突给出明确错误
    let err = store
        .save_ai_provider_config(
            None,
            "openai-main",
            "openai-compatible",
            "https://x",
            "m",
            "k",
            0.7,
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("已存在"), "同名应报友好错误: {err}");

    let _ = std::fs::remove_file(path);
}
