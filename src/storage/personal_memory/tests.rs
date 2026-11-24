use super::*;

#[path = "tests/management.rs"]
mod management;
#[path = "tests/review.rs"]
mod review;
#[path = "tests/runtime.rs"]
mod runtime;
#[path = "tests/strategies.rs"]
mod strategies;

struct Fixture {
    store: Store,
    conversation: String,
    directory: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("elsewhen-memory-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let store = Store::open(&directory.join("memory.db")).unwrap();
        let conversation = store.create_conversation(Some("记忆测试"), None).unwrap();
        Self {
            store,
            conversation,
            directory,
        }
    }

    fn candidate(&self, slug: &str, content: &str) -> Claim {
        let input = self
            .store
            .send_message(&self.conversation, "user", content, None)
            .unwrap();
        self.store
            .upsert_wiki_page(
                &super::super::WikiPageDraft {
                    slug: slug.into(),
                    kind: "habit".into(),
                    title: slug.into(),
                    summary: content.into(),
                    content_md: content.into(),
                    tags: vec![],
                    source_event_ids: vec![],
                    status: "active".into(),
                    reason: "test".into(),
                    source_url: None,
                },
                super::super::ContentPolicy::Always,
            )
            .unwrap();
        self.store.propose_memory(&self.conversation,&json!({"page_slug":slug,"kind":"procedural","subject":"user","predicate":"theme","source_message_id":input})).unwrap()
    }

    fn act(&self, mut args: Value) -> Result<Value> {
        args["reviewed_basis"] = self.store.memory_action_basis(&self.conversation, &args)?;
        self.store.apply_memory_action(&self.conversation, &args)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn candidate_is_pending_until_version_bound_review_and_confirmation_does_not_prove_support() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("theme", "我希望使用浅色主题");
    assert_eq!(claim.state, "pending");
    assert!(
        fixture
            .store
            .search_memories(&fixture.conversation, &json!({}))
            .unwrap()
            .is_empty()
    );
    fixture
        .act(json!({"operation":"confirm","claim_id":claim.id}))
        .unwrap();
    let found = fixture
        .store
        .search_memories(&fixture.conversation, &json!({"query":"主题"}))
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["support"], "unassessed");
    assert_eq!(found[0]["execution_permission"], false);
}

#[test]
fn suppression_survives_proposal_replay_and_rebuild() {
    for operation in ["dismiss", "delete"] {
        let fixture = Fixture::new();
        let claim = fixture.candidate("theme", "我希望使用浅色主题");
        fixture
            .act(json!({"operation":operation,"claim_id":claim.id}))
            .unwrap();
        fixture.store.memory_rebuild().unwrap();
        let replay=fixture.store.propose_memory(&fixture.conversation,&json!({"page_slug":"theme","kind":"procedural","subject":"user","predicate":"theme"})).unwrap();
        assert_eq!(
            replay.state,
            if operation == "delete" {
                "deleted"
            } else {
                "dismissed"
            }
        );
        assert!(
            fixture
                .store
                .search_memories(&fixture.conversation, &json!({}))
                .unwrap()
                .is_empty()
        );
        fixture
            .act(json!({"operation":"confirm","claim_id":claim.id}))
            .unwrap_err();
    }
}

#[test]
fn source_changes_and_stale_approvals_do_not_confirm_a_new_body() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("theme", "我希望使用浅色主题");
    let mut action = json!({"operation":"confirm","claim_id":claim.id});
    action["reviewed_basis"] = fixture
        .store
        .memory_action_basis(&fixture.conversation, &action)
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "UPDATE wiki_pages SET content_md='新版本' WHERE slug='theme'",
            [],
        )
        .unwrap();
    assert!(
        fixture
            .store
            .apply_memory_action(&fixture.conversation, &action)
            .is_err()
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
fn explicit_replacement_retains_history_but_recalls_only_current_preference() {
    let fixture = Fixture::new();
    let old = fixture.candidate("light", "我希望使用浅色主题");
    fixture
        .act(json!({"operation":"confirm","claim_id":old.id}))
        .unwrap();
    let new = fixture.candidate("dark", "我现在改为深色主题");
    fixture
        .act(json!({"operation":"supersede","claim_id":old.id,"replacement_id":new.id}))
        .unwrap();
    let found = fixture
        .store
        .search_memories(&fixture.conversation, &json!({}))
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["id"], new.id);
    assert_eq!(
        fixture.store.memory_claim(&old.id).unwrap().unwrap().state,
        "superseded"
    );
}

