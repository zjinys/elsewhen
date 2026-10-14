use super::*;
use crate::event::NewEvent;
use rusqlite::OptionalExtension;
use uuid::Uuid;

struct Database {
    store: Store,
    dir: std::path::PathBuf,
}
impl Database {
    fn new(device: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("elsewhen-sync-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "test-dataset".into(),
                device_id: device.into(),
                stream_id: format!("stream-{device}"),
                processor_device_id: "a".into(),
            })
            .unwrap();
        Self { store, dir }
    }
    fn count(&self, table: &str) -> i64 {
        self.store
            .connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn pair(a: &Database, b: &Database) {
    let ai = a.store.sync_identity().unwrap().unwrap();
    let bi = b.store.sync_identity().unwrap().unwrap();
    a.store
        .authorize_sync_member(&bi.device_id, &bi.stream_id)
        .unwrap();
    b.store
        .authorize_sync_member(&ai.device_id, &ai.stream_id)
        .unwrap();
}
fn deliver(a: &Database, b: &Database) -> Vec<SyncOperation> {
    let ai = a.store.sync_identity().unwrap().unwrap();
    let bi = b.store.sync_identity().unwrap().unwrap();
    let ops = a.store.pending_sync_operations(&bi.device_id, 100).unwrap();
    for op in &ops {
        b.store
            .receive_sync_operation(&ai.device_id, &serde_json::to_vec(op).unwrap())
            .unwrap();
    }
    ops
}

#[test]
fn conversation_conflict_never_hides_the_new_immutable_fact() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let c = a.store.create_conversation(None, None).unwrap();
    deliver(&a, &b);
    let captured = b
        .store
        .submit_conversation_input(&c, "offline fact survives", None)
        .unwrap();
    a.store.set_conversation_archived(&c, true).unwrap();
    a.store.delete_archived_conversation(&c).unwrap();
    deliver(&b, &a);
    assert_eq!(a.count("events"), 1);
    assert_eq!(a.count("messages"), 0);
    assert_eq!(a.count("sync_conflicts"), 1);
    let projected = a.store.get_input_record(&captured.id).unwrap().unwrap();
    assert_eq!(projected.event_id, captured.event_id);
    assert!(projected.message_id.is_none());
    assert_eq!(
        a.store.list_events().unwrap()[0].raw_text,
        "offline fact survives"
    );
}

#[test]
fn invalid_remote_clock_cannot_break_local_capture() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("remote", "capture", None)
        .unwrap();
    let mut operation = a.store.pending_sync_operations("b", 10).unwrap().remove(0);
    operation.logical_clock = 9_007_199_254_740_991;
    // P4a: a far-ahead clock (long-offline / snapshot-needed) is quarantined
    // as 'isolated', not a hard receive failure, and never breaks local capture.
    b.store
        .receive_sync_operation("a", &serde_json::to_vec(&operation).unwrap())
        .unwrap();
    assert_eq!(
        op_state(&b, &operation.op_id).map(|(s, e)| (s, e)),
        Some((
            "isolated".to_string(),
            Some("sync_clock_requires_snapshot".to_string())
        ))
    );
    b.store
        .submit_input_as_event("still local", "capture", None)
        .unwrap();
    assert_eq!(b.store.list_events().unwrap()[0].raw_text, "still local");
}

#[test]
fn cross_origin_missing_base_waits_instead_of_creating_a_false_conflict() {
    let a = Database::new("a");
    let b = Database::new("b");
    let c = Database::new("c");
    pair(&a, &b);
    pair(&b, &c);
    let conversation = a.store.create_conversation(Some("initial"), None).unwrap();
    deliver(&a, &b);
    b.store
        .rename_conversation(&conversation, "b update")
        .unwrap();
    deliver(&b, &c);
    assert_eq!(c.count("sync_conflicts"), 0);
    assert!(c.store.get_conversation(&conversation).unwrap().is_none());
    pair(&a, &c);
    deliver(&a, &c);
    assert_eq!(
        c.store
            .get_conversation(&conversation)
            .unwrap()
            .unwrap()
            .title
            .as_deref(),
        Some("b update")
    );
    assert_eq!(c.count("sync_conflicts"), 0);
}

#[test]
fn deletion_without_unseen_edits_preserves_events_and_propagates() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let conversation = a.store.create_conversation(None, None).unwrap();
    a.store
        .submit_conversation_input(&conversation, "keep raw", None)
        .unwrap();
    deliver(&a, &b);
    a.store
        .set_conversation_archived(&conversation, true)
        .unwrap();
    a.store.delete_archived_conversation(&conversation).unwrap();
    deliver(&a, &b);
    assert!(b.store.get_conversation(&conversation).unwrap().is_none());
    assert_eq!(b.count("events"), 1);
    assert_eq!(b.count("sync_conflicts"), 0);
}

