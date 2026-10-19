use super::capture_wiki;
use super::deliver;
use super::isolated_count;
use super::pair;
use super::seed_wiki_page;
use super::*;

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
fn wiki_revision_oversize_reason_is_clamped_in_envelope_not_rejected() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    seed_wiki_page(&a, "page-1", "topic/p1", "页面一", "v1", &[]);
    capture_wiki(&a, "page-1");
    deliver(&a, &b);
    // AI 生成的长修改理由（>512 字节）曾触发信封校验
    // sync_wiki_revision_field_invalid，导致接受提案的整个业务事务
    // 回滚（提案永远无法采纳）。捕获时必须截断而不是拒绝。
    let long_reason = format!("用户确认知识建议：{}", "矛盾标注".repeat(200));
    assert!(long_reason.len() > 512);
    let tx = a.store.begin_write_transaction().unwrap();
    a.store
        .record_wiki_revision("page-1", "v2", &long_reason, None)
        .unwrap();
    if let Some(tx) = tx {
        tx.commit().unwrap();
    }
    // 本地审计保留全文。
    let local: String = a
        .store
        .connection
        .query_row(
            "SELECT reason FROM wiki_revisions WHERE page_id='page-1' ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(local, long_reason);
    deliver(&a, &b);
    let remote: String = b
        .store
        .connection
        .query_row(
            "SELECT reason FROM wiki_revisions WHERE page_id='page-1' ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(remote.len() <= 512);
    assert!(remote.ends_with('…'));
}
