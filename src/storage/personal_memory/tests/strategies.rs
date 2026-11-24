use super::*;
use crate::ai::{
    memory::{self, ContextMessage, SegmentPurpose},
    provider::{AiProvider, AiReply},
    tool::{ToolCall, ToolRegistry, ToolRuntime, ToolSpec, dispatch_with_knowledge},
};
use crate::storage::GoalPhase;
use std::cell::RefCell;

fn active(fixture: &Fixture, slug: &str, text: &str) -> Claim {
    let claim = fixture.candidate(slug, text);
    fixture
        .store
        .connection
        .execute(
            "UPDATE personal_memory_claims SET predicate=?1 WHERE id=?2",
            params![slug, claim.id],
        )
        .unwrap();
    fixture
        .act(json!({"operation":"confirm","claim_id":claim.id}))
        .unwrap();
    fixture.store.memory_claim(&claim.id).unwrap().unwrap()
}

fn search(fixture: &Fixture, args: Value) -> Vec<Value> {
    fixture
        .store
        .search_memories(&fixture.conversation, &args)
        .unwrap()
}

fn dispatch(fixture: &Fixture, name: &str, args: Value) -> Value {
    let result = dispatch_with_knowledge(
        &ToolCall::new(name, args),
        &ToolRegistry::default(),
        &fixture.store,
        &fixture.conversation,
        &[],
        ToolRuntime::default(),
    );
    serde_json::from_str(&result.content).unwrap()
}

#[test]
fn pin_and_salience_break_relevance_ties_without_changing_truth_or_permissions() {
    let fixture = Fixture::new();
    let pinned = active(&fixture, "older", "theme compact");
    let ordinary = active(&fixture, "newer", "theme detailed");
    fixture
        .act(json!({"operation":"pin","claim_id":pinned.id}))
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "UPDATE personal_memory_claims SET updated_at='2020-01-01' WHERE id=?1",
            [&pinned.id],
        )
        .unwrap();
    assert_eq!(
        search(&fixture, json!({"query":"theme"}))[0]["id"],
        pinned.id
    );
    assert_eq!(
        search(&fixture, json!({"query":"theme detailed"}))[0]["id"],
        ordinary.id
    );
    assert!(search(&fixture, json!({"query":"unrelated"})).is_empty());
    fixture
        .act(json!({"operation":"unpin","claim_id":pinned.id}))
        .unwrap();
    fixture
        .act(json!({"operation":"set_salience","claim_id":pinned.id,"salience":"high"}))
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "UPDATE personal_memory_claims SET updated_at='2020-01-01' WHERE id=?1",
            [&pinned.id],
        )
        .unwrap();
    let found = search(&fixture, json!({"query":"theme"}));
    assert_eq!(found[0]["id"], pinned.id);
    assert_eq!(found[0]["support"], "unassessed");
    assert_eq!(found[0]["execution_permission"], false);
    fixture
        .act(json!({"operation":"conflict","claim_id":pinned.id}))
        .unwrap();
    assert_eq!(search(&fixture, json!({"query":"theme"})).len(), 1);
    assert!(
        fixture
            .act(json!({"operation":"set_salience","claim_id":ordinary.id,"salience":"urgent"}))
            .is_err()
    );
}

