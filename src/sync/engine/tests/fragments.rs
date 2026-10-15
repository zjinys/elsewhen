use super::*;
use crate::sync::fragments::{CHUNK_BYTES, OperationFragment};

fn part(
    bytes: &[u8],
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> (EnvelopeHeader, OperationFragment) {
    let envelope = EncryptedEnvelope::from_bytes(bytes).unwrap();
    let header = envelope.unverified_header().clone();
    let (part, _) = OperationFragment::decode(
        &crypto::open(&envelope, &header, key, &signer.public_key()).unwrap(),
    )
    .unwrap();
    (header, part)
}
fn sealed(
    header: EnvelopeHeader,
    part: &OperationFragment,
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> Vec<u8> {
    crypto::seal(header, key, signer, &serde_json::to_vec(part).unwrap())
        .unwrap()
        .to_bytes()
        .unwrap()
}

#[test]
fn large_capture_fragments_reassemble_after_restart_and_ack_only_the_complete_operation() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let text = "large delta 原文 ".repeat(80_000);
    a.store
        .submit_input_as_event(&text, "capture", None)
        .unwrap();
    assert_eq!(
        stage_operations(&a.store, "b", 1, &key, &signer).unwrap(),
        1
    );
    let wires = a.wires("b");
    assert!(wires.len() > 4);
    assert!(
        wires
            .iter()
            .all(|(id, bytes)| id.starts_with("op-part-")
                && bytes.len() <= crypto::MAX_ENVELOPE_BYTES)
    );
    stage_operations(&a.store, "b", 1, &key, &signer).unwrap();
    assert_eq!(a.wires("b"), wires);
    for (_, bytes) in wires.iter().skip(1).rev() {
        assert!(accept(&b.store, bytes, "a", &key, &signer));
    }
    assert_eq!(b.count("events"), 0);
    assert_eq!(b.count("sync_receipts"), 0);
    let reopened = Store::open(&b.store.path).unwrap();
    assert!(accept(&reopened, &wires[0].1, "a", &key, &signer));
    assert_eq!(b.count("events"), 1);
    assert_eq!(b.count("input_records"), 1);
    assert!(
        b.store.list_events().unwrap()[0].raw_text == a.store.list_events().unwrap()[0].raw_text,
        "the imported event must exactly match the immutable source event"
    );
    assert_eq!(b.count("analysis_jobs"), 0);
    let blobs: i64 = b.store.connection.query_row("SELECT COUNT(*) FROM sync_operation_fragment_parts WHERE encrypted_envelope IS NOT NULL",[],|r|r.get(0)).unwrap();
    assert_eq!(blobs, 0, "retain hashes, release assembled ciphertext");
    let backward_key = ContentKey::generate().unwrap();
    let backward_signer = DeviceIdentity::generate().unwrap();
    stage_receipts(&b.store, "a", 1, &backward_key, &backward_signer).unwrap();
    let receipt = &b.wires("a")[0].1;
    assert!(accept(
        &a.store,
        receipt,
        "b",
        &backward_key,
        &backward_signer
    ));
    assert!(
        a.store
            .pending_sync_operations("b", 100)
            .unwrap()
            .is_empty()
    );
    assert!(
        a.store
            .due_sync_wire_envelopes("b", i64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    for (_, bytes) in &wires {
        assert!(accept(&b.store, bytes, "a", &key, &signer));
    }
    assert_eq!(b.count("events"), 1);
    // Same fragment identity with changed bytes is rejected even after compaction.
    let (header, mut changed) = part(&wires[0].1, &key, &signer);
    changed.data = STANDARD.encode(vec![b'x'; CHUNK_BYTES]);
    assert!(
        accept_relay_payload(
            &b.store,
            &STANDARD.encode(sealed(header, &changed, &key, &signer)),
            "a",
            1,
            &key,
            &signer.public_key()
        )
        .is_err()
    );
}

#[test]
fn large_message_waits_for_its_conversation_without_acknowledging_the_operation() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let conversation = a.store.create_conversation(Some("parent"), None).unwrap();
    a.store
        .submit_conversation_input(&conversation, &"large input ".repeat(60_000), None)
        .unwrap();
    stage_operations(&a.store, "b", 1, &key, &signer).unwrap();
    let wires = a.wires("b");
    for (_, bytes) in wires.iter().filter(|(id, _)| id.starts_with("op-part-")) {
        assert!(accept(&b.store, bytes, "a", &key, &signer));
    }
    assert_eq!(b.count("events"), 0);
    assert_eq!(b.count("sync_receipts"), 0);
    let (_, parent) = wires
        .iter()
        .find(|(id, _)| !id.starts_with("op-part-"))
        .unwrap();
    assert!(accept(&b.store, parent, "a", &key, &signer));
    assert_eq!(b.count("events"), 1);
    assert_eq!(b.count("messages"), 1);
    assert_eq!(b.count("sync_receipts"), 2);
}

#[test]
fn invalid_hash_or_domain_commit_failure_never_commits_half_a_large_capture() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    a.store
        .submit_input_as_event(&"rollback input ".repeat(50_000), "capture", None)
        .unwrap();
    stage_operations(&a.store, "b", 1, &key, &signer).unwrap();
    let wires = a.wires("b");
    // Inject a receipt failure at the final domain commit, after event insertion.
    b.store.connection.execute_batch("CREATE TRIGGER fail_fragment_receipt BEFORE INSERT ON sync_receipts BEGIN SELECT RAISE(ABORT,'synthetic receipt failure'); END;").unwrap();
    for (_, bytes) in wires.iter().skip(1) {
        assert!(accept(&b.store, bytes, "a", &key, &signer));
    }
    assert!(
        accept_relay_payload(
            &b.store,
            &STANDARD.encode(&wires[0].1),
            "a",
            1,
            &key,
            &signer.public_key()
        )
        .is_err()
    );
    assert_eq!(b.count("events"), 0);
    assert_eq!(b.count("input_records"), 0);
    assert_eq!(b.count("sync_operations"), 0);
    b.store
        .connection
        .execute_batch("DROP TRIGGER fail_fragment_receipt")
        .unwrap();
    assert!(accept(&b.store, &wires[0].1, "a", &key, &signer));
    assert_eq!(b.count("events"), 1);
    let c = Database::new("c");
    c.store.authorize_sync_member("a", "stream-a").unwrap();
    for (position, (_, bytes)) in wires.iter().enumerate() {
        let (mut header, mut changed) = part(bytes, &key, &signer);
        header.recipient_device_id = "c".into();
        changed.payload_hash = "0".repeat(64);
        let bytes = sealed(header, &changed, &key, &signer);
        let result = accept_relay_payload(
            &c.store,
            &STANDARD.encode(bytes),
            "a",
            1,
            &key,
            &signer.public_key(),
        );
        if position + 1 == wires.len() {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap());
        }
    }
    assert_eq!(c.count("events"), 0);
    assert_eq!(c.count("sync_receipts"), 0);
}