#[test]
fn application_receipts_are_durable_and_recreated_after_lost_delivery() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("receipt-test", "capture", None)
        .unwrap();
    let ops = deliver(&a, &b);
    let receipts = b.store.pending_sync_receipts("a", 100).unwrap();
    assert_eq!(receipts.len(), 1);
    b.store.mark_sync_receipt_sent(&ops[0].op_id).unwrap();
    assert!(b.store.pending_sync_receipts("a", 100).unwrap().is_empty());
    deliver(&a, &b);
    assert_eq!(b.store.pending_sync_receipts("a", 100).unwrap(), receipts);
    assert!(
        a.store
            .accept_sync_receipt("stranger", &receipts[0])
            .is_err()
    );
    a.store.accept_sync_receipt("b", &receipts[0]).unwrap();
    assert!(
        a.store
            .pending_sync_operations("b", 100)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn receipt_failure_rolls_back_domain_and_frontier_together() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("receipt-rollback", "capture", None)
        .unwrap();
    b.store.connection.execute_batch("CREATE TRIGGER fail_receipt BEFORE INSERT ON sync_receipts BEGIN SELECT RAISE(ABORT,'synthetic receipt failure'); END;").unwrap();
    let op = a.store.pending_sync_operations("b", 100).unwrap().remove(0);
    assert!(
        b.store
            .receive_sync_operation("a", &serde_json::to_vec(&op).unwrap())
            .is_err()
    );
    assert_eq!(b.count("events"), 0);
    assert_eq!(b.count("sync_operations"), 0);
    assert!(b.store.pending_sync_receipts("a", 100).unwrap().is_empty());
}

#[test]
fn logical_message_order_is_consistent_after_concurrent_appends() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let c = a.store.create_conversation(None, None).unwrap();
    deliver(&a, &b);
    let first = a.store.send_message(&c, "user", "first", None).unwrap();
    deliver(&a, &b);
    let reply = b
        .store
        .send_message(&c, "assistant", "reply", Some(&first))
        .unwrap();
    let independent = a
        .store
        .send_message(&c, "user", "independent", None)
        .unwrap();
    deliver(&a, &b);
    deliver(&b, &a);
    let ids = |db: &Database| {
        db.store
            .list_messages(&c)
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&a), ids(&b));
    let order = ids(&a);
    assert!(order.iter().position(|id| id == &first) < order.iter().position(|id| id == &reply));
    assert!(order.contains(&independent));
}

#[test]
fn capture_is_one_atomic_operation_and_replay_has_no_jobs_or_echo() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let conversation = a.store.create_conversation(Some("test"), None).unwrap();
    let input = a
        .store
        .submit_conversation_input(&conversation, "original input", Some("local-key"))
        .unwrap();
    let ops = deliver(&a, &b);
    assert_eq!(ops.len(), 2);
    assert_eq!(b.count("events"), 1);
    assert_eq!(b.count("messages"), 1);
    assert_eq!(b.count("input_records"), 1);
    assert_eq!(b.count("analysis_jobs"), 0);
    assert_eq!(b.count("knowledge_digest_jobs"), 0);
    assert_eq!(b.count("sync_outbox"), 0);
    assert_eq!(
        b.store
            .get_input_record(&input.id)
            .unwrap()
            .unwrap()
            .event_id,
        input.event_id
    );
    deliver(&a, &b);
    assert_eq!(b.count("events"), 1);
    assert_eq!(b.count("sync_operations"), 2);
    assert_eq!(
        b.store.list_messages(&conversation).unwrap()[0].content,
        "original input"
    );
}

#[test]
fn two_devices_append_offline_and_preserve_identity() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let now = chrono::Utc::now();
    let event = a
        .store
        .insert_event(NewEvent {
            occurred_at: now,
            recorded_at: now,
            raw_text: "a raw",
            source: "capture",
        })
        .unwrap();
    let input = b
        .store
        .submit_input_as_event("b raw", "capture", Some("same-local-key"))
        .unwrap();
    deliver(&a, &b);
    deliver(&b, &a);
    assert_eq!(a.count("events"), 2);
    assert_eq!(b.count("events"), 2);
    assert!(a.store.list_events().unwrap().iter().any(|e| e.id == event));
    assert!(
        b.store
            .list_events()
            .unwrap()
            .iter()
            .any(|e| Some(&e.id) == input.event_id.as_ref())
    );
    assert_eq!(
        a.count("analysis_jobs"),
        2,
        "the processor owns both local and imported work"
    );
    assert_eq!(
        b.count("analysis_jobs"),
        1,
        "the non-processor retains only its local pending eligibility"
    );
    assert!(b.store.claim_analysis_job().unwrap().is_none());
}

#[test]
fn out_of_order_waits_then_drains_without_losing_aggregate() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let c = a.store.create_conversation(None, None).unwrap();
    a.store
        .submit_conversation_input(&c, "after parent", None)
        .unwrap();
    let ops = a.store.pending_sync_operations("b", 100).unwrap();
    let report = b
        .store
        .receive_sync_operation("a", &serde_json::to_vec(&ops[1]).unwrap())
        .unwrap();
    assert_eq!(report.waiting, 1);
    assert_eq!(b.count("events"), 0);
    let report = b
        .store
        .receive_sync_operation("a", &serde_json::to_vec(&ops[0]).unwrap())
        .unwrap();
    assert_eq!(report.applied, 2);
    assert_eq!(report.waiting, 0);
    assert_eq!(b.count("events"), 1);
}

