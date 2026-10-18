use super::*;

struct Fixture {
    store: Store,
    path: std::path::PathBuf,
    conversation: String,
    input: String,
}
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("elsewhen-run-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path).unwrap();
        let conversation = store
            .create_conversation(Some("run fixture"), None)
            .unwrap();
        let input = store
            .send_message(&conversation, "user", "original input", None)
            .unwrap();
        Store::mark_initialized(&path).unwrap();
        Self {
            store,
            path,
            conversation,
            input,
        }
    }
    fn run(&self) -> ChatRun {
        self.store
            .create_chat_run(&self.conversation, &self.input, "key", "owner")
            .unwrap()
    }
    fn clean(self) {
        drop(self.store);
        let _ = std::fs::remove_file(self.path);
    }
}

fn event(text: &str) -> crate::event::NewEvent<'_> {
    let now = chrono::Utc::now();
    crate::event::NewEvent {
        raw_text: text,
        occurred_at: now,
        recorded_at: now,
        source: "tool",
    }
}

#[test]
fn cancellation_flush_cannot_revive_terminal_run_or_start_actions() {
    let f = Fixture::new();
    let run = f.run();
    f.store.cancel_chat_run(&f.conversation, &run.id).unwrap();
    assert!(
        f.store
            .append_chat_run_event(&run.id, "owner", "tool_started", &json!({"call_id":"late"}))
            .is_err()
    );
    f.store
        .flush_chat_run_checkpoint(&run.id, "owner", "safe prefix")
        .unwrap();
    f.store
        .finish_chat_run(&run.id, "owner", "cancelled", None, None)
        .unwrap();
    assert!(
        f.store
            .flush_chat_run_checkpoint(&run.id, "owner", "late")
            .is_err()
    );
    assert_eq!(
        f.store.chat_run(&run.id).unwrap().unwrap().content,
        "safe prefix"
    );
    f.clean();
}

#[test]
fn missing_tool_receipt_is_explicitly_uncertain_after_recovery() {
    let f = Fixture::new();
    let run = f.run();
    f.store
        .append_chat_run_event(
            &run.id,
            "owner",
            "tool_started",
            &json!({"call_id":"unfinished","name":"lookup"}),
        )
        .unwrap();
    f.store
        .connection
        .execute(
            "UPDATE chat_runs SET lease_until='2000-01-01' WHERE id=?1",
            [&run.id],
        )
        .unwrap();
    assert_eq!(f.store.recover_expired_chat_runs().unwrap(), 1);
    assert_eq!(f.store.recover_expired_chat_runs().unwrap(), 0);
    let events = f.store.chat_run_events(&run.id, 0, 100).unwrap();
    assert_eq!(
        events.iter().filter(|e| e.kind == "tool_uncertain").count(),
        1
    );
    assert_eq!(events.last().unwrap().kind, "interrupted");
    f.clean();
}

#[test]
fn durable_context_basis_rejects_edits_but_not_later_input() {
    let f = Fixture::new();
    let run = f.run();
    f.store
        .send_message(&f.conversation, "user", "later", None)
        .unwrap();
    let snapshot = f
        .store
        .snapshot_for_input(&f.conversation, &f.input)
        .unwrap();
    snapshot
        .validate_chat_run_context(&run.id, &f.conversation)
        .unwrap();
    f.store
        .connection
        .execute(
            "UPDATE messages SET content='edited' WHERE id=?1",
            [&f.input],
        )
        .unwrap();
    let changed = f
        .store
        .snapshot_for_input(&f.conversation, &f.input)
        .unwrap();
    assert!(
        changed
            .validate_chat_run_context(&run.id, &f.conversation)
            .is_err()
    );
    // A worker already holding its fixed context continues to see that exact basis.
    snapshot
        .validate_chat_run_context(&run.id, &f.conversation)
        .unwrap();
    drop(changed);
    drop(snapshot);
    f.clean();
}

