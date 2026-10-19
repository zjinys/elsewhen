use super::super::content_chat::{
    is_confirmation, is_declination, parse_rule_proposal, trim_noise,
};
use super::super::parse_tools::{extract_claimed_titles, parse_tool_call_envelope};
use super::super::*;
use super::*;
use serde_json::{Value, json};

#[test]
fn verify_claimed_writes_is_store_grounded() {
    use crate::storage::{ContentPolicy, WikiPageDraft};
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let mk = |slug: &str, title: &str| WikiPageDraft {
        slug: slug.to_string(),
        kind: "topic".to_string(),
        title: title.to_string(),
        summary: "s".to_string(),
        content_md: "c".to_string(),
        tags: vec![],
        source_event_ids: vec![],
        status: "active".to_string(),
        reason: "test".to_string(),
        source_url: None,
    };

    // 本轮写入：turn_start 卡在写入之前 → Clean
    let before = chrono::Utc::now();
    store
        .upsert_wiki_page(&mk("kb-new-1", "本轮新页"), ContentPolicy::Always)
        .unwrap();
    let v = verify_claimed_writes(&store, &conv, &["本轮新页".to_string()], before).unwrap();
    assert!(matches!(v, ClaimVerdict::Clean), "{v:?}");

    // 旧页面：turn_start 推到写入之后 → Unproven（旧页面≠本轮写入）
    let after = chrono::Utc::now() + chrono::Duration::hours(1);
    let v = verify_claimed_writes(&store, &conv, &["本轮新页".to_string()], after).unwrap();
    assert!(matches!(v, ClaimVerdict::Unproven(_)), "{v:?}");

    // 只有草稿：DraftOnly（草拟≠保存）
    store
        .create_pending_action(
            &conv,
            "save_knowledge_draft",
            &serde_json::json!({"title": "草稿页", "content_md": "x"}).to_string(),
        )
        .unwrap();
    let v =
        verify_claimed_writes(&store, &conv, &["草稿页".to_string()], chrono::Utc::now()).unwrap();
    assert!(matches!(v, ClaimVerdict::DraftOnly(_)), "{v:?}");

    // 查无此页：Unproven
    let v = verify_claimed_writes(
        &store,
        &conv,
        &["根本不存在".to_string()],
        chrono::Utc::now(),
    )
    .unwrap();
    assert!(matches!(v, ClaimVerdict::Unproven(_)), "{v:?}");

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn agent_loop_native_tool_calls_round_trip() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "帮我看看规则库里有什么")];
    let registry = ToolRegistry::default();

    // 第一轮：原生 tool_calls 请求 list_rules；第二轮：收敛文本
    let provider = ScriptedProvider::new(vec![
        Ok(AiReply {
            content: String::new(),
            tool_calls: vec![ToolCall::new("list_rules", json!({}))],
            model: Some("fake".to_string()),
            usage: None,
            reasoning_content: None,
            finish_reason: None,
        }),
        Ok(AiReply::text("规则库查好了。")),
    ]);

    let outcome = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &registry,
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(outcome.content, "规则库查好了。");
    assert!(
        provider.saw_tools_on_first_call(),
        "首轮应携带原生 tools 清单"
    );

    // 上下文里应有 assistant(tool_calls) 回传 + tool 结果
    let tool_result = context
        .iter()
        .find(|m| m.role == "tool")
        .expect("应有 tool 结果消息");
    assert!(
        !tool_result.tool_call_id.as_deref().unwrap_or("").is_empty(),
        "tool 消息应带 tool_call_id"
    );

    let assistant_echo = context
        .iter()
        .find(|m| m.tool_calls.is_some())
        .expect("应有 assistant 回传");
    assert_eq!(assistant_echo.tool_calls.as_ref().unwrap().len(), 1);

    let _ = &provider;
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn knowledge_tool_round_returns_only_supplied_bounded_citations() {
    let (store, path) = temporary_database();
    let page = crate::wiki::save_text_page(
        &"SQLite 事务原文。".repeat(3000),
        Some("SQLite 事务"),
        &[],
        &store,
    )
    .unwrap();
    let conv = store.create_conversation(Some("引用测试"), None).unwrap();
    let mut context = vec![
        ContextMessage::new("system", crate::knowledge::CITATION_INSTRUCTIONS),
        ContextMessage::new("user", "请解释 SQLite 事务"),
    ];
    let provider = ScriptedProvider::new(vec![
        Ok(AiReply {
            content: String::new(),
            tool_calls: vec![ToolCall::new("get_wiki_page", json!({"slug":page.slug}))],
            model: None,
            usage: None,
            reasoning_content: None,
            finish_reason: None,
        }),
        Ok(AiReply::text(format!(
            "事务说明 [[kb:{}]] [[kb:invented]]",
            page.slug
        ))),
    ]);
    let reply = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    let selected = crate::knowledge::candidates_in_context(&context);
    assert_eq!(selected.len(), 1);
    assert!(selected[0].excerpt.chars().count() <= 2500);
    let (answer, cited) = crate::knowledge::validate_answer_citations(&reply.content, &selected);
    assert_eq!(cited.len(), 1);
    assert_eq!(cited[0].page_slug, page.slug);
    assert!(!answer.contains("invented"));
    assert!(
        context
            .iter()
            .map(|m| m.content.chars().count())
            .sum::<usize>()
            <= 24000
    );
    assert_eq!(
        store
            .source_snapshot(&cited[0].sources[0].snapshot_id)
            .unwrap()
            .unwrap()
            .content_md,
        page.content_md
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn agent_loop_text_protocol_fallback() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "统计一下最近的 token 用量")];
    let registry = ToolRegistry::default();

    let provider = ScriptedProvider::new(vec![
        Ok(AiReply::text(
            "[工具调用]{\"name\":\"get_daily_token_usage\",\"arguments\":{}}",
        )),
        Ok(AiReply::text("最近没有 AI 调用记录。")),
    ]);

    let outcome = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &registry,
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(outcome.content, "最近没有 AI 调用记录。");

    // 文本协议下工具结果以 system 角色注入
    let result_msg = context
        .iter()
        .find(|m| m.role == "system" && m.content.contains("工具「get_daily_token_usage」"))
        .expect("应有工具结果 system 消息");
    assert!(
        result_msg.content.contains("没有 AI 调用记录"),
        "{}",
        result_msg.content
    );

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn agent_loop_converges_immediately_without_tools() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "你好")];
    let registry = ToolRegistry::default();

    let provider = ScriptedProvider::new(vec![Ok(AiReply::text("你好呀。"))]);
    let outcome = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &registry,
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(outcome.content, "你好呀。");
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn agent_loop_falls_back_to_text_when_native_errors() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "查一下最近 token")];
    let registry = ToolRegistry::default();

    // 第一轮（带 tools）：模型不支持 tools 字段 → 报错；第二轮（无 tools）：纯文本收敛
    let provider = ScriptedProvider::new(vec![
        Err(anyhow::anyhow!("400: tools 字段不被支持")),
        Ok(AiReply::text("最近没有 AI 调用记录。")),
    ]);

    let outcome = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &registry,
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(outcome.content, "最近没有 AI 调用记录。");
    assert!(provider.saw_tools_on_first_call(), "首轮仍应尝试原生工具");
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn direct_query_matches_token_usage_question() {
    let (store, path) = temporary_database();
    store
        .record_token_usage(None, 100, 50, 150, Some("fake-model"))
        .unwrap();
    let hit = direct_query(&store, "我最近 token 用了多少？")
        .expect("直查不报错")
        .expect("应命中 token 直查");
    assert_eq!(hit.0, "token 用量");
    assert!(hit.1.contains("150 tokens"), "{}", hit.1);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn direct_query_matches_rules_question() {
    let (store, path) = temporary_database();
    store
        .add_rule(
            "和大型企业的人沟通重要事项必须留痕",
            crate::storage::RuleStatus::Active,
            None,
        )
        .unwrap();
    let hit = direct_query(&store, "我的规则库里现在有哪些规则？")
        .expect("直查不报错")
        .expect("应命中规则直查");
    assert_eq!(hit.0, "个人规则库");
    assert!(hit.1.contains("留痕"), "{}", hit.1);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn direct_query_ignores_normal_questions() {
    let (store, path) = temporary_database();
    for q in [
        "张三有王五沟通吗",
        "帮我记录一下今天的事",
        "把这条存进知识库",
    ] {
        assert!(
            direct_query(&store, q).expect("直查不报错").is_none(),
            "不应命中: {q}"
        );
    }
    drop(store);
    let _ = std::fs::remove_file(path);
}

/// 声明调用但没有正文或调用字段，与普通空回复分开处理。
#[test]
fn missing_tool_call_detects_incomplete_response() {
    assert!(missing_tool_call_response(&reply_with(
        Some("function_call"),
        "",
        vec![]
    )));
    // 部分 provider 用 tool_calls 而非 function_call 表示同一件事
    assert!(missing_tool_call_response(&reply_with(
        Some("tool_calls"),
        "",
        vec![]
    )));
    // 正文其实是空白字符时同样算「没有正文」
    assert!(missing_tool_call_response(&reply_with(
        Some("function_call"),
        "  \n ",
        vec![]
    )));
}

/// 误报会让诊断变得不可信——正常的空回复、真实拿到了 tool_calls、
/// 正常有正文的三种情况都必须安静退出。
#[test]
fn missing_tool_call_does_not_misreport_normal_replies() {
    // 模型自己返回空（推理型模型烧光输出预算的常见情形）
    assert!(!missing_tool_call_response(&reply_with(
        Some("stop"),
        "",
        vec![]
    )));
    assert!(!missing_tool_call_response(&reply_with(None, "", vec![])));
    // 真的拿到了工具调用：此时正文为空是正常的
    assert!(!missing_tool_call_response(&reply_with(
        Some("function_call"),
        "",
        vec![ToolCall::new("list_rules", json!({}))]
    )));
    // 有正文就与本签名无关
    assert!(!missing_tool_call_response(&reply_with(
        Some("function_call"),
        "我来帮你查一下。",
        vec![]
    )));
}

// ── 外网搜索意图接线（note_web_search_intent）──────────────────────

fn agent_store() -> (crate::storage::Store, std::path::PathBuf) {
    temporary_database()
}

fn agent_registry(defer: bool) -> crate::ai::tool::ToolRegistry {
    if defer {
        crate::ai::tool::ToolRegistry::deferred_runtime()
    } else {
        crate::ai::tool::ToolRegistry::default()
    }
}

fn intent_system_notes(context: &[ContextMessage]) -> Vec<String> {
    context
        .iter()
        .filter(|m| m.role == "system" && m.content.contains("外网搜索"))
        .map(|m| m.content.clone())
        .collect()
}

#[test]
fn web_search_intent_disabled_pushes_honest_guidance() {
    let (store, path) = agent_store();
    let registry = agent_registry(true);
    // 默认配置（未启用）
    let mut context = vec![ContextMessage::new("user", "帮我搜一下最近的 Rust 新闻")];
    crate::ai::conversation::note_web_search_intent(&mut context, &registry, &store);
    let notes = intent_system_notes(&context);
    assert_eq!(notes.len(), 1);
    assert!(notes[0].contains("尚未启用"), "未启用 → 指向设置页");
    assert!(notes[0].contains("不要装作已搜索"), "必须带不编造纪律");
    drop(path);
}

#[test]
fn web_search_intent_enabled_deferred_points_at_enable_tools() {
    let (store, path) = agent_store();
    store
        .save_web_search_config(true, "tavily", "", "k-test", 5)
        .unwrap();
    let registry = agent_registry(true);
    let mut context = vec![ContextMessage::new("user", "上网查查这个模型的参数规模")];
    crate::ai::conversation::note_web_search_intent(&mut context, &registry, &store);
    let notes = intent_system_notes(&context);
    assert_eq!(notes.len(), 1);
    assert!(
        notes[0].contains("enable_tools"),
        "已启用但延迟 → 指向 enable_tools 登记"
    );
    drop(path);
}

#[test]
fn web_search_intent_absent_marks_nothing() {
    let (store, path) = agent_store();
    store
        .save_web_search_config(true, "tavily", "", "k-test", 5)
        .unwrap();
    let registry = agent_registry(true);
    // 否定词优先 + 无触发词的两类文本都不打扰
    let mut context = vec![ContextMessage::new("user", "别搜了，先帮我整理一下笔记")];
    crate::ai::conversation::note_web_search_intent(&mut context, &registry, &store);
    assert!(intent_system_notes(&context).is_empty(), "否定词优先不触发");
    let mut context = vec![ContextMessage::new("user", "整理一下今天的事件")];
    crate::ai::conversation::note_web_search_intent(&mut context, &registry, &store);
    assert!(intent_system_notes(&context).is_empty(), "非意图文本零注入");
    drop(path);
}

#[test]
fn web_search_intent_enabled_eager_protocol_injects_nothing() {
    let (store, path) = agent_store();
    store
        .save_web_search_config(true, "tavily", "", "k-test", 5)
        .unwrap();
    let registry = agent_registry(false);
    let mut context = vec![ContextMessage::new("user", "搜一下今天的新闻")];
    crate::ai::conversation::note_web_search_intent(&mut context, &registry, &store);
    assert!(
        intent_system_notes(&context).is_empty(),
        "常驻注入协议下模型本可直接调，无需提示"
    );
    drop(path);
}