#[test]
fn independently_confirming_different_values_requires_clarification() {
    let fixture = Fixture::new();
    let old = fixture.candidate("light", "我希望使用浅色主题");
    let new = fixture.candidate("dark", "我希望使用深色主题");
    fixture
        .act(json!({"operation":"confirm","claim_id":old.id}))
        .unwrap();
    fixture
        .act(json!({"operation":"confirm","claim_id":new.id}))
        .unwrap();
    assert!(
        fixture
            .store
            .search_memories(&fixture.conversation, &json!({}))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fixture
            .store
            .search_memories(
                &fixture.conversation,
                &json!({"state":"conflicted","include_history":true})
            )
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn grants_are_directed_versioned_revocable_snapshots() {
    let fixture = Fixture::new();
    let source = fixture
        .store
        .create_conversation(Some("源讨论"), None)
        .unwrap();
    fixture
        .store
        .send_message(&source, "user", "部署方案选择多区域", None)
        .unwrap();
    fixture
        .store
        .send_message(&source, "assistant", "原因是容灾", None)
        .unwrap();
    assert!(
        fixture
            .store
            .memory_grant(&source, &fixture.conversation)
            .unwrap()
            .is_none()
    );
    fixture
        .act(json!({"operation":"share_conversation","source_conversation_id":source}))
        .unwrap();
    let grant = fixture
        .store
        .memory_grant(&source, &fixture.conversation)
        .unwrap()
        .unwrap();
    assert!(
        fixture
            .store
            .memory_grant(&fixture.conversation, &source)
            .unwrap()
            .is_none()
    );
    fixture
        .store
        .send_message(&source, "user", "尚未授权的新消息", None)
        .unwrap();
    assert_eq!(
        fixture
            .store
            .memory_grant(&source, &fixture.conversation)
            .unwrap()
            .unwrap(),
        grant
    );
    let found = crate::ai::memory::history::search_shared(
        &fixture.store,
        &fixture.conversation,
        &["部署方案".into()],
    )
    .unwrap();
    let text = serde_json::to_string(&found).unwrap();
    assert!(text.contains("容灾"));
    assert!(!text.contains("尚未授权"));
    fixture
        .act(json!({"operation":"revoke_conversation","source_conversation_id":source}))
        .unwrap();
    assert!(
        fixture
            .store
            .memory_grant(&source, &fixture.conversation)
            .unwrap()
            .is_none()
    );
}

#[test]
fn blocked_categories_and_scope_are_enforced_by_core() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("light", "我希望使用浅色主题");
    fixture
        .act(json!({"operation":"confirm","claim_id":claim.id}))
        .unwrap();
    let other = fixture
        .store
        .create_conversation(Some("其他讨论"), None)
        .unwrap();
    assert!(
        fixture
            .store
            .search_memories(&other, &json!({}))
            .unwrap()
            .is_empty()
    );
    assert!(
        fixture
            .store
            .memory_action_basis(&other, &json!({"operation":"delete","claim_id":claim.id}))
            .is_err()
    );
    fixture
        .act(json!({"operation":"block_kind","kind":"procedural"}))
        .unwrap();
    assert!(fixture.store.propose_memory(&fixture.conversation,&json!({"page_slug":"light","kind":"procedural","subject":"user","predicate":"theme"})).is_err());
}

#[test]
fn deleted_claim_cannot_return_through_changed_page_or_review_evidence() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("theme", "我希望使用浅色主题");
    fixture
        .act(json!({"operation":"delete","claim_id":claim.id}))
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "UPDATE wiki_pages SET content_md='我希望使用深色主题' WHERE slug='theme'",
            [],
        )
        .unwrap();
    let replay = fixture
        .store
        .propose_memory(
            &fixture.conversation,
            &json!({"page_slug":"theme","kind":"procedural","subject":"user","predicate":"theme"}),
        )
        .unwrap();
    assert_eq!(replay.id, claim.id);
    assert_eq!(replay.state, "deleted");
    let renamed = fixture.store.propose_memory(&fixture.conversation, &json!({"page_slug":"theme","kind":"semantic","subject":"myself","predicate":"display-preference"})).unwrap();
    assert_eq!(renamed.id, claim.id);
    assert_eq!(renamed.state, "deleted");
    let basis = fixture
        .store
        .memory_action_basis(
            &fixture.conversation,
            &json!({"operation":"restore","claim_id":claim.id}),
        )
        .unwrap();
    assert!(!basis.to_string().contains("浅色"));
    assert!(basis["claim"]["basis"][0]["quote"].is_null());
}