#[test]
fn exclusion_survives_rebuild_reopen_and_allows_explicit_inspection_and_restoration() {
    let fixture = Fixture::new();
    let claim = active(&fixture, "theme", "我希望使用浅色主题");
    let mut context = vec![ContextMessage::new("user", "主题")];
    memory::claims::refresh(&fixture.store, &fixture.conversation, &mut context).unwrap();
    assert!(
        context
            .iter()
            .any(|message| message.segment.purpose == SegmentPurpose::PersonalMemory)
    );
    fixture
        .act(json!({"operation":"exclude","claim_id":claim.id}))
        .unwrap();
    assert!(!memory::claims::current(&fixture.store, &context).unwrap());
    memory::claims::refresh(&fixture.store, &fixture.conversation, &mut context).unwrap();
    assert!(
        !context
            .iter()
            .any(|message| message.segment.purpose == SegmentPurpose::PersonalMemory)
    );
    assert!(search(&fixture, json!({"query":"主题"})).is_empty());
    assert!(!fixture.store.memory_page_auto_current("theme").unwrap());
    assert!(
        fixture
            .store
            .memory_message_suppressed(&claim.basis[0].id)
            .unwrap()
    );
    fixture.store.memory_rebuild().unwrap();
    let reopened = Store::open(&fixture.directory.join("memory.db")).unwrap();
    assert!(reopened.memory_claim(&claim.id).unwrap().unwrap().excluded);
    let inspection_input = fixture
        .store
        .send_message(&fixture.conversation, "user", "请查看被排除的记忆", None)
        .unwrap();
    let inspection_event = fixture
        .store
        .insert_event(crate::event::NewEvent::now("请查看被排除的记忆"))
        .unwrap();
    fixture.store.connection.execute("INSERT INTO chat_input_effects(input_message_id,event_id,created_at) VALUES(?1,?2,datetime('now'))",params![inspection_input,inspection_event]).unwrap();
    let result = dispatch(
        &fixture,
        "search_memories",
        json!({"query":"主题","include_excluded":true}),
    );
    assert_eq!(result["memory_claims"][0]["state"], "active");
    assert_eq!(result["memory_claims"][0]["excluded_review"], true);
    let manual = vec![ContextMessage::new("tool", result.to_string())];
    assert!(memory::claims::current(&fixture.store, &manual).unwrap());
    let count: i64 = fixture
        .store
        .connection
        .query_row(
            "SELECT COALESCE(SUM(json_array_length(candidates_json)),0) FROM knowledge_usage",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    fixture
        .store
        .send_message(&fixture.conversation, "user", "聊一个新话题", None)
        .unwrap();
    assert!(!memory::claims::current(&fixture.store, &manual).unwrap());
    let mut cached = manual.clone();
    memory::claims::refresh(&fixture.store, &fixture.conversation, &mut cached).unwrap();
    let cached_result: Value = serde_json::from_str(&cached[0].content).unwrap();
    assert!(
        cached_result["memory_claims"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        fixture
            .store
            .memory_message_suppressed(&inspection_input)
            .unwrap()
    );
    assert!(
        fixture
            .store
            .memory_source_suppressed("event", &inspection_event)
            .unwrap()
    );
    assert!(!memory::history::eligible(&fixture.store, &inspection_input).unwrap());
    fixture.act(json!({"operation":"clear_usage"})).unwrap();
    assert!(
        fixture
            .store
            .memory_message_suppressed(&inspection_input)
            .unwrap()
    );
    fixture
        .act(json!({"operation":"include","claim_id":claim.id}))
        .unwrap();
    assert!(!memory::claims::current(&fixture.store, &manual).unwrap());
    assert_eq!(search(&fixture, json!({"query":"主题"})).len(), 1);
    assert!(
        !fixture
            .store
            .memory_message_suppressed(&claim.basis[0].id)
            .unwrap()
    );
    assert!(
        !fixture
            .store
            .memory_message_suppressed(&inspection_input)
            .unwrap()
    );
}

#[test]
fn controls_require_owner_and_reviewed_versions_and_do_not_transfer_to_replacement() {
    let fixture = Fixture::new();
    let old = active(&fixture, "old-theme", "我希望使用浅色主题");
    let mut stale = json!({"operation":"pin","claim_id":old.id});
    stale["reviewed_basis"] = fixture
        .store
        .memory_action_basis(&fixture.conversation, &stale)
        .unwrap();
    fixture
        .act(json!({"operation":"exclude","claim_id":old.id}))
        .unwrap();
    assert!(
        fixture
            .store
            .apply_memory_action(&fixture.conversation, &stale)
            .is_err()
    );
    let other = fixture
        .store
        .create_conversation(Some("其他讨论"), None)
        .unwrap();
    assert!(
        fixture
            .store
            .memory_action_basis(&other, &json!({"operation":"include","claim_id":old.id}))
            .is_err()
    );
    fixture
        .act(json!({"operation":"pin","claim_id":old.id}))
        .unwrap();
    let new = fixture.candidate("new-theme", "我希望使用深色主题");
    fixture
        .store
        .connection
        .execute(
            "UPDATE personal_memory_claims SET predicate='old-theme' WHERE id=?1",
            [&new.id],
        )
        .unwrap();
    fixture
        .act(json!({"operation":"supersede","claim_id":old.id,"replacement_id":new.id}))
        .unwrap();
    let replacement = fixture.store.memory_claim(&new.id).unwrap().unwrap();
    assert!(!replacement.pinned);
    assert!(!replacement.excluded);
    assert_eq!(replacement.salience, "normal");
}

#[test]
fn goal_links_boost_only_active_focus_and_invalidate_cached_results_without_sharing() {
    let fixture = Fixture::new();
    let linked = active(&fixture, "limited", "theme compact");
    let other = active(&fixture, "ordinary", "theme detailed");
    let goal = fixture
        .store
        .create_goal("发布个人产品", GoalPhase::Near)
        .unwrap();
    fixture.act(json!({"operation":"link_goal","claim_id":linked.id,"goal_id":goal.id,"reason":"本目标优先简洁方案"})).unwrap();
    fixture
        .store
        .connection
        .execute(
            "UPDATE personal_memory_claims SET updated_at='2020-01-01' WHERE id=?1",
            [&linked.id],
        )
        .unwrap();
    assert_eq!(
        search(&fixture, json!({"query":"theme"}))[0]["id"],
        other.id
    );
    let found = search(&fixture, json!({"query":"theme","focus_goal_id":goal.id}));
    assert_eq!(found[0]["id"], linked.id);
    let associated = search(
        &fixture,
        json!({"query":"milestone","focus_goal_id":goal.id}),
    );
    assert_eq!(associated.len(), 1);
    assert_eq!(associated[0]["id"], linked.id);
    assert!(search(&fixture, json!({"query":"milestone"})).is_empty());
    assert_eq!(found[0]["goal_links"][0]["reason"], "本目标优先简洁方案");
    let context = vec![ContextMessage::new(
        "tool",
        json!({"memory_claims":found}).to_string(),
    )];
    assert!(memory::claims::current(&fixture.store, &context).unwrap());
    fixture.store.pause_goal(&goal.id).unwrap();
    assert!(
        search(
            &fixture,
            json!({"query":"milestone","focus_goal_id":goal.id})
        )
        .is_empty()
    );
    assert!(!memory::claims::current(&fixture.store, &context).unwrap());
    assert_eq!(
        search(&fixture, json!({"query":"theme","focus_goal_id":goal.id}))[0]["id"],
        other.id
    );
    assert!(fixture.act(json!({"operation":"link_goal","claim_id":other.id,"goal_id":goal.id,"reason":"适用"})).is_err());
    let foreign = fixture
        .store
        .create_conversation(Some("外部目标讨论"), None)
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "UPDATE conversations SET goal_id=?1 WHERE id=?2",
            params![goal.id, foreign],
        )
        .unwrap();
    assert!(
        fixture
            .store
            .search_memories(&foreign, &json!({"query":"theme","focus_goal_id":goal.id}))
            .unwrap()
            .is_empty()
    );
    fixture.store.complete_goal(&goal.id).unwrap();
    assert_eq!(search(&fixture, json!({"query":"theme"})).len(), 2);
    fixture
        .act(json!({"operation":"unlink_goal","claim_id":linked.id,"goal_id":goal.id}))
        .unwrap();
    assert!(
        fixture
            .store
            .memory_goal_links(&linked.id)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn goal_change_after_review_requires_a_new_card() {
    let fixture = Fixture::new();
    let claim = active(&fixture, "theme", "theme compact");
    let goal = fixture
        .store
        .create_goal("发布产品", GoalPhase::Near)
        .unwrap();
    let mut args =
        json!({"operation":"link_goal","claim_id":claim.id,"goal_id":goal.id,"reason":"交付约束"});
    args["reviewed_basis"] = fixture
        .store
        .memory_action_basis(&fixture.conversation, &args)
        .unwrap();
    fixture
        .store
        .update_goal(&goal.id, "发布其他产品", GoalPhase::Near)
        .unwrap();
    assert!(
        fixture
            .store
            .apply_memory_action(&fixture.conversation, &args)
            .is_err()
    );
}

#[test]
fn clearing_goal_workspace_removes_links_without_deleting_personal_memory() {
    let fixture = Fixture::new();
    let claim = active(&fixture, "theme", "主题偏好");
    let goal = fixture
        .store
        .create_goal("发布产品", GoalPhase::Near)
        .unwrap();
    fixture.act(json!({"operation":"link_goal","claim_id":claim.id,"goal_id":goal.id,"reason":"本目标的背景"})).unwrap();
    let context = vec![ContextMessage::new(
        "tool",
        json!({"memory_claims":search(&fixture,json!({"query":"主题"}))}).to_string(),
    )];
    assert!(memory::claims::current(&fixture.store, &context).unwrap());
    fixture.store.clear_goal_workspace_data().unwrap();
    assert!(!memory::claims::current(&fixture.store, &context).unwrap());
    assert!(
        fixture
            .store
            .memory_goal_links(&claim.id)
            .unwrap()
            .is_empty()
    );
    assert_eq!(search(&fixture, json!({"query":"主题"})).len(), 1);
    assert!(
        fixture
            .store
            .get_conversation(&fixture.conversation)
            .unwrap()
            .is_some()
    );
}

#[test]
fn pin_tool_creates_a_real_review_card_and_applies_only_after_approval() {
    let fixture = Fixture::new();
    let claim = active(&fixture, "theme", "主题偏好");
    let result = dispatch(
        &fixture,
        "review_memory",
        json!({"operation":"pin","claim_id":claim.id}),
    );
    assert_eq!(result["status"], "pending_user_review");
    assert!(
        !fixture
            .store
            .memory_claim(&claim.id)
            .unwrap()
            .unwrap()
            .pinned
    );
    let pending = fixture
        .store
        .pending_actions_for_conversation(&fixture.conversation)
        .unwrap();
    let card = fixture
        .store
        .chat_proposal("action", &pending[0].id, Some(&fixture.conversation))
        .unwrap()
        .unwrap();
    let receipt = crate::ai::approval::resolve_chat_proposal(
        &fixture.store,
        "action",
        &card.id,
        Some(&fixture.conversation),
        card.version,
        &card.approval_token,
        "accept",
    )
    .unwrap();
    assert_eq!(receipt.status, "succeeded");
    assert!(
        fixture
            .store
            .memory_claim(&claim.id)
            .unwrap()
            .unwrap()
            .pinned
    );
}

#[derive(Default)]
struct ReviewCapture(RefCell<Vec<String>>);

impl AiProvider for ReviewCapture {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        self.0.borrow_mut().push(messages[1].content.clone());
        Ok(AiReply::text(
            &json!({"support":"supported","supporting":[],"counterexamples":[],"unknown":[]})
                .to_string(),
        ))
    }
}

#[test]
fn salience_review_reserves_the_fourth_daily_slot_for_old_ordinary_work() {
    let fixture = Fixture::new();
    let ordinary = active(&fixture, "ordinary", "普通长期偏好");
    fixture
        .act(json!({"operation":"set_salience","claim_id":ordinary.id,"salience":"low"}))
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "UPDATE personal_memory_claims SET review_after=datetime('now','-10 days') WHERE id=?1",
            [&ordinary.id],
        )
        .unwrap();
    let excluded = active(&fixture, "excluded", "不可自动处理的主题");
    fixture
        .act(json!({"operation":"exclude","claim_id":excluded.id}))
        .unwrap();
    for number in 0..4 {
        let claim = active(
            &fixture,
            &format!("high-{number}"),
            &format!("高优先偏好 {number}"),
        );
        fixture
            .act(json!({"operation":"set_salience","claim_id":claim.id,"salience":"high"}))
            .unwrap();
    }
    let provider = ReviewCapture::default();
    for _ in 0..4 {
        assert_eq!(fixture.store.review_memory(&provider).unwrap(), 1);
    }
    assert_eq!(fixture.store.review_memory(&provider).unwrap(), 0);
    let seen = provider.0.borrow();
    assert!(seen[0].contains("高优先"));
    assert!(seen[3].contains("普通长期"));
    assert!(!seen.iter().any(|text| text.contains("不可自动")));
}

#[test]
fn automatic_usage_does_not_raise_rank_but_deduped_citations_raise_review_priority() {
    for kind in ["injected", "retrieved", "cited"] {
        let fixture = Fixture::new();
        let ordinary = active(&fixture, "ordinary", "普通主题");
        let used = active(&fixture, "used", "高使用主题");
        fixture.store.connection.execute("UPDATE personal_memory_claims SET review_after=datetime('now','-10 days') WHERE id=?1",[&ordinary.id]).unwrap();
        for attempt in ["attempt1", "attempt2"] {
            fixture
                .store
                .memory_receipt("one-logical-request", attempt, &used, kind, "sent")
                .unwrap();
        }
        fixture
            .store
            .connection
            .execute(
                "UPDATE personal_memory_claims SET updated_at='2020-01-01' WHERE id=?1",
                [&used.id],
            )
            .unwrap();
        assert_eq!(
            search(&fixture, json!({"query":"主题"}))[0]["id"],
            ordinary.id
        );
        let provider = ReviewCapture::default();
        fixture.store.review_memory(&provider).unwrap();
        assert_eq!(provider.0.borrow()[0].contains("高使用"), kind == "cited");
    }
}

#[test]
fn proposing_same_text_in_another_conversation_does_not_disclose_existing_claim() {
    let fixture=Fixture::new();
    let first=fixture.store.send_message(&fixture.conversation,"user","我喜欢中文回答",None).unwrap();
    let mut args=json!({"content":"用户喜欢中文回答","kind":"semantic","subject":"user","predicate":"preference","source_message_id":first});
    let old=fixture.store.propose_direct_memory(&fixture.conversation,&args).unwrap();
    let other=fixture.store.create_conversation(Some("other"),None).unwrap();
    let message=fixture.store.send_message(&other,"user","我喜欢中文回答",None).unwrap();
    args["source_message_id"]=message.into();
    let result=dispatch_with_knowledge(&ToolCall::new("propose_memory",args),&ToolRegistry::default(),&fixture.store,&other,&[],ToolRuntime::default());
    assert!(!result.content.contains(&old.id));
    assert!(result.content.contains("其他范围"));
    let unchanged=fixture.store.memory_claim(&old.id).unwrap().unwrap();
    assert_eq!(unchanged.conversation_id,fixture.conversation);
    assert_eq!(unchanged.version,old.version);
    assert_eq!(unchanged.basis.len(),1);
}

#[test]
fn maintenance_measures_independent_memory_without_wiki_page() {
    let f = Fixture::new();
    let message = f.store.send_message(&f.conversation, "user", "我喜欢简洁直接的回答", None).unwrap();
    let content = "用户喜欢简洁直接的回答";
    let claim = f.store.propose_direct_memory(&f.conversation, &json!({
        "content":content, "kind":"semantic", "subject":"user",
        "predicate":"preference", "source_message_id":message
    })).unwrap();
    f.store.edit_direct_memory(&claim.id, claim.version, content).unwrap();
    let report = f.store.memory_maintenance(&f.conversation).unwrap();
    assert_eq!(report["derived_body_chars"], content.chars().count());
}

#[test]
fn maintenance_reports_visible_duplicate_and_broken_sources_without_mutation() {
    let fixture = Fixture::new();
    let first = active(&fixture, "first", "theme compact design constraint");
    let duplicate = active(&fixture, "duplicate", "theme compact design constraint");
    fixture
        .act(json!({"operation":"exclude","claim_id":duplicate.id}))
        .unwrap();
    fixture
        .act(json!({"operation":"pin","claim_id":first.id}))
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "UPDATE wiki_pages SET content_md='changed' WHERE slug='first'",
            [],
        )
        .unwrap();
    let report = dispatch(&fixture, "search_memories", json!({"view":"maintenance"}));
    assert_eq!(report["scanned"], 2);
    assert_eq!(report["excluded"], 1);
    assert_eq!(report["pinned"], 1);
    assert_eq!(report["exact_body_groups"].as_array().unwrap().len(), 1);
    assert_eq!(report["unavailable_or_stale"].as_array().unwrap().len(), 1);
    assert_eq!(report["automatic_mutation"], false);
    assert_eq!(
        fixture
            .store
            .memory_claim(&first.id)
            .unwrap()
            .unwrap()
            .state,
        "active"
    );
    let foreign = fixture
        .store
        .create_conversation(Some("外部"), None)
        .unwrap();
    let invisible = fixture.store.memory_maintenance(&foreign).unwrap();
    assert_eq!(invisible["scanned"], 0);
    assert_eq!(invisible["exact_body_groups"], json!([]));
}