#[test]
fn same_operation_with_changed_content_is_isolated_and_not_applied() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("original", "capture", None)
        .unwrap();
    let mut op = deliver(&a, &b).remove(0);
    if let SyncMutation::Capture { event, input, .. } = &mut op.mutation {
        event.raw_text = "different".into();
        input.as_mut().unwrap().raw_text = "different".into();
    }
    // P4a: a forged op (same identity, different bytes) is quarantined as
    // 'isolated', not applied, and does not abort the receive.
    b.store
        .receive_sync_operation("a", &serde_json::to_vec(&op).unwrap())
        .unwrap();
    assert_eq!(b.store.list_events().unwrap()[0].raw_text, "original");
    assert_eq!(
        op_state(&b, &op.op_id).map(|(s, _)| s),
        Some("isolated".to_string())
    );
    assert_eq!(isolated_count(&b), 1);
}

#[test]
fn untrusted_device_dataset_and_version_never_apply() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("original", "capture", None)
        .unwrap();
    let op = a.store.pending_sync_operations("b", 100).unwrap().remove(0);
    assert!(
        b.store
            .receive_sync_operation("stranger", &serde_json::to_vec(&op).unwrap())
            .is_err()
    );
    let mut other = op.clone();
    other.dataset_id = "other".into();
    assert!(
        b.store
            .receive_sync_operation("a", &serde_json::to_vec(&other).unwrap())
            .is_err()
    );
    other = op.clone();
    other.protocol_version = 999;
    // P4a: an unsupported-version op from a then-active member is quarantined
    // (isolated), not a hard failure; it never applies. The final count below
    // expects this one isolated row.
    b.store
        .receive_sync_operation("a", &serde_json::to_vec(&other).unwrap())
        .unwrap();
    assert_eq!(
        op_state(&b, &other.op_id).map(|(s, _)| s),
        Some("isolated".to_string())
    );
    b.store.revoke_sync_member("a").unwrap();
    assert!(
        b.store
            .receive_sync_operation("a", &serde_json::to_vec(&op).unwrap())
            .is_err()
    );
    assert_eq!(b.count("events"), 0);
    // Only the isolated unsupported-version op is recorded; nothing applied.
    assert_eq!(b.count("sync_operations"), 1);
    assert_eq!(isolated_count(&b), 1);
}

#[test]
fn outbox_failure_rolls_back_capture_and_sequence() {
    let a = Database::new("a");
    a.store.connection.execute_batch("CREATE TRIGGER fail_sync BEFORE INSERT ON sync_operations BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    assert!(
        a.store
            .submit_input_as_event("must roll back", "capture", None)
            .is_err()
    );
    assert_eq!(a.count("events"), 0);
    assert_eq!(a.count("analysis_jobs"), 0);
    assert_eq!(a.count("input_records"), 0);
    let seq: i64 = a
        .store
        .connection
        .query_row("SELECT next_sequence FROM sync_replica", [], |r| r.get(0))
        .unwrap();
    assert_eq!(seq, 1);
}

#[test]
fn apply_failure_does_not_commit_half_capture_or_advance_frontier() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("aggregate", "capture", None)
        .unwrap();
    b.store.connection.execute_batch("CREATE TRIGGER fail_sync_input BEFORE INSERT ON input_records BEGIN SELECT RAISE(ABORT,'synthetic input failure'); END;").unwrap();
    let op = a.store.pending_sync_operations("b", 100).unwrap().remove(0);
    let report = b
        .store
        .receive_sync_operation("a", &serde_json::to_vec(&op).unwrap())
        .unwrap();
    assert_eq!(report.rejected, 1);
    assert_eq!(b.count("events"), 0);
    assert_eq!(b.count("input_records"), 0);
    let seq: i64 = b
        .store
        .connection
        .query_row(
            "SELECT applied_sequence FROM sync_frontiers WHERE origin_stream_id='stream-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(seq, 0);
}

#[test]
fn concurrent_title_changes_are_preserved_as_conflicts() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let c = a.store.create_conversation(Some("initial"), None).unwrap();
    deliver(&a, &b);
    a.store.rename_conversation(&c, "a edit").unwrap();
    b.store.rename_conversation(&c, "b edit").unwrap();
    deliver(&a, &b);
    deliver(&b, &a);
    assert_eq!(a.count("sync_conflicts"), 1);
    assert_eq!(b.count("sync_conflicts"), 1);
    assert_eq!(
        a.store
            .get_conversation(&c)
            .unwrap()
            .unwrap()
            .title
            .as_deref(),
        Some("a edit")
    );
    assert_eq!(
        b.store
            .get_conversation(&c)
            .unwrap()
            .unwrap()
            .title
            .as_deref(),
        Some("b edit")
    );
}

#[test]
fn deletion_propagates_without_deleting_raw_events_and_late_message_conflicts() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let c = a.store.create_conversation(Some("initial"), None).unwrap();
    a.store
        .submit_conversation_input(&c, "retained raw", None)
        .unwrap();
    deliver(&a, &b);
    b.store
        .send_message(&c, "user", "offline message", None)
        .unwrap();
    a.store.set_conversation_archived(&c, true).unwrap();
    assert!(a.store.delete_archived_conversation(&c).unwrap());
    deliver(&a, &b);
    deliver(&b, &a);
    assert!(a.store.get_conversation(&c).unwrap().is_none());
    assert!(
        b.store.get_conversation(&c).unwrap().is_some(),
        "remote deletion must not erase an unseen offline message"
    );
    assert_eq!(b.count("sync_conflicts"), 1);
    assert_eq!(a.count("events"), 1);
    assert_eq!(b.count("events"), 1);
    assert_eq!(a.count("sync_conflicts"), 1);
    assert_eq!(a.count("sync_object_heads"), 1);
}

