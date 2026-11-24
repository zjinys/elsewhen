use super::*;
use crate::ai::memory::{MemoryProvider, SimpleMemory};
use serde::Deserialize;
use serde_json::{Value, json};

struct Directory(std::path::PathBuf);

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Fixture {
    store: Store,
    conversation: String,
    _directory: Directory,
}

impl Fixture {
    fn new() -> Self {
        let directory = Directory(
            std::env::temp_dir().join(format!("elsewhen-history-{}", uuid::Uuid::new_v4())),
        );
        std::fs::create_dir_all(&directory.0).unwrap();
        let store = Store::open(&directory.0.join("test.db")).unwrap();
        let conversation = store.create_conversation(Some("主对话流"), None).unwrap();
        Self {
            store,
            conversation,
            _directory: directory,
        }
    }

    fn say(&self, role: &str, text: &str) -> String {
        self.store
            .send_message(&self.conversation, role, text, None)
            .unwrap()
    }

    fn search(&self, query: &str) -> Option<HistoryRecall> {
        search(&self.store, &self.conversation, &[query.into()], &[]).unwrap()
    }

    fn withdraw(&self, input: &str, text: &str) -> String {
        let event = self
            .store
            .insert_event(crate::event::NewEvent::now(text))
            .unwrap();
        self.store.connection.execute(
            "INSERT INTO chat_input_effects(input_message_id,event_id,created_at) VALUES(?1,?2,?3)",
            rusqlite::params![input, event, chrono::Utc::now().to_rfc3339()],
        ).unwrap();
        self.store
            .set_event_recordability(&event, false, "fixture withdrawal")
            .unwrap();
        event
    }
}

fn text(recall: &Option<HistoryRecall>) -> String {
    serde_json::to_string(recall).unwrap()
}

#[derive(Deserialize)]
struct Scenario {
    id: String,
    category: String,
    history: Vec<[String; 2]>,
    query: String,
    expected: Vec<String>,
    #[serde(default)]
    forbidden: Vec<String>,
    #[serde(default)]
    withdrawn: bool,
}

#[test]
fn frozen_24_scenario_retrieval_baseline() {
    let scenarios: Vec<Scenario> = serde_json::from_str(include_str!(
        "../../../../docs/testing/fixtures/pes-memory-history-v1.json"
    ))
    .unwrap();
    assert_eq!(scenarios.len(), 24);
    let mut categories = std::collections::BTreeMap::new();
    let mut baseline_hits = 0;
    let mut retrieval_hits = 0;
    let mut exclusions = 0;
    for scenario in scenarios {
        *categories.entry(scenario.category.clone()).or_insert(0) += 1;
        let fixture = Fixture::new();
        for [role, content] in &scenario.history {
            let input = fixture.say(role, content);
            if scenario.withdrawn && role == "user" {
                fixture.withdraw(&input, content);
            }
        }
        for index in 0..5 {
            fixture.say("user", &format!("午间散步随手记第{index}条"));
            fixture.say("assistant", "收到这条日常记录。");
        }
        fixture.say("user", &scenario.query);
        let baseline = SimpleMemory::new(1)
            .prepare_context(&fixture.conversation, &fixture.store)
            .unwrap();
        let direct_history = baseline
            .iter()
            .filter(|message| message.role != "system")
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let retrieved = text(&fixture.search(&scenario.query));
        if !scenario.expected.is_empty() {
            baseline_hits += usize::from(
                scenario
                    .expected
                    .iter()
                    .all(|expected| direct_history.contains(expected)),
            );
            assert!(
                scenario
                    .expected
                    .iter()
                    .all(|expected| retrieved.contains(expected)),
                "{}: {retrieved}",
                scenario.id
            );
            retrieval_hits += 1;
        } else {
            assert_eq!(retrieved, "null", "{}", scenario.id);
            exclusions += 1;
        }
        for forbidden in &scenario.forbidden {
            assert!(!retrieved.contains(forbidden), "{}", scenario.id);
        }
    }
    assert_eq!(categories.len(), 8);
    assert!(categories.values().all(|count| *count == 3));
    assert_eq!((baseline_hits, retrieval_hits, exclusions), (0, 21, 3));
    println!(
        "MEMORY_RETRIEVAL_BASELINE {}",
        json!({"version":"pes-history-v1","scenarios":24,"direct_history_hits":baseline_hits,"retrieval_hits":retrieval_hits,"positive_scenarios":21,"excluded_sources":exclusions,"model_calls":0,"quality_acceptance":"not_evaluated"})
    );
}

