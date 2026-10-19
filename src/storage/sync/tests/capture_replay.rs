use super::deliver;
use super::pair;
use super::*;

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