#[test]
fn only_matching_application_acknowledgement_removes_pending_delivery() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store.submit_input_as_event("a", "capture", None).unwrap();
    let op = a.store.pending_sync_operations("b", 100).unwrap().remove(0);
    let digest = hash(serde_json::to_string(&op).unwrap().as_bytes());
    assert!(
        a.store
            .acknowledge_sync_operation("b", &op.op_id, "wrong")
            .is_err()
    );
    assert_eq!(a.store.pending_sync_operations("b", 100).unwrap().len(), 1);
    a.store
        .acknowledge_sync_operation("b", &op.op_id, &digest)
        .unwrap();
    assert!(
        a.store
            .pending_sync_operations("b", 100)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        a.count("sync_operations"),
        1,
        "application ack does not prematurely GC history"
    );
}

#[test]
fn revoked_waiting_stream_does_not_block_other_device() {
    let a = Database::new("a");
    let b = Database::new("b");
    let c = Database::new("c");
    pair(&a, &b);
    pair(&b, &c);
    a.store
        .submit_input_as_event("a1", "capture", None)
        .unwrap();
    a.store
        .submit_input_as_event("a2", "capture", None)
        .unwrap();
    let ops = a.store.pending_sync_operations("b", 100).unwrap();
    b.store
        .receive_sync_operation("a", &serde_json::to_vec(&ops[1]).unwrap())
        .unwrap();
    b.store.revoke_sync_member("a").unwrap();
    c.store
        .submit_input_as_event("c1", "capture", None)
        .unwrap();
    deliver(&c, &b);
    assert_eq!(b.count("events"), 1);
    assert_eq!(b.store.retry_waiting_sync_operations().unwrap().waiting, 1);
}

// ---- P4a: S01 source isolation, idempotency fidelity, long-offline catch-up ----