#[test]
fn fixed_input_snapshot_excludes_later_messages_and_their_events() {
    let f = Fixture::new();
    let snapshot = f
        .store
        .snapshot_for_input(&f.conversation, &f.input)
        .unwrap();
    f.store
        .submit_conversation_input(&f.conversation, "later input", Some("later"))
        .unwrap();
    assert_eq!(snapshot.list_messages(&f.conversation).unwrap().len(), 1);
    assert_eq!(
        snapshot.list_messages(&f.conversation).unwrap()[0].content,
        "original input"
    );
    assert!(
        snapshot
            .latest_event_id_for_conversation(&f.conversation)
            .unwrap()
            .is_none()
    );
    assert!(
        f.store
            .latest_event_id_for_conversation(&f.conversation)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        snapshot
            .clone()
            .list_messages(&f.conversation)
            .unwrap()
            .len(),
        1
    );
    drop(snapshot);
    f.clean();
}

#[test]
fn direct_capture_is_committed_once_with_both_jobs_and_input_link() {
    let f = Fixture::new();
    let snapshot = f
        .store
        .snapshot_for_input(&f.conversation, &f.input)
        .unwrap();
    let id = snapshot.insert_event(event("captured fact")).unwrap();
    let again = snapshot
        .insert_event(event("do not rewrite the original"))
        .unwrap();
    assert_eq!(id, again);
    assert_eq!(f.store.list_events().unwrap().len(), 1);
    assert_eq!(f.store.list_events().unwrap()[0].raw_text, "captured fact");
    assert_eq!(
        snapshot
            .latest_event_id_for_conversation(&f.conversation)
            .unwrap(),
        Some(id.clone())
    );
    for table in ["analysis_jobs", "knowledge_digest_jobs"] {
        let count: i64 = f
            .store
            .connection
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE event_id=?1"),
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
    drop(snapshot);
    f.clean();
}

#[test]
fn deleted_capture_leaves_a_tombstone_instead_of_recreating_it() {
    let f = Fixture::new();
    let snapshot = f
        .store
        .snapshot_for_input(&f.conversation, &f.input)
        .unwrap();
    let id = snapshot.insert_event(event("one fact")).unwrap();
    // Imported legacy databases may already contain a removed source link.
    // Simulate the tombstone without deleting append-only production events.
    f.store
        .connection
        .execute(
            "UPDATE chat_input_effects SET event_id=NULL WHERE input_message_id=?1",
            [&f.input],
        )
        .unwrap();
    assert!(snapshot.insert_event(event("one fact")).is_err());
    assert_eq!(f.store.list_events().unwrap()[0].id, id);
    drop(snapshot);
    f.clean();
}

#[test]
fn every_reopened_connection_enforces_foreign_keys() {
    let f = Fixture::new();
    let reopened = Store::open(&f.path).unwrap();
    let enabled: i64 = reopened
        .connection
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(enabled, 1);
    assert!(
        reopened
            .send_message("missing-conversation", "user", "not orphaned", None)
            .is_err()
    );
    drop(reopened);
    f.clean();
}

#[test]
fn request_keys_bind_input_and_only_one_run_is_active() {
    let f = Fixture::new();
    let run = f.run();
    let same = f
        .store
        .create_chat_run(&f.conversation, &f.input, "key", "other-owner")
        .unwrap();
    assert_eq!(same.id, run.id);
    assert!(
        f.store
            .create_chat_run(&f.conversation, &f.input, "different", "other")
            .is_err()
    );
    let other = f
        .store
        .send_message(&f.conversation, "user", "other input", None)
        .unwrap();
    assert!(
        f.store
            .create_chat_run(&f.conversation, &other, "key", "owner")
            .is_err()
    );
    f.clean();
}

#[test]
fn event_sequence_and_text_snapshot_remain_consistent() {
    let f = Fixture::new();
    let run = f.run();
    f.store
        .append_chat_run_event(&run.id, "owner", "phase", &json!({"phase":"requesting"}))
        .unwrap();
    f.store
        .append_chat_run_event(&run.id, "owner", "text_delta", &json!({"text":"first"}))
        .unwrap();
    let snapshot = f.store.chat_run(&run.id).unwrap().unwrap();
    f.store
        .append_chat_run_event(&run.id, "owner", "text_delta", &json!({"text":" second"}))
        .unwrap();
    assert_eq!(
        f.store.chat_run_draft(&run.id, snapshot.sequence).unwrap(),
        "first"
    );
    let later = f
        .store
        .chat_run_events(&run.id, snapshot.sequence, 10)
        .unwrap();
    assert_eq!(later.len(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(&later[0].payload_json).unwrap()["text"],
        " second"
    );
    assert_eq!(
        f.store.chat_run(&run.id).unwrap().unwrap().content,
        "first second"
    );
    f.store
        .append_chat_run_event(&run.id, "owner", "draft_reset", &json!({}))
        .unwrap();
    assert_eq!(f.store.chat_run(&run.id).unwrap().unwrap().content, "");
    assert!(
        f.store
            .append_chat_run_event(&run.id, "wrong-owner", "text_delta", &json!({"text":"bad"}))
            .is_err()
    );
    f.clean();
}

#[test]
fn completion_is_atomic_and_terminal_events_cannot_be_replayed() {
    let f = Fixture::new();
    let run = f.run();
    let event = f
        .store
        .finish_chat_run(&run.id, "owner", "completed", Some("verified answer"), None)
        .unwrap();
    let completed = f.store.chat_run(&run.id).unwrap().unwrap();
    assert_eq!(completed.status, "completed");
    assert_eq!(completed.sequence, event.sequence);
    let messages = f.store.list_messages(&f.conversation).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(Some(messages[1].id.clone()), completed.final_message_id);
    assert_eq!(messages[1].content, "verified answer");
    assert!(
        f.store
            .finish_chat_run(&run.id, "owner", "completed", Some("duplicate"), None)
            .is_err()
    );
    assert!(
        f.store
            .append_chat_run_event(&run.id, "owner", "text_delta", &json!({"text":"late"}))
            .is_err()
    );
    assert!(
        f.store
            .cancel_chat_run(&f.conversation, &run.id)
            .unwrap()
            .is_none()
    );
    assert_eq!(f.store.list_messages(&f.conversation).unwrap().len(), 2);
    f.clean();
}

#[test]
fn cancellation_fences_completion_but_keeps_partial_content_as_run_data() {
    let f = Fixture::new();
    let run = f.run();
    f.store
        .append_chat_run_event(&run.id, "owner", "text_delta", &json!({"text":"partial"}))
        .unwrap();
    assert!(
        f.store
            .cancel_chat_run("wrong-conversation", &run.id)
            .unwrap()
            .is_none()
    );
    assert!(
        f.store
            .cancel_chat_run(&f.conversation, &run.id)
            .unwrap()
            .is_some()
    );
    assert!(
        f.store
            .finish_chat_run(&run.id, "owner", "completed", Some("late answer"), None)
            .is_err()
    );
    f.store
        .finish_chat_run(&run.id, "owner", "cancelled", None, None)
        .unwrap();
    let cancelled = f.store.chat_run(&run.id).unwrap().unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(cancelled.content, "partial");
    assert!(cancelled.final_message_id.is_none());
    assert_eq!(f.store.list_messages(&f.conversation).unwrap().len(), 1);
    assert!(
        f.store
            .create_chat_run(&f.conversation, &f.input, "retry", "new-owner")
            .is_ok()
    );
    f.clean();
}

#[test]
fn expiry_retains_tool_results_and_releases_active_slot_without_replay() {
    let f = Fixture::new();
    let run = f.run();
    f.store
        .append_chat_run_event(
            &run.id,
            "owner",
            "tool_result",
            &json!({"call_id":"call","content":"actual result","success":true}),
        )
        .unwrap();
    let (request, _) = f
        .store
        .start_chat_request(&run.id, "owner", "fixture", None)
        .unwrap();
    assert_eq!(f.store.recover_expired_chat_runs().unwrap(), 0);
    f.store
        .connection
        .execute(
            "UPDATE chat_runs SET lease_until='2000-01-01T00:00:00+00:00' WHERE id=?1",
            [&run.id],
        )
        .unwrap();
    assert_eq!(f.store.recover_expired_chat_runs().unwrap(), 1);
    assert_eq!(
        f.store.chat_run(&run.id).unwrap().unwrap().status,
        "interrupted"
    );
    let state: String = f
        .store
        .connection
        .query_row(
            "SELECT status FROM chat_run_requests WHERE id=?1",
            [request],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "interrupted");
    assert!(
        f.store
            .chat_run_events(&run.id, 0, 100)
            .unwrap()
            .iter()
            .any(|e| e.kind == "tool_result")
    );
    assert!(
        f.store
            .finish_chat_run(&run.id, "owner", "completed", Some("late"), None)
            .is_err()
    );
    assert!(
        f.store
            .create_chat_run(&f.conversation, &f.input, "after-recovery", "new-owner")
            .is_ok()
    );
    f.clean();
}

#[test]
fn requests_have_individual_usage_and_history_lists_are_metadata_only() {
    let f = Fixture::new();
    let run = f.run();
    let (request, _) = f
        .store
        .start_chat_request(&run.id, "owner", "fixture", Some("model"))
        .unwrap();
    f.store
        .finish_chat_request(
            &run.id,
            "owner",
            &request,
            "completed",
            Some("model"),
            Some((20, 10)),
            Some(12),
        )
        .unwrap();
    assert!(
        f.store
            .finish_chat_request(&run.id, "owner", &request, "completed", None, None, None)
            .is_err()
    );
    assert_eq!(f.store.chat_run(&run.id).unwrap().unwrap().request_count, 1);
    f.store
        .finish_chat_run(&run.id, "owner", "completed", Some("full body"), None)
        .unwrap();
    assert!(
        f.store.list_chat_runs(&f.conversation, 0, 10).unwrap()[0]
            .content
            .is_empty()
    );
    assert_eq!(
        f.store.chat_run(&run.id).unwrap().unwrap().content,
        "full body"
    );
    f.clean();
}

#[test]
fn deleting_a_conversation_removes_its_run_and_blocks_late_writes() {
    let f = Fixture::new();
    let run = f.run();
    f.store
        .set_conversation_archived(&f.conversation, true)
        .unwrap();
    f.store
        .delete_archived_conversation(&f.conversation)
        .unwrap();
    assert!(f.store.chat_run(&run.id).unwrap().is_none());
    assert!(f.store.chat_run_events(&run.id, 0, 100).unwrap().is_empty());
    assert!(
        f.store
            .finish_chat_run(&run.id, "owner", "completed", Some("zombie"), None)
            .is_err()
    );
    assert!(f.store.get_conversation(&f.conversation).unwrap().is_none());
    f.clean();
}

/// P1 验收口径 2 的观测链：request_budget 事件里的占位化条数要落到
/// chat_run_requests 列上（每次覆盖，终态行留着最后一次装配的值）。
#[test]
fn request_budget_event_records_pruned_tool_results() {
    let f = Fixture::new();
    let run = f.run();
    let (request, _) = f
        .store
        .start_chat_request(&run.id, "owner", "fixture", Some("model"))
        .unwrap();
    let report = |pruned: usize| {
        serde_json::json!({
            "request_id": request,
            "attempt": 1,
            "trigger": "initial",
            "report": {"pruned_tool_results": pruned}
        })
    };
    f.store
        .append_chat_run_event(&run.id, "owner", "request_budget", &report(3))
        .unwrap();
    // 第二次装配未占位化：覆盖为 0 而不是保留旧值（重试链取末次）。
    f.store
        .append_chat_run_event(&run.id, "owner", "request_budget", &report(0))
        .unwrap();
    let stored: Option<i64> = f
        .store
        .connection
        .query_row(
            "SELECT pruned_tool_results FROM chat_run_requests WHERE id=?1",
            [&request],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, Some(0), "终态行应留最后一次装配的值");
    // 非预算事件不得碰这列。
    f.store
        .append_chat_run_event(
            &run.id,
            "owner",
            "tool_result",
            &serde_json::json!({"call_id":"c","content":"x","success":true}),
        )
        .unwrap();
    let still: Option<i64> = f
        .store
        .connection
        .query_row(
            "SELECT pruned_tool_results FROM chat_run_requests WHERE id=?1",
            [&request],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(still, Some(0));
    f.clean();
}
