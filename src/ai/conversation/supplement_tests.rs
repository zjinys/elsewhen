use super::*;
use serde_json::json;
use std::cell::Cell;

struct Fixture(std::path::PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct SteeringProvider<'a> {
    store: &'a Store,
    conversation: &'a str,
    run: &'a str,
    calls: Cell<usize>,
    tool: bool,
    accept: bool,
}
impl AiProvider for SteeringProvider<'_> {
    fn accepts_supplement(
        &self,
        _: &[ContextMessage],
        _: Option<&[super::super::tool::ToolSpec]>,
    ) -> Result<bool> {
        Ok(self.accept)
    }
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        _: Option<&[super::super::tool::ToolSpec]>,
    ) -> Result<AiReply> {
        let call = self.calls.get();
        self.calls.set(call + 1);
        if call == 0 {
            assert!(
                !messages
                    .iter()
                    .any(|m| m.segment.purpose == SegmentPurpose::TaskSupplement)
            );
            let input =
                self.store
                    .send_message(self.conversation, "user", "只保留项目相关部分", None)?;
            self.store
                .enqueue_chat_supplement(self.conversation, &input, self.run, "{}")?;
            if !self.tool {
                return Ok(AiReply::text("普通回答已结束。"));
            }
            let mut reply = AiReply::text("");
            reply.tool_calls.push(ToolCall {
                id: "rules-read".into(),
                name: "list_rules".into(),
                arguments: json!({}),
                raw_arguments: None,
            });
            return Ok(reply);
        }
        assert!(
            messages
                .iter()
                .any(|m| m.role == "tool" && m.tool_call_id.as_deref() == Some("rules-read"))
        );
        assert_eq!(
            messages
                .iter()
                .any(|m| m.segment.purpose == SegmentPurpose::TaskSupplement
                    && m.content.contains("只保留项目相关部分")),
            self.accept
        );
        Ok(AiReply::text("整理结果只含项目相关部分。"))
    }
}

#[test]
fn supplements_enter_only_the_request_after_a_completed_tool_exchange() {
    for (tool, accept) in [(true, true), (true, false), (false, true)] {
        let dir = Fixture(
            std::env::temp_dir().join(format!("elsewhen-steering-loop-{}", uuid::Uuid::new_v4())),
        );
        std::fs::create_dir_all(&dir.0).unwrap();
        let store = Store::open(&dir.0.join("test.db")).unwrap();
        let conversation = store.create_conversation(Some("steering"), None).unwrap();
        let input = store
            .send_message(&conversation, "user", "整理材料", None)
            .unwrap();
        let run = store
            .create_chat_run(&conversation, &input, "first", "owner")
            .unwrap();
        let mut snapshot = store.snapshot_for_input(&conversation, &input).unwrap();
        snapshot.chat_run_identity = Some(super::super::generation::RunIdentity {
            id: run.id.clone(),
            owner: "owner".into(),
        });
        let provider = SteeringProvider {
            store: &store,
            conversation: &conversation,
            run: &run.id,
            calls: Cell::new(0),
            tool,
            accept,
        };
        let mut context = vec![ContextMessage::new("user", "整理材料")];
        let result = run_agent_loop(
            &provider,
            "fixture",
            &mut context,
            &ToolRegistry::default(),
            &snapshot,
            &conversation,
            chrono::Utc::now(),
        )
        .unwrap();
        store
            .finish_chat_run(&run.id, "owner", "completed", Some(&result.content), None)
            .unwrap();
        let queued = store
            .list_chat_input_queue(&conversation, 0, 100)
            .unwrap()
            .remove(0);
        assert_eq!(
            queued.status,
            if tool && accept {
                "injected"
            } else {
                "pending"
            }
        );
        assert_eq!(provider.calls.get(), if tool { 2 } else { 1 });
        assert!(
            store
                .pending_actions_for_conversation(&conversation)
                .unwrap()
                .is_empty()
        );
        if tool && accept {
            assert!(
                store
                    .create_chat_run(&conversation, &queued.input_message_id, "duplicate", "next")
                    .is_err()
            );
            let retry = store
                .create_chat_run(&conversation, &input, "retry-original", "next")
                .unwrap();
            let mut retry_store = store.snapshot_for_input(&conversation, &input).unwrap();
            retry_store.chat_run_identity = Some(super::super::generation::RunIdentity {
                id: retry.id,
                owner: "next".into(),
            });
            let mut retry_context = vec![ContextMessage::new("user", "整理材料")];
            super::super::supplements::restore(&retry_store, &conversation, &mut retry_context)
                .unwrap();
            assert!(
                retry_context
                    .iter()
                    .any(|m| m.content.contains("只保留项目相关部分"))
            );
            assert_eq!(
                store.recent_chat_injections(&conversation).unwrap().len(),
                1
            );
        }
    }
}

