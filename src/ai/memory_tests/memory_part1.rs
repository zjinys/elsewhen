#[allow(unused_imports)]
use super::*;

/// D6 今日记忆真检索：当日事件注入为可选背景段；没有记录时不注入。
#[test]
fn today_memory_background_is_real_retrieval() {
    let (store, conversation, path) = setup_store();
    // 会话里那段消息不算事件；事件库今天还没有记录 → 不注入
    assert!(
        today_memory_background(&store, &conversation)
            .unwrap()
            .is_none()
    );
    let event = store
        .insert_event(crate::event::NewEvent::now("今天跑了五公里"))
        .unwrap();
    let _ = event;
    let today = today_memory_background(&store, &conversation)
        .unwrap()
        .expect("今天有记录时应注入");
    assert!(today.content.contains("今天跑了五公里"));
    assert!(today.content.contains("真检索"));
    // 注入段是可选背景：预算紧张时允许被整体省略
    let context = SimpleMemory::new(1)
        .prepare_context(&conversation, &store)
        .unwrap();
    let idx = context
        .iter()
        .position(|m| m.optional_background && m.content.contains("今天跑了五公里"));
    assert!(idx.is_some(), "正常预算下今天记忆段应在上下文里");
    assert!(idx.unwrap() >= 1, "今天的记忆应排在 system 角色设定之后");
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn preliminary_selection_cannot_hide_a_broken_earlier_exchange() {
    let mut context = vec![
        ContextMessage::new("system", "rules"),
        ContextMessage::new("user", "old question ".repeat(400)),
        ContextMessage::tool_result("missing-call".into(), "orphan".into()),
        ContextMessage::new("user", "current"),
    ];
    let before = format!("{context:?}");
    compress_context_for_model(&mut context, 100, "gpt-4o");
    assert_eq!(format!("{context:?}"), before);
    assert!(crate::ai::budget::validate_context_exchanges(&context).is_err());
}

#[test]
fn simple_memory_retains_the_user_at_the_exchange_boundary() {
    let (store, conversation, path) = setup_store();
    store
        .send_message(&conversation, "assistant", "first answer", None)
        .unwrap();
    store
        .send_message(&conversation, "assistant", "second answer", None)
        .unwrap();
    let context = SimpleMemory::new(1)
        .prepare_context(&conversation, &store)
        .unwrap();
    let history: Vec<_> = context
        .iter()
        .filter(|message| message.role != "system")
        .collect();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].role, "user");
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn preliminary_selection_never_keeps_an_orphan_tool_result() {
    // 10-19 P0-b：最近两个交换无条件保留，可被省略的历史必须比它们更老。
    let early_call = ToolCall::new("early_lookup", serde_json::json!({}));
    let call = ToolCall::new("lookup", serde_json::json!({}));
    let mut context = vec![
        ContextMessage::new("system", "rules"),
        ContextMessage::new("user", "early question ".repeat(400)),
        ContextMessage::assistant_with_tool_calls(String::new(), vec![early_call.clone()], None),
        ContextMessage::tool_result(early_call.id.clone(), "early result".into()),
        ContextMessage::new("user", "middle question ".repeat(400)),
        ContextMessage::new("assistant", "middle answer"),
        ContextMessage::new("user", "old question ".repeat(400)),
        ContextMessage::assistant_with_tool_calls(String::new(), vec![call.clone()], None),
        ContextMessage::tool_result(call.id, "tiny result".into()),
        ContextMessage::new("user", "current"),
    ];
    compress_context_for_model(&mut context, 100, "gpt-4o");
    // 早期交换（含它的工具消息）整组消失，绝不落单
    assert!(!context.iter().any(|m| m.content.contains("early result")));
    assert!(
        !context.iter().any(|m| m
            .tool_calls
            .as_ref()
            .is_some_and(|calls| calls.iter().any(|c| c.id == early_call.id))),
        "省略的交换不得留下孤儿 tool 调用"
    );
    // 最近一个交换连同工具结果完整保留
    assert!(
        context
            .iter()
            .any(|m| m.role == "tool" && m.content == "tiny result")
    );
    assert_eq!(context.last().unwrap().content, "current");
}

