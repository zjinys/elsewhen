use super::*;

#[test]
fn snapshot_resumes_its_original_encrypted_view_after_writes_restart_and_abort() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    a.store
        .submit_input_as_event(&"multi-chunk original ".repeat(200_000), "capture", None)
        .unwrap();
    let plane = MemBlob::new();
    let upload = mem_upload(&plane, "b", 1);
    let id = "stable-resume";
    stage_snapshot(&a.store, "b", id, 1, &key, &signer, &upload).unwrap();
    assert_eq!(export_state(&a.store, id), "building");
    assert!(a.store.snapshot_export_cached(id).unwrap());
    let blob: String = a
        .store
        .connection
        .query_row(
            "SELECT blob_id FROM sync_snapshot_exports WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(plane.live_chunks(&blob), WINDOW_CHUNKS);
    let wires = a.wires("b");
    let start = &wires
        .iter()
        .find(|(id, _)| id == "snapshot-start:stable-resume")
        .unwrap()
        .1;
    assert!(accept(&b.store, start, "a", &key, &signer));
    let public = signer.public_key();
    let download = mem_download(&plane, "a", 1, &key, &public);
    assert!(!import_blob_snapshots(&b.store, &download, 1).unwrap());
    let job = a.store.claim_analysis_job().unwrap().unwrap();
    a.store.complete_analysis(&job, "test", "{}").unwrap();
    a.store
        .submit_input_as_event("after pinned snapshot", "capture", None)
        .unwrap();
    a.store.abort_snapshot_export(id).unwrap();
    // The relay may expire the original blob while the receiver keeps its
    // durable prefix. Replay identical chunks from the cached export and let
    // the receiver re-ack that prefix rather than deadlocking on a full window.
    crate::sync::transport_relay::BlobSource::close(plane.as_ref(), &blob, "cancel").unwrap();
    for _ in 0..16 {
        let reopened = Store::open(&a.store.path).unwrap();
        stage_snapshot(&reopened, "b", id, 1, &key, &signer, &upload).unwrap();
        if export_state(&a.store, id) == "ready" {
            break;
        }
        assert!(!import_blob_snapshots(&b.store, &download, 1).unwrap());
    }
    assert_eq!(export_state(&a.store, id), "ready");
    let wires = a.wires("b");
    let manifest = &wires
        .iter()
        .find(|(id, _)| id == "snapshot-end:stable-resume")
        .unwrap()
        .1;
    assert!(accept(&b.store, manifest, "a", &key, &signer));
    assert!(import_blob_snapshots(&b.store, &download, 8).unwrap());
    assert_eq!(b.count("events"), 1);
    assert_eq!(
        b.count("event_analyses"),
        0,
        "later analysis belongs to incremental sync"
    );
    assert!(!b.store.snapshot_import_pending().unwrap());
    assert_eq!(a.count("events"), 2);

    let receipt_key = ContentKey::generate().unwrap();
    let receipt_signer = DeviceIdentity::generate().unwrap();
    stage_receipts(&b.store, "a", 1, &receipt_key, &receipt_signer).unwrap();
    let receipt = b
        .wires("a")
        .into_iter()
        .find(|(id, _)| id == &envelope_receipt_id("snapshot-end:stable-resume"))
        .unwrap();
    assert!(accept(
        &a.store,
        &receipt.1,
        "b",
        &receipt_key,
        &receipt_signer
    ));
    assert_eq!(
        a.count("sync_snapshot_export_chunks"),
        0,
        "release local ciphertext only after application acknowledgement"
    );
}

#[test]
fn cached_resnapshot_claim_is_reissued_on_the_next_tick_without_waiting_for_stall() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event(&"multi-chunk original ".repeat(200_000), "capture", None)
        .unwrap();
    a.store.connection.execute("INSERT INTO sync_member_state(device_id,confirmed_sequence,needs_resnapshot) VALUES('b',0,1)",[]).unwrap();
    let claim = a.store.claim_sync_resnapshot("b").unwrap().unwrap();
    let plane = MemBlob::new();
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    stage_snapshot(
        &a.store,
        "b",
        &claim.snapshot_id,
        1,
        &key,
        &signer,
        &mem_upload(&plane, "b", 1),
    )
    .unwrap();
    a.store
        .submit_input_as_event("keep using the source", "capture", None)
        .unwrap();
    let next = a.store.claim_sync_resnapshot("b").unwrap().unwrap();
    assert_eq!(next.snapshot_id, claim.snapshot_id);
    assert_eq!(export_state(&a.store, &claim.snapshot_id), "building");
}
