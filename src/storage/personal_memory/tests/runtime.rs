use super::*;
use crate::ai::{
    memory::{self, ContextMessage, SegmentPurpose},
    provider::{AiProvider, AiReply, ProviderEvent},
    tool::ToolSpec,
};

struct BudgetProvider {
    include: bool,
    fail: bool,
}

impl AiProvider for BudgetProvider {
    fn generate_reply_with_tools(
        &self,
        _: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        unreachable!()
    }

    fn generate_observed(
        &self,
        messages: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
        observer: &mut dyn FnMut(ProviderEvent<'_>) -> Result<()>,
    ) -> Result<AiReply> {
        let mut wire = json!({"model":"test","messages":messages.iter().map(|message|json!({"role":message.role,"content":message.content})).collect::<Vec<_>>()});
        let mut policies = crate::ai::budget::RequestPolicies::from_context(&messages);
        let (fit, mut report) = policies.fit_reported("test", &mut wire, 8192, 512);
        fit?;
        if !self.include {
            for segment in &mut report.segments {
                if segment.segment.purpose == SegmentPurpose::PersonalMemory {
                    segment.tokens_after = 0;
                    segment.omission_reason = Some("optional_background");
                }
            }
        }
        observer(ProviderEvent::Budget {
            attempt: 1,
            trigger: "initial",
            report: &report,
        })?;
        observer(ProviderEvent::Budget {
            attempt: 2,
            trigger: "retry",
            report: &report,
        })?;
        ensure!(!self.fail, "transport failure");
        Ok(AiReply::text("主题建议"))
    }
}

#[test]
fn request_receipts_distinguish_omitted_retry_and_failed_transports() {
    for (include, fail, expected) in [(false, false, 0), (true, false, 1), (true, true, 0)] {
        let fixture = Fixture::new();
        let claim = fixture.candidate("theme", "我希望使用浅色主题");
        fixture
            .act(json!({"operation":"confirm","claim_id":claim.id}))
            .unwrap();
        let mut context = vec![ContextMessage::new("user", "主题")];
        memory::claims::refresh(&fixture.store, &fixture.conversation, &mut context).unwrap();
        assert!(
            context
                .iter()
                .any(|message| message.segment.purpose == SegmentPurpose::PersonalMemory)
        );
        let reply = crate::ai::generation::request(
            &fixture.store,
            &BudgetProvider { include, fail },
            "memory-test",
            context,
            None,
        );
        assert_eq!(reply.is_err(), fail);
        let sent: i64 = fixture.store.connection.query_row("SELECT COUNT(*) FROM personal_memory_receipts WHERE access_kind='injected' AND outcome='sent'", [], |row| row.get(0)).unwrap();
        assert_eq!(sent, expected);
        if include {
            let uncertain: i64 = fixture
                .store
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM personal_memory_receipts WHERE outcome='unknown'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(uncertain, if fail { 2 } else { 1 });
        }
    }
}

#[test]
fn compact_index_is_opt_in_bounded_and_rebuild_preserves_decisions() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("theme", "我希望使用浅色主题");
    fixture
        .act(json!({"operation":"confirm","claim_id":claim.id}))
        .unwrap();
    assert!(
        fixture
            .store
            .memory_policy("compact_index")
            .unwrap()
            .is_none()
    );
    fixture.act(json!({"operation":"enable_index"})).unwrap();
    let mut context = vec![ContextMessage::new("user", "主题")];
    memory::claims::refresh(&fixture.store, &fixture.conversation, &mut context).unwrap();
    let segments: Vec<_> = context
        .iter()
        .filter(|message| message.segment.purpose == SegmentPurpose::PersonalMemory)
        .collect();
    assert!(!segments.is_empty());
    assert!(segments.len() <= 8);
    assert!(
        segments
            .iter()
            .map(|message| message.content.chars().count())
            .sum::<usize>()
            <= 800
    );
    fixture
        .act(json!({"operation":"delete","claim_id":claim.id}))
        .unwrap();
    assert!(!memory::claims::current(&fixture.store, &context).unwrap());
    fixture.act(json!({"operation":"rebuild_index"})).unwrap();
    memory::claims::refresh(&fixture.store, &fixture.conversation, &mut context).unwrap();
    assert!(
        !context
            .iter()
            .any(|message| message.segment.purpose == SegmentPurpose::PersonalMemory)
    );
}

