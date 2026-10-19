use super::*;

#[test]
fn system_prompt_describes_own_capabilities() {
    let (store, conversation_id, path) = setup_store();
    let context = SimpleMemory::new(10)
        .prepare_context(&conversation_id, &store)
        .unwrap();

    let system = system_text(&context);
    // AI 应知道这套系统真实具备的能力与入口，而不是瞎猜
    assert!(system.contains("导入"), "应描述导入能力（推文/网页/文本）");
    assert!(system.contains("知识库"), "应描述知识库能力");
    assert!(system.contains("个人规则库"), "应描述规则库能力");
    assert!(system.contains("保存到知识库"), "应描述推文保存入库入口");
    assert!(system.contains("设置"), "应描述设置页能力");

    let _ = std::fs::remove_file(path);
}

#[test]
fn auto_recorded_main_input_hides_record_event_tool() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let conversation_id = store
        .create_conversation(Some("主对话流"), Some("diary"))
        .unwrap();
    store
        .submit_conversation_input(&conversation_id, "今天完成了整理", Some("main-1"))
        .unwrap();

    let context = SimpleMemory::new(10)
        .prepare_context(&conversation_id, &store)
        .unwrap();
    let system = system_text(&context);
    assert!(system.contains("已由系统原样保存"));
    assert!(!system.contains("record_event"));

    let _ = std::fs::remove_file(path);
}

#[test]
fn legacy_conversation_without_auto_record_keeps_record_event_tool() {
    let (store, conversation_id, path) = setup_store();
    let context = SimpleMemory::new(10)
        .prepare_context(&conversation_id, &store)
        .unwrap();
    let system = system_text(&context);
    assert!(system.contains("调用 record_event"));
    assert!(!system.contains("可用工具（name：用途）"));
    assert!(system.contains("当前对话没有自动事件记录"));

    let _ = std::fs::remove_file(path);
}

#[test]
fn knowledge_mentor_hides_record_event_tool() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let conversation_id = store
        .create_wiki_chat_conversation("topic/test", "测试主题")
        .unwrap();
    store
        .send_message(&conversation_id, "user", "分析一下这页", None)
        .unwrap();

    let context = SimpleMemory::new(10)
        .prepare_context(&conversation_id, &store)
        .unwrap();
    let system = system_text(&context);
    assert!(system.contains("知识页专业导师"));
    assert!(!system.contains("record_event"));

    let _ = std::fs::remove_file(path);
}

#[test]
fn recent_background_excludes_current_stream_and_wiki_before_limiting() {
    let (store, conversation_id, path) = setup_store();
    assert!(
        recent_background(&store, &conversation_id)
            .unwrap()
            .is_none()
    );
    let other = store
        .create_conversation(Some("历史普通对话"), None)
        .unwrap();
    store
        .send_message(&other, "user", "历史普通会话背景", None)
        .unwrap();
    let wiki = store
        .create_wiki_chat_conversation("topic/test", "知识页")
        .unwrap();
    for _ in 0..8 {
        store
            .send_message(&wiki, "user", "知识页无关讨论", None)
            .unwrap();
        store
            .send_message(&conversation_id, "user", "当前流已有消息", None)
            .unwrap();
    }
    let background = recent_background(&store, &conversation_id)
        .unwrap()
        .unwrap();
    assert!(background.optional_background);
    assert!(background.content.contains("历史普通会话背景"));
    assert!(!background.content.contains("知识页无关讨论"));
    assert!(!background.content.contains("当前流已有消息"));
    assert!(recent_background(&store, &wiki).unwrap().is_none());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn system_prompt_injects_cross_conversation_recent_messages() {
    let (store, conversation_id, path) = setup_store();
    // 在另一个对话里发一条「近期事件」，验证它会被注入到本对话的 system 提示
    let other_id = store
        .create_conversation(Some("另一个对话"), Some("general"))
        .unwrap();
    store
        .send_message(
            &other_id,
            "user",
            "昨天给张三沟通了网站改版的事情，必须留痕",
            None,
        )
        .unwrap();

    let context = SimpleMemory::new(10)
        .prepare_context(&conversation_id, &store)
        .unwrap();

    assert!(!context[0].content.contains("昨天给张三"));
    let background = context.iter().find(|m| m.optional_background).unwrap();
    let system = &background.content;
    assert!(
        system.contains("张三"),
        "跨对话近期消息应被注入 system 提示，实际: {}",
        system
    );
    assert!(system.contains("留痕"), "近期消息内容应整体可见");

    let _ = std::fs::remove_file(path);
}

/// R2B：stable_setup 瘦身——BASE 只剩横切纪律（<6K tokens@启发式口径），
/// 常驻工具的句式指南渲染为独立第二条 system message。
#[test]
fn stable_setup_split_keeps_core_and_moves_guides_to_second_message() {
    use crate::ai::budget::text_tokens;
    let base_tokens = text_tokens("deepseek-flash", SYSTEM_PROMPT_BASE);
    assert!(base_tokens < 6_500, "CORE 应显著瘦身，实际 {base_tokens}");
    // 全部指南文本不再出现在 BASE 里（以 web_search 句式与归档句式为样本）
    for (_, _, text) in TOOL_GUIDES {
        assert!(
            !SYSTEM_PROMPT_BASE.contains(text.chars().take(30).collect::<String>().as_str()),
            "指南文本不应残留在常驻基底"
        );
    }
    let (_, _, events_guide) = TOOL_GUIDES[3];
    assert!(events_guide.contains("list_events_by_date"));
}
