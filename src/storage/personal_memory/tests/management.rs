use super::*;
use crate::storage::approvals::ChatProposal;

#[test]
fn management_list_uses_actual_text_and_hides_removed_memory() {
    let f = Fixture::new();
    let input = f.store.send_message(&f.conversation, "user", "我喜欢简洁回答", None).unwrap();
    let claim = f.store.propose_direct_memory(&f.conversation, &json!({
        "content":"用户喜欢简洁回答", "kind":"semantic", "subject":"user",
        "predicate":"preference", "source_message_id":input
    })).unwrap();
    let list = f.store.memory_management_list(&json!({}), 0, 30).unwrap();
    assert_eq!(list["items"][0]["title"], "用户喜欢简洁回答");
    assert_eq!(list["items"][0]["content"], "用户喜欢简洁回答");
    f.store.edit_direct_memory(&claim.id, claim.version, "用户偏好先给结论").unwrap();
    for state in ["deleted", "dismissed"] {
        f.store.connection.execute("UPDATE personal_memory_claims SET state=?1 WHERE id=?2", params![state, claim.id]).unwrap();
        let list = f.store.memory_management_list(&json!({}), 0, 30).unwrap();
        assert!(list["items"][0]["content"].is_null());
        assert_eq!(list["items"][0]["title"], "已移除的记忆");
        let detail = f.store.memory_management_detail(&claim.id).unwrap();
        assert!(detail["decisions"][0]["args"].is_null());
        assert!(!detail.to_string().contains("用户偏好先给结论"));
        assert!(!detail.to_string().contains("用户喜欢简洁回答"));
    }
}

#[test]
fn management_runtime_empty_claims_can_coexist_with_event_background() {
    use crate::ai::memory::{MemoryProvider, SimpleMemory};

    let fixture = Fixture::new();
    let content = "今天用番茄钟完成了三轮学习";
    fixture
        .store
        .insert_event(crate::event::NewEvent::now(content))
        .unwrap();
    let listing = fixture
        .store
        .memory_management_list(&json!({}), 0, 30)
        .unwrap();
    assert_eq!(listing["total"], 0);
    assert!(listing["items"].as_array().unwrap().is_empty());
    let context = SimpleMemory::new(20)
        .prepare_context(&fixture.conversation, &fixture.store)
        .unwrap();
    assert!(
        context
            .iter()
            .any(|message| message.content.contains(content))
    );
    assert!(!context.iter().any(|message| {
        message.segment.purpose == crate::ai::memory::SegmentPurpose::PersonalMemory
    }));
}

#[test]
fn management_lists_and_opens_main_personal_profile_when_no_claim_exists() {
    let fixture = Fixture::new();
    fixture
        .store
        .save_planning_profile(
            &crate::storage::goal_grounding::PlanningProfile {
                skills: "Rust 开发".into(),
                resources: "本地工作区".into(),
                limits: String::new(),
                preferences: "先给结论，再给细节".into(),
            },
            None,
        )
        .unwrap();
    let listing = fixture
        .store
        .memory_management_list(&json!({}), 0, 30)
        .unwrap();
    assert_eq!(listing["total"], 1);
    assert_eq!(listing["items"][0]["id"], "profile/me");
    let detail = fixture
        .store
        .memory_management_detail("profile/me")
        .unwrap();
    assert_eq!(detail["kind"], "profile");
    assert!(detail["content"].as_str().unwrap().contains("Rust 开发"));
}

