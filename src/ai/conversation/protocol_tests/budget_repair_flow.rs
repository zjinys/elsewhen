use super::super::super::*;
use super::super::continuity_tests::*;
use super::missing_call_reply;
use super::protocol_http_fixture;
use super::*;

#[test]
fn short_new_conversation_fits_conservative_32k_budget() {
    let question = "明天就放国庆的假了，我其实挺讨厌放这种假的";
    let (url, server) = protocol_http_fixture(vec![(
        200,
        json!({"choices":[{"message":{"content":"你说讨厌这种假，是调休打乱了节奏，还是假期本身让你不自在？"},"finish_reason":"stop"}]}),
    )]);
    let (store, path) = temporary_database();
    store
        .upsert_ai_provider_config(&url, "fixture", "fixture-key")
        .unwrap();
    let conv = store.create_conversation(None, None).unwrap();
    store.send_message(&conv, "user", question, None).unwrap();
    generate_conversation_reply(&conv, &store, None).unwrap();
    let requests = server.join().unwrap();
    let request = &requests[0];
    let count = crate::ai::budget::input_tokens("unknown", request);
    eprintln!(
        "short conversation conservative input={count}, tools_bytes={}",
        request["tools"].to_string().len()
    );
    assert!(
        request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["role"] == "user" && m["content"] == question)
    );
    drop(store);
    let _ = std::fs::remove_file(path);
    // 生成过程本身可能因歷史+固定段贴线而报 RequestBudgetError（见
    // generate 校验零余量），本断言守护的是：短会话全量请求不得远超预算，
    // 且工具清单自身不膨胀：tools JSON 估算 token 有硬上限（千级膨胀
    // 会在此显式失败）。常规短会话的请求大头是 tools+系统提示。
    let limit = crate::ai::budget::input_limit(32768, 4096).unwrap() + 4096;
    if count > limit {
        panic!("short input exceeds 32K request budget: {count} > {limit}");
    }
    assert!(!request.to_string().contains("可用工具（name：用途）"));
    let messages = request["messages"].as_array().unwrap();
    assert_eq!(
        messages
            .iter()
            .filter(|m| m["content"]
                .as_str()
                .is_some_and(|c| c.starts_with("<current_time>")))
            .count(),
        1
    );
    assert!(messages.iter().any(|m| {
        m["content"]
            .as_str()
            .is_some_and(|c| c.contains("User sent:"))
    }));
    for message in messages {
        assert!(message.get("message_time").is_none());
        assert!(message.get("sampled_time").is_none());
        assert!(message.get("time_reminder").is_none());
    }
}