#[test]
fn deleting_memory_invalidates_cached_search_results_before_provider_retry() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("theme", "我希望使用浅色主题");
    fixture
        .act(json!({"operation":"confirm","claim_id":claim.id}))
        .unwrap();
    let claims = fixture
        .store
        .search_memories(&fixture.conversation, &json!({}))
        .unwrap();
    let mut context = vec![ContextMessage::new(
        "tool",
        json!({"memory_claims":claims}).to_string(),
    )];
    assert!(memory::claims::current(&fixture.store, &context).unwrap());
    fixture
        .act(json!({"operation":"delete","claim_id":claim.id}))
        .unwrap();
    assert!(!memory::claims::current(&fixture.store, &context).unwrap());
    memory::claims::refresh(&fixture.store, &fixture.conversation, &mut context).unwrap();
    assert!(!context[0].content.contains("浅色"));
    assert!(memory::claims::current(&fixture.store, &context).unwrap());
}

#[test]
fn revoked_shared_history_is_removed_from_cached_tool_and_automatic_context() {
    let fixture = Fixture::new();
    let source = fixture
        .store
        .create_conversation(Some("部署讨论"), None)
        .unwrap();
    fixture
        .store
        .send_message(&source, "user", "部署方案选择多区域", None)
        .unwrap();
    fixture
        .store
        .send_message(&source, "assistant", "部署方案包含容灾", None)
        .unwrap();
    fixture
        .act(json!({"operation":"share_conversation","source_conversation_id":source}))
        .unwrap();
    let shared =
        memory::history::search_shared(&fixture.store, &fixture.conversation, &["部署方案".into()])
            .unwrap();
    let mut context = vec![ContextMessage::new(
        "tool",
        json!({"conversation_history":null,"shared_history":shared}).to_string(),
    )];
    assert!(
        memory::history::context_current(&fixture.store, &fixture.conversation, &context).unwrap()
    );
    fixture
        .act(json!({"operation":"revoke_conversation","source_conversation_id":source}))
        .unwrap();
    assert!(
        !memory::history::context_current(&fixture.store, &fixture.conversation, &context).unwrap()
    );
    memory::history::refresh(&fixture.store, &fixture.conversation, &mut context);
    assert!(!context[0].content.contains("容灾"));
    assert!(
        memory::history::context_current(&fixture.store, &fixture.conversation, &context).unwrap()
    );
}

#[test]
fn queued_inputs_are_excluded_from_event_recall_and_claim_evidence() {
    let mut fixture = Fixture::new();
    let current = fixture
        .store
        .send_message(&fixture.conversation, "user", "主题如何选择", None)
        .unwrap();
    let messages = fixture.store.list_messages(&fixture.conversation).unwrap();
    let future = fixture.candidate("future", "我希望使用未来主题");
    let event = fixture
        .store
        .insert_event(crate::event::NewEvent::now("未来主题来自排队输入"))
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "INSERT INTO chat_input_effects(input_message_id,event_id,created_at) VALUES(?1,?2,?3)",
            params![future.basis[0].id, event, now()],
        )
        .unwrap();
    fixture.store.conversation_snapshot = Some(crate::storage::ConversationSnapshot {
        conversation_id: fixture.conversation.clone(),
        input_message_id: current,
        messages: messages.into(),
    });
    assert!(!fixture.store.memory_event_visible(&event).unwrap());
    assert_eq!(
        fixture.store.memory_validity(&future).unwrap(),
        "unavailable"
    );
    let recalled =
        crate::knowledge::recall::event_excerpts(&fixture.store, &["主题".into()], 3, None)
            .unwrap();
    assert!(recalled.is_empty());
    let today = memory::today_memory_background(&fixture.store, &fixture.conversation).unwrap();
    assert!(today.is_none());
}

#[test]
fn deleted_message_memory_is_not_revived_through_its_recorded_event() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("theme", "我希望使用浅色主题");
    let event = fixture
        .store
        .insert_event(crate::event::NewEvent::now("我希望使用浅色主题"))
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "INSERT INTO chat_input_effects(input_message_id,event_id,created_at) VALUES(?1,?2,?3)",
            params![claim.basis[0].id, event, now()],
        )
        .unwrap();
    let mut context = vec![ContextMessage::new("tool", json!({"conversation_history":null,"events":[{"event_id":event,"excerpt":"我希望使用浅色主题"}]}).to_string())];
    fixture
        .act(json!({"operation":"delete","claim_id":claim.id}))
        .unwrap();
    assert!(
        fixture
            .store
            .memory_source_suppressed("event", &event)
            .unwrap()
    );
    assert!(
        crate::knowledge::recall::event_excerpts(&fixture.store, &["主题".into()], 3, None)
            .unwrap()
            .is_empty()
    );
    assert!(
        !memory::history::context_current(&fixture.store, &fixture.conversation, &context).unwrap()
    );
    memory::history::refresh(&fixture.store, &fixture.conversation, &mut context);
    assert!(!context[0].content.contains("浅色"));
    assert!(
        memory::today_memory_background(&fixture.store, &fixture.conversation)
            .unwrap()
            .is_none()
    );
}