fn op_state(db: &Database, op_id: &str) -> Option<(String, Option<String>)> {
    db.store
        .connection
        .query_row(
            "SELECT state,error_code FROM sync_operations WHERE op_id=?1",
            [op_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .unwrap()
}

fn isolated_count(db: &Database) -> i64 {
    db.store
        .connection
        .query_row(
            "SELECT count(*) FROM sync_operations WHERE state='isolated'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn same_identity_different_content_is_quarantined_not_applied_or_rolled_back() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("original", "capture", None)
        .unwrap();
    deliver(&a, &b);
    assert_eq!(b.store.list_events().unwrap()[0].raw_text, "original");

    // a produces seq-2; b receives the GENUINE seq-2 first (so the identity
    // stream-a:2 is now known), then an attacker replays the same identity with
    // different bytes. The forged replay must be quarantined, not applied.
    a.store
        .submit_input_as_event("second", "capture", None)
        .unwrap();
    let genuine2 = a
        .store
        .pending_sync_operations("b", 10)
        .unwrap()
        .into_iter()
        .find(|o| o.origin_sequence == 2)
        .expect("genuine seq-2 op");
    deliver(&a, &b); // genuine seq-2 lands on b
    assert!(
        b.store
            .list_events()
            .unwrap()
            .iter()
            .any(|e| e.raw_text == "second")
    );
    let mut forged = genuine2.clone();
    if let SyncMutation::Capture { event, input, .. } = &mut forged.mutation {
        event.raw_text = "forged".into();
        if let Some(input) = input {
            input.raw_text = "forged".into();
        }
    }
    // Receiving the forged copy: same identity as an already-recorded op but
    // different bytes -> quarantined 'isolated', no abort, no rollback.
    b.store
        .receive_sync_operation("a", &serde_json::to_vec(&forged).unwrap())
        .unwrap();
    assert_eq!(
        op_state(&b, &forged.op_id).map(|(s, _)| s),
        Some("isolated".to_string())
    );
    assert_eq!(isolated_count(&b), 1);
    // The genuine applied event is untouched; forged content never landed.
    assert!(
        !b.store
            .list_events()
            .unwrap()
            .iter()
            .any(|e| e.raw_text == "forged")
    );
    assert!(
        b.store
            .list_events()
            .unwrap()
            .iter()
            .any(|e| e.raw_text == "second")
    );
    // Local capture is unaffected by the quarantined op.
    b.store
        .submit_input_as_event("b local", "capture", None)
        .unwrap();
    assert!(
        b.store
            .list_events()
            .unwrap()
            .iter()
            .any(|e| e.raw_text == "b local")
    );
}

#[test]
fn undecidable_divergence_does_not_apply_or_block_other_streams() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("one", "capture", None)
        .unwrap();
    a.store
        .submit_input_as_event("two", "capture", None)
        .unwrap();
    a.store
        .submit_input_as_event("three", "capture", None)
        .unwrap();
    let ops = a.store.pending_sync_operations("b", 100).unwrap();
    assert_eq!(ops.len(), 3);

    // Deliver op 1 genuinely, then the GENUINE op 2 (so stream-a:2 is known),
    // then a forged replay of op 2 (same identity, diff bytes), then op 3.
    // The forged op 2 is quarantined; op 3 waits behind the undecidable seq-2
    // (frontier cannot skip it). The forged payload is never applied.
    b.store
        .receive_sync_operation("a", &serde_json::to_vec(&ops[0]).unwrap())
        .unwrap();
    b.store
        .receive_sync_operation("a", &serde_json::to_vec(&ops[1]).unwrap())
        .unwrap();
    assert!(
        b.store
            .list_events()
            .unwrap()
            .iter()
            .any(|e| e.raw_text == "two")
    );
    let mut forged2 = ops[1].clone();
    if let SyncMutation::Capture { event, .. } = &mut forged2.mutation {
        event.raw_text = "forged-two".into();
    }
    b.store
        .receive_sync_operation("a", &serde_json::to_vec(&forged2).unwrap())
        .unwrap();
    assert_eq!(isolated_count(&b), 1);
    // The forged copy is quarantined; the genuine seq-2 was already applied, so
    // the real chain is intact and seq-3 still applies. The quarantined forged
    // payload is never applied, and the divergence stays recorded for review.
    let report = b
        .store
        .receive_sync_operation("a", &serde_json::to_vec(&ops[2]).unwrap())
        .unwrap();
    assert_eq!(report.applied, 1); // genuine seq-3 applied
    let texts: Vec<String> = b
        .store
        .list_events()
        .unwrap()
        .iter()
        .map(|e| e.raw_text.clone())
        .collect();
    assert!(texts.contains(&"one".to_string()));
    assert!(texts.contains(&"two".to_string()));
    assert!(texts.contains(&"three".to_string()));
    assert!(!texts.contains(&"forged-two".to_string()));
    assert_eq!(isolated_count(&b), 1);
    // The undecidable stream does not poison a different device: a second
    // device's stream still applies its own in-order ops.
    let d = Database::new("d");
    pair(&a, &d);
    pair(&b, &d);
    d.store
        .submit_input_as_event("d1", "capture", None)
        .unwrap();
    deliver(&d, &b);
    assert!(
        b.store
            .list_events()
            .unwrap()
            .iter()
            .any(|e| e.raw_text == "d1")
    );
}

#[test]
fn unknown_protocol_version_is_isolated_and_does_not_block() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("v1", "capture", None)
        .unwrap();
    let mut op = a.store.pending_sync_operations("b", 10).unwrap().remove(0);
    op.protocol_version = 99;
    // P4a: an unsupported op from an authenticated member is quarantined as
    // 'isolated' (no abort), records nothing domain-side, and does not block.
    b.store
        .receive_sync_operation("a", &serde_json::to_vec(&op).unwrap())
        .unwrap();
    assert_eq!(b.count("events"), 0);
    assert_eq!(
        op_state(&b, &op.op_id).map(|(s, e)| (s, e)),
        Some((
            "isolated".to_string(),
            Some("sync_protocol_unsupported".to_string())
        ))
    );
    // A subsequent valid op on the SAME stream cannot leapfrog the unsupported
    // seq-1 (the frontier cannot skip it), so it waits; but a DIFFERENT device's
    // stream is unaffected.
    a.store
        .submit_input_as_event("v1-ok", "capture", None)
        .unwrap();
    let ok = a
        .store
        .pending_sync_operations("b", 10)
        .unwrap()
        .into_iter()
        .find(|o| o.origin_sequence == 2)
        .expect("seq-2 op");
    let report = b
        .store
        .receive_sync_operation("a", &serde_json::to_vec(&ok).unwrap())
        .unwrap();
    assert_eq!(report.waiting, 1); // parked behind the undecidable seq-1
    assert!(
        !b.store
            .list_events()
            .unwrap()
            .iter()
            .any(|e| e.raw_text == "v1-ok")
    );
    // The unsupported op stayed isolated and never applied.
    assert_eq!(isolated_count(&b), 1);
    // Another device's stream still flows.
    let d = Database::new("d");
    pair(&b, &d);
    d.store
        .submit_input_as_event("d-ok", "capture", None)
        .unwrap();
    deliver(&d, &b);
    assert!(
        b.store
            .list_events()
            .unwrap()
            .iter()
            .any(|e| e.raw_text == "d-ok")
    );
}

#[test]
fn idempotency_key_is_preserved_across_devices() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let input = a
        .store
        .submit_input_as_event("keyed input", "capture", Some("user-key-1"))
        .unwrap();
    deliver(&a, &b);
    let replicated = b
        .store
        .get_input_record(&input.id)
        .unwrap()
        .expect("input replicated");
    assert_eq!(replicated.idempotency_key.as_deref(), Some("user-key-1"));
    // The idempotency unique index holds on the replica too.
    assert_eq!(b.count("input_records"), 1);
}