#[test]
fn unknown_model_preselection_uses_bytes_and_drops_optional_first() {
    // 10-19 P0-b：hello 交换是最近第二交换（不可省略）；字节 vs token
    // 估算的差异改用更早的大轮展示：已知模型按 CJK 2 字符/token 估
    // 得下、未知模型按字节估得上。
    let original = vec![
        ContextMessage::new("system", "rules"),
        ContextMessage::new("user", "背景材料极长 ".repeat(50)),
        ContextMessage::new("assistant", "背景答复"),
        ContextMessage::new("user", "hello ".repeat(80)),
        ContextMessage::new("assistant", "answer"),
        ContextMessage::new("user", "current"),
    ];
    let mut known = original.clone();
    let mut unknown = original;
    compress_context_for_model(&mut known, 600, "gpt-4o");
    compress_context_for_model(&mut unknown, 600, "unknown-model");
    assert!(known.iter().any(|m| m.content.starts_with("背景材料极长")));
    assert!(
        !unknown
            .iter()
            .any(|m| m.content.starts_with("背景材料极长"))
    );
    assert!(known.iter().any(|m| m.content.starts_with("hello")));
    assert!(unknown.iter().any(|m| m.content.starts_with("hello")));
    let mut background = ContextMessage::new("system", "optional".repeat(400));
    background.optional_background = true;
    known.insert(1, background);
    compress_context_for_model(&mut known, 250, "gpt-4o");
    assert!(known.iter().any(|m| m.content.starts_with("hello")));
    assert!(!known.iter().any(|m| m.optional_background));
}

#[test]
fn compress_context_preserves_system_and_latest_turns() {
    // 10-19 P0-b：最近两个交换硬保留，被省略的历史放在它们之前。
    let mut context = vec![
        ContextMessage::new("system", "system prompt"),
        ContextMessage::new("user", "更早的历史 ".repeat(200)),
        ContextMessage::new("assistant", "更早的答复 ".repeat(200)),
        ContextMessage::new("user", "很早以前的背景 ".repeat(80)),
        ContextMessage::new("assistant", "很早以前的回答 ".repeat(80)),
        ContextMessage::new("user", "当前问题"),
    ];

    compress_context(&mut context, 40);

    assert_eq!(
        context.first().map(|m| m.content.as_str()),
        Some("system prompt")
    );
    assert!(context.iter().any(|m| m.content == "当前问题"));
    assert!(
        context
            .iter()
            .any(|m| m.content.contains("自动压缩") || m.content.contains("前文省略"))
    );
    // 10-19 P0-b 后整体不再承诺 ≤ 预算（最近交换+当前硬保留）；改为
    // 验证省略发生在最近交换之前、且被省略内容不再出现。
    assert!(
        !context
            .iter()
            .any(|m| m.content.contains("更早的历史") || m.content.contains("更早的答复"))
    );
    assert!(context.iter().any(|m| m.content.contains("很早以前的背景")));
}

#[test]
fn compress_context_keeps_primary_prompt_and_current_question_with_large_page() {
    let primary = "核心指令和工具协议".repeat(10);
    let question = "请根据这页给出决策";
    let mut context = vec![
        ContextMessage::new("system", primary.clone()),
        ContextMessage::new("user", "旧对话".repeat(100)),
        ContextMessage::new("user", question),
        ContextMessage::new("system", format!("页面正文：{}", "内容".repeat(500))),
    ];

    let budget = estimate_tokens(&primary) + estimate_tokens(question) + 100;
    compress_context(&mut context, budget);

    assert_eq!(context[0].content, primary);
    assert_eq!(context.last().unwrap().content, question);
    assert!(
        context
            .iter()
            .any(|m| m.content == format!("页面正文：{}", "内容".repeat(500)))
    );
    // Required evidence may exceed the memory estimate; only the complete
    // provider boundary may reject it, never silently shorten it here.
    assert!(
        context
            .iter()
            .map(|m| estimate_tokens(&m.content))
            .sum::<usize>()
            > budget
    );
}