#[test]
fn request_guard_rechecks_summary_and_today_without_interpreting_user_json_as_tool_data() {
    let mut fixture = Fixture::new();
    let claim = fixture.candidate("theme", "我希望使用浅色主题");
    let event = fixture
        .store
        .insert_event(crate::event::NewEvent::now("我希望使用浅色主题"))
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "INSERT INTO chat_input_effects(input_message_id,event_id,created_at) VALUES(?1,?2,?3)",
            params![claim.basis[0].id, event, now()],
        )
        .unwrap();
    let input = "请分析 JSON：{\"memory_claims\":[{\"id\":\"example\"}],\"events\":[{\"name\":\"example\"}]}";
    let current = fixture
        .store
        .send_message(&fixture.conversation, "user", input, None)
        .unwrap();
    let messages = fixture.store.list_messages(&fixture.conversation).unwrap();
    fixture.store.conversation_snapshot = Some(crate::storage::ConversationSnapshot {
        conversation_id: fixture.conversation.clone(),
        input_message_id: current,
        messages: messages.into(),
    });
    let summary = ContextMessage::new("system", "用户偏好浅色主题").with_segment(
        SegmentPurpose::Summary,
        memory::SegmentOrigin::Derived,
        Some("summary"),
        memory::OmissionPolicy::OptionalBackground,
    );
    let today = memory::today_memory_background(&fixture.store, &fixture.conversation)
        .unwrap()
        .unwrap();
    let mut context = vec![summary, today, ContextMessage::new("user", input)];
    assert!(memory::claims::current(&fixture.store, &context).unwrap());
    fixture
        .act(json!({"operation":"delete","claim_id":claim.id}))
        .unwrap();
    assert!(!memory::claims::current(&fixture.store, &context).unwrap());
    memory::refresh_recalled_context(&fixture.store, &fixture.conversation, &mut context).unwrap();
    memory::claims::refresh(&fixture.store, &fixture.conversation, &mut context).unwrap();
    assert!(
        !context
            .iter()
            .any(|message| message.content.contains("浅色"))
    );
    assert_eq!(
        context
            .iter()
            .find(|message| message.role == "user")
            .unwrap()
            .content,
        input
    );
    assert!(memory::claims::current(&fixture.store, &context).unwrap());
}

#[test]
fn tool_result_budget_omits_whole_memories_without_logging_unread_retrieval() {
    use crate::ai::tool::{ToolCall, ToolRegistry, ToolRuntime, dispatch_with_knowledge};
    let fixture = Fixture::new();
    let claim = fixture.candidate("theme", &format!("主题{}", "用户偏好原文材料".repeat(300)));
    fixture
        .act(json!({"operation":"confirm","claim_id":claim.id}))
        .unwrap();
    let registry = ToolRegistry::default();
    let call = ToolCall::new("search_memories", json!({"query":"主题"}));
    let result = dispatch_with_knowledge(
        &call,
        &registry,
        &fixture.store,
        &fixture.conversation,
        &[],
        ToolRuntime {
            result_budget: Some((600, "test")),
            ..Default::default()
        },
    );
    let value: Value = serde_json::from_str(&result.content).unwrap();
    assert_eq!(value["memory_claims"].as_array().unwrap().len(), 0);
    assert_eq!(value["omitted_by_budget"], 1);
    let retrieved: i64 = fixture
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM personal_memory_receipts WHERE access_kind='retrieved'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retrieved, 0);
    assert!(
        crate::ai::budget::text_tokens("test", &Value::String(result.content).to_string()) <= 600
    );
    let source = fixture
        .store
        .create_conversation(Some("共享主题讨论"), None)
        .unwrap();
    fixture
        .store
        .send_message(
            &source,
            "user",
            &format!("主题{}", "来源材料".repeat(300)),
            None,
        )
        .unwrap();
    fixture
        .act(json!({"operation":"share_conversation","source_conversation_id":source}))
        .unwrap();
    let call = ToolCall::new("search_knowledge_base", json!({"query":"主题"}));
    let result = dispatch_with_knowledge(
        &call,
        &registry,
        &fixture.store,
        &fixture.conversation,
        &[],
        ToolRuntime {
            result_budget: Some((600, "test")),
            ..Default::default()
        },
    );
    let value: Value = serde_json::from_str(&result.content).unwrap();
    assert!(value["shared_history"].as_array().unwrap().is_empty());
    assert_eq!(value["history_omitted_by_budget"], true);
    assert!(
        crate::ai::budget::text_tokens("test", &Value::String(result.content).to_string()) <= 600
    );
}