#[test]
fn management_runtime_claim_identity_version_and_exclusion_are_shared() {
    use crate::ai::memory::{self, ContextMessage, SegmentPurpose};

    let fixture = Fixture::new();
    let claim = fixture.candidate("theme", "我希望使用浅色主题");
    let mut context = vec![ContextMessage::new("user", "主题")];
    memory::claims::refresh(&fixture.store, &fixture.conversation, &mut context).unwrap();
    assert!(
        !context
            .iter()
            .any(|message| { message.segment.purpose == SegmentPurpose::PersonalMemory })
    );

    let proposal = prepare(
        &fixture,
        &claim,
        json!({"operation":"confirm","scope":"personal"}),
    )
    .unwrap();
    decide(&fixture, &proposal, true).unwrap();
    let detail = fixture.store.memory_management_detail(&claim.id).unwrap();
    let listing = fixture
        .store
        .memory_management_list(&json!({}), 0, 30)
        .unwrap();
    let retrieved = fixture
        .store
        .search_memories(&fixture.conversation, &json!({"query":"主题"}))
        .unwrap();
    assert_eq!(retrieved.len(), 1);
    assert_eq!(listing["items"][0]["id"], detail["claim"]["id"]);
    assert_eq!(listing["items"][0]["version"], detail["claim"]["version"]);
    assert_eq!(retrieved[0]["id"], detail["claim"]["id"]);
    assert_eq!(retrieved[0]["version"], detail["claim"]["version"]);
    memory::claims::refresh(&fixture.store, &fixture.conversation, &mut context).unwrap();
    let supplied = context
        .iter()
        .find(|message| message.segment.purpose == SegmentPurpose::PersonalMemory)
        .unwrap();
    let rendered: Value =
        serde_json::from_str(&supplied.content[supplied.content.find('{').unwrap()..]).unwrap();
    assert_eq!(rendered["id"], detail["claim"]["id"]);
    assert_eq!(rendered["version"], detail["claim"]["version"]);

    let current = fixture.store.memory_claim(&claim.id).unwrap().unwrap();
    let proposal = prepare(&fixture, &current, json!({"operation":"exclude"})).unwrap();
    decide(&fixture, &proposal, true).unwrap();
    memory::claims::refresh(&fixture.store, &fixture.conversation, &mut context).unwrap();
    assert!(
        !context
            .iter()
            .any(|message| { message.segment.purpose == SegmentPurpose::PersonalMemory })
    );
    assert!(
        fixture
            .store
            .search_memories(&fixture.conversation, &json!({"query":"主题"}))
            .unwrap()
            .is_empty()
    );
    let listing = fixture
        .store
        .memory_management_list(&json!({}), 0, 30)
        .unwrap();
    assert_eq!(listing["total"], 1);
    assert_eq!(listing["items"][0]["id"], claim.id);
    assert_eq!(listing["items"][0]["excluded"], true);
}

fn prepare(fixture: &Fixture, claim: &Claim, args: Value) -> Result<ChatProposal> {
    let detail = fixture.store.memory_management_detail(&claim.id)?;
    fixture.store.prepare_memory_management_action(
        &claim.id,
        detail["claim"]["version"].as_i64().unwrap(),
        detail["detail_token"].as_str().unwrap(),
        &args,
    )
}

fn decide(
    fixture: &Fixture,
    proposal: &ChatProposal,
    accept: bool,
) -> Result<crate::storage::approvals::ChatDecision> {
    fixture.store.decide_memory_management_action(
        &proposal.id,
        proposal.conversation_id.as_deref().unwrap(),
        proposal.version,
        &proposal.approval_token,
        accept,
    )
}

#[test]
fn management_pages_all_local_owners_without_the_retrieval_window_limit() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("entry", "本人偏好简短说明");
    for index in 0..215 {
        fixture.store.connection.execute("INSERT INTO personal_memory_claims(id,page_slug,body_hash,kind,subject,predicate,state,scope,conversation_id,basis_json,updated_at,review_after) SELECT ?1,page_slug,body_hash,kind,subject,?2,state,scope,conversation_id,basis_json,updated_at,review_after FROM personal_memory_claims WHERE id=?3",params![format!("copy-{index:03}"),format!("theme-{index}"),claim.id]).unwrap();
    }
    let page = fixture
        .store
        .memory_management_list(&json!({}), 200, 50)
        .unwrap();
    assert_eq!(page["total"], 216);
    assert_eq!(page["items"].as_array().unwrap().len(), 16);
    assert!(page["items"][0]["content"].is_null());
    let last = fixture
        .store
        .memory_management_list(&json!({}), 216, 30)
        .unwrap();
    assert!(last["items"].as_array().unwrap().is_empty());
    let search = fixture
        .store
        .memory_management_list(&json!({"query":"theme-214"}), 0, 30)
        .unwrap();
    assert_eq!(search["total"], 1);
    let literal = fixture
        .store
        .memory_management_list(&json!({"query":"%' OR 1=1"}), 0, 30)
        .unwrap();
    assert_eq!(literal["total"], 0);
    let foreign = fixture
        .store
        .create_conversation(Some("知识讨论"), None)
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "UPDATE conversations SET wiki_page_slug='entry' WHERE id=?1",
            [&foreign],
        )
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "UPDATE personal_memory_claims SET conversation_id=?1 WHERE id=?2",
            params![foreign, claim.id],
        )
        .unwrap();
    let scoped = fixture
        .store
        .memory_management_list(
            &json!({"conversation_kind":"wiki","conversation_id":foreign}),
            0,
            30,
        )
        .unwrap();
    assert_eq!(scoped["total"], 1);
    assert!(
        fixture
            .store
            .search_memories(
                &fixture.conversation,
                &json!({"include_history":true,"limit":20})
            )
            .unwrap()
            .iter()
            .all(|item| item["id"] != claim.id)
    );
}

