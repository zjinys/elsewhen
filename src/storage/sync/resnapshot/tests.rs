//! P4c follow-up — automatic re-snapshot scheduling: durable request rows,
//! nonce rotation, request-id binding, watermark re-seeding, and the
//! fail-closed rules. Fixture mirrors `gc/tests.rs`.

use crate::storage::Store;
use crate::storage::sync::{BeginExport, ReplicaIdentity};
use rusqlite::OptionalExtension;
use uuid::Uuid;

struct Db {
    store: Store,
    dir: std::path::PathBuf,
}

impl Db {
    fn new(device: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("elsewhen-sync-resnap-{}", Uuid::new_v4()));
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

    fn capture_n(&self, prefix: &str, n: usize) {
        for i in 0..n {
            self.store
                .submit_input_as_event(&format!("{prefix}-{i}"), "capture", None)
                .unwrap();
        }
    }

    fn flag(&self, member: &str) {
        self.store
            .connection
            .execute(
                "INSERT INTO sync_member_state(device_id,confirmed_sequence,needs_resnapshot)
                 VALUES(?1,0,1)
                 ON CONFLICT(device_id) DO UPDATE SET needs_resnapshot=1",
                [member],
            )
            .unwrap();
    }

    fn flagged(&self, member: &str) -> bool {
        self.store
            .connection
            .query_row(
                "SELECT needs_resnapshot FROM sync_member_state WHERE device_id=?1",
                [member],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .unwrap()
            .map(|v| v != 0)
            .unwrap_or(false)
    }

    fn export_state(&self, id: &str) -> Option<String> {
        self.store
            .connection
            .query_row(
                "SELECT state FROM sync_snapshot_exports WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .unwrap()
    }

    fn export_watermark(&self, id: &str) -> i64 {
        self.store
            .connection
            .query_row(
                "SELECT local_watermark FROM sync_snapshot_exports WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn watermark(&self, member: &str) -> Option<i64> {
        self.store
            .connection
            .query_row(
                "SELECT confirmed_sequence FROM sync_member_state WHERE device_id=?1",
                [member],
                |r| r.get(0),
            )
            .optional()
            .unwrap()
    }

    fn mark_export_ready(&self, id: &str) {
        self.store
            .connection
            .execute(
                "UPDATE sync_snapshot_exports SET state='ready' WHERE id=?1",
                [id],
            )
            .unwrap();
    }

    fn end_wire_hash(&self, export_id: &str) -> String {
        let message_id = format!("snapshot-end:{export_id}");
        self.store
            .connection
            .execute(
                "INSERT INTO sync_wire_outbox(message_id,recipient_device_id,payload_hash,key_version,encrypted_envelope)
                 VALUES(?1,'b','end-hash',1,x'00')
                 ON CONFLICT(message_id,recipient_device_id) DO NOTHING",
                [&message_id],
            )
            .unwrap();
        "end-hash".to_string()
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn pair(a: &Db, b: &Db) {
    a.store
        .authorize_sync_member(&b.device(), &b.stream())
        .unwrap();
    b.store
        .authorize_sync_member(&a.device(), &a.stream())
        .unwrap();
}

impl Db {
    fn device(&self) -> String {
        self.store.sync_identity().unwrap().unwrap().device_id
    }
    fn stream(&self) -> String {
        self.store.sync_identity().unwrap().unwrap().stream_id
    }
}

#[test]
fn claim_only_for_flagged_active_members_and_is_idempotent() {
    let a = Db::new("a");
    let b = Db::new("b");
    pair(&a, &b);

    // Not flagged → nothing.
    assert!(a.store.claim_sync_resnapshot("b").unwrap().is_none());
    // Self and unknown devices → nothing.
    a.flag("a");
    assert!(a.store.claim_sync_resnapshot("a").unwrap().is_none());
    a.flag("ghost");
    assert!(a.store.claim_sync_resnapshot("ghost").unwrap().is_none());

    a.flag("b");
    let claim = a
        .store
        .claim_sync_resnapshot("b")
        .unwrap()
        .expect("flagged active member claims");
    assert_eq!(claim.member_device_id, "b");
    assert_eq!(claim.snapshot_id, "resnap-b-1");

    // Idempotent while no export row exists: same claim again.
    let again = a.store.claim_sync_resnapshot("b").unwrap().unwrap();
    assert_eq!(again.snapshot_id, "resnap-b-1");
}

#[test]
fn in_flight_export_blocks_new_claims_until_finished() {
    let a = Db::new("a");
    let b = Db::new("b");
    pair(&a, &b);
    a.capture_n("cap", 5);
    a.flag("b");
    let claim = a.store.claim_sync_resnapshot("b").unwrap().unwrap();

    // A building export keeps the claim: no rotation, no second delivery.
    assert!(matches!(
        a.store
            .begin_snapshot_export(&claim.snapshot_id, "b", 1)
            .unwrap(),
        BeginExport::Begun
    ));
    assert!(a.store.claim_sync_resnapshot("b").unwrap().is_none());
    assert_eq!(a.export_watermark(&claim.snapshot_id), 5);

    // Aborted attempts free the member to RESUME the same request (same id:
    // the rebuilt export is byte-identical under a reused id, so a partially
    // delivered old transfer can never be confused with a new generation).
    a.store.abort_snapshot_export(&claim.snapshot_id).unwrap();
    let retry = a.store.claim_sync_resnapshot("b").unwrap().unwrap();
    assert_eq!(retry.snapshot_id, "resnap-b-1");
}

#[test]
fn begin_export_rejects_ids_that_do_not_match_the_current_request() {
    let a = Db::new("a");
    let b = Db::new("b");
    pair(&a, &b);
    a.flag("b");
    let claim = a.store.claim_sync_resnapshot("b").unwrap().unwrap();

    // A stale id from an earlier/foreign request is refused: an old transfer
    // can never satisfy the current request.
    let err = a
        .store
        .begin_snapshot_export("resnap-b-99", "b", 1)
        .unwrap_err();
    assert!(
        err.to_string().contains("resnapshot_request_mismatch"),
        "unexpected: {err}"
    );
    assert!(matches!(
        a.store
            .begin_snapshot_export(&claim.snapshot_id, "b", 1)
            .unwrap(),
        BeginExport::Begun
    ));
}

#[test]
fn finish_completes_only_the_current_acknowledged_request_and_reseeds_watermark() {
    let a = Db::new("a");
    let b = Db::new("b");
    pair(&a, &b);
    a.capture_n("pre", 7);
    a.flag("b");
    let claim = a.store.claim_sync_resnapshot("b").unwrap().unwrap();
    assert!(matches!(
        a.store
            .begin_snapshot_export(&claim.snapshot_id, "b", 1)
            .unwrap(),
        BeginExport::Begun
    ));
    assert_eq!(a.export_watermark(&claim.snapshot_id), 7);
    a.capture_n("post", 3); // live frontier now 10, above the build watermark

    // Not acknowledged yet: finish is a no-op, the flag stays.
    assert_eq!(a.store.finish_sync_resnapshot_requests().unwrap(), 0);
    assert!(a.flagged("b"));

    // Acknowledge the export exactly as the receipt path does (peer, id,
    // end-wire hash); the export pins the build-time watermark (7).
    a.mark_export_ready(&claim.snapshot_id);
    a.store
        .acknowledge_snapshot_export(
            "b",
            &claim.snapshot_id,
            &format!("snapshot-end:{}", claim.snapshot_id),
            &a.end_wire_hash(&claim.snapshot_id),
        )
        .unwrap();
    assert_eq!(a.store.finish_sync_resnapshot_requests().unwrap(), 1);
    assert!(!a.flagged("b"), "flag cleared on completion");
    assert_eq!(
        a.watermark("b"),
        Some(7),
        "watermark re-seeded at build-time frontier, not 0 and not live"
    );
    // Request row is gone; second finish is a no-op.
    assert_eq!(a.store.finish_sync_resnapshot_requests().unwrap(), 0);
}

#[test]
fn reflag_rotates_request_so_an_old_ack_cannot_satisfy_the_new_one() {
    let a = Db::new("a");
    let b = Db::new("b");
    pair(&a, &b);
    a.capture_n("cap", 3);
    a.flag("b");
    let first = a.store.claim_sync_resnapshot("b").unwrap().unwrap();
    assert!(matches!(
        a.store
            .begin_snapshot_export(&first.snapshot_id, "b", 1)
            .unwrap(),
        BeginExport::Begun
    ));
    // Old export completes with its build-time watermark (3)...
    a.mark_export_ready(&first.snapshot_id);
    a.store
        .acknowledge_snapshot_export(
            "b",
            &first.snapshot_id,
            &format!("snapshot-end:{}", first.snapshot_id),
            &a.end_wire_hash(&first.snapshot_id),
        )
        .unwrap();
    assert_eq!(a.store.finish_sync_resnapshot_requests().unwrap(), 1);

    // ...then the member lags again: a NEW request with a NEW id.
    a.flag("b");
    let second = a.store.claim_sync_resnapshot("b").unwrap().unwrap();
    assert_ne!(first.snapshot_id, second.snapshot_id);
    assert_eq!(second.snapshot_id, "resnap-b-2");
    // The already-acknowledged old export must not satisfy the new request.
    assert_eq!(a.store.finish_sync_resnapshot_requests().unwrap(), 0);
    assert!(a.flagged("b"), "new request still open");
}

#[test]
fn resnapshot_states_report_pending_flagged_members_read_only() {
    let a = Db::new("a");
    let b = Db::new("b");
    let c = Db::new("c");
    pair(&a, &b);
    pair(&a, &c);
    assert!(a.store.sync_resnapshot_states().unwrap().is_empty());
    a.flag("c");
    a.flag("b");
    let claim = a.store.claim_sync_resnapshot("b").unwrap().unwrap();
    a.store
        .begin_snapshot_export(&claim.snapshot_id, "b", 1)
        .unwrap();
    let states = a.store.sync_resnapshot_states().unwrap();
    assert_eq!(states.len(), 2);
    let b_state = states.iter().find(|s| s.member_device_id == "b").unwrap();
    assert_eq!(b_state.snapshot_id.as_deref(), Some("resnap-b-1"));
    assert_eq!(b_state.export_state.as_deref(), Some("building"));
    let c_state = states.iter().find(|s| s.member_device_id == "c").unwrap();
    assert_eq!(c_state.snapshot_id, None);
    assert_eq!(c_state.export_state, None);
}