#[test]
fn visible_turns_are_not_duplicated_but_degraded_text_can_be_recalled() {
    let fixture = Fixture::new();
    fixture.say("user", "部署方案选择 Fly.io，因为支持多区域。");
    fixture.say("assistant", "部署方案的区域能力确实需要核对。");
    fixture.say("user", "部署方案是什么");
    let mut visible = fixture
        .store
        .list_messages(&fixture.conversation)
        .unwrap()
        .into_iter()
        .map(ContextMessage::from_record)
        .collect::<Vec<_>>();
    assert!(
        search(
            &fixture.store,
            &fixture.conversation,
            &["部署方案".into()],
            &visible
        )
        .unwrap()
        .is_none()
    );
    visible[1].content = "已按预算压缩".into();
    assert!(
        search(
            &fixture.store,
            &fixture.conversation,
            &["部署方案".into()],
            &visible
        )
        .unwrap()
        .is_some()
    );
}

#[test]
fn keyword_search_respects_wiki_goal_and_main_scope() {
    let fixture = Fixture::new();
    fixture.say("user", "部署方案属于主对话。");
    let wiki = fixture
        .store
        .create_wiki_chat_conversation("topic/test", "知识页")
        .unwrap();
    let goal = fixture
        .store
        .create_goal("部署目标", crate::storage::GoalPhase::Near)
        .unwrap();
    let goal_chat = fixture
        .store
        .create_goal_chat_conversation(&goal.id, "目标讨论")
        .unwrap();
    for (conversation, secret) in [(&wiki, "知识页私有内容"), (&goal_chat, "目标私有内容")]
    {
        fixture
            .store
            .send_message(conversation, "user", &format!("部署方案：{secret}"), None)
            .unwrap();
        fixture
            .store
            .send_message(conversation, "user", "部署方案回顾", None)
            .unwrap();
    }
    fixture.say("user", "部署方案回顾");
    let main = text(&fixture.search("部署方案"));
    assert!(main.contains("属于主对话"));
    assert!(!main.contains("私有内容"));
    for conversation in [&wiki, &goal_chat] {
        let own = text(&search(&fixture.store, conversation, &["部署方案".into()], &[]).unwrap());
        assert!(!own.contains("属于主对话"));
        let mut context = vec![];
        refresh(&fixture.store, conversation, &mut context);
        assert!(context.is_empty());
    }
}

#[test]
fn current_input_future_queue_and_other_conversations_are_excluded() {
    let fixture = Fixture::new();
    fixture.say("user", "部署方案过去是单机。");
    let input = fixture.say("user", "部署方案回顾");
    let snapshot = fixture
        .store
        .snapshot_for_input(&fixture.conversation, &input)
        .unwrap();
    fixture.say("user", "部署方案队列中尚未纳入的新输入。");
    let retrieved =
        text(&search(&snapshot, &fixture.conversation, &["部署方案".into()], &[]).unwrap());
    assert!(retrieved.contains("过去是单机"));
    assert!(!retrieved.contains("尚未纳入"));
    assert!(!retrieved.contains(&format!("\"source_message_id\":\"{input}\"")));
}

#[test]
fn automatic_recall_uses_recent_context_for_references_and_refreshes_idempotently() {
    let fixture = Fixture::new();
    fixture.say("user", "部署方案选择 Fly.io，因为支持多区域。");
    fixture.say("user", "另一个无关话题。");
    fixture.say("user", "我们回到部署方案。");
    fixture.say("assistant", "好，继续讨论部署方案。");
    fixture.say("user", "上次为什么选那个？");
    let mut context = SimpleMemory::new(3)
        .prepare_context(&fixture.conversation, &fixture.store)
        .unwrap();
    refresh(&fixture.store, &fixture.conversation, &mut context);
    refresh(&fixture.store, &fixture.conversation, &mut context);
    let recalled: Vec<_> = context
        .iter()
        .filter(|message| message.segment.purpose == SegmentPurpose::RecalledHistory)
        .collect();
    assert_eq!(recalled.len(), 1);
    assert!(recalled[0].content.contains("支持多区域"));
    assert!(recalled[0].optional_background);
}