#[test]
fn usage_cleanup_is_reviewed_and_keeps_user_decisions_and_claims() {
    let fixture = Fixture::new();
    let claim = active(&fixture, "theme", "主题偏好");
    fixture
        .act(json!({"operation":"pin","claim_id":claim.id}))
        .unwrap();
    fixture
        .store
        .memory_receipt("request", "attempt1", &claim, "cited", "sent")
        .unwrap();
    let mut outdated = json!({"operation":"clear_usage"});
    outdated["reviewed_basis"] = fixture
        .store
        .memory_action_basis(&fixture.conversation, &outdated)
        .unwrap();
    fixture
        .store
        .memory_receipt("request2", "attempt2", &claim, "cited", "sent")
        .unwrap();
    assert!(
        fixture
            .store
            .apply_memory_action(&fixture.conversation, &outdated)
            .is_err()
    );
    let result = dispatch(
        &fixture,
        "review_memory",
        json!({"operation":"clear_usage"}),
    );
    assert_eq!(result["status"], "pending_user_review");
    assert_eq!(fixture.store.memory_usage_basis().unwrap()["receipts"], 2);
    fixture.act(json!({"operation":"clear_usage"})).unwrap();
    assert_eq!(fixture.store.memory_usage_basis().unwrap()["receipts"], 0);
    assert!(
        fixture
            .store
            .memory_claim(&claim.id)
            .unwrap()
            .unwrap()
            .pinned
    );
    let decisions: i64 = fixture
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM personal_memory_decisions",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(decisions, 3);
}