#[test]
fn local_budget_error_does_not_retry_as_text_or_pollute_context() {
    struct BudgetFailure(std::cell::Cell<usize>);
    impl AiProvider for BudgetFailure {
        fn generate_reply_with_tools(
            &self,
            _: Vec<ContextMessage>,
            _: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            self.0.set(self.0.get() + 1);
            Err(crate::ai::budget::RequestBudgetError("fixture budget exceeded".into()).into())
        }
    }
    let (store, path) = temporary_database();
    let conv = store.create_conversation(None, None).unwrap();
    let mut context = vec![ContextMessage::new(
        "user",
        "明天就放国庆的假了，我其实挺讨厌放这种假的",
    )];
    let before = context[0].content.clone();
    let provider = BudgetFailure(std::cell::Cell::new(0));
    let error = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .err()
    .unwrap();
    assert!(
        error
            .downcast_ref::<crate::ai::budget::RequestBudgetError>()
            .is_some()
    );
    assert_eq!(provider.0.get(), 1);
    assert_eq!(context.len(), 1);
    assert_eq!(context[0].content, before);
    let next = RecordingProvider::new(vec![AiReply::text("听起来这种假期并不让你期待。")]);
    run_agent_loop(
        &next,
        "test-provider",
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert!(next.requests.borrow()[0].1);
    assert!(
        !next.requests.borrow()[0]
            .0
            .iter()
            .any(|m| m.content == TEXT_PROTOCOL_NUDGE)
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn short_main_stream_input_survives_424_with_32k_fallback() {
    let (failed_url, failed_server) = protocol_http_fixture(vec![(
        424,
        json!({"error":{"code":"bad_response_status_code","type":"upstream_error"}}),
    )]);
    let answer = "这次放假好像并没有让你轻松下来。";
    let (url, server) = protocol_http_fixture(vec![(
        200,
        json!({"choices":[{"message":{"content":answer},"finish_reason":"stop"}]}),
    )]);
    let (store, path) = temporary_database();
    let first = store
        .save_ai_provider_config(
            None,
            "first",
            "openai-compatible",
            &failed_url,
            "fixture",
            "fixture-key",
            0.7,
            Some(4096),
        )
        .unwrap();
    let fallback = store
        .save_ai_provider_config(
            None,
            "fallback",
            "openai-compatible",
            &url,
            "fixture",
            "fixture-key",
            0.7,
            Some(4096),
        )
        .unwrap();
    store.set_active_ai_provider_config(&first).unwrap();
    let conv = store
        .create_conversation(Some("主对话流"), Some("diary"))
        .unwrap();
    let question = "明天就放国庆的假了，我其实挺讨厌放这种假的";
    store
        .submit_conversation_input(&conv, question, Some("holiday-budget-fixture"))
        .unwrap();
    assert_eq!(
        generate_conversation_reply(&conv, &store, None).unwrap(),
        answer
    );
    let failed = failed_server.join().unwrap();
    let requests = server.join().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(requests.len(), 1);
    assert_eq!(failed[0]["messages"], requests[0]["messages"]);
    assert!(requests[0]["tools"].is_array());
    assert!(crate::ai::budget::input_tokens("fixture", &requests[0]) <= 27034);
    assert!(!requests[0].to_string().contains("record_event"));
    // failover 落点现在写 ai_provider_runtime_active（本机事实，迁移 55），
    // 不再改写 is_active——is_active 只表达用户显式激活。校验运行时落点。
    let runtime_head: String = store
        .connection
        .query_row(
            "SELECT config_id FROM ai_provider_runtime_active WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(runtime_head, fallback);
    assert_eq!(store.list_events().unwrap().len(), 1);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_call_recovers_through_text_tools_without_duplicate_drafts() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "整理视频方案并存进知识库")];
    let provider = RecordingProvider::new(vec![
        missing_call_reply(),
        AiReply::text(
            "[工具调用]{\"name\":\"save_knowledge_draft\",\"arguments\":{\"title\":\"视频方案\",\"content_md\":\"固定镜头，保持纹理。\"}}",
        ),
        AiReply::text("视频方案草稿已整理，确认后保存。"),
    ]);
    let result = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(result.content, "视频方案草稿已整理，确认后保存。");
    assert_eq!(result.rounds_used, 3);
    assert_eq!(result.empty_retries, 0);
    assert_eq!((result.prompt_tokens, result.completion_tokens), (200, 43));
    let requests = provider.requests.borrow();
    assert_eq!(
        requests.iter().map(|r| r.1).collect::<Vec<_>>(),
        vec![true, false, false]
    );
    assert_eq!(
        requests[0]
            .0
            .iter()
            .filter(|m| m.content.starts_with("可用工具（name：用途）"))
            .count(),
        0
    );
    for request in &requests[1..] {
        assert_eq!(
            request
                .0
                .iter()
                .filter(|m| m.content.starts_with("可用工具（name：用途）"))
                .count(),
            1
        );
    }
    assert!(
        requests[1]
            .0
            .iter()
            .any(|m| m.content == TEXT_PROTOCOL_NUDGE)
    );
    assert!(
        requests[2]
            .0
            .iter()
            .any(|m| m.content.contains("视频方案") && m.role == "system")
    );
    assert_eq!(
        store.pending_actions_for_conversation(&conv).unwrap().len(),
        1
    );
    assert!(store.list_wiki_pages(None, None).unwrap().is_empty());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn repeated_missing_calls_finalize_in_three_requests_with_original_task() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let request = "完善视频方案，保存完整草稿";
    store.send_message(&conv, "user", request, None).unwrap();
    let mut context = vec![ContextMessage::new("user", request)];
    let provider = RecordingProvider::new(vec![missing_call_reply(); 3]);
    let result = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(result.rounds_used, 3);
    assert_eq!((result.prompt_tokens, result.completion_tokens), (600, 129));
    assert!(result.content.contains("完善视频方案"));
    assert!(!result.content.contains("模型"));
    assert!(!future_promise_detected(&result.content));
    let requests = provider.requests.borrow();
    assert_eq!(
        requests.iter().map(|r| r.1).collect::<Vec<_>>(),
        vec![true, false, false]
    );
    assert!(
        requests[2]
            .0
            .iter()
            .any(|m| m.content == FINAL_ANSWER_NUDGE)
    );
    assert!(
        store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty()
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}
