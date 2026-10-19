use super::temp_db;
use super::*;

#[test]
fn list_events_by_date_returns_days_events() {
    let (store, path) = temp_db();
    let registry = ToolRegistry::default();
    let today = chrono::Local::now().date_naive();
    let yesterday = today - chrono::Duration::days(1);
    store
        .insert_event(crate::event::NewEvent {
            raw_text: "今天的事件",
            occurred_at: chrono::Utc::now(),
            recorded_at: chrono::Utc::now(),
            source: "test",
        })
        .unwrap();
    store
        .insert_event(crate::event::NewEvent {
            raw_text: "昨天的事件",
            occurred_at: chrono::Utc::now() - chrono::Duration::days(1),
            recorded_at: chrono::Utc::now() - chrono::Duration::days(1),
            source: "test",
        })
        .unwrap();

    let call = ToolCall::new(
        "list_events_by_date",
        json!({"date": today.format("%Y-%m-%d").to_string()}),
    );
    let result = dispatch(&call, &registry, &store, "conv-1");
    assert!(result.content.contains("今天的事件"), "{}", result.content);
    assert!(!result.content.contains("昨天的事件"), "{}", result.content);
    assert!(result.content.contains("条事件"), "{}", result.content);

    let call = ToolCall::new(
        "list_events_by_date",
        json!({"date": yesterday.format("%Y-%m-%d").to_string()}),
    );
    let result = dispatch(&call, &registry, &store, "conv-1");
    assert!(result.content.contains("昨天的事件"), "{}", result.content);

    // 坏格式与空结果都友好返回
    let call = ToolCall::new("list_events_by_date", json!({"date": "2026/06/20"}));
    let result = dispatch(&call, &registry, &store, "conv-1");
    assert!(result.content.contains("YYYY-MM-DD"), "{}", result.content);
    let call = ToolCall::new("list_events_by_date", json!({"date": "2030-01-01"}));
    let result = dispatch(&call, &registry, &store, "conv-1");
    assert!(
        result.content.contains("没有事件记录"),
        "{}",
        result.content
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn archive_conversations_by_title_is_confirm_gated_then_archives() {
    let (store, path) = temp_db();
    let registry = ToolRegistry::default();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    // 主对话：空标题（显示为「新对话」）、Hello 两个、其他
    store.create_conversation(None, None).unwrap(); // 显示「新对话」
    store
        .create_conversation(Some("Hello World"), None)
        .unwrap();
    store
        .create_conversation(Some("Phase Hello 2"), None)
        .unwrap();
    store.create_conversation(Some("其他"), None).unwrap();
    // 知识页内聊天：不应被匹配
    store
        .create_wiki_chat_conversation("person/x", "页内对话")
        .unwrap();

    // 1) 包含匹配：草拟两个 Hello，未确认前不归档
    let call = ToolCall::new(
        "archive_conversations_by_title",
        json!({"contains": "hello"}),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(result.content.contains("2 个匹配"), "{}", result.content);
    assert!(result.content.contains("Hello World"), "{}", result.content);
    let pendings = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(pendings.len(), 1);
    assert_eq!(store.list_conversations().unwrap().len(), 5, "确认前不归档");

    // 2) 确认执行：归档 2 个（执行不删 pending，手动删以模拟确认流）
    let summary = execute_pending_action(&store, &pendings[0]).unwrap();
    assert!(summary.contains("已归档 2 个对话"), "{summary}");
    store.delete_pending_action(&pendings[0].id).unwrap();
    let remaining_titles: Vec<String> = store
        .list_conversations()
        .unwrap()
        .iter()
        .map(|c| c.title.as_deref().unwrap_or("新对话").to_string())
        .collect();
    assert_eq!(remaining_titles.len(), 3);
    for t in ["t", "新对话", "其他"] {
        assert!(
            remaining_titles.iter().any(|x| x == t),
            "缺 {t}: {remaining_titles:?}"
        );
    }

    // 3) 精确匹配「新对话」= 空标题会话
    let call = ToolCall::new("archive_conversations_by_title", json!({"title": "新对话"}));
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(result.content.contains("1 个匹配"), "{}", result.content);
    let pendings = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(pendings.len(), 1);
    execute_pending_action(&store, &pendings[0]).unwrap();
    store.delete_pending_action(&pendings[0].id).unwrap();
    let remaining_titles: Vec<String> = store
        .list_conversations()
        .unwrap()
        .iter()
        .map(|c| c.title.as_deref().unwrap_or("新对话").to_string())
        .collect();
    assert_eq!(remaining_titles.len(), 2);
    assert!(
        !remaining_titles.iter().any(|x| x == "新对话"),
        "{remaining_titles:?}"
    );

    // 4) 无匹配：友好错误，不登记
    let call = ToolCall::new(
        "archive_conversations_by_title",
        json!({"contains": "不存在的"}),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(result.content.contains("没有找到"), "{}", result.content);
    assert!(
        store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty()
    );

    // 5) 知识页聊天始终未被动过
    assert!(
        store
            .get_conversation(
                &store
                    .find_wiki_chat_conversation("person/x")
                    .unwrap()
                    .unwrap()
            )
            .unwrap()
            .unwrap()
            .archived
            == false
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

// ── R3：审批续跑闭环 ──

/// accept 成功 → 注入带确定性标记的续跑指令并入队生成；节流 2 条/5 分钟；
/// reject 或失败决策不触发。
#[test]
fn approval_followup_injects_and_throttles() {
    use crate::storage::approvals::{ChatDecision, ChatProposal, FOLLOWUP_MARK};
    let (store, _dir) = temp_db();
    let conversation = store.create_conversation(Some("续跑"), None).unwrap();
    let proposal = ChatProposal {
        kind: "action".into(),
        id: "p1".into(),
        conversation_id: Some(conversation.clone()),
        action: "create_todo".into(),
        args_json: "{}".into(),
        version: 1,
        created_at: "2026-10-20T00:00:00Z".into(),
        approval_token: "tok".into(),
        target_basis_json: "{}".into(),
    };
    let decision = |decision: &str, status: &str| ChatDecision {
        kind: "action".into(),
        id: "p1".into(),
        conversation_id: Some(conversation.clone()),
        version: 1,
        approval_token: "tok".into(),
        decision: decision.into(),
        status: status.into(),
        result: "已创建待办草稿".into(),
        action: "create_todo".into(),
        args_json: "{}".into(),
    };
    // reject 不续跑
    assert!(
        !crate::ai::approval::enqueue_approval_followup(
            &store,
            &proposal,
            &decision("reject", "declined")
        )
        .unwrap()
    );
    // accept 成功 → 续跑入队
    assert!(
        crate::ai::approval::enqueue_approval_followup(
            &store,
            &proposal,
            &decision("accept", "succeeded")
        )
        .unwrap()
    );
    let messages = store.list_messages(&conversation).unwrap();
    let followups: Vec<_> = messages
        .iter()
        .filter(|m| m.role == "user" && m.content.starts_with(FOLLOWUP_MARK))
        .collect();
    assert_eq!(followups.len(), 1);
    assert!(followups[0].content.contains("create_todo"));
    // 队列里有一项待生成输入
    let queue = store.list_chat_input_queue(&conversation, 0, 10).unwrap();
    assert_eq!(queue.len(), 1);
    // 第 2、3 条 → 节流（窗口内 ≥2 即放弃）
    assert!(
        crate::ai::approval::enqueue_approval_followup(
            &store,
            &proposal,
            &decision("accept", "succeeded")
        )
        .unwrap()
    );
    assert!(
        !crate::ai::approval::enqueue_approval_followup(
            &store,
            &proposal,
            &decision("accept", "succeeded")
        )
        .unwrap()
    );
}