#[test]
fn active_goal_association_prioritizes_review_but_pausing_removes_the_boost() {
    for paused in [false, true] {
        let fixture = Fixture::new();
        let ordinary = active(&fixture, "ordinary", "普通主题");
        let linked = active(&fixture, "linked", "目标相关主题");
        let goal = fixture
            .store
            .create_goal("发布产品", GoalPhase::Near)
            .unwrap();
        fixture.act(json!({"operation":"link_goal","claim_id":linked.id,"goal_id":goal.id,"reason":"该目标的时间约束"})).unwrap();
        fixture.store.connection.execute("UPDATE personal_memory_claims SET review_after=datetime('now','-10 days') WHERE id=?1",[&ordinary.id]).unwrap();
        if paused {
            fixture.store.pause_goal(&goal.id).unwrap();
        }
        let provider = ReviewCapture::default();
        fixture.store.review_memory(&provider).unwrap();
        assert_eq!(provider.0.borrow()[0].contains("目标相关"), !paused);
    }
}

#[test]
fn maintenance_scans_more_than_twenty_and_omits_whole_hints_to_fit_tool_budget() {
    let fixture = Fixture::new();
    let claim = active(&fixture, "theme", "theme compact design constraint");
    for number in 0..24 {
        fixture.store.connection.execute("INSERT INTO personal_memory_claims(id,page_slug,body_hash,kind,subject,predicate,state,scope,conversation_id,confirmed_hash,basis_json,updated_at,review_after) SELECT ?1,page_slug,body_hash,kind,subject,?1,state,scope,conversation_id,confirmed_hash,basis_json,updated_at,review_after FROM personal_memory_claims WHERE id=?2",params![format!("duplicate-{number}"),claim.id]).unwrap();
    }
    let foreign = fixture
        .store
        .create_conversation(Some("外部讨论"), None)
        .unwrap();
    for number in 0..210 {
        fixture.store.connection.execute("INSERT INTO personal_memory_claims(id,page_slug,body_hash,kind,subject,predicate,state,scope,conversation_id,confirmed_hash,basis_json,updated_at,review_after) SELECT ?1,page_slug,body_hash,kind,subject,?1,state,scope,?3,confirmed_hash,basis_json,'2100-01-01',review_after FROM personal_memory_claims WHERE id=?2",params![format!("foreign-{number}"),claim.id,foreign]).unwrap();
    }
    let report = dispatch(&fixture, "search_memories", json!({"view":"maintenance"}));
    assert_eq!(report["scanned"], 25);
    let mut minimal = report.clone();
    for field in [
        "unavailable_or_stale",
        "exact_body_groups",
        "shared_source_groups",
        "near_duplicate_hints",
    ] {
        minimal[field] = json!([]);
    }
    let tokens =
        crate::ai::budget::text_tokens("test", &Value::String(minimal.to_string()).to_string()) + 8;
    let result = dispatch_with_knowledge(
        &ToolCall::new("search_memories", json!({"view":"maintenance"})),
        &ToolRegistry::default(),
        &fixture.store,
        &fixture.conversation,
        &[],
        ToolRuntime {
            result_budget: Some((tokens, "test")),
            ..Default::default()
        },
    );
    let value: Value = serde_json::from_str(&result.content).unwrap();
    assert_eq!(value["scanned"], 25);
    assert!(value["omitted_entries_by_budget"].as_u64().unwrap() > 0);
    assert!(
        crate::ai::budget::text_tokens("test", &Value::String(result.content).to_string())
            <= tokens
    );
    assert!(
        crate::storage::sync::snapshot::SNAPSHOT_EXCLUDED_TABLES
            .iter()
            .any(|(table, _)| *table == "personal_memory_goal_links")
    );
}
