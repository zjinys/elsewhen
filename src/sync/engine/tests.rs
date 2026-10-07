use super::snapshot::stage_snapshot;
use super::*;
use crate::storage::sync::ReplicaIdentity;
use rusqlite::params;

mod fragments;

struct Database {
    store: Store,
    directory: std::path::PathBuf,
}
impl Database {
    fn new(device: &str) -> Self {
        let directory =
            std::env::temp_dir().join(format!("elsewhen-sync-engine-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let store = Store::open(&directory.join("test.db")).unwrap();
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "dataset".into(),
                device_id: device.into(),
                stream_id: format!("stream-{device}"),
                processor_device_id: "a".into(),
            })
            .unwrap();
        Self { store, directory }
    }
    fn wires(&self, peer: &str) -> Vec<(String, Vec<u8>)> {
        self.store.connection.prepare("SELECT message_id,encrypted_envelope FROM sync_wire_outbox WHERE recipient_device_id=?1 ORDER BY rowid")
            .unwrap().query_map([peer], |r| Ok((r.get(0)?,r.get(1)?))).unwrap()
            .collect::<rusqlite::Result<Vec<_>>>().unwrap()
    }
    fn count(&self, table: &str) -> i64 {
        self.store
            .connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
fn pair(a: &Database, b: &Database) {
    a.store.authorize_sync_member("b", "stream-b").unwrap();
    b.store.authorize_sync_member("a", "stream-a").unwrap();
}
fn accept(
    store: &Store,
    bytes: &[u8],
    peer: &str,
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> bool {
    accept_relay_payload(
        store,
        &STANDARD.encode(bytes),
        peer,
        1,
        key,
        &signer.public_key(),
    )
    .unwrap()
}

#[test]
fn snapshot_delivery_survives_manifest_first_restart_and_lost_application_receipt() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let backward_key = ContentKey::generate().unwrap();
    let backward_signer = DeviceIdentity::generate().unwrap();
    a.store
        .submit_input_as_event(&"multi-chunk original ".repeat(60_000), "capture", None)
        .unwrap();
    a.store
        .save_ai_provider_config(
            None,
            "provider",
            "openai-compatible",
            "http://127.0.0.1:9999/v1",
            "model",
            "SYNTHETIC_SNAPSHOT_SECRET",
            0.5,
            Some(1024),
        )
        .unwrap();
    stage_provider_updates(
        &a.store,
        &[EncryptionPeer {
            device_id: "b",
            key_version: 1,
            key: &key,
        }],
        &signer,
    )
    .unwrap();
    stage_snapshot(&a.store, "b", "initial", 1, &key, &signer).unwrap();
    let original_wires = a.wires("b");
    let chunks: Vec<_> = original_wires
        .iter()
        .filter(|(id, _)| id.starts_with("snapshot:"))
        .collect();
    assert!(chunks.len() > 4);
    assert!(original_wires.iter().all(|(_,bytes)| !String::from_utf8_lossy(bytes).contains("SYNTHETIC_SNAPSHOT_SECRET")));
    let manifest = &original_wires
        .iter()
        .find(|(id, _)| id == "snapshot-end:initial")
        .unwrap()
        .1;
    a.store
        .submit_input_as_event("after checkpoint", "capture", None)
        .unwrap();
    let reopened_source = Store::open(&a.store.path).unwrap();
    stage_snapshot(&reopened_source, "b", "initial", 1, &key, &signer).unwrap();
    assert_eq!(
        original_wires,
        a.wires("b"),
        "retry must not regenerate the snapshot from a newer source view"
    );
    b.store.begin_snapshot_import("initial", "a", 1).unwrap();
    assert!(accept(&b.store, manifest, "a", &key, &signer));
    assert_eq!(b.count("events"), 0);
    assert!(
        b.store
            .pending_sync_wire_receipts("a", 100)
            .unwrap()
            .is_empty()
    );
    // A provider update and an event arriving early must not contaminate staging.
    let provider = &original_wires
        .iter()
        .find(|(id, _)| id.starts_with("provider:"))
        .unwrap()
        .1;
    assert!(!accept(&b.store, provider, "a", &key, &signer));
    let op = a
        .store
        .pending_sync_operations("b", 100)
        .unwrap()
        .pop()
        .unwrap();
    let header =
        EnvelopeHeader::new("dataset", "a", "b", 1, operation_message_id(&op.op_id)).unwrap();
    let delta = crypto::seal(header, &key, &signer, &serde_json::to_vec(&op).unwrap())
        .unwrap()
        .to_bytes()
        .unwrap();
    assert!(!accept(&b.store, &delta, "a", &key, &signer));
    assert_eq!(b.count("sync_operations"), 0);
    assert!(accept(&b.store, &chunks[0].1, "a", &key, &signer));
    let reopened = Store::open(&b.store.path).unwrap();
    for (_, bytes) in chunks.iter().skip(1).rev() {
        assert!(accept(&reopened, bytes, "a", &key, &signer));
    }
    assert!(!b.store.snapshot_import_pending().unwrap());
    assert_eq!(b.count("events"), 1);
    assert_eq!(
        b.store
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        "SYNTHETIC_SNAPSHOT_SECRET"
    );
    assert_eq!(b.count("analysis_jobs"), 0);
    assert_eq!(b.count("chat_runs"), 0);
    assert!(accept(&b.store, provider, "a", &key, &signer));
    assert!(accept(&b.store, &delta, "a", &key, &signer));
    assert_eq!(b.count("events"), 2);
    // Simulate a successful receipt upload whose response/content was lost.
    stage_receipts(&b.store, "a", 1, &backward_key, &backward_signer).unwrap();
    b.store
        .mark_sync_wire_receipt_sent("a", "snapshot-end:initial")
        .unwrap();
    let pending = b.wires("a");
    let final_receipt_id = envelope_receipt_id("snapshot-end:initial");
    let original_receipt = &pending
        .iter()
        .find(|(id, _)| id == &final_receipt_id)
        .unwrap()
        .1;
    let receipt_hash = crate::storage::sync::hash(
        &crypto::open(
            &EncryptedEnvelope::from_bytes(original_receipt).unwrap(),
            &EnvelopeHeader::new("dataset", "b", "a", 1, &final_receipt_id).unwrap(),
            &backward_key,
            &backward_signer.public_key(),
        )
        .unwrap(),
    );
    b.store
        .acknowledge_sync_wire("a", &final_receipt_id, &receipt_hash)
        .unwrap();
    // Replayed manifest (or chunk) regenerates only the receipt, never the data.
    assert!(accept(&b.store, manifest, "a", &key, &signer));
    stage_receipts(&b.store, "a", 1, &backward_key, &backward_signer).unwrap();
    assert_eq!(b.count("events"), 2);
    assert!(
        b.store
            .due_sync_wire_envelopes("a", i64::MAX, 10)
            .unwrap()
            .iter()
            .any(|e| e.message_id == final_receipt_id)
    );
    assert!(accept(
        &a.store,
        original_receipt,
        "b",
        &backward_key,
        &backward_signer
    ));
    assert!(accept(
        &a.store,
        original_receipt,
        "b",
        &backward_key,
        &backward_signer
    ));
    let remaining: i64 = a.store.connection.query_row("SELECT COUNT(*) FROM sync_wire_outbox WHERE message_id LIKE 'snapshot%' AND state!='acknowledged'",[],|r|r.get(0)).unwrap();
    assert_eq!(remaining, 0);
    assert_eq!(
        a.store.pending_sync_operations("b", 100).unwrap().len(),
        2,
        "snapshot confirmation is not a per-operation GC watermark"
    );
}

#[test]
fn partial_export_is_not_uploadable_and_new_request_is_required_after_restart() {
    let a = Database::new("a");
    a.store.authorize_sync_member("b", "stream-b").unwrap();
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    assert!(a.store.begin_snapshot_export("partial", "b", 1).unwrap());
    let header = EnvelopeHeader::new("dataset", "a", "b", 1, "snapshot:partial:0").unwrap();
    a.store
        .stage_snapshot_export_payload("partial", &header, &key, &signer, b"incomplete", false)
        .unwrap();
    assert!(
        a.store
            .due_sync_wire_envelopes("b", i64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    let reopened = Store::open(&a.store.path).unwrap();
    assert!(stage_snapshot(&reopened, "b", "partial", 1, &key, &signer).is_err());
    reopened.abort_snapshot_export("partial").unwrap();
    assert!(
        reopened
            .due_sync_wire_envelopes("b", i64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    stage_snapshot(&reopened, "b", "replacement", 1, &key, &signer).unwrap();
    assert!(
        !reopened
            .due_sync_wire_envelopes("b", i64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    assert!(
        a.store
            .acknowledge_snapshot_export("b", "partial", "wrong")
            .is_err()
    );
    assert!(
        a.store
            .acknowledge_snapshot_export("b", "replacement", "wrong")
            .is_err()
    );
    assert!(
        a.store
            .begin_snapshot_export("replacement", "b", 2)
            .is_err()
    );
    a.store.revoke_sync_member("b").unwrap();
    assert!(
        a.store
            .due_sync_wire_envelopes("b", i64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    assert!(stage_snapshot(&a.store, "b", "replacement", 1, &key, &signer).is_err());
}

#[test]
fn unsolicited_snapshot_never_selects_or_overwrites_the_receiving_database() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    stage_snapshot(&a.store, "b", "not-requested", 1, &key, &signer).unwrap();
    for (_, bytes) in a.wires("b") {
        assert!(
            accept_relay_payload(
                &b.store,
                &STANDARD.encode(bytes),
                "a",
                1,
                &key,
                &signer.public_key()
            )
            .is_err()
        );
    }
    assert_eq!(b.count("sync_snapshot_target"), 0);
    assert_eq!(b.count("sync_snapshot_chunks"), 0);
    b.store
        .submit_input_as_event("local stays", "capture", None)
        .unwrap();
    assert!(
        b.store
            .begin_snapshot_import("not-requested", "a", 1)
            .is_err()
    );
    assert_eq!(b.store.list_events().unwrap()[0].raw_text, "local stays");
}

#[test]
fn scan_cursor_is_durable_and_not_an_application_frontier() {
    let b = Database::new("b");
    assert_eq!(b.store.relay_scan_cursor("mailbox").unwrap(), "0");
    b.store
        .save_relay_scan_cursor("mailbox", "0000000000000003")
        .unwrap();
    let reopened = Store::open(&b.store.path).unwrap();
    assert_eq!(
        reopened.relay_scan_cursor("mailbox").unwrap(),
        "0000000000000003"
    );
    assert_eq!(b.count("sync_receipts"), 0);
    assert_eq!(
        b.store
            .connection
            .query_row(
                "SELECT applied_sequence FROM sync_frontiers WHERE origin_stream_id=?1",
                params!["stream-b"],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    reopened.save_relay_scan_cursor("mailbox", "0").unwrap();
    assert_eq!(b.store.relay_scan_cursor("mailbox").unwrap(), "0");
}
