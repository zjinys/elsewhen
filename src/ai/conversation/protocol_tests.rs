// Included after continuity_tests.rs to share the recording provider fixture.

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
    assert!(request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["role"] == "user" && m["content"] == question));
    drop(store);
    let _ = std::fs::remove_file(path);
    assert!(
        count <= 27034,
        "short input exceeds 32K request budget: {count}"
    );
    assert!(!request.to_string().contains("可用工具（name：用途）"));
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
    assert!(error
        .downcast_ref::<crate::ai::budget::RequestBudgetError>()
        .is_some());
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
    assert!(!next.requests.borrow()[0]
        .0
        .iter()
        .any(|m| m.content == TEXT_PROTOCOL_NUDGE));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn http_optional_background_is_omitted_without_internal_metadata() {
    let (url, server) = protocol_http_fixture(vec![(
        200,
        json!({"choices":[{"message":{"content":"回答"},"finish_reason":"stop"}]}),
    )]);
    let provider = OpenAiCompatibleProvider::new(super::super::provider::OpenAiCompatibleConfig {
        base_url: url,
        api_key: "fixture-key".into(),
        model: "fixture".into(),
        temperature: 0.7,
        max_tokens: Some(512),
        context_window: Some(4096),
    })
    .unwrap();
    let mut background = ContextMessage::new("system", "其他会话背景".repeat(1000));
    background.optional_background = true;
    let question = "当前问题全文";
    provider
        .generate_reply(vec![
            ContextMessage::new("system", "必要规则"),
            background,
            ContextMessage::new("user", question),
        ])
        .unwrap();
    let requests = server.join().unwrap();
    let serialized = requests[0].to_string();
    assert!(serialized.contains(question));
    assert!(serialized.contains("已省略"));
    assert!(!serialized.contains("其他会话背景"));
    assert!(!serialized.contains("optional_background"));
}

#[test]
fn text_fallback_does_not_offer_record_event_for_auto_recorded_input() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("主对话流"), None).unwrap();
    store
        .submit_conversation_input(&conv, "今天终于完成了整理", Some("budget-fixture"))
        .unwrap();
    let mut context = vec![ContextMessage::new("user", "今天终于完成了整理")];
    let provider = RecordingProvider::new(vec![
        missing_call_reply(),
        AiReply::text("总算完成了，听起来松了口气。"),
    ]);
    run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    let requests = provider.requests.borrow();
    let list = requests[1]
        .0
        .iter()
        .find(|m| m.content.starts_with("可用工具（name：用途）"))
        .unwrap();
    assert!(!list.content.contains("record_event"));
    assert!(list.content.contains("search_knowledge_base"));
    assert!(!requests[1].1);
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
    assert_eq!(
        store.active_ai_provider_config().unwrap().unwrap().id,
        fallback
    );
    assert_eq!(store.list_events().unwrap().len(), 1);
    drop(store);
    let _ = std::fs::remove_file(path);
}

fn missing_call_reply() -> AiReply {
    let mut reply = reply_with(Some("function_call"), "", vec![]);
    reply.usage = Some(TokenUsage {
        prompt_tokens: 200,
        completion_tokens: 43,
        total_tokens: 243,
    });
    reply
}