#[test]
fn management_preview_requires_exact_detail_and_approval_is_idempotent_without_chat_followup() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("approve", "本人偏好先说结论");
    let proposal = prepare(
        &fixture,
        &claim,
        json!({"operation":"confirm","scope":"personal"}),
    )
    .unwrap();
    assert_eq!(
        fixture
            .store
            .memory_claim(&claim.id)
            .unwrap()
            .unwrap()
            .state,
        "pending"
    );
    let receipt = decide(&fixture, &proposal, true).unwrap();
    assert_eq!(receipt.status, "succeeded");
    let current = fixture.store.memory_claim(&claim.id).unwrap().unwrap();
    assert_eq!(current.state, "active");
    assert_eq!(current.scope, "personal");
    assert_eq!(decide(&fixture, &proposal, true).unwrap().id, receipt.id);
    let queued: i64 = fixture
        .store
        .connection
        .query_row(
            "SELECT count(*) FROM messages WHERE role='user' AND content LIKE '〔审批后续跑〕%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(queued, 0);
    let proposal = prepare(&fixture, &current, json!({"operation":"pin"})).unwrap();
    decide(&fixture, &proposal, false).unwrap();
    assert!(
        !fixture
            .store
            .memory_claim(&claim.id)
            .unwrap()
            .unwrap()
            .pinned
    );
}

#[test]
fn management_rejects_changed_sources_at_preparation_and_at_approval() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("changed", "本人偏好完整来源");
    let detail = fixture.store.memory_management_detail(&claim.id).unwrap();
    let proposal = prepare(&fixture, &claim, json!({"operation":"confirm"})).unwrap();
    fixture
        .store
        .connection
        .execute(
            "UPDATE wiki_pages SET content_md='new body' WHERE slug='changed'",
            [],
        )
        .unwrap();
    assert!(
        fixture
            .store
            .prepare_memory_management_action(
                &claim.id,
                claim.version,
                detail["detail_token"].as_str().unwrap(),
                &json!({"operation":"pin"})
            )
            .is_err()
    );
    assert!(
        decide(&fixture, &proposal, true).is_err()
            || fixture
                .store
                .memory_claim(&claim.id)
                .unwrap()
                .unwrap()
                .state
                == "pending"
    );
    assert_eq!(
        fixture
            .store
            .memory_claim(&claim.id)
            .unwrap()
            .unwrap()
            .state,
        "pending"
    );
    assert!(fixture.store.memory_management_detail(&claim.id).unwrap()["content"].is_null());
}

#[test]
fn management_exclusion_is_manual_inspection_without_background_permission() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("private", "本人偏好不自动提及此事");
    fixture
        .act(json!({"operation":"confirm","claim_id":claim.id,"scope":"personal"}))
        .unwrap();
    fixture
        .act(json!({"operation":"exclude","claim_id":claim.id}))
        .unwrap();
    let detail = fixture.store.memory_management_detail(&claim.id).unwrap();
    assert!(detail["content"].as_str().unwrap().contains("不自动提及"));
    assert_eq!(detail["claim"]["excluded"], true);
    assert_eq!(
        fixture
            .store
            .memory_management_list(&json!({"excluded":true}), 0, 30)
            .unwrap()["total"],
        1
    );
    assert!(
        fixture
            .store
            .search_memories(&fixture.conversation, &json!({}))
            .unwrap()
            .is_empty()
    );
    let markers: i64 = fixture
        .store
        .connection
        .query_row(
            "SELECT count(*) FROM personal_memory_inspections",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(markers, 0);
    fixture
        .act(json!({"operation":"delete","claim_id":claim.id}))
        .unwrap();
    let deleted = fixture.store.memory_management_detail(&claim.id).unwrap();
    assert!(deleted["content"].is_null());
    assert!(deleted["claim"]["basis"][0]["quote"].is_null());
    assert!(deleted["sources"][0]["quote"].is_null());
    assert_eq!(
        fixture
            .store
            .memory_management_list(&json!({"state":"deleted"}), 0, 30)
            .unwrap()["total"],
        1
    );
    let restore = prepare(&fixture, &claim, json!({"operation":"restore"})).unwrap();
    assert_eq!(
        decide(&fixture, &restore, true).unwrap().status,
        "succeeded"
    );
    assert_eq!(
        fixture
            .store
            .memory_claim(&claim.id)
            .unwrap()
            .unwrap()
            .state,
        "pending"
    );
}