#[test]
fn capture_replay_with_idempotency_key_has_no_duplicate_or_jobs() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("replay-me", "capture", Some("rk"))
        .unwrap();
    deliver(&a, &b);
    deliver(&a, &b); // replay
    assert_eq!(b.count("events"), 1);
    assert_eq!(b.count("input_records"), 1);
    assert_eq!(b.count("analysis_jobs"), 0);
    assert_eq!(b.count("knowledge_digest_jobs"), 0);
    assert_eq!(b.count("sync_outbox"), 0);
}

#[test]
fn long_offline_third_device_catches_up_via_replay() {
    let a = Database::new("a");
    let b = Database::new("b");
    let c = Database::new("c");
    pair(&a, &b);
    pair(&a, &c);
    pair(&b, &c);
    // a and b exchange while c is "offline".
    a.store
        .submit_input_as_event("a1", "capture", None)
        .unwrap();
    deliver(&a, &b);
    b.store
        .submit_input_as_event("b1", "capture", None)
        .unwrap();
    deliver(&b, &a);
    // c was offline for both; now it comes back. The replication core does NOT
    // auto-forward received ops: each device forwards only its own locally
    // produced ops. So c catches up from each source directly (a sends a1, b
    // sends b1). b holds a1 but does not re-forward it.
    let a_ops = a.store.pending_sync_operations("c", 100).unwrap();
    let b_ops = b.store.pending_sync_operations("c", 100).unwrap();
    assert_eq!(a_ops.len(), 1); // a1 (own op only)
    assert_eq!(b_ops.len(), 1); // b1 (own op only; replicated a1 is not re-forwarded)
    deliver(&a, &c);
    deliver(&b, &c);
    let texts: Vec<String> = c
        .store
        .list_events()
        .unwrap()
        .iter()
        .map(|e| e.raw_text.clone())
        .collect();
    assert!(texts.contains(&"a1".to_string()));
    assert!(texts.contains(&"b1".to_string()));
    assert_eq!(c.count("analysis_jobs"), 0);
    assert_eq!(c.count("sync_outbox"), 0);
    // Replaying the same backlog is idempotent: no duplicate events.
    deliver(&a, &c);
    deliver(&b, &c);
    assert_eq!(c.count("events"), 2);
}

#[test]
fn outbox_lists_local_ops_in_sequence_order() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("one", "capture", None)
        .unwrap();
    a.store
        .submit_input_as_event("two", "capture", None)
        .unwrap();
    let ops = a.store.pending_sync_operations("b", 10).unwrap();
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].origin_sequence, 1);
    assert_eq!(ops[1].origin_sequence, 2);
}

fn capture_wiki(db: &Database, id: &str) {
    let tx = db.store.begin_write_transaction().unwrap();
    db.store.sync_record_wiki_page(id, false).unwrap();
    if let Some(tx) = tx {
        tx.commit().unwrap();
    }
}

fn seed_wiki_page(
    db: &Database,
    id: &str,
    slug: &str,
    title: &str,
    content: &str,
    evidence: &[&str],
) {
    let now = chrono::Utc::now().to_rfc3339();
    let mut all_evidence: Vec<String> = evidence.iter().map(|e| e.to_string()).collect();
    all_evidence.sort();
    all_evidence.dedup();
    db.store
        .connection
        .execute(
            "INSERT INTO wiki_pages(id,slug,kind,title,summary,content_md,tags,source_event_ids, \
             evidence_count,first_seen_at,last_seen_at,status,created_at,updated_at,area) \
             VALUES(?1,?2,'topic',?3,'',?4,'[]',?5,?6,'2026-01-01T00:00:00+00:00','2026-01-01T00:00:00+00:00','active','2026-01-01T00:00:00+00:00','2026-01-01T00:00:00+00:00','insight')",
            params![
                id,
                slug,
                title,
                content,
                serde_json::to_string(&all_evidence).unwrap(),
                (all_evidence.len() as i64).max(1),
            ],
        )
        .unwrap();
}

#[test]
fn wiki_page_edits_replicate_and_stop_at_revision_cas() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    seed_wiki_page(&a, "page-1", "topic/p1", "页面一", "v1", &[]);
    capture_wiki(&a, "page-1");
    deliver(&a, &b);
    assert_eq!(
        b.store
            .get_wiki_page("topic/p1")
            .unwrap()
            .unwrap()
            .content_md,
        "v1"
    );
    // 双端离线并发编辑：两端从同一 base 出发的修改都到达对端时，
    // 各自至少落一条冲突候选，且没有任何一端被时间更新顺序覆盖丢失。
    a.store
        .save_wiki_page_content("topic/p1", "edit-from-a", "a 改", None)
        .unwrap();
    b.store
        .save_wiki_page_content("topic/p1", "edit-from-b", "b 改", None)
        .unwrap();
    deliver(&b, &a);
    deliver(&a, &b);
    assert!(a.count("sync_conflicts") >= 1, "A 端必须保留冲突候选");
    assert!(b.count("sync_conflicts") >= 1, "B 端必须保留冲突候选");
    let a_page = a.store.get_wiki_page("topic/p1").unwrap().unwrap();
    assert_eq!(a_page.content_md, "edit-from-a", "冲突版本不得被静默覆盖");
    let b_page = b.store.get_wiki_page("topic/p1").unwrap().unwrap();
    assert_eq!(b_page.content_md, "edit-from-b", "冲突版本不得被静默覆盖");
}