#[test]
fn missing_call_recovers_through_text_tools_without_duplicate_drafts() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "整理视频方案并存进知识库")];
    let provider = RecordingProvider::new(vec![
        missing_call_reply(),
        AiReply::text("[工具调用]{\"name\":\"save_knowledge_draft\",\"arguments\":{\"title\":\"视频方案\",\"content_md\":\"固定镜头，保持纹理。\"}}"),
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
    assert!(requests[1]
        .0
        .iter()
        .any(|m| m.content == TEXT_PROTOCOL_NUDGE));
    assert!(requests[2]
        .0
        .iter()
        .any(|m| m.content.contains("视频方案") && m.role == "system"));
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
    assert!(requests[2]
        .0
        .iter()
        .any(|m| m.content == FINAL_ANSWER_NUDGE));
    assert!(store
        .pending_actions_for_conversation(&conv)
        .unwrap()
        .is_empty());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn switching_after_a_native_call_preserves_results_as_text_and_does_not_replay() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "把视频方案整理保存")];
    let provider = RecordingProvider::new(vec![
        native_reply(
            "这是方案正文",
            "save_knowledge_draft",
            json!({"title":"视频方案", "content_md":"保持纹理"}),
        ),
        missing_call_reply(),
        AiReply::text("视频方案草稿待确认，尚未保存为知识页。"),
    ]);
    run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    let requests = provider.requests.borrow();
    assert_eq!(
        requests.iter().map(|r| r.1).collect::<Vec<_>>(),
        vec![true, true, false]
    );
    let text_context = &requests[2].0;
    assert!(text_context
        .iter()
        .all(|m| m.role != "tool" && m.tool_calls.is_none() && m.tool_call_id.is_none()));
    assert!(text_context
        .iter()
        .any(|m| m.content.contains("这是方案正文") && m.content.contains("save_knowledge_draft")));
    assert!(text_context
        .iter()
        .any(|m| m.role == "system" && m.content.contains("视频方案")));
    assert_eq!(
        store.pending_actions_for_conversation(&conv).unwrap().len(),
        1
    );
    assert!(store.list_wiki_pages(None, None).unwrap().is_empty());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn native_reply_during_text_fallback_does_not_reenable_native_requests() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "查规则")];
    let provider = RecordingProvider::new(vec![
        missing_call_reply(),
        native_reply("", "list_rules", json!({})),
        AiReply::text("还没有生效的个人规则。"),
    ]);
    run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    let requests = provider.requests.borrow();
    assert_eq!(
        requests.iter().map(|r| r.1).collect::<Vec<_>>(),
        vec![true, false, false]
    );
    assert!(requests[2]
        .0
        .iter()
        .all(|m| m.role != "tool" && m.tool_calls.is_none()));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn empty_reasoning_or_length_response_does_not_claim_tools_are_unsupported() {
    for reason in ["stop", "length", "content_filter"] {
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("test"), None).unwrap();
        let mut context = vec![ContextMessage::new("user", "看看方案")];
        let mut first = reply_with(Some(reason), "", vec![]);
        first.reasoning_content = Some("internal reasoning".into());
        let provider = RecordingProvider::new(vec![first, AiReply::text("方案尚需补充时间安排。")]);
        run_agent_loop(
            &provider,
            "test-provider",
            &mut context,
            &ToolRegistry::default(),
            &store,
            &conv,
            chrono::Utc::now(),
        )
        .unwrap();
        assert!(provider.requests.borrow().iter().all(|r| r.1));
        assert!(!context.iter().any(|m| m.content == TEXT_PROTOCOL_NUDGE));
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}

