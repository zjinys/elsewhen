use super::deliver;
use super::pair;
use super::*;

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