#[test]
fn chat_snapshot_permission_does_not_share_personal_claims_or_their_future_versions() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("theme", "我希望使用浅色主题");
    fixture
        .act(json!({"operation":"confirm","claim_id":claim.id}))
        .unwrap();
    let other = fixture
        .store
        .create_conversation(Some("另一个讨论"), None)
        .unwrap();
    let mut grant =
        json!({"operation":"share_conversation","source_conversation_id":fixture.conversation});
    grant["reviewed_basis"] = fixture.store.memory_action_basis(&other, &grant).unwrap();
    fixture.store.apply_memory_action(&other, &grant).unwrap();
    assert!(
        fixture
            .store
            .search_memories(&other, &json!({"include_history":true}))
            .unwrap()
            .is_empty()
    );
    assert!(fixture.store.propose_memory(&other, &json!({"page_slug":"theme","kind":"procedural","subject":"user","predicate":"theme"})).is_err());
}

#[test]
fn duplicate_claim_merges_sources_without_counting_independent_support() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("theme", "我希望使用浅色主题");
    let next = fixture
        .store
        .send_message(&fixture.conversation, "user", "我希望使用浅色主题", None)
        .unwrap();
    let args = json!({"page_slug":"theme","kind":"procedural","subject":"user","predicate":"theme","source_message_id":next});
    let merged = fixture
        .store
        .propose_memory(&fixture.conversation, &args)
        .unwrap();
    let replay = fixture
        .store
        .propose_memory(&fixture.conversation, &args)
        .unwrap();
    assert_eq!(merged.id, claim.id);
    assert_eq!(merged.basis.len(), 2);
    assert_eq!(merged.version, claim.version + 1);
    assert_eq!(replay.version, merged.version);
    assert_eq!(merged.state, "pending");
    assert_eq!(merged.support, "unassessed");
    assert!(
        merged
            .basis
            .iter()
            .all(|source| source.independent_group.is_none())
    );
}

#[test]
fn explicit_self_report_is_bound_to_verbatim_user_input_without_second_confirmation() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("theme", "我希望使用浅色主题");
    fixture
        .store
        .connection
        .execute(
            "DELETE FROM personal_memory_claims WHERE id=?1",
            [&claim.id],
        )
        .unwrap();
    let args = json!({"page_slug":"theme","kind":"procedural","subject":"user","predicate":"theme","source_message_id":claim.basis[0].id,"explicit_preference":true});
    let recorded = fixture
        .store
        .propose_memory(&fixture.conversation, &args)
        .unwrap();
    assert_eq!(recorded.state, "active");
    assert_eq!(recorded.scope, "conversation");
    assert_eq!(recorded.support, "unassessed");
    let audit: String = fixture
        .store
        .connection
        .query_row(
            "SELECT args_json FROM personal_memory_decisions WHERE claim_id=?1",
            [&recorded.id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(audit.contains("explicit_user_input"));
    for text in [
        "如果可以，我希望使用深色主题",
        "我希望使用深色主题（假设）",
        "我希望使用“深色主题”",
    ] {
        let slug = uuid::Uuid::new_v4().to_string();
        let rejected = fixture.candidate(&slug, text);
        fixture
            .store
            .connection
            .execute(
                "DELETE FROM personal_memory_claims WHERE id=?1",
                [&rejected.id],
            )
            .unwrap();
        let mut args = args.clone();
        args["page_slug"] = slug.into();
        args["source_message_id"] = rejected.basis[0].id.clone().into();
        assert!(
            fixture
                .store
                .propose_memory(&fixture.conversation, &args)
                .is_err()
        );
    }
}

#[test]
fn existing_pending_claim_accepts_verbatim_explicit_input_once() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("theme", "我希望使用浅色主题");
    let args = json!({"page_slug":"theme","kind":"procedural","subject":"user","predicate":"theme","source_message_id":claim.basis[0].id,"explicit_preference":true});
    let active = fixture
        .store
        .propose_memory(&fixture.conversation, &args)
        .unwrap();
    let replay = fixture
        .store
        .propose_memory(&fixture.conversation, &args)
        .unwrap();
    assert_eq!(active.id, claim.id);
    assert_eq!(active.state, "active");
    assert_eq!(replay.version, active.version);
}