#[test]
fn management_cannot_smuggle_claim_owners_or_global_settings() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("owner", "本人偏好明确边界");
    assert!(
        prepare(
            &fixture,
            &claim,
            json!({"operation":"confirm","claim_id":"another"})
        )
        .is_err()
    );
    assert!(prepare(&fixture, &claim, json!({"operation":"clear_usage"})).is_err());
    let unrelated = fixture
        .store
        .create_pending_action(
            &fixture.conversation,
            "create_todo",
            "{\"content\":\"unrelated\"}",
        )
        .unwrap();
    let proposal = fixture
        .store
        .chat_proposal("action", &unrelated, Some(&fixture.conversation))
        .unwrap()
        .unwrap();
    assert!(decide(&fixture, &proposal, true).is_err());
    let detail = fixture.store.memory_management_detail(&claim.id).unwrap();
    fixture
        .act(json!({"operation":"pin","claim_id":claim.id}))
        .unwrap();
    assert!(
        fixture
            .store
            .prepare_memory_management_action(
                &claim.id,
                claim.version,
                detail["detail_token"].as_str().unwrap(),
                &json!({"operation":"exclude"})
            )
            .is_err()
    );
}

#[test]
fn management_unavailable_owner_remains_inspectable_but_cannot_mutate() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("archived", "本人偏好保留记录");
    fixture
        .store
        .connection
        .execute(
            "UPDATE conversations SET archived=1 WHERE id=?1",
            [&fixture.conversation],
        )
        .unwrap();
    let detail = fixture.store.memory_management_detail(&claim.id).unwrap();
    assert_eq!(detail["can_review"], false);
    assert_eq!(
        fixture
            .store
            .memory_management_list(&json!({}), 0, 30)
            .unwrap()["total"],
        1
    );
    assert!(prepare(&fixture, &claim, json!({"operation":"pin"})).is_err());
}

#[test]
fn direct_memory_edit_changes_recall_without_a_wiki_page() {
    let f = Fixture::new();
    let input = f.store.send_message(&f.conversation, "user", "我喜欢简洁直接的回答", None).unwrap();
    let claim = f.store.propose_direct_memory(&f.conversation, &json!({"content":"用户喜欢简洁直接的回答","kind":"semantic","subject":"user","predicate":"preference","source_message_id":input})).unwrap();
    f.store.edit_direct_memory(&claim.id, claim.version, "用户偏好详细解释").unwrap();
    assert!(f.store.edit_direct_memory(&claim.id, claim.version, "旧版本覆盖").is_err());
    let found = f.store.search_memories(&f.conversation, &json!({"query":"详细解释"})).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["excerpt"], "用户偏好详细解释");
    assert!(f.store.get_wiki_page("").unwrap().is_none());
}

#[test]
fn automatic_extraction_skips_unavailable_page_conversation_without_stalling_other_messages() {
    let f=Fixture::new();
    let unavailable=f.store.create_conversation(Some("missing page"),None).unwrap();
    f.store.connection.execute("UPDATE conversations SET wiki_page_slug='missing-page' WHERE id=?1",[&unavailable]).unwrap();
    f.store.send_message(&unavailable,"user","我喜欢中文回答",None).unwrap();
    f.store.send_message(&f.conversation,"user","我喜欢简洁回答",None).unwrap();
    assert_eq!(crate::knowledge_background::run_automatic_memory_extraction(&f.store).unwrap(),1);
    assert_eq!(crate::knowledge_background::run_automatic_memory_extraction(&f.store).unwrap(),0);
}

