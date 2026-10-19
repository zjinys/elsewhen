use super::super::super::*;
use super::native_reply;
use super::*;

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
        "test-provider",
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
        "test-provider",
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
        None,
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
fn empty_retry_preserves_native_protocol_and_counts_both_requests() {
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
            prompt_cached_tokens: None,
        }),
        reasoning_content: None,
        finish_reason: None,
    };
    let provider = RecordingProvider::new(vec![with_usage("", 200), with_usage("这是答复", 300)]);
    let mut context = vec![ContextMessage::new("user", "你好")];
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
    assert_eq!((result.prompt_tokens, result.completion_tokens), (500, 20));
    assert_eq!(result.rounds_used, 2);
    assert!(provider.requests.borrow().iter().all(|(_, tools)| *tools));
    drop(store);
    let _ = std::fs::remove_file(path);
}

/// 事故 2026-10-18 根因 A：run 没能生成答复时，知识检索结果曾被拼成
/// 「已取得的结果：…」顶包。自动兜底必须只产诚实失败消息。
#[test]
fn automatic_failure_fallback_never_passes_tool_results_off_as_an_answer() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let page =
        crate::wiki::save_text_page("糖尿病足预警方案正文", Some("病程数据"), &[], &store).unwrap();
    let mut context = vec![ContextMessage::new("user", "糖尿病足预警这个具体怎么做呢")];
    let provider = RecordingProvider::new(vec![
        native_reply("", "get_wiki_page", json!({"slug": page.slug})),
        AiReply::text(""),
        AiReply::text(""),
        AiReply::text(""),
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
    // 点名原请求 + 承认没完成，不把检索到的整页内容当成回答
    assert!(result.content.contains("糖尿病足预警这个具体怎么做呢"));
    assert!(result.content.contains("这一轮还没有完成"));
    assert!(!result.content.contains("糖尿病足预警方案正文"));
    assert!(!result.content.contains("knowledge_candidates"));
    assert!(!result.content.contains("snapshot_id"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

/// 事故 2026-10-18 P1-A：只有用户显式追问进展时，才允许把工具结果
/// 作为已知材料汇报；自动触发永远只产诚实失败消息。
#[test]
fn only_explicit_progress_questions_may_report_known_results() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    store
        .send_message(&conv, "user", "把发布清单整理一版", None)
        .unwrap();
    let call = ToolCall::new("search_notes", json!({"query": "发布清单"}));
    let results = vec![ToolResultMsg::ok(&call, "发布清单的检查要点正文".into())];
    let automatic = progress_fallback(
        &store,
        &conv,
        &[ContextMessage::new("user", "把发布清单整理一版")],
        &results,
        Some("生成没有产出可用答复。"),
    )
    .unwrap();
    assert!(automatic.contains("把发布清单整理一版"));
    assert!(automatic.contains("这一轮还没有完成"));
    assert!(!automatic.contains("发布清单的检查要点正文"));
    store
        .send_message(&conv, "user", "搞定了吗？", None)
        .unwrap();
    let explicit = progress_fallback(
        &store,
        &conv,
        &[ContextMessage::new("user", "搞定了吗？")],
        &results,
        None,
    )
    .unwrap();
    assert!(explicit.contains("发布清单的检查要点正文"));
    assert!(!explicit.contains("这一轮还没有完成"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

/// 事故 2026-10-18 根因 B：用户确认后的失败必须点名「答应的事没做到」。
#[test]
fn failure_notice_quotes_request_and_admits_unfulfilled_promise() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    for (role, text) in [
        ("user", "把 ESP32 糖尿病监测的思路整理进产品灵感页"),
        ("assistant", "我这就把这块整理成一条灵感记录。"),
        ("user", "好"),
    ] {
        store.send_message(&conv, role, text, None).unwrap();
    }
    let notice = failure_notice(&store, &conv, "本轮必要上下文超过预算；请缩小任务范围").unwrap();
    assert!(notice.contains("把 ESP32 糖尿病监测的思路整理进产品灵感页"));
    assert!(notice.contains("这一轮还没有完成"));
    assert!(notice.contains("同样没有做到"));
    assert!(!notice.contains("已保存"));
    // 失败说明之后追问进展，必须判定为还没完成并接上原任务
    assert!(FollowUp::load(&store, &conv).unwrap().is_none());
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
    // This fixture tests original-request recovery, not a long completion.
    // Keep its output reserve explicit; the separate 32K regression covers
    // default-budget requests with the required time context included.
    store
        .save_ai_provider_config(
            None,
            "default",
            "openai-compatible",
            &format!("http://{}/v1", listener.local_addr().unwrap()),
            "fixture",
            "test-key",
            0.7,
            Some(512),
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
    assert!(
        store
            .list_messages(&conv)
            .unwrap()
            .iter()
            .any(|m| m.role == "user" && m.content == "？")
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}