#[test]
fn unavailable_source_grant_can_still_be_revoked() {
    let fixture = Fixture::new();
    let source = fixture.store.create_conversation(Some("源"), None).unwrap();
    fixture
        .store
        .send_message(&source, "user", "讨论主题", None)
        .unwrap();
    fixture
        .act(json!({"operation":"share_conversation","source_conversation_id":source}))
        .unwrap();
    fixture
        .store
        .connection
        .execute("UPDATE conversations SET archived=1 WHERE id=?1", [&source])
        .unwrap();
    assert!(
        fixture
            .store
            .memory_grant(&source, &fixture.conversation)
            .unwrap()
            .is_none()
    );
    fixture
        .act(json!({"operation":"revoke_conversation","source_conversation_id":source}))
        .unwrap();
    let enabled: bool = fixture
        .store
        .connection
        .query_row(
            "SELECT enabled FROM personal_memory_grants WHERE source_id=?1",
            [&source],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!enabled);
}

#[test]
fn reviewed_time_window_limits_recall_and_can_be_renewed_without_changing_text() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("theme", "我希望使用浅色主题");
    fixture
        .act(
            json!({"operation":"confirm","claim_id":claim.id,"valid_until":"2000-01-01T00:00:00Z"}),
        )
        .unwrap();
    assert!(
        fixture
            .store
            .search_memories(&fixture.conversation, &json!({}))
            .unwrap()
            .is_empty()
    );
    fixture
        .act(json!({"operation":"confirm","claim_id":claim.id,"valid_from":"2099-01-01T00:00:00Z"}))
        .unwrap();
    assert!(
        fixture
            .store
            .search_memories(&fixture.conversation, &json!({}))
            .unwrap()
            .is_empty()
    );
    fixture
        .act(json!({"operation":"confirm","claim_id":claim.id}))
        .unwrap();
    assert_eq!(
        fixture
            .store
            .search_memories(&fixture.conversation, &json!({}))
            .unwrap()
            .len(),
        1
    );
    assert!(fixture.act(json!({"operation":"confirm","claim_id":claim.id,"valid_from":"2099-01-01T00:00:00Z","valid_until":"2000-01-01T00:00:00Z"})).is_err());
}

#[test]
fn reference_scope_is_not_promoted_to_personal_background_and_review_shows_claim_text() {
    let fixture = Fixture::new();
    let claim = fixture.candidate("theme", "我希望使用浅色主题");
    let basis = fixture
        .store
        .memory_action_basis(
            &fixture.conversation,
            &json!({"operation":"confirm","claim_id":claim.id}),
        )
        .unwrap();
    assert_eq!(basis["content_excerpt"], "我希望使用浅色主题");
    assert_eq!(basis["complete"], true);
    fixture
        .act(json!({"operation":"confirm","claim_id":claim.id,"scope":"reference"}))
        .unwrap();
    assert!(fixture.store.memory_page_current("theme").unwrap());
    assert!(
        !fixture
            .store
            .memory_page_background_current("theme", &fixture.conversation)
            .unwrap()
    );
    fixture
        .act(json!({"operation":"confirm","claim_id":claim.id,"scope":"conversation"}))
        .unwrap();
    assert!(
        fixture
            .store
            .memory_page_background_current("theme", &fixture.conversation)
            .unwrap()
    );
}