#[test]
fn wiki_page_same_slug_from_two_ids_is_an_explicit_conflict_not_a_remap() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    // 两个各自的 device stream 上各自建页：slug 撞车（同名不同 id）。
    seed_wiki_page(&a, "page-a", "topic/dup", "同名页 A", "from-a", &[]);
    seed_wiki_page(&b, "page-b", "topic/dup", "同名页 B", "from-b", &[]);
    capture_wiki(&a, "page-a");
    deliver(&a, &b);
    assert_eq!(b.count("sync_conflicts"), 1, "slug 撞车必须显式冲突");
    let winner = b.store.get_wiki_page("topic/dup").unwrap().unwrap();
    assert_eq!(winner.id, "page-b", "本地页引用不得被静默重挂到对端 id");
    let conflict: String = b
        .store
        .connection
        .query_row("SELECT reason FROM sync_conflicts LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(conflict, "slug_owned_by_other_id");
}

#[test]
fn wiki_page_evidence_waits_for_missing_event_then_applies() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    // 先在 A 捕获一个事件，再给出一页把它当证据引用；但只把「页」送到 B。
    a.store
        .submit_input_as_event("证据事件文本", "capture", None)
        .unwrap();
    let event_id = a.store.list_events().unwrap()[0].id.clone();
    seed_wiki_page(&a, "page-ev", "topic/ev", "证据页", "正文", &[&event_id]);
    capture_wiki(&a, "page-ev");
    let ai = a.store.sync_identity().unwrap().unwrap();
    let bi = b.store.sync_identity().unwrap().unwrap();
    let ops = a.store.pending_sync_operations(&bi.device_id, 100).unwrap();
    // 只投递引用页面的 op（跳过事件 op），证据未到：等待，不落库、不推断。
    let wiki_idx = ops
        .iter()
        .position(|op| {
            matches!(
                op.mutation,
                crate::sync::protocol::SyncMutation::WikiPageChanged { .. }
            )
        })
        .unwrap();
    let report = b
        .store
        .receive_sync_operation(&ai.device_id, &serde_json::to_vec(&ops[wiki_idx]).unwrap())
        .unwrap();
    assert_eq!(report.waiting, 1, "证据缺失必须等待");
    assert_eq!(b.count("wiki_pages"), 0);
    // 事件随后补齐，容忍等待态重投：页成功落地且证据继续存在。
    for op in &ops {
        b.store
            .receive_sync_operation(&ai.device_id, &serde_json::to_vec(op).unwrap())
            .unwrap();
    }
    let page = b.store.get_wiki_page("topic/ev").unwrap().unwrap();
    assert_eq!(page.content_md, "正文");
    assert_eq!(b.count("events"), 1);
}

#[test]
fn wiki_page_edit_after_delete_keeps_candidate() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    seed_wiki_page(&b, "page-1", "topic/gone", "被删页", "旧正文", &[]);
    capture_wiki(&b, "page-1");
    deliver(&b, &a);
    // A 端离线编辑，B 端删除；A 的编辑到达已删除的页面时保留候选。
    a.store
        .save_wiki_page_content("topic/gone", "offline edit", "离线改", None)
        .unwrap();
    {
        let tx = b.store.begin_write_transaction().unwrap();
        b.store.sync_record_wiki_page("page-1", true).unwrap();
        if let Some(tx) = tx {
            tx.commit().unwrap();
        }
    }
    deliver(&a, &b);
    assert_eq!(b.count("sync_conflicts"), 1);
    let conflict: String = b
        .store
        .connection
        .query_row("SELECT reason FROM sync_conflicts LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(conflict, "wiki_page_deleted");
    // 冲突版本不落库：b 的本地行保持原内容。物理删除只来自本端的业务删除
    // API（delete_wiki_page）；capture(deleted=true) 本身不改写本地行。
    let row = b.store.get_wiki_page("topic/gone").unwrap().unwrap();
    assert_eq!(row.content_md, "旧正文");
    let deleted: bool = b
        .store
        .connection
        .query_row("SELECT deleted FROM sync_object_heads WHERE object_kind='wiki_page' AND object_id='page-1'", [], |r| r.get(0))
        .unwrap();
    assert!(deleted, "删除捕获必须推进 head");
}

#[test]
fn wiki_page_business_delete_removes_row_and_replicates() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("证据事件文本", "capture", None)
        .unwrap();
    let event_id = a.store.list_events().unwrap()[0].id.clone();
    seed_wiki_page(&a, "page-1", "topic/gone", "要走页", "正文", &[&event_id]);
    // 一条修订记录，验证级联清理。
    a.store
        .save_wiki_page_content("topic/gone", "正文 v2", "二次编辑", None)
        .unwrap();
    capture_wiki(&a, "page-1");
    deliver(&a, &b);
    assert!(b.store.get_wiki_page("topic/gone").unwrap().is_some());
    // 业务删除：本端行与修订级联消失，head 推进为已删。
    assert!(a.store.delete_wiki_page("topic/gone").unwrap());
    assert!(a.store.get_wiki_page("topic/gone").unwrap().is_none());
    let revisions: i64 = a
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM wiki_revisions WHERE page_id='page-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(revisions, 0, "修订必须随页面级联删除");
    assert!(
        a.store
            .sync_object_head("wiki_page", "page-1")
            .unwrap()
            .is_some()
    );
    // 幂等：不存在再删返回 false。
    assert!(!a.store.delete_wiki_page("topic/gone").unwrap());
    // 复制：B 端同样物理删除（删除状态是全端一致的，不是仅本端标记）。
    deliver(&a, &b);
    assert!(b.store.get_wiki_page("topic/gone").unwrap().is_none());
    assert!(
        b.store
            .sync_object_head("wiki_page", "page-1")
            .unwrap()
            .is_some()
    );
}

