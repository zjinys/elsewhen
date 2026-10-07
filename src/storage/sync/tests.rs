use super::*;
use crate::event::NewEvent;
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
    assert!(
        b.store
            .receive_sync_operation("a", &serde_json::to_vec(&operation).unwrap())
            .is_err()
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
fn same_operation_with_changed_content_is_rejected() {
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
    assert!(
        b.store
            .receive_sync_operation("a", &serde_json::to_vec(&op).unwrap())
            .is_err()
    );
    assert_eq!(b.store.list_events().unwrap()[0].raw_text, "original");
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
    assert!(
        b.store
            .receive_sync_operation("a", &serde_json::to_vec(&other).unwrap())
            .is_err()
    );
    b.store.revoke_sync_member("a").unwrap();
    assert!(
        b.store
            .receive_sync_operation("a", &serde_json::to_vec(&op).unwrap())
            .is_err()
    );
    assert_eq!(b.count("events"), 0);
    assert_eq!(b.count("sync_operations"), 0);
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