#[test]
fn automatic_memory_cursor_and_ledger_are_idempotent() {
    let f = Fixture::new();
    f.store.send_message(&f.conversation, "user", "我喜欢简洁直接的回答", None).unwrap();
    crate::knowledge_background::run_automatic_memory_extraction(&f.store).unwrap();
    let before: i64 = f.store.connection.query_row("SELECT count(*) FROM personal_memory_claims", [], |r|r.get(0)).unwrap();
    crate::knowledge_background::run_automatic_memory_extraction(&f.store).unwrap();
    let after: i64 = f.store.connection.query_row("SELECT count(*) FROM personal_memory_claims", [], |r|r.get(0)).unwrap();
    assert_eq!(before, 1);
    assert_eq!(before, after);
    let runs: i64 = f.store.connection.query_row("SELECT count(*) FROM knowledge_background_runs WHERE task='memory-extraction'", [], |r|r.get(0)).unwrap();
    assert_eq!(runs, 1);
}

#[test]
fn automatic_memory_failure_rolls_back_cursor_and_candidates() {
    let f = Fixture::new();
    f.store.send_message(&f.conversation, "user", "我喜欢简洁直接的回答", None).unwrap();
    f.store.connection.execute_batch("CREATE TRIGGER fail_memory BEFORE INSERT ON personal_memory_claims BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
    assert!(crate::knowledge_background::run_automatic_memory_extraction(&f.store).is_err());
    let cursor: i64 = f.store.connection.query_row("SELECT last_message_rowid FROM personal_memory_extraction_state WHERE id=1", [], |r|r.get(0)).unwrap();
    assert_eq!(cursor, 0);
    f.store.connection.execute_batch("DROP TRIGGER fail_memory;").unwrap();
    assert_eq!(crate::knowledge_background::run_automatic_memory_extraction(&f.store).unwrap(), 1);
}

#[test]
fn deleted_direct_memory_cannot_be_edited_or_read_as_active_content() {
    let f = Fixture::new();
    let input = f.store.send_message(&f.conversation, "user", "我喜欢简洁直接的回答", None).unwrap();
    let claim = f.store.propose_direct_memory(&f.conversation, &json!({"content":"用户喜欢简洁直接的回答","kind":"semantic","subject":"user","predicate":"preference","source_message_id":input})).unwrap();
    f.store.connection.execute("UPDATE personal_memory_claims SET state='deleted' WHERE id=?1", [&claim.id]).unwrap();
    assert!(f.store.edit_direct_memory(&claim.id, claim.version, "重新启用").is_err());
    let detail = f.store.memory_management_detail(&claim.id).unwrap();
    assert!(detail["content"].is_null());
    assert!(detail["claim"]["content"].is_null());
}

#[test]
fn opposite_preference_is_conflicted_without_overwriting_confirmed_memory() {
    let f = Fixture::new();
    let propose = |text: &str, content: &str| {
        let input = f.store.send_message(&f.conversation, "user", text, None).unwrap();
        f.store.propose_direct_memory(&f.conversation, &json!({"content":content,"kind":"semantic","subject":"user","predicate":"preference","source_message_id":input})).unwrap()
    };
    let first = propose("我喜欢详细解释", "用户喜欢详细解释");
    f.store.edit_direct_memory(&first.id, first.version, "用户喜欢详细解释").unwrap();
    let second = propose("我不喜欢详细解释", "用户不喜欢详细解释");
    assert_eq!(second.state, "conflicted");
    let original = f.store.memory_claim(&first.id).unwrap().unwrap();
    assert_eq!(original.state, "active");
    assert_eq!(original.content.as_deref(), Some("用户喜欢详细解释"));
}

#[test]
fn blocked_memory_kind_does_not_stall_extraction_cursor() {
    let f = Fixture::new();
    f.store.send_message(&f.conversation, "user", "我喜欢简洁直接的回答", None).unwrap();
    f.store.connection.execute("INSERT INTO personal_memory_policy(key,value) VALUES('blocked:semantic','true')", []).unwrap();
    assert_eq!(crate::knowledge_background::run_automatic_memory_extraction(&f.store).unwrap(), 0);
    let cursor: i64 = f.store.connection.query_row("SELECT last_message_rowid FROM personal_memory_extraction_state WHERE id=1", [], |r|r.get(0)).unwrap();
    assert!(cursor > 0);
    let count: i64 = f.store.connection.query_row("SELECT count(*) FROM personal_memory_claims", [], |r|r.get(0)).unwrap();
    assert_eq!(count, 0);
}

#[test]
fn equivalent_pending_memory_ignores_punctuation_without_overwriting_text() {
    let f = Fixture::new();
    let input = f.store.send_message(&f.conversation, "user", "我喜欢简洁直接的回答", None).unwrap();
    let args = json!({"content":"用户喜欢简洁直接的回答。","kind":"semantic","subject":"user","predicate":"preference","source_message_id":input});
    let first = f.store.propose_direct_memory(&f.conversation, &args).unwrap();
    let mut repeated = args.clone();
    repeated["content"] = "用户喜欢 简洁直接的回答".into();
    let second = f.store.propose_direct_memory(&f.conversation, &repeated).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.content.as_deref(), Some("用户喜欢简洁直接的回答。"));
}

