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