fn protocol_http_fixture(
    replies: Vec<(u16, Value)>,
) -> (String, std::thread::JoinHandle<Vec<Value>>) {
    use std::io::{BufRead, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, reply) in replies {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(std::time::Instant::now() < deadline, "missing HTTP request");
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(e) => panic!("fixture accept: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            requests.push(serde_json::from_slice(&body).unwrap());
            let body = reply.to_string();
            let reason = if status == 200 { "OK" } else { "Bad Request" };
            write!(stream, "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
        requests
    });
    (url, server)
}

#[test]
fn http_missing_function_call_recovers_executes_once_and_persists_answer() {
    let (url, server) = protocol_http_fixture(vec![
        (
            200,
            json!({"model":"gpt-4o", "choices":[{"message":{"role":"assistant"}, "finish_reason":"function_call"}], "usage":{"prompt_tokens":200,"completion_tokens":43,"total_tokens":243}}),
        ),
        (
            200,
            json!({"choices":[{"message":{"content":"[工具调用]{\"name\":\"save_knowledge_draft\",\"arguments\":{\"title\":\"视频方案\",\"content_md\":\"保持镜头稳定。\"}}"},"finish_reason":"stop"}]}),
        ),
        (
            200,
            json!({"choices":[{"message":{"content":"视频方案已整理为待确认草稿，确认后保存。"},"finish_reason":"stop"}]}),
        ),
    ]);
    let (store, path) = temporary_database();
    store
        .upsert_ai_provider_config(&url, "gpt-4o", "fixture-key")
        .unwrap();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    store
        .send_message(&conv, "user", "把稳定镜头的方案整理保存", None)
        .unwrap();
    let answer = generate_conversation_reply(&conv, &store, None).unwrap();
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[0]["tools"].is_array());
    for request in &requests[1..] {
        assert!(request.get("tools").is_none());
        assert!(request.get("tool_choice").is_none());
        assert!(request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["role"] != "tool" && m.get("tool_calls").is_none()));
    }
    assert_eq!(answer, "视频方案已整理为待确认草稿，确认后保存。");
    assert_eq!(
        store.list_messages(&conv).unwrap().last().unwrap().content,
        answer
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
fn http_legacy_function_call_executes_and_echoes_matching_call_id() {
    let args = "{ \"title\": \"视频方案\", \"content_md\": \"保持纹理\" }";
    let (url, server) = protocol_http_fixture(vec![
        (
            200,
            json!({"choices":[{"message":{"content":"整理镜头方案", "function_call":{"name":"save_knowledge_draft","arguments":args}}, "finish_reason":"function_call"}]}),
        ),
        (
            200,
            json!({"choices":[{"message":{"content":"视频方案草稿待确认。"},"finish_reason":"stop"}]}),
        ),
    ]);
    let (store, path) = temporary_database();
    store
        .upsert_ai_provider_config(&url, "gpt-4o", "fixture-key")
        .unwrap();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    store
        .send_message(&conv, "user", "整理视频方案并保存", None)
        .unwrap();
    assert_eq!(
        generate_conversation_reply(&conv, &store, None).unwrap(),
        "视频方案草稿待确认。"
    );
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[1]["tools"].is_array());
    let messages = requests[1]["messages"].as_array().unwrap();
    let assistant = messages
        .iter()
        .find(|m| m.get("tool_calls").is_some())
        .unwrap();
    let tool = messages.iter().find(|m| m["role"] == "tool").unwrap();
    assert_eq!(assistant["content"], "整理镜头方案");
    assert_eq!(assistant["tool_calls"][0]["function"]["arguments"], args);
    assert_eq!(assistant["tool_calls"][0]["id"], tool["tool_call_id"]);
    assert_eq!(
        store.pending_actions_for_conversation(&conv).unwrap().len(),
        1
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn http_protocol_error_after_a_write_keeps_the_draft_and_normalizes_history() {
    let (url, server) = protocol_http_fixture(vec![
        (
            200,
            json!({"choices":[{"message":{"tool_calls":[{"id":"c1","function":{"name":"save_knowledge_draft","arguments":"{\"title\":\"视频方案\",\"content_md\":\"保持纹理\"}"}}]}, "finish_reason":"tool_calls"}]}),
        ),
        (
            400,
            json!({"error":{"message":"unsupported tool messages"}}),
        ),
        (
            200,
            json!({"choices":[{"message":{"content":"视频方案草稿待确认。"}, "finish_reason":"stop"}]}),
        ),
    ]);
    let (store, path) = temporary_database();
    store
        .upsert_ai_provider_config(&url, "gpt-4o", "fixture-key")
        .unwrap();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    store
        .send_message(&conv, "user", "整理视频方案并保存", None)
        .unwrap();
    assert_eq!(
        generate_conversation_reply(&conv, &store, None).unwrap(),
        "视频方案草稿待确认。"
    );
    let requests = server.join().unwrap();
    assert!(requests[2].get("tools").is_none());
    let messages = requests[2]["messages"].as_array().unwrap();
    assert!(messages.iter().all(|m| m["role"] != "tool"
        && m.get("tool_calls").is_none()
        && m.get("tool_call_id").is_none()));
    assert!(messages
        .iter()
        .any(|m| m["role"] == "system" && m["content"].as_str().unwrap().contains("视频方案")));
    assert_eq!(
        store.pending_actions_for_conversation(&conv).unwrap().len(),
        1
    );
    assert!(store.list_wiki_pages(None, None).unwrap().is_empty());
    drop(store);
    let _ = std::fs::remove_file(path);
}