#[test]
fn wiki_page_rename_replicates_and_migrates_references() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("证据事件文本", "capture", None)
        .unwrap();
    let event_id = a.store.list_events().unwrap()[0].id.clone();
    seed_wiki_page(
        &a,
        "page-1",
        "topic/网站改版",
        "网站改版",
        "正文",
        &[&event_id],
    );
    capture_wiki(&a, "page-1");
    deliver(&a, &b);
    // b 端建一条指向该页的关系与一个页内会话（rename 应用端要迁移它们）。
    b.store
        .connection
        .execute(
            "INSERT INTO relations(id,from_slug,from_kind,to_slug,to_kind,relation,confidence,created_at,last_seen_at) \
             VALUES('rel-1','person/张三','person','topic/网站改版','topic','跟进',3,'2026-01-01T00:00:00+00:00','2026-01-01T00:00:00+00:00')",
            [],
        )
        .unwrap();
    b.store
        .create_wiki_chat_conversation("topic/网站改版", "[知识页] 网站改版")
        .unwrap();
    // a 端业务重命名（捕获在 rename_wiki_page 内部）。
    let outcome = a
        .store
        .rename_wiki_page("topic/网站改版", "测试项目 尾款", "更正项目名")
        .unwrap();
    assert!(outcome.changed);
    assert_eq!(outcome.new_slug, "topic/测试项目-尾款");
    assert!(a.store.get_wiki_page("topic/网站改版").unwrap().is_none());
    deliver(&a, &b);
    // b 端：页换 slug，关系与会话同步迁移，旧 slug 释放。
    assert!(b.store.get_wiki_page("topic/网站改版").unwrap().is_none());
    let renamed = b
        .store
        .get_wiki_page("topic/测试项目-尾款")
        .unwrap()
        .unwrap();
    assert_eq!(renamed.title, "测试项目 尾款");
    let rel_to: String = b
        .store
        .connection
        .query_row(
            "SELECT to_slug FROM relations WHERE from_slug='person/张三'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rel_to, "topic/测试项目-尾款");
    let chats: i64 = b
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM conversations WHERE wiki_page_slug='topic/测试项目-尾款'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(chats, 1);
    // 幂等重放：重复投递不产生新效果。
    deliver(&a, &b);
    assert!(
        b.store
            .get_wiki_page("topic/测试项目-尾款")
            .unwrap()
            .is_some()
    );
}

#[test]
fn offline_renames_on_both_sides_conflict_explicitly() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("证据事件文本", "capture", None)
        .unwrap();
    let event_id = a.store.list_events().unwrap()[0].id.clone();
    seed_wiki_page(&a, "page-1", "topic/项目", "项目", "正文", &[&event_id]);
    capture_wiki(&a, "page-1");
    deliver(&a, &b);
    // 双端离线各自改名（不同标题/不同 slug）。
    a.store.rename_wiki_page("topic/项目", "甲名", "a").unwrap();
    b.store.rename_wiki_page("topic/项目", "乙名", "b").unwrap();
    // 交叉投递：两端各自保留本端改名，b→a 的到达 op 落显式冲突不选边。
    deliver(&a, &b);
    deliver(&b, &a);
    let diverged_a: i64 = a
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sync_conflicts WHERE reason='wiki_page_rename_diverged'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(diverged_a > 0, "两端各自改名必须显式冲突");
    // 两端均保留本端改名结果，未被对端覆盖。
    assert!(b.store.get_wiki_page("topic/乙名").unwrap().is_some());
    assert!(a.store.get_wiki_page("topic/甲名").unwrap().is_some());
}

#[test]
fn rename_arriving_after_delete_stays_conflict_candidate() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("证据事件文本", "capture", None)
        .unwrap();
    let event_id = a.store.list_events().unwrap()[0].id.clone();
    seed_wiki_page(&a, "page-1", "topic/项目", "项目", "正文", &[&event_id]);
    capture_wiki(&a, "page-1");
    deliver(&a, &b);
    // b 离线删除；a 端改名后再投递。
    assert!(b.store.delete_wiki_page("topic/项目").unwrap());
    a.store.rename_wiki_page("topic/项目", "新名", "a").unwrap();
    deliver(&a, &b);
    // b 端页已被业务删除物理移除：重命名 op 落冲突候选（等删除态对齐后人工审）。
    let pending: i64 = b
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sync_conflicts WHERE reason='wiki_page_deleted'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(pending > 0, "删除后的迟到重命名应保留为候选");
}
