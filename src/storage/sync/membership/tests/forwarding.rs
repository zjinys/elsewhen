use super::*;

fn three() -> (Db, Db, Db) {
    let a = Db::owner();
    let b = Db::new("b");
    let c = Db::new("c");
    enroll(&a, &b);
    let (_, _, roster) = enroll(&a, &c);
    b.store
        .apply_signed_roster(&roster.encode().unwrap())
        .unwrap();
    (a, b, c)
}

fn deliver(source: &Db, target: &Db, recipient: &str) {
    let key = ContentKey::from_bytes([7; 32]);
    engine::stage_operations(&source.store, recipient, 1, &key, &source.signer).unwrap();
    let wires: Vec<Vec<u8>> = source.store.connection.prepare("SELECT encrypted_envelope FROM sync_wire_outbox WHERE recipient_device_id=?1 AND state='pending' ORDER BY rowid").unwrap().query_map([recipient],|r|r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    for wire in wires {
        assert!(
            engine::accept_relay_payload(
                &target.store,
                &STANDARD.encode(wire),
                &source.device.device_id,
                1,
                &key,
                &source.signer.public_key()
            )
            .unwrap()
        );
    }
}

#[test]
fn owner_forwards_leaf_operations_and_each_hop_waits_for_real_application_receipts() {
    let (a, b, c) = three();
    let text = "forwarded large input ".repeat(30_000);
    b.store
        .submit_input_as_event(&text, "capture", None)
        .unwrap();
    assert!(
        b.store
            .pending_sync_operations("c", 100)
            .unwrap()
            .is_empty()
    );
    deliver(&b, &a, "a");
    assert_eq!(a.store.list_events().unwrap()[0].raw_text, text.trim());
    let leaf_receipt = a.store.pending_sync_receipts("b", 100).unwrap().remove(0);
    b.store.accept_sync_receipt("a", &leaf_receipt).unwrap();
    assert!(
        b.store
            .pending_sync_operations("a", 100)
            .unwrap()
            .is_empty()
    );
    assert_eq!(a.store.pending_sync_operations("c", 100).unwrap().len(), 1);
    deliver(&a, &c, "c");
    let event = c.store.list_events().unwrap().remove(0);
    assert_eq!(event.raw_text, text.trim());
    assert!(c.store.pending_sync_receipts("b", 100).unwrap().is_empty());
    let receipts = c.store.pending_sync_receipts("a", 100).unwrap();
    assert_eq!(receipts.len(), 1);
    a.store.accept_sync_receipt("c", &receipts[0]).unwrap();
    assert!(
        a.store
            .pending_sync_operations("c", 100)
            .unwrap()
            .is_empty()
    );
    let foreign_watermark: i64 = a.store.connection.query_row("SELECT COALESCE((SELECT confirmed_sequence FROM sync_member_state WHERE device_id='c'),0)",[],|r|r.get(0)).unwrap();
    assert_eq!(
        foreign_watermark, 0,
        "a forwarded stream's sequence cannot advance the owner's GC watermark"
    );

    // A lost application receipt can be recreated without duplicating data.
    deliver(&a, &c, "c");
    assert_eq!(c.store.list_events().unwrap().len(), 1);
    assert_eq!(c.store.pending_sync_receipts("a", 100).unwrap().len(), 1);
}

#[test]
fn only_the_pinned_owner_can_attest_another_active_members_operation() {
    let (a, b, c) = three();
    c.store
        .submit_input_as_event("unforgeable origin", "capture", None)
        .unwrap();
    let op = c.store.pending_sync_operations("a", 100).unwrap().remove(0);
    let bytes = serde_json::to_vec(&op).unwrap();
    assert_eq!(
        a.store
            .receive_sync_operation("b", &bytes)
            .unwrap_err()
            .to_string(),
        "sync_sender_mismatch"
    );
    a.store.receive_sync_operation("c", &bytes).unwrap();
    b.store.receive_sync_operation("a", &bytes).unwrap();
    assert_eq!(b.store.list_events().unwrap().len(), 1);

    let mut roster = a.store.current_signed_roster().unwrap().unwrap().roster;
    roster.revision += 1;
    roster
        .members
        .iter_mut()
        .find(|m| m.device.device_id == "c")
        .unwrap()
        .active = false;
    b.store
        .apply_signed_roster(
            &SignedRoster::sign(roster, &a.signer)
                .unwrap()
                .encode()
                .unwrap(),
        )
        .unwrap();
    assert!(
        b.store.receive_sync_operation("a", &bytes).is_err(),
        "owner forwarding never revives a revoked source"
    );
}

#[test]
fn upgrade_removes_unrouted_leaf_wires_without_losing_the_owner_delivery() {
    let (a, b, c) = three();
    b.store
        .submit_input_as_event(&"legacy large input ".repeat(30_000), "capture", None)
        .unwrap();
    // Simulate an older binary's all-members outbox and fragmented envelopes.
    b.store.connection.execute(
        "INSERT INTO sync_outbox(op_id,recipient_device_id) SELECT op_id,'c' FROM sync_operations WHERE local=1", [],
    ).unwrap();
    engine::stage_operations(
        &b.store,
        "c",
        1,
        &ContentKey::from_bytes([7; 32]),
        &b.signer,
    )
    .unwrap();
    let before: i64 = b.store.connection.query_row("SELECT COUNT(*) FROM sync_wire_outbox WHERE recipient_device_id='c' AND state='pending'",[],|r|r.get(0)).unwrap();
    assert!(before > 1);
    b.store.reconcile_sync_upstream_outbox().unwrap();
    b.store.reconcile_sync_upstream_outbox().unwrap();
    let after: i64 = b.store.connection.query_row("SELECT COUNT(*) FROM sync_wire_outbox WHERE recipient_device_id='c' AND state='pending'",[],|r|r.get(0)).unwrap();
    assert_eq!(after, 0);
    assert_eq!(b.store.pending_sync_operations("a", 100).unwrap().len(), 1);
    deliver(&b, &a, "a");
    deliver(&a, &c, "c");
    assert_eq!(c.store.list_events().unwrap().len(), 1);
}