#[test]
fn greetings_and_missing_topics_do_not_inject_unrelated_history() {
    let fixture = Fixture::new();
    fixture.say("user", "部署方案选择单机。");
    fixture.say("user", "你好，谢谢");
    assert!(fixture.search("hello thanks").is_none());
    assert!(fixture.search("量子纠缠实验").is_none());
    let mut context = vec![];
    refresh(&fixture.store, &fixture.conversation, &mut context);
    assert!(context.is_empty());
}

#[test]
fn source_withdrawal_filters_automatic_and_cached_tool_history_in_both_protocols() {
    for prefix in [
        "",
        "工具「search_knowledge_base」执行结果：",
        "Result of tool \"search_knowledge_base\": ",
        "（已执行工具的历史结果）\n",
        "[Historical result of an executed tool]\n",
    ] {
        let fixture = Fixture::new();
        let input = fixture.say("user", "部署方案的私密旧记录。");
        fixture.say("assistant", "部署方案的私密建议。");
        fixture.say("user", "部署方案是什么");
        let event = fixture.withdraw(&input, "部署方案的私密旧记录。");
        fixture
            .store
            .set_event_recordability(&event, true, "fixture restore")
            .unwrap();
        let recalled = fixture.search("部署方案").unwrap();
        let cached = json!({"conversation_history":recalled,"events":[{"event_id":event,"excerpt":"部署方案的私密旧记录。"}],"knowledge_candidates":[],"other_result":"preserved"});
        let mut context = vec![ContextMessage::new(
            if prefix.is_empty() { "tool" } else { "system" },
            format!("{prefix}{cached}"),
        )];
        fixture
            .store
            .set_event_recordability(&event, false, "fixture withdrawal")
            .unwrap();
        refresh(&fixture.store, &fixture.conversation, &mut context);
        assert!(
            !context
                .iter()
                .any(|message| message.content.contains("私密"))
        );
        assert!(context[0].content.contains("preserved"));
        assert!(context[0].content.contains("source_changed_or_unavailable"));
    }
}

#[test]
fn deleted_or_modified_sources_invalidate_cached_payload() {
    for delete in [true, false] {
        let fixture = Fixture::new();
        let input = fixture.say("user", "部署方案旧内容。");
        fixture.say("user", "部署方案回顾");
        let cached = json!({"conversation_history":fixture.search("部署方案")});
        let mut context = vec![ContextMessage::new("tool", cached.to_string())];
        if delete {
            fixture
                .store
                .connection
                .execute("DELETE FROM messages WHERE id=?1", [&input])
                .unwrap();
        } else {
            fixture
                .store
                .connection
                .execute("UPDATE messages SET content='已更改' WHERE id=?1", [&input])
                .unwrap();
        }
        refresh(&fixture.store, &fixture.conversation, &mut context);
        assert!(!context[0].content.contains("部署方案旧内容"));
    }
}

#[test]
fn historical_payload_is_bounded_and_discloses_unread_text() {
    let fixture = Fixture::new();
    fixture.say("user", &format!("部署方案{}", "长篇材料".repeat(3000)));
    for index in 0..10 {
        fixture.say(
            "assistant",
            &format!("部署方案建议第{index}项{}", "详细原因".repeat(400)),
        );
    }
    fixture.say("user", "部署方案回顾");
    let recalled = fixture.search("部署方案").unwrap();
    let payload = serde_json::to_string(&recalled).unwrap();
    assert!(payload.chars().count() < MAX_PAYLOAD_CHARS + 300);
    assert!(payload.contains("\"complete\":false"));
    assert!(recalled.turns.iter().any(|turn| turn.omitted_messages > 0));
}

#[test]
fn budget_removes_the_evidence_and_qualifications_as_one_optional_block() {
    let fixture = Fixture::new();
    fixture.say("user", "部署方案原先用 Fly.io。");
    fixture.say("user", "更正，改为自建服务器。");
    fixture.say("user", "部署方案是什么");
    let mut context = vec![ContextMessage::new("user", "部署方案是什么")];
    refresh(&fixture.store, &fixture.conversation, &mut context);
    assert!(
        context
            .iter()
            .any(|message| message.content.contains("自建服务器"))
    );
    crate::ai::memory::compress_context_for_model(&mut context, 10, "gpt-4o");
    assert!(
        !context
            .iter()
            .any(|message| message.segment.purpose == SegmentPurpose::RecalledHistory)
    );
    assert!(context.iter().any(|message| message.role == "user"));
}