#[test]
fn fragments_enforce_peer_quotas_and_unknown_versions_without_blocking_other_origins() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let mut fragment = OperationFragment {
        version: 1,
        op_id: "stream-a:1".into(),
        payload_hash: "0".repeat(64),
        total_bytes: CHUNK_BYTES + 1,
        chunk_count: 2,
        index: 0,
        data: STANDARD.encode(vec![b'x'; CHUNK_BYTES]),
    };
    for sequence in 1..=5 {
        fragment.op_id = format!("stream-a:{sequence}");
        let header = EnvelopeHeader::new("dataset", "a", "b", 1, fragment.message_id()).unwrap();
        let result = accept_relay_payload(
            &b.store,
            &STANDARD.encode(sealed(header, &fragment, &key, &signer)),
            "a",
            1,
            &key,
            &signer.public_key(),
        );
        if sequence <= 4 {
            assert!(result.unwrap());
        } else {
            assert!(result.is_err());
        }
    }
    assert_eq!(b.count("sync_operation_fragments"), 4);
    fragment.version = 999;
    assert!(OperationFragment::decode(&serde_json::to_vec(&fragment).unwrap()).is_err());
    fragment.version = 1;
    fragment.total_bytes = usize::MAX;
    assert!(OperationFragment::decode(&serde_json::to_vec(&fragment).unwrap()).is_err());
    let c = Database::new("c");
    b.store.authorize_sync_member("c", "stream-c").unwrap();
    c.store.authorize_sync_member("b", "stream-b").unwrap();
    c.store
        .submit_input_as_event("independent source", "capture", None)
        .unwrap();
    stage_operations(&c.store, "b", 1, &key, &signer).unwrap();
    assert!(accept(&b.store, &c.wires("b")[0].1, "c", &key, &signer));
    assert_eq!(b.count("events"), 1);
    assert_eq!(b.store.pending_sync_receipts("a", 100).unwrap().len(), 0);
}
