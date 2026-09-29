// Included in conversation::tests to reuse the isolated Store and provider fixtures.

struct RecordingProvider {
    replies: std::cell::RefCell<VecDeque<AiReply>>,
    requests: std::cell::RefCell<Vec<(Vec<ContextMessage>, bool)>>,
}

impl RecordingProvider {
    fn new(replies: Vec<AiReply>) -> Self {
        Self {
            replies: std::cell::RefCell::new(replies.into()),
            requests: Default::default(),
        }
    }
}

impl AiProvider for RecordingProvider {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        self.requests.borrow_mut().push((messages, tools.is_some()));
        Ok(self
            .replies
            .borrow_mut()
            .pop_front()
            .expect("unexpected extra model request"))
    }
}

fn native_reply(content: &str, name: &str, args: Value) -> AiReply {
    AiReply {
        content: content.into(),
        tool_calls: vec![ToolCall::new(name, args)],
        model: None,
        usage: None,
        reasoning_content: None,
        finish_reason: None,
    }
}

#[test]
fn read_only_request_cannot_end_with_repeated_future_promises() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "分析这份发布方案")];
    let provider = RecordingProvider::new(vec![
        AiReply::text("我去处理，一会给你回复。"),
        AiReply::text("请稍等，我先去查一下。"),
        AiReply::text("稍后给你结果。"),
    ]);
    let result = run_agent_loop(
        &provider,
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert!(result.content.contains("发布方案"));
    assert!(result.content.contains("还没有完成"));
    assert!(!future_promise_detected(&result.content));
    assert!(!result.content.contains("模型"));
    assert!(
        !provider.requests.borrow().last().unwrap().1,
        "final request must disable tools"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn echoed_empty_model_errors_do_not_replace_a_progress_answer() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    for (role, text) in [
        ("user", "改进这份视频 prompt"),
        ("assistant", "我去做，一会给你回复"),
        ("user", "？"),
    ] {
        store.send_message(&conv, role, text, None).unwrap();
    }
    let mut context = vec![ContextMessage::new("user", "？")];
    let provider = RecordingProvider::new(vec![
        AiReply::text("抱歉，模型回复了空内容。"),
        AiReply::text("模型没有返回内容，请再发一次。"),
        AiReply::text("抱歉，模型返回空内容。"),
    ]);
    let result = run_agent_loop(
        &provider,
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert!(result.content.contains("改进这份视频 prompt"));
    assert!(!result.content.contains("模型"));
    assert!(!result.content.contains("请再发"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn question_marks_recover_original_task_across_legacy_empty_replies_and_restart() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let original = "合并这两份视频 prompt，保留稳定镜头并生成派生稿";
    for (role, text) in [
        ("user", original),
        ("assistant", "我准备整理一下，稍等片刻。"),
        ("user", "？"),
        ("assistant", "抱歉，模型没有返回内容，请重试一次。"),
        ("user", "搞定了吗？"),
    ] {
        store.send_message(&conv, role, text, None).unwrap();
    }
    drop(store);
    let store = Store::open(&path).unwrap();
    let followup = FollowUp::load(&store, &conv).unwrap().unwrap();
    assert_eq!(followup.request, original);
    assert!(followup.unresolved);
    let fallback = progress_fallback(
        &store,
        &conv,
        &[ContextMessage::new("user", "搞定了吗？")],
        &[],
    )
    .unwrap();
    assert!(fallback.contains(original));
    assert!(!fallback.contains("请把"));
    assert!(!fallback.contains("模型"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn progress_recovery_handles_old_failure_variants_and_unanswered_new_requests() {
    let (store, path) = temporary_database();
    for failure in [
        "抱歉，模型回复了空内容。",
        "模型返回了空内容，请再试一次。",
        "模型返回空内容。",
    ] {
        let conv = store.create_conversation(Some("test"), None).unwrap();
        for (role, text) in [
            ("user", "改进这份视频 prompt"),
            ("assistant", failure),
            ("user", "？"),
        ] {
            store.send_message(&conv, role, text, None).unwrap();
        }
        assert!(FollowUp::load(&store, &conv).unwrap().unwrap().unresolved);
    }
    let conv = store.create_conversation(Some("test"), None).unwrap();
    for (role, text) in [
        ("user", "解释什么是固定镜头"),
        ("assistant", "固定镜头是保持机位不动进行拍摄。"),
        ("user", "帮我改进这份新的视频 prompt"),
        ("user", "？"),
    ] {
        store.send_message(&conv, role, text, None).unwrap();
    }
    let followup = FollowUp::load(&store, &conv).unwrap().unwrap();
    assert_eq!(followup.request, "帮我改进这份新的视频 prompt");
    assert!(followup.unresolved, "旧任务的答复不能算作新任务已完成");
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn unrelated_question_or_cancel_does_not_restart_an_old_task() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    store.send_message(&conv, "user", "？", None).unwrap();
    assert!(FollowUp::load(&store, &conv).unwrap().is_none());
    for (role, text) in [
        ("user", "帮我整理原文"),
        ("assistant", "请稍等"),
        ("user", "不用了"),
        ("user", "？"),
    ] {
        store.send_message(&conv, role, text, None).unwrap();
    }
    assert!(FollowUp::load(&store, &conv).unwrap().is_none());
    store
        .send_message(&conv, "user", "解释什么是 SQLite？", None)
        .unwrap();
    assert!(FollowUp::load(&store, &conv).unwrap().is_none());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn status_of_a_previously_saved_page_is_verified_since_the_original_request() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    store
        .send_message(&conv, "user", "保存这份测试笔记", None)
        .unwrap();
    crate::wiki::save_text_page("已保存正文", Some("测试笔记"), &[], &store).unwrap();
    store
        .send_message(&conv, "assistant", "已保存「测试笔记」。", None)
        .unwrap();
    store
        .send_message(&conv, "user", "搞定了吗？", None)
        .unwrap();
    assert!(!FollowUp::load(&store, &conv).unwrap().unwrap().unresolved);
    let mut context = vec![ContextMessage::new("user", "搞定了吗？")];
    let provider =
        RecordingProvider::new(vec![AiReply::text("已保存「测试笔记」，之前就完成了。")]);
    let result = run_agent_loop(
        &provider,
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert!(result.content.contains("之前就完成"));
    assert_eq!(provider.requests.borrow().len(), 1);
    assert_eq!(store.list_wiki_pages(None, None).unwrap().len(), 1);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn unverified_old_save_claim_is_not_treated_as_a_completed_task() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    for (role, text) in [
        ("user", "保存成测试笔记"),
        ("assistant", "已保存「测试笔记」。"),
        ("user", "？"),
    ] {
        store.send_message(&conv, role, text, None).unwrap();
    }
    assert!(FollowUp::load(&store, &conv).unwrap().unwrap().unresolved);
    let result = progress_fallback(&store, &conv, &[], &[]).unwrap();
    assert!(!result.contains("已保存"));
    assert!(result.contains("还没有完成"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn native_intermediate_body_is_echoed_and_last_round_finalizes() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "查一下规则")];
    let mut first = native_reply("已经整理出的要点需要结合规则检查", "list_rules", json!({}));
    first.reasoning_content = Some("provider-private-reasoning".into());
    let provider = RecordingProvider::new(vec![
        first,
        native_reply("", "list_rules", json!({})),
        native_reply("", "list_rules", json!({})),
        AiReply::text("已经查完，目前没有规则。"),
    ]);
    let result = run_agent_loop(
        &provider,
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    let requests = provider.requests.borrow();
    let echoed = requests[1]
        .0
        .iter()
        .find(|m| m.tool_calls.is_some())
        .unwrap();
    assert_eq!(echoed.content, "已经整理出的要点需要结合规则检查");
    assert_eq!(
        echoed.reasoning_content.as_deref(),
        Some("provider-private-reasoning")
    );
    assert_eq!(requests.len(), MAX_TOOL_ROUNDS);
    assert!(!requests.last().unwrap().1);
    assert_eq!(result.content, "已经查完，目前没有规则。");
    assert!(!result.content.contains("private-reasoning"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn finalization_never_dispatches_native_or_text_writes() {
    for native in [true, false] {
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("test"), None).unwrap();
        let mut context = vec![ContextMessage::new("user", "查一下规则")];
        let final_reply = if native {
            native_reply(
                "已保存",
                "save_knowledge_draft",
                json!({"title":"unexpected", "content_md":"no"}),
            )
        } else {
            AiReply::text("[工具调用]{\"name\":\"save_knowledge_draft\",\"arguments\":{\"title\":\"unexpected\",\"content_md\":\"no\"}}")
        };
        let provider = RecordingProvider::new(vec![
            native_reply("", "list_rules", json!({})),
            native_reply("", "list_rules", json!({})),
            native_reply("", "list_rules", json!({})),
            final_reply,
        ]);
        let result = run_agent_loop(
            &provider,
            &mut context,
            &ToolRegistry::default(),
            &store,
            &conv,
            chrono::Utc::now(),
        )
        .unwrap();
        assert!(store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty());
        assert!(!result.content.trim().is_empty());
        assert!(!result.content.contains("已保存"));
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn protected_material_rejects_an_impossible_revision_before_creating_pending() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let page = crate::wiki::save_text_page("原文", Some("视频原料"), &[], &store).unwrap();
    let args = json!({"slug":page.slug, "title":"改进稿", "content_md":"新内容", "change_note":"改进", "save_as":"revision"});
    let registry = ToolRegistry::default();
    let rejected = dispatch(
        &ToolCall::new("save_wiki_revision", args.clone()),
        &registry,
        &store,
        &conv,
    );
    assert!(!rejected.success);
    assert!(rejected.content.contains("derivative"));
    assert!(store
        .pending_actions_for_conversation(&conv)
        .unwrap()
        .is_empty());
    let mut allowed = args;
    allowed["save_as"] = json!("derivative");
    let drafted = dispatch(
        &ToolCall::new("save_wiki_revision", allowed),
        &registry,
        &store,
        &conv,
    );
    assert!(drafted.success);
    assert_eq!(
        store.pending_actions_for_conversation(&conv).unwrap().len(),
        1
    );
    assert_eq!(
        store.get_wiki_page(&page.slug).unwrap().unwrap().content_md,
        "原文"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn status_question_shows_existing_draft_without_confirming_or_duplicating_it() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    for (role, text) in [
        ("user", "帮我把视频 prompt 保存成派生稿"),
        ("assistant", "请稍等"),
        ("user", "？"),
    ] {
        store.send_message(&conv, role, text, None).unwrap();
    }
    store
        .create_pending_action(
            &conv,
            "save_knowledge_draft",
            &json!({"title":"视频草稿", "content_md":"完整视频草稿正文"}).to_string(),
        )
        .unwrap();
    let (summary, wrote) = handle_pending_action_confirmation(&store, &conv).unwrap();
    assert!(summary.is_none());
    assert!(!wrote);
    let mut context = vec![ContextMessage::new("user", "？")];
    let provider = RecordingProvider::new(vec![
        native_reply(
            "",
            "save_knowledge_draft",
            json!({"title":"重复草稿", "content_md":"no"}),
        ),
        AiReply::text(""),
        AiReply::text(""),
        AiReply::text(""),
    ]);
    let result = run_agent_loop(
        &provider,
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(
        store.pending_actions_for_conversation(&conv).unwrap().len(),
        1
    );
    assert!(store.list_wiki_pages(None, None).unwrap().is_empty());
    assert!(result.content.contains("完整视频草稿正文"));
    assert!(result.content.contains("尚未执行保存"));
    assert!(!result.content.contains("模型"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn successful_confirmation_survives_failure_to_generate_final_words() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    store
        .create_pending_action(
            &conv,
            "save_knowledge_draft",
            &json!({"title":"测试知识", "content_md":"有用内容"}).to_string(),
        )
        .unwrap();
    store.send_message(&conv, "user", "好", None).unwrap();
    let (summary, wrote) = handle_pending_action_confirmation(&store, &conv).unwrap();
    assert!(wrote);
    let mut context = vec![
        ContextMessage::new("user", "好"),
        ContextMessage::new(
            "system",
            format!(
                "（内部记录）你刚才提议的写操作已被用户确认，执行结果如下：\n{}",
                summary.unwrap()
            ),
        ),
    ];
    let provider = RecordingProvider::new((0..5).map(|_| AiReply::text("")).collect());
    let result = run_agent_loop(
        &provider,
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(store.list_wiki_pages(None, None).unwrap().len(), 1);
    assert!(result.content.contains("测试知识"));
    assert!(!result.content.contains("还没有完成"));
    assert!(!result.content.contains("重发"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn empty_compatibility_attempt_is_counted_in_usage() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let with_usage = |body: &str, n| AiReply {
        content: body.into(),
        tool_calls: vec![],
        model: None,
        usage: Some(TokenUsage {
            prompt_tokens: n,
            completion_tokens: 10,
            total_tokens: n + 10,
        }),
        reasoning_content: None,
        finish_reason: None,
    };
    let provider = RecordingProvider::new(vec![with_usage("", 200), with_usage("这是答复", 300)]);
    let mut context = vec![ContextMessage::new("user", "你好")];
    let result = run_agent_loop(
        &provider,
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!((result.prompt_tokens, result.completion_tokens), (500, 20));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn fallback_renders_found_knowledge_without_exposing_protocol_json() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let page =
        crate::wiki::save_text_page("固定机位，保留纹理", Some("视频原料"), &[], &store).unwrap();
    let mut context = vec![ContextMessage::new("user", "看看视频原料")];
    let provider = RecordingProvider::new(vec![
        native_reply("", "get_wiki_page", json!({"slug":page.slug})),
        AiReply::text(""),
        AiReply::text(""),
        AiReply::text(""),
    ]);
    let result = run_agent_loop(
        &provider,
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert!(result.content.contains("固定机位，保留纹理"));
    assert!(!result.content.contains("knowledge_candidates"));
    assert!(!result.content.contains("snapshot_id"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn followup_full_request_reaches_http_provider_after_memory_compression() {
    use std::io::{BufRead, Read, Write};
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let original = format!(
        "请改进视频 prompt，保留以下要求：{}",
        "保持稳定镜头和刺绣纹理。".repeat(80)
    );
    for (role, text) in [
        ("user", original.as_str()),
        ("assistant", "我去处理，一会给你回复。"),
        ("user", "？"),
    ] {
        store.send_message(&conv, role, text, None).unwrap();
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    store
        .upsert_ai_provider_config(
            &format!("http://{}/v1", listener.local_addr().unwrap()),
            "fixture",
            "test-key",
        )
        .unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse::<usize>().unwrap();
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let request: Value = serde_json::from_slice(&body).unwrap();
        let response = json!({"model":"fixture", "choices":[{"message":{"content":"上一条还没有交付。这是保留稳定镜头后的完整改进稿：固定机位，逐步形成刺绣纹理。"}}]}).to_string();
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
        request
    });
    let answer = generate_conversation_reply(
        &conv,
        &store,
        Some(ConversationConfig {
            memory_type: MemoryType::SlidingWindow { max_tokens: 32 },
            provider_type: ProviderType::OpenAiCompatible,
        }),
    )
    .unwrap();
    let request = server.join().unwrap();
    let recalled = request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| {
            m["content"]
                .as_str()
                .unwrap_or("")
                .contains("original_request")
        })
        .unwrap();
    assert!(recalled["content"].as_str().unwrap().contains(&original));
    assert!(answer.contains("完整改进稿"));
    assert!(store
        .list_messages(&conv)
        .unwrap()
        .iter()
        .any(|m| m.role == "user" && m.content == "？"));
    drop(store);
    let _ = std::fs::remove_file(path);
}
