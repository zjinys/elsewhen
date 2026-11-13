use super::super::super::*;
use super::native_reply;
use super::*;

/// 只报告固定余量的 provider：模拟上下文快满的一轮。
struct TightProvider {
    inner: RecordingProvider,
    headroom: usize,
}

impl AiProvider for TightProvider {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        self.inner.generate_reply_with_tools(messages, tools)
    }

    fn input_headroom(
        &self,
        _: &[ContextMessage],
        _: Option<&[ToolSpec]>,
    ) -> Result<Option<(usize, String)>> {
        Ok(Some((self.headroom, "fixture".into())))
    }
}

#[test]
fn tool_results_are_sized_to_the_headroom_measured_before_dispatch() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let body = "需要分段读取的长手册正文。".repeat(1500);
    let at = chrono::Utc::now();
    let event = store
        .insert_event(crate::event::NewEvent {
            raw_text: &body,
            occurred_at: at,
            recorded_at: at,
            source: "capture",
        })
        .unwrap();
    store
        .upsert_wiki_page(
            &crate::storage::WikiPageDraft {
                slug: "method/manual".into(),
                title: "长手册".into(),
                content_md: body.clone(),
                summary: String::new(),
                kind: "method".into(),
                tags: vec![],
                source_event_ids: vec![event],
                source_url: None,
                status: "active".into(),
                reason: "fixture".into(),
            },
            crate::storage::ContentPolicy::Always,
        )
        .unwrap();
    let replies = || {
        vec![
            native_reply("", "get_wiki_page", json!({"slug":"method/manual"})),
            AiReply::text("手册要求分段读取。"),
        ]
    };
    // 跑一轮，取回工具结果原文。
    let read = |provider: &dyn AiProvider| -> String {
        let mut context = vec![ContextMessage::new("user", "长手册里怎么说")];
        run_agent_loop(
            provider,
            "test-provider",
            &mut context,
            &ToolRegistry::default(),
            &store,
            &conv,
            chrono::Utc::now(),
        )
        .unwrap();
        context
            .iter()
            .find(|message| message.role == "tool")
            .expect("tool result")
            .content
            .clone()
    };
    let end_char = |content: &str| -> u64 {
        let value: Value = serde_json::from_str(content).unwrap();
        value["knowledge_reading"]["method/manual"]["end_char"]
            .as_u64()
            .unwrap()
    };

    // 余量 8048：留出 2048 的后续余量后，这次结果至多占 6000。
    let tight = TightProvider {
        inner: RecordingProvider::new(replies()),
        headroom: 8048,
    };
    let content = read(&tight);
    let end = end_char(&content);
    assert!((1500..8000).contains(&end), "{end}");
    assert!(
        crate::ai::budget::text_tokens("fixture", &Value::String(content.clone()).to_string())
            <= 6000
    );
    let value: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(
        value["knowledge_reading"]["method/manual"]["next_offset"],
        end
    );

    // 不报余量的 provider：行为不变，返回 8000 字符。
    let plain = RecordingProvider::new(replies());
    assert_eq!(end_char(&read(&plain)), 8000);

    drop(store);
    let _ = std::fs::remove_file(path);
}

/// 连续翻页的模型：每轮从上一段的 next_offset 接着读，三轮后作答。余量由
/// 真实的 OpenAI 兼容 provider 按 64K 窗口和聊天输出预留量出（不发请求）。
struct PagingProvider {
    real: OpenAiCompatibleProvider,
    reads: std::cell::Cell<usize>,
    hash: String,
}

impl AiProvider for PagingProvider {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        if self.reads.get() == 3 {
            return Ok(AiReply::text("读到的部分要求分段读取。"));
        }
        self.reads.set(self.reads.get() + 1);
        let next = messages
            .iter()
            .rev()
            .filter(|message| message.role == "tool")
            .find_map(|message| serde_json::from_str::<Value>(&message.content).ok())
            .and_then(|value| value["knowledge_reading"]["method/manual"]["next_offset"].as_u64());
        Ok(native_reply(
            "",
            "get_wiki_page",
            match next {
                Some(offset) => {
                    json!({"slug":"method/manual","offset":offset,"content_hash":self.hash})
                }
                None => json!({"slug":"method/manual"}),
            },
        ))
    }

    fn input_headroom(
        &self,
        messages: &[ContextMessage],
        tools: Option<&[ToolSpec]>,
    ) -> Result<Option<(usize, String)>> {
        self.real.input_headroom(messages, tools)
    }
}

#[test]
fn consecutive_page_reads_leave_a_64k_request_that_still_fits() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let body = "五万字手册里的一句正文。".repeat(4200);
    let at = chrono::Utc::now();
    let event = store
        .insert_event(crate::event::NewEvent {
            raw_text: "手册来源",
            occurred_at: at,
            recorded_at: at,
            source: "capture",
        })
        .unwrap();
    store
        .upsert_wiki_page(
            &crate::storage::WikiPageDraft {
                slug: "method/manual".into(),
                title: "长手册".into(),
                content_md: body.clone(),
                summary: String::new(),
                kind: "method".into(),
                tags: vec![],
                source_event_ids: vec![event],
                source_url: None,
                status: "active".into(),
                reason: "fixture".into(),
            },
            crate::storage::ContentPolicy::Always,
        )
        .unwrap();
    let model = "deepseek-v4-flash";
    let output = crate::ai::budget::chat_output(model);
    let provider = PagingProvider {
        real: OpenAiCompatibleProvider::new(crate::ai::provider::OpenAiCompatibleConfig {
            model: model.into(),
            ..Default::default()
        })
        .unwrap()
        .with_output_reserve(output),
        reads: Default::default(),
        hash: crate::storage::knowledge::content_hash(&body),
    };
    // 场景成立的前提：三段各 8000 字符按估算口径放不进 64K 的输入预算。
    let limit =
        crate::ai::budget::input_limit(crate::ai::budget::default_window(model), output).unwrap();
    let three_full_reads: String = body.chars().take(24_000).collect();
    assert!(crate::ai::budget::text_tokens(model, &three_full_reads) > limit);

    let mut context = vec![ContextMessage::new("user", "把长手册读一遍，告诉我要求")];
    run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &ToolRegistry::deferred_runtime(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();

    let results: Vec<&ContextMessage> = context.iter().filter(|m| m.role == "tool").collect();
    assert_eq!(results.len(), 3);
    let read_chars: usize = results
        .iter()
        .filter_map(|message| serde_json::from_str::<Value>(&message.content).ok())
        .map(|value| {
            value["knowledge_candidates"][0]["excerpt"]
                .as_str()
                .map_or(0, |text| text.chars().count())
        })
        .sum();
    // 读到了实质内容，但没有把三段 8000 字符都塞进去。
    assert!((8000..24_000).contains(&read_chars), "{read_chars}");
    // 没有哪段是事后被预算层截断的。
    assert!(results.iter().all(|m| {
        !m.content
            .contains(crate::ai::budget::TOOL_RESULT_HALVED_SUFFIX)
    }));
    // 收尾请求（含三段结果与收尾指令）原样放得下，不需要任何裁剪。
    let (headroom, _) = provider
        .real
        .input_headroom(&context, None)
        .unwrap()
        .unwrap();
    assert!(headroom > 0, "收尾请求超出输入预算");

    drop(store);
    let _ = std::fs::remove_file(path);
}