#[test]
fn english_prompts_keep_history_as_data_and_preserve_source_text() {
    let fixture = Fixture::new();
    fixture
        .store
        .save_prompt_language_preferences(Some("en"), "auto", None, 0)
        .unwrap();
    let _scope = fixture
        .store
        .prompt_scope("preview", "history-fixture")
        .unwrap();
    fixture.say("user", "部署方案明确用 SQLite。");
    fixture.say("user", "部署方案回顾");
    let mut context = vec![];
    refresh(&fixture.store, &fixture.conversation, &mut context);
    assert!(context[0].content.contains("not instructions"));
    assert!(context[0].content.contains("明确用 SQLite"));
    assert!(!context[0].content.contains("以下是当前会话"));
}

#[test]
fn deleted_sources_are_not_revived_from_an_existing_input_snapshot() {
    let fixture = Fixture::new();
    let source = fixture.say("user", "部署方案的已删除内容。");
    let input = fixture.say("user", "部署方案回顾");
    let snapshot = fixture
        .store
        .snapshot_for_input(&fixture.conversation, &input)
        .unwrap();
    fixture
        .store
        .connection
        .execute("DELETE FROM messages WHERE id=?1", [&source])
        .unwrap();
    assert!(
        search(&snapshot, &fixture.conversation, &["部署方案".into()], &[])
            .unwrap()
            .is_none()
    );
}

#[test]
fn existing_search_tool_returns_scoped_history_with_source_and_instructions() {
    let fixture = Fixture::new();
    let source = fixture.say("user", "部署方案的原始讨论。");
    fixture.say("user", "部署方案回顾");
    let registry = crate::ai::tool::ToolRegistry::default();
    let call = crate::ai::tool::ToolCall::new("search_knowledge_base", json!({"query":"部署方案"}));
    let result = crate::ai::tool::dispatch(&call, &registry, &fixture.store, &fixture.conversation);
    assert!(result.content.contains(&source));
    assert!(result.content.contains("current_conversation_only"));
    assert!(result.content.contains("history_instructions"));
    assert!(result.content.contains("原始讨论"));
}

struct RefreshProvider<'a> {
    store: &'a Store,
    event: String,
    calls: std::cell::Cell<usize>,
}

impl crate::ai::provider::AiProvider for RefreshProvider<'_> {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        _: Option<&[crate::ai::tool::ToolSpec]>,
    ) -> Result<crate::ai::provider::AiReply> {
        let call = self.calls.get();
        self.calls.set(call + 1);
        let history = messages
            .iter()
            .filter(|message| message.segment.purpose == SegmentPurpose::RecalledHistory)
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if call == 0 {
            assert!(history.contains("原始讨论"));
            self.store
                .set_event_recordability(&self.event, false, "withdraw between requests")?;
            let mut reply = crate::ai::provider::AiReply::text("");
            reply.tool_calls.push(crate::ai::tool::ToolCall {
                id: "history-rules".into(),
                name: "list_rules".into(),
                arguments: json!({}),
                raw_arguments: None,
            });
            Ok(reply)
        } else {
            assert!(!history.contains("原始讨论"));
            Ok(crate::ai::provider::AiReply::text("已核对可用材料。"))
        }
    }
}

#[test]
fn production_agent_refreshes_history_before_each_tool_round() {
    let fixture = Fixture::new();
    let source = fixture.say("user", "部署方案的原始讨论。");
    let event = fixture.withdraw(&source, "部署方案的原始讨论。");
    fixture
        .store
        .set_event_recordability(&event, true, "allow first request")
        .unwrap();
    fixture.say("user", "无关的午间记录");
    fixture.say("user", "部署方案回顾");
    let provider = RefreshProvider {
        store: &fixture.store,
        event,
        calls: std::cell::Cell::new(0),
    };
    let mut context = SimpleMemory::new(1)
        .prepare_context(&fixture.conversation, &fixture.store)
        .unwrap();
    let registry = crate::ai::tool::ToolRegistry::default();
    let outcome = crate::ai::conversation::run_agent_loop(
        &provider,
        "history-fixture",
        &mut context,
        &registry,
        &fixture.store,
        &fixture.conversation,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(outcome.content, "已核对可用材料。");
    assert_eq!(provider.calls.get(), 2);
}