#[test]
fn user_edit_records_previous_and_current_memory_in_one_transaction() {
    let f = Fixture::new();
    let input = f.store.send_message(&f.conversation, "user", "我喜欢简洁直接的回答", None).unwrap();
    let claim = f.store.propose_direct_memory(&f.conversation, &json!({"content":"旧正文","kind":"semantic","subject":"user","predicate":"preference","source_message_id":input})).unwrap();
    f.store.edit_direct_memory(&claim.id, claim.version, "用户偏好详细回答").unwrap();
    let history: String = f.store.connection.query_row("SELECT args_json FROM personal_memory_decisions WHERE claim_id=?1 AND operation='edit_content'", [&claim.id], |r|r.get(0)).unwrap();
    let history: Value = serde_json::from_str(&history).unwrap();
    assert_eq!(history["previous_content"], "旧正文");
    assert_eq!(history["content"], "用户偏好详细回答");
}

#[test]
fn deleted_memory_punctuation_variants_do_not_reappear() {
    let f = Fixture::new();
    let input = f.store.send_message(&f.conversation, "user", "我喜欢简洁直接的回答", None).unwrap();
    let mut args = json!({"content":"用户喜欢简洁直接的回答。","kind":"semantic","subject":"user","predicate":"preference","source_message_id":input});
    let first = f.store.propose_direct_memory(&f.conversation, &args).unwrap();
    f.store.connection.execute("UPDATE personal_memory_claims SET state='deleted' WHERE id=?1", [&first.id]).unwrap();
    let second_input = f.store.send_message(&f.conversation, "user", "我喜欢 简洁直接的回答", None).unwrap();
    args["source_message_id"] = second_input.into();
    args["content"] = "用户喜欢 简洁直接的回答".into();
    let suppressed = f.store.propose_direct_memory(&f.conversation, &args).unwrap();
    assert_eq!(suppressed.id, first.id);
    assert_eq!(suppressed.state, "deleted");
}

#[test]
fn supplemental_pending_memory_supersedes_only_unconfirmed_candidate() {
    let f = Fixture::new();
    let input = f.store.send_message(&f.conversation,"user","我喜欢简洁直接的回答，也希望附带例子",None).unwrap();
    let mut args = json!({"content":"用户喜欢简洁直接的回答","kind":"semantic","subject":"user","predicate":"preference","source_message_id":input});
    let first = f.store.propose_direct_memory(&f.conversation,&args).unwrap();
    let additional = f.store.send_message(&f.conversation,"user","我喜欢简洁直接的回答，并附带例子",None).unwrap();
    args["source_message_id"] = additional.into();
    args["content"] = "用户喜欢简洁直接的回答，并附带例子".into();
    let second = f.store.propose_direct_memory(&f.conversation,&args).unwrap();
    let old = f.store.memory_claim(&first.id).unwrap().unwrap();
    assert_eq!(old.state,"superseded");
    assert_eq!(old.superseded_by.as_deref(),Some(second.id.as_str()));
    assert_eq!(second.state,"pending");
    assert_eq!(second.basis.len(),2);
}

#[test]
fn supplemental_memory_preserves_user_confirmed_text() {
    let f = Fixture::new();
    let input = f.store.send_message(&f.conversation,"user","我喜欢简洁直接的回答，并附带例子",None).unwrap();
    let mut args = json!({"content":"用户喜欢简洁直接的回答","kind":"semantic","subject":"user","predicate":"preference","source_message_id":input});
    let first = f.store.propose_direct_memory(&f.conversation,&args).unwrap();
    f.store.edit_direct_memory(&first.id,first.version,"用户喜欢简洁直接的回答").unwrap();
    args["content"] = "用户喜欢简洁直接的回答，并附带例子".into();
    f.store.propose_direct_memory(&f.conversation,&args).unwrap();
    let original = f.store.memory_claim(&first.id).unwrap().unwrap();
    assert_eq!(original.state,"active");
    assert!(original.superseded_by.is_none());
    assert_eq!(original.content.as_deref(),Some("用户喜欢简洁直接的回答"));
}
