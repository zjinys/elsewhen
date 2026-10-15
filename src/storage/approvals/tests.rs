use super::*;
use crate::ai::approval::resolve_chat_proposal;
use crate::storage::RuleStatus;

struct Fixture {
    store: Store,
    path: std::path::PathBuf,
    conversation: String,
}
impl Fixture {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("elsewhen-approval-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path).unwrap();
        let conversation = store
            .create_conversation(Some("review fixture"), None)
            .unwrap();
        Self {
            store,
            path,
            conversation,
        }
    }
    fn todo(&self, title: &str) -> ChatProposal {
        let id = self
            .store
            .create_pending_action(
                &self.conversation,
                "create_todo",
                &json!({"title":title}).to_string(),
            )
            .unwrap();
        self.store
            .chat_proposal("action", &id, Some(&self.conversation))
            .unwrap()
            .unwrap()
    }
    fn resolve(&self, proposal: &ChatProposal, decision: &str) -> Result<ChatDecision> {
        resolve_chat_proposal(
            &self.store,
            &proposal.kind,
            &proposal.id,
            proposal.conversation_id.as_deref(),
            proposal.version,
            &proposal.approval_token,
            decision,
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn claim(f: &Fixture, proposal: &ChatProposal) -> ChatProposal {
    match f
        .store
        .claim_chat_proposal(
            &proposal.kind,
            &proposal.id,
            proposal.conversation_id.as_deref(),
            proposal.version,
            &proposal.approval_token,
            "accept",
        )
        .unwrap()
    {
        ProposalClaim::Ready(proposal) => proposal,
        ProposalClaim::Existing(_) => panic!("unexpected existing decision"),
    }
}

#[test]
fn target_change_after_claim_is_checked_inside_the_write_transaction() {
    let f = Fixture::new();
    let page = crate::wiki::save_text_page("original", Some("target"), &[], &f.store).unwrap();
    let id = f
        .store
        .create_pending_action(
            &f.conversation,
            "rename_wiki_page",
            &json!({"slug":page.slug,"new_title":"AI rename"}).to_string(),
        )
        .unwrap();
    let proposal = f
        .store
        .chat_proposal("action", &id, Some(&f.conversation))
        .unwrap()
        .unwrap();
    let claimed = claim(&f, &proposal);
    f.store
        .rename_wiki_page(&page.slug, "human edit", "manual")
        .unwrap();
    let receipt = crate::ai::approval::execute_claimed_proposal(&f.store, &claimed).unwrap();
    assert_eq!(receipt.status, "failed");
    assert_eq!(
        f.store.get_wiki_page(&page.slug).unwrap().unwrap().title,
        "human edit"
    );
}

#[test]
fn archive_target_change_after_claim_does_not_archive_new_state() {
    let f = Fixture::new();
    let target = f
        .store
        .create_conversation(Some("old title"), None)
        .unwrap();
    let id = f
        .store
        .create_pending_action(
            &f.conversation,
            "archive_conversations_by_title",
            &json!({"ids":[target],"titles":["old title"]}).to_string(),
        )
        .unwrap();
    let proposal = f
        .store
        .chat_proposal("action", &id, Some(&f.conversation))
        .unwrap()
        .unwrap();
    let claimed = claim(&f, &proposal);
    f.store.rename_conversation(&target, "new title").unwrap();
    assert_eq!(
        crate::ai::approval::execute_claimed_proposal(&f.store, &claimed)
            .unwrap()
            .status,
        "failed"
    );
    assert!(!f.store.get_conversation(&target).unwrap().unwrap().archived);
}

#[test]
fn name_resolution_changes_invalidate_relationship_tokens() {
    let f = Fixture::new();
    let id = f
        .store
        .create_pending_action(
            &f.conversation,
            "propose_people_relations",
            &json!({"people":[{"name":"Alex"}],"relations":[{"person":"Alex","target":"Project"}]})
                .to_string(),
        )
        .unwrap();
    let proposal = f
        .store
        .chat_proposal("action", &id, Some(&f.conversation))
        .unwrap()
        .unwrap();
    crate::wiki::save_text_page("new material", Some("Alex"), &[], &f.store).unwrap();
    assert!(f.resolve(&proposal, "accept").is_err());
}

#[test]
fn relationship_disambiguation_can_be_edited_but_source_identity_cannot() {
    let f = Fixture::new();
    let person = crate::wiki::save_text_page("person", Some("Alex"), &[], &f.store).unwrap();
    let target = crate::wiki::save_text_page("project", Some("Project"), &[], &f.store).unwrap();
    let args =
        json!({"people":[{"name":"Alex"}],"relations":[{"person":"Alex","target":"Project"}]});
    let id = f
        .store
        .create_pending_action(
            &f.conversation,
            "propose_people_relations",
            &args.to_string(),
        )
        .unwrap();
    let proposal = f
        .store
        .chat_proposal("action", &id, Some(&f.conversation))
        .unwrap()
        .unwrap();
    let mut edited = args.clone();
    edited["relations"][0]["from_slug"] = json!(person.slug);
    edited["relations"][0]["to_slug"] = json!(target.slug);
    let updated = f
        .store
        .edit_chat_proposal(
            "action",
            &id,
            Some(&f.conversation),
            proposal.version,
            &proposal.approval_token,
            &edited.to_string(),
        )
        .unwrap();
    assert!(updated.version > proposal.version);
    edited["source_event_id"] = json!("forged");
    assert!(
        f.store
            .edit_chat_proposal(
                "action",
                &id,
                Some(&f.conversation),
                updated.version,
                &updated.approval_token,
                &edited.to_string()
            )
            .is_err()
    );
    assert_eq!(f.resolve(&updated, "accept").unwrap().status, "succeeded");
}

#[test]
fn invalid_explicit_relation_target_never_falls_back_by_name() {
    let f = Fixture::new();
    crate::wiki::save_text_page("person", Some("Alex"), &[], &f.store).unwrap();
    let before = f.store.list_wiki_pages(None, None).unwrap().len();
    let args = json!({"people":[{"name":"New person"},{"name":"Alex"}],"relations":[{"person":"Alex","target":"Project","from_slug":"missing-person"}]});
    let id = f
        .store
        .create_pending_action(
            &f.conversation,
            "propose_people_relations",
            &args.to_string(),
        )
        .unwrap();
    let p = f
        .store
        .chat_proposal("action", &id, Some(&f.conversation))
        .unwrap()
        .unwrap();
    assert_eq!(f.resolve(&p, "accept").unwrap().status, "failed");
    assert_eq!(f.store.list_wiki_pages(None, None).unwrap().len(), before);
}

#[test]
fn recovery_preserves_live_other_owner_and_late_completion_is_fenced() {
    let f = Fixture::new();
    let p = f.todo("lease");
    let claimed = claim(&f, &p);
    f.store.connection.execute("UPDATE chat_proposal_decisions SET executor_id='other-live-process' WHERE proposal_id=?1", [&p.id]).unwrap();
    assert_eq!(f.store.recover_chat_proposal_decisions().unwrap(), 0);
    assert!(
        f.store
            .finish_chat_proposal(&claimed, Ok("late".into()))
            .is_err()
    );
    f.store.connection.execute("UPDATE chat_proposal_decisions SET lease_until='2000-01-01T00:00:00+00:00' WHERE proposal_id=?1", [&p.id]).unwrap();
    assert_eq!(f.store.recover_chat_proposal_decisions().unwrap(), 1);
    assert!(crate::ai::approval::execute_claimed_proposal(&f.store, &claimed).is_err());
    assert!(f.store.list_todos(None).unwrap().is_empty());
}

#[test]
fn corrupt_proposal_can_be_rejected_without_blocking_other_proposals() {
    let f = Fixture::new();
    f.todo("good");
    let id = f
        .store
        .create_pending_action(&f.conversation, "create_todo", "broken-json")
        .unwrap();
    let proposals = f
        .store
        .list_chat_proposals(Some(&f.conversation), 0, 10)
        .unwrap();
    assert_eq!(proposals.len(), 2);
    let corrupt = proposals.iter().find(|p| p.id == id).unwrap();
    assert!(f.resolve(corrupt, "accept").is_err());
    assert_eq!(f.resolve(corrupt, "reject").unwrap().status, "declined");
}

#[test]
fn concurrent_confirmation_executes_the_business_action_once() {
    let f = Fixture::new();
    let proposal = f.todo("concurrent");
    Store::mark_initialized(&f.path).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut threads = Vec::new();
    for _ in 0..2 {
        let path = f.path.clone();
        let proposal = proposal.clone();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            let store = Store::open(&path).unwrap();
            barrier.wait();
            resolve_chat_proposal(
                &store,
                &proposal.kind,
                &proposal.id,
                proposal.conversation_id.as_deref(),
                proposal.version,
                &proposal.approval_token,
                "accept",
            )
            .unwrap()
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(f.store.list_todos(None).unwrap().len(), 1);
    assert_eq!(f.resolve(&proposal, "accept").unwrap().status, "succeeded");
}

#[test]
fn only_selected_action_executes_and_duplicate_confirmation_returns_receipt() {
    let f = Fixture::new();
    let a = f.todo("A");
    let b = f.todo("B");
    let receipt = f.resolve(&a, "accept").unwrap();
    assert_eq!(receipt.status, "succeeded");
    assert_eq!(f.store.list_todos(None).unwrap().len(), 1);
    let duplicate = f.resolve(&a, "accept").unwrap();
    assert_eq!(duplicate.result, receipt.result);
    assert_eq!(f.store.list_todos(None).unwrap().len(), 1);
    assert!(
        f.store
            .chat_proposal("action", &b.id, Some(&f.conversation))
            .unwrap()
            .is_some()
    );
    assert!(f.resolve(&a, "reject").is_err());
}

#[test]
fn cross_conversation_and_wrong_kind_do_not_authorize_a_write() {
    let f = Fixture::new();
    let a = f.todo("A");
    let other = f.store.create_conversation(None, None).unwrap();
    assert!(
        resolve_chat_proposal(
            &f.store,
            "action",
            &a.id,
            Some(&other),
            a.version,
            &a.approval_token,
            "accept"
        )
        .is_err()
    );
    assert!(
        resolve_chat_proposal(
            &f.store,
            "rule",
            &a.id,
            Some(&f.conversation),
            a.version,
            &a.approval_token,
            "accept"
        )
        .is_err()
    );
    assert!(f.store.list_todos(None).unwrap().is_empty());
}

#[test]
fn edits_and_reverts_invalidate_old_approval_versions() {
    let f = Fixture::new();
    let original = f.todo("A");
    f.store
        .update_pending_action_args(&original.id, &json!({"title":"B"}).to_string())
        .unwrap();
    f.store
        .update_pending_action_args(&original.id, &original.args_json)
        .unwrap();
    let latest = f
        .store
        .chat_proposal("action", &original.id, Some(&f.conversation))
        .unwrap()
        .unwrap();
    assert_eq!(latest.version, original.version + 2);
    assert!(f.resolve(&original, "accept").is_err());
    assert_eq!(f.resolve(&latest, "accept").unwrap().status, "succeeded");
}

#[test]
fn reviewed_edit_preserves_target_and_source_fields() {
    let f = Fixture::new();
    let id = f
        .store
        .create_pending_action(
            &f.conversation,
            "save_wiki_revision",
            &json!({"slug":"p","base_hash":"basis","content_md":"old"}).to_string(),
        )
        .unwrap();
    let p = f
        .store
        .chat_proposal("action", &id, Some(&f.conversation))
        .unwrap()
        .unwrap();
    assert!(
        f.store
            .edit_chat_proposal(
                "action",
                &id,
                Some(&f.conversation),
                p.version,
                &p.approval_token,
                &json!({"slug":"other","base_hash":"basis","content_md":"new"}).to_string()
            )
            .is_err()
    );
    let edited = f
        .store
        .edit_chat_proposal(
            "action",
            &id,
            Some(&f.conversation),
            p.version,
            &p.approval_token,
            &json!({"slug":"p","base_hash":"basis","content_md":"new"}).to_string(),
        )
        .unwrap();
    assert!(edited.version > p.version);
    assert!(f.resolve(&p, "accept").is_err());
}

#[test]
fn target_revision_change_invalidates_the_preview_token() {
    let f = Fixture::new();
    let page = crate::wiki::save_text_page("original", Some("target"), &[], &f.store).unwrap();
    let id = f
        .store
        .create_pending_action(
            &f.conversation,
            "rename_wiki_page",
            &json!({"slug":page.slug,"new_title":"renamed"}).to_string(),
        )
        .unwrap();
    let p = f
        .store
        .chat_proposal("action", &id, Some(&f.conversation))
        .unwrap()
        .unwrap();
    f.store
        .connection
        .execute(
            "UPDATE wiki_pages SET content_md='changed' WHERE id=?1",
            [&page.id],
        )
        .unwrap();
    assert!(f.resolve(&p, "accept").is_err());
    assert_eq!(
        f.store.get_wiki_page(&page.slug).unwrap().unwrap().title,
        "target"
    );
    assert!(
        f.store
            .chat_decision("action", &id, Some(&f.conversation))
            .unwrap()
            .is_none()
    );
}

#[test]
fn rules_are_approved_individually_and_legacy_global_rules_are_not_in_conversations() {
    let f = Fixture::new();
    let a = f
        .store
        .add_rule("A", RuleStatus::Pending, Some(&f.conversation))
        .unwrap();
    let b = f
        .store
        .add_rule("B", RuleStatus::Pending, Some(&f.conversation))
        .unwrap();
    let global = f
        .store
        .add_rule("legacy", RuleStatus::Pending, None)
        .unwrap();
    let p = f
        .store
        .chat_proposal("rule", &a, Some(&f.conversation))
        .unwrap()
        .unwrap();
    assert_eq!(f.resolve(&p, "accept").unwrap().status, "succeeded");
    assert_eq!(f.store.list_active_rules().unwrap().len(), 1);
    let pending = f
        .store
        .list_chat_proposals(Some(&f.conversation), 0, 100)
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, b);
    assert!(
        f.store
            .chat_proposal("rule", &global, Some(&f.conversation))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        f.store.list_chat_proposals(None, 0, 100).unwrap()[0].id,
        global
    );
}

#[test]
fn rejected_rule_keeps_an_auditable_snapshot_and_does_not_activate() {
    let f = Fixture::new();
    let id = f
        .store
        .add_rule("draft", RuleStatus::Pending, Some(&f.conversation))
        .unwrap();
    let p = f
        .store
        .chat_proposal("rule", &id, Some(&f.conversation))
        .unwrap()
        .unwrap();
    assert_eq!(f.resolve(&p, "reject").unwrap().status, "declined");
    assert!(f.store.list_active_rules().unwrap().is_empty());
    let args: String = f
        .store
        .connection
        .query_row(
            "SELECT args_json FROM chat_proposal_decisions WHERE proposal_id=?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(args.contains("draft"));
    assert!(f.resolve(&p, "accept").is_err());
}

#[test]
fn interrupted_claim_does_not_replay_an_already_committed_business_write() {
    let f = Fixture::new();
    let p = f.todo("once");
    assert!(matches!(
        f.store
            .claim_chat_proposal(
                &p.kind,
                &p.id,
                p.conversation_id.as_deref(),
                p.version,
                &p.approval_token,
                "accept"
            )
            .unwrap(),
        ProposalClaim::Ready(_)
    ));
    // Simulate a crash after the business commit, before its result receipt.
    f.store
        .create_todo("once", "normal", None, None, None, None)
        .unwrap();
    // A still-valid lease must survive another bridge initialization.
    assert_eq!(f.store.recover_chat_proposal_decisions().unwrap(), 0);
    f.store.connection.execute("UPDATE chat_proposal_decisions SET lease_until='2000-01-01T00:00:00+00:00' WHERE proposal_id=?1", [&p.id]).unwrap();
    assert_eq!(f.store.recover_chat_proposal_decisions().unwrap(), 1);
    assert_eq!(f.resolve(&p, "accept").unwrap().status, "uncertain");
    assert_eq!(f.store.list_todos(None).unwrap().len(), 1);
    assert!(
        f.store
            .edit_chat_proposal(
                &p.kind,
                &p.id,
                p.conversation_id.as_deref(),
                p.version,
                &p.approval_token,
                &p.args_json
            )
            .is_err()
    );
}

#[test]
fn failed_atomic_execution_rolls_back_and_is_not_silently_retried() {
    let f = Fixture::new();
    let id = f
        .store
        .create_pending_action(&f.conversation, "create_todo", "{}")
        .unwrap();
    let p = f
        .store
        .chat_proposal("action", &id, Some(&f.conversation))
        .unwrap()
        .unwrap();
    assert_eq!(f.resolve(&p, "accept").unwrap().status, "failed");
    assert_eq!(f.resolve(&p, "accept").unwrap().status, "failed");
    assert!(f.store.list_todos(None).unwrap().is_empty());
}
