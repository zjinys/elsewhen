use super::super::super::run_agent_loop;
use super::super::continuity_tests::RecordingProvider;
use super::*;
use crate::ai::tool::ToolRegistry;

#[test]
fn english_text_protocol_repairs_and_finalization_use_the_pinned_bundle() {
    let (store, _path) = temporary_database();
    store
        .save_prompt_language_preferences(Some("en"), "fixed", Some("en"), 0)
        .unwrap();
    let _scope = store.prompt_scope("preview", "english-protocol").unwrap();
    let conversation = store
        .create_conversation(Some("language fixture"), None)
        .unwrap();
    let mut registry = ToolRegistry::default();
    registry.configure_language(&store).unwrap();
    // Changes to preferences after preparation must not affect repair rounds.
    store
        .save_prompt_language_preferences(Some("zh-Hans"), "fixed", Some("zh-Hans"), 1)
        .unwrap();
    let provider = RecordingProvider::new(vec![
        missing_call_reply(),
        AiReply::text("[工具调用]{broken}"),
        AiReply::text("[工具调用]{\"name\":\"list_rules\",\"arguments\":{}}"),
        AiReply::text("No active rules were found."),
    ]);
    let mut context = vec![ContextMessage::new("user", "List my active rules.")];
    let outcome = run_agent_loop(
        &provider,
        "fixture",
        &mut context,
        &registry,
        &store,
        &conversation,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(outcome.content, "No active rules were found.");
    let requests = provider.requests.borrow();
    assert_eq!(requests.len(), 4);
    assert!(
        requests[1]
            .0
            .iter()
            .any(|m| m.content.contains("this turn now uses text tool calls"))
    );
    assert!(requests[2].0.iter().any(|m| {
        m.content
            .contains("its format was invalid and nothing was executed")
    }));
    let final_request = &requests[3];
    assert!(!final_request.1, "finalization must not offer tools");
    assert!(
        final_request
            .0
            .iter()
            .any(|m| m.content.contains("Deliver the final reply"))
    );
    assert!(
        final_request
            .0
            .iter()
            .any(|m| m.content.contains("Facts for this turn")
                && m.content.contains("list_rules — query: succeeded"))
    );
    assert!(
        final_request
            .0
            .iter()
            .any(|m| m.content.starts_with("Result of tool \"list_rules\""))
    );
    assert!(
        !final_request
            .0
            .iter()
            .any(|m| m.content.contains("系统提示：") || m.content.contains("本轮必须向用户交付"))
    );
    assert!(
        store
            .pending_actions_for_conversation(&conversation)
            .unwrap()
            .is_empty()
    );
    assert!(store.list_events().unwrap().is_empty());
}

#[test]
fn english_empty_and_promise_repairs_remain_bounded() {
    let (store, _path) = temporary_database();
    store
        .save_prompt_language_preferences(Some("en"), "fixed", Some("en"), 0)
        .unwrap();
    let _scope = store.prompt_scope("preview", "english-repairs").unwrap();
    let conversation = store
        .create_conversation(Some("language fixture"), None)
        .unwrap();
    let provider = RecordingProvider::new(vec![
        AiReply::text(""),
        AiReply::text("Please wait. I will prepare the explanation."),
        AiReply::text("Please wait. I will prepare the explanation."),
        AiReply::text("The task remains unfinished."),
    ]);
    let mut context = vec![ContextMessage::new("user", "Explain this concept.")];
    let outcome = run_agent_loop(
        &provider,
        "fixture",
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conversation,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(outcome.rounds_used, 4);
    let requests = provider.requests.borrow();
    assert!(
        requests[1]
            .0
            .iter()
            .any(|m| m.content.contains("your last reply was empty"))
    );
    assert!(
        requests[2]
            .0
            .iter()
            .any(|m| m.content.contains("only announced future work"))
    );
    assert!(!requests[3].1);
    assert!(
        requests[3]
            .0
            .iter()
            .any(|m| m.content.contains("Deliver the final reply"))
    );
    assert!(store.list_events().unwrap().is_empty());
}

#[test]
fn temporary_content_chat_http_uses_explicit_reply_language_and_preserves_source() {
    let (store, _path) = temporary_database();
    store
        .save_prompt_language_preferences(Some("en"), "fixed", Some("zh-Hans"), 0)
        .unwrap();
    let (url, server) = protocol_http_fixture(vec![(
        200,
        json!({"choices":[{"message":{"content":"The source describes a bounded experiment."},"finish_reason":"stop"}]}),
    )]);
    store
        .upsert_ai_provider_config(&url, "kimi-fixture", "fixture-key")
        .unwrap();
    let source = "原始材料😀 <<content>> {{secret:key}}";
    let answer = super::super::super::generate_content_chat(
        source,
        &[super::super::super::ContentChatMessage {
            role: "user".into(),
            content: "请用英文回答。Explain this source.".into(),
        }],
        &store,
    )
    .unwrap();
    assert_eq!(answer, "The source describes a bounded experiment.");
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 1);
    let messages = requests[0]["messages"].as_array().unwrap();
    let system = messages[0]["content"].as_str().unwrap();
    assert!(system.contains("Act as a content mentor"));
    assert!(system.contains("Output language: en"));
    assert!(system.ends_with(source));
    assert!(requests[0].get("tools").is_none());
    assert!(store.list_events().unwrap().is_empty());
    let saved_messages: i64 = store
        .connection
        .query_row("SELECT count(*) FROM messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(saved_messages, 0);
    let policy: String = store
        .connection
        .query_row(
            "SELECT response_policy_json FROM prompt_bundles WHERE owner_id LIKE 'content_chat:%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let policy: serde_json::Value = serde_json::from_str(&policy).unwrap();
    assert_eq!(policy["source"], "explicit_request");
}

#[test]
fn bilingual_provider_failover_retains_bundle_after_preferences_change() {
    for (locale, next_locale, question) in [
        (
            "en",
            "zh-Hans",
            "Explain how this task continues across providers.",
        ),
        ("zh-Hans", "en", "解释如何在不同渠道间继续当前任务。"),
    ] {
        let (store, path) = temporary_database();
        store
            .save_prompt_language_preferences(Some(locale), "fixed", Some(locale), 0)
            .unwrap();
        let (failed_url, failed_server) = protocol_http_fixture(vec![(
            503,
            json!({"error":{"message":"fixture temporarily unavailable"}}),
        )]);
        let (working_url, working_server) = protocol_http_fixture(vec![(
            200,
            json!({"choices":[{"message":{"content":"The original request is preserved across providers."},"finish_reason":"stop"}]}),
        )]);
        store
            .upsert_ai_provider_config(&failed_url, "kimi-fixture-failed", "fixture-key")
            .unwrap();
        store
            .save_ai_provider_config(
                None,
                "fallback language fixture",
                "openai-compatible",
                &working_url,
                "kimi-fixture-working",
                "fixture-key",
                0.3,
                Some(512),
            )
            .unwrap();
        let conversation = store
            .create_conversation(Some("language failover"), None)
            .unwrap();
        store
            .send_message(&conversation, "user", question, None)
            .unwrap();
        // Capture first; a second connection changes settings after the first
        // provider request is observed but before the failover request is built.
        let scope = store
            .prompt_scope("preview", "english-provider-failover")
            .unwrap();
        let settings_path = path.clone();
        let settings = std::thread::spawn(move || {
            let requests = failed_server.join().unwrap();
            let another = Store::open(&settings_path).unwrap();
            another
                .save_prompt_language_preferences(Some(next_locale), "fixed", Some(next_locale), 1)
                .unwrap();
            requests
        });
        let configs = store.list_ai_provider_configs_for_runtime().unwrap();
        let reply = super::super::super::generate_with_provider_configs(
            &conversation,
            &store,
            Some(crate::ai::ConversationConfig {
                memory_type: crate::ai::MemoryType::Simple { max_messages: 10 },
                ..Default::default()
            }),
            configs,
        )
        .unwrap();
        assert_eq!(reply, "The original request is preserved across providers.");
        let first = settings.join().unwrap();
        let second = working_server.join().unwrap();
        assert_eq!((first.len(), second.len()), (1, 1));
        let first_system = first[0]["messages"][0]["content"].as_str().unwrap();
        let second_system = second[0]["messages"][0]["content"].as_str().unwrap();
        assert_eq!(first_system, second_system);
        assert!(second_system.contains(&format!("Output language: {locale}")));
        assert_eq!(first[0]["tools"], second[0]["tools"]);
        assert_eq!(store.run_language().unwrap().instruction_locale, locale);
        assert_eq!(
            store
                .prompt_language_preferences()
                .unwrap()
                .instruction_locale
                .as_deref(),
            Some(next_locale)
        );
        drop(scope);
        assert_eq!(
            store.run_language().unwrap().instruction_locale,
            next_locale
        );
    }
}

#[test]
fn english_false_save_claim_is_repaired_using_actual_pending_draft_state() {
    let (store, _path) = temporary_database();
    store
        .save_prompt_language_preferences(Some("en"), "fixed", Some("en"), 0)
        .unwrap();
    let _scope = store.prompt_scope("preview", "english-save-claim").unwrap();
    let conversation = store
        .create_conversation(Some("claim fixture"), None)
        .unwrap();
    store
        .create_pending_action(
            &conversation,
            "save_knowledge_draft",
            &json!({"title":"Release Notes","content_md":"Original draft."}).to_string(),
        )
        .unwrap();
    let provider = RecordingProvider::new(vec![
        AiReply::text("I've saved \"Release Notes\". Would you like anything else?"),
        AiReply::text("The draft is awaiting review and has not been saved to the knowledge base."),
    ]);
    let mut context = vec![ContextMessage::new("user", "What is the draft status?")];
    let result = run_agent_loop(
        &provider,
        "fixture",
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conversation,
        chrono::Utc::now(),
    )
    .unwrap();
    assert!(result.content.contains("has not been saved"));
    let requests = provider.requests.borrow();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].0.iter().any(|m| {
        m.content.contains("still awaits review")
            && m.content.contains("Release Notes")
            && m.content
                .contains("Natural-language acknowledgements never approve writes")
    }));
    assert_eq!(
        store
            .pending_actions_for_conversation(&conversation)
            .unwrap()
            .len(),
        1
    );
    assert!(
        store
            .find_wiki_page_by_title("Release Notes")
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .list_chat_decisions(Some(&conversation), 0, 10)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn english_continue_preserves_original_task_and_failure_reply_language() {
    let (store, _path) = temporary_database();
    // Instruction language and generated fallback reply language are independent.
    store
        .save_prompt_language_preferences(Some("zh-Hans"), "fixed", Some("en"), 0)
        .unwrap();
    let _scope = store.prompt_scope("preview", "english-followup").unwrap();
    let conversation = store
        .create_conversation(Some("continuity fixture"), None)
        .unwrap();
    let request = "Review the source and explain its limitations. 原文事实 <<request>>";
    for (role, content) in [
        ("user", request),
        (
            "assistant",
            "I'll prepare the explanation and get back to you.",
        ),
        ("user", "Any updates?"),
    ] {
        store
            .send_message(&conversation, role, content, None)
            .unwrap();
    }
    let followup = super::super::super::continuity::FollowUp::load(&store, &conversation)
        .unwrap()
        .unwrap();
    assert!(followup.unresolved);
    assert_eq!(followup.request, request);
    let result =
        super::super::super::continuity::progress_fallback(&store, &conversation, &[], &[], None)
            .unwrap();
    assert!(result.starts_with("Regarding your request"));
    assert!(result.contains(request));
    assert!(result.contains("No background task is continuing"));
    store
        .send_message(&conversation, "assistant", &result, None)
        .unwrap();
    store
        .send_message(&conversation, "user", "Continue", None)
        .unwrap();
    let next = super::super::super::continuity::FollowUp::load(&store, &conversation)
        .unwrap()
        .unwrap();
    assert!(
        next.unresolved,
        "an honest fallback does not complete the original task"
    );
    assert_eq!(next.request, request);
    store
        .create_pending_action(
            &conversation,
            "create_todo",
            &json!({"title":"原文待办","content_md":"Draft <<request>>"}).to_string(),
        )
        .unwrap();
    let notice =
        super::super::super::continuity::progress_fallback(&store, &conversation, &[], &[], None)
            .unwrap();
    assert!(notice.contains("awaiting review"));
    assert!(notice.contains("原文待办"));
    assert!(notice.contains("Draft <<request>>"));
    assert!(notice.contains("through its review card"));
    assert!(store.list_todos(None).unwrap().is_empty());
}

#[test]
fn english_direct_queries_and_search_guidance_preserve_user_rules_without_writes() {
    let (store, _path) = temporary_database();
    store
        .save_prompt_language_preferences(Some("en"), "fixed", Some("en"), 0)
        .unwrap();
    let _scope = store
        .prompt_scope("preview", "english-direct-query")
        .unwrap();
    let text = "用户原始规则 <<kind>> {{secret:key}}";
    store
        .add_rule(text, crate::storage::RuleStatus::Active, None)
        .unwrap();
    let (_, rules) = super::super::super::direct_query(&store, "What are my active rules?")
        .unwrap()
        .unwrap();
    assert!(rules.starts_with("Active personal rules:"));
    assert!(rules.contains(text));
    let (_, usage) = super::super::super::direct_query(&store, "Show my AI usage.")
        .unwrap()
        .unwrap();
    assert!(usage.starts_with("Daily token usage:"));
    assert!(usage.contains("No AI calls"));
    let mut context = vec![ContextMessage::new(
        "user",
        "Search the web for this topic.",
    )];
    super::super::super::note_web_search_intent(
        &mut context,
        &ToolRegistry::deferred_runtime(),
        &store,
    )
    .unwrap();
    assert!(
        context[1].content.contains("Search must be")
            || context[1].content.contains("but it is disabled")
    );
    let mut negative = vec![ContextMessage::new(
        "user",
        "Do not search the web for the latest news.",
    )];
    super::super::super::note_web_search_intent(&mut negative, &ToolRegistry::default(), &store)
        .unwrap();
    assert_eq!(negative.len(), 1);
    assert!(store.list_events().unwrap().is_empty());
}