/// 2026-10-18 事故「预算前移」：客户端可能整包挂载超大材料（手机 v48 曾
/// 一次挂 27 页全文）。挂载视图必须先截到上限内并如实声明；原文不受影响。
#[test]
fn giant_supplement_is_clipped_with_honest_marker_at_mount() {
    let input = crate::storage::chat_supplements::ChatSupplement {
        queue_id: 1,
        input_id: "m1".into(),
        content: "素材段落，包含术语定义与出处标注。".repeat(8000),
        created_at: "2026-10-18T00:00:00Z".into(),
        basis: String::new(),
        context_json: "{}".into(),
    };
    let original_chars = input.content.chars().count();
    let path = std::env::temp_dir().join(format!(
        "elsewhen-supplement-mount-test-{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = crate::storage::Store::open(&path).unwrap();
    let message = super::super::supplements::context(&store, &input);
    let content = message.content;
    assert!(
        content.chars().count() < original_chars,
        "挂载视图必须小于原文（截断发生）"
    );
    assert!(content.contains("已按挂载上限截断"), "截断必须带如实声明");
    // 常规尺寸不受影响
    let small = crate::storage::chat_supplements::ChatSupplement {
        content: "把报告只保留项目相关部分。".into(),
        ..input.clone()
    };
    let small_message = super::super::supplements::context(&store, &small);
    assert!(small_message.content.contains("把报告只保留项目相关部分。"));
    assert!(!small_message.content.contains("已按挂载上限截断"));
}

/// 2026-10-18「预算前移」：引用式补充材料（消息开头 @wiki(slug) 行）在挂载
/// 时展开为每页 AI 摘要 + get_wiki_page 取回路径，不再携带全文；引用不存在
/// 的页面就地如实声明。
#[test]
fn page_reference_supplement_expands_summaries_and_discloses_missing() {
    let path = std::env::temp_dir().join(format!(
        "elsewhen-supplement-ref-test-{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = crate::storage::Store::open(&path).unwrap();
    let mut body = "手册要点：先校准，再装配。\n".to_string();
    for i in 0..40 {
        body.push_str(&format!("第{i}段细节长文正文不应进挂载。", i = i));
    }
    let page = crate::wiki::save_text_page(&body, Some("装配手册"), &[], &store).unwrap();
    let missing = format!("note-absent{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let input = crate::storage::chat_supplements::ChatSupplement {
        queue_id: 1,
        input_id: "m1".into(),
        content: format!(
            "@wiki({})\n@wiki({missing})\n请基于这些页面继续修订方案",
            page.slug
        ),
        created_at: "2026-10-19T00:00:00Z".into(),
        basis: String::new(),
        context_json: "{}".into(),
    };
    let message = super::super::supplements::context(&store, &input);
    let content = message.content;
    // 每页挂摘要 + 取回路径，不带全文（正文末段必须缺席）
    assert!(content.contains(&page.title), "应展开页面标题");
    assert!(content.contains("全文未挂载"));
    assert!(content.contains("get_wiki_page"));
    assert!(
        !content.contains("第39段细节长文正文不应进挂载。"),
        "挂载视图不得携带全文末段"
    );
    // 缺失页如实声明，用户原话保留
    assert!(content.contains(&missing));
    assert!(content.contains("不存在或已删除"));
    assert!(content.contains("用户原话：请基于这些页面继续修订方案"));
    // 纯文本补充不受影响：没有引用行时原样挂载
    let plain = crate::storage::chat_supplements::ChatSupplement {
        content: "普通补充说明".into(),
        ..input.clone()
    };
    let plain_message = super::super::supplements::context(&store, &plain);
    assert!(plain_message.content.contains("普通补充说明"));
    assert!(!plain_message.content.contains("知识引用展开"));
}
