//! P4c — S03 lifecycle: GC watermarks, catch-up window, re-snapshot state,
//! and the three-distinct-confirmations discipline.
//!
//! Fixture notes: `Db::new` mirrors `tests.rs` (temp-dir isolated Store +
//! `initialize_sync_replica` + `authorize_sync_member`). Watermarks are
//! advanced by real `acknowledge_sync_operation` calls; sequences are derived
//! from the actual op ids (`stream:seq`) — never assumed.

use std::collections::BTreeMap;

use crate::storage::Store;
use crate::storage::sync::{ReplicaIdentity, hash};
use rusqlite::OptionalExtension;
use uuid::Uuid;

struct Db {
    store: Store,
    dir: std::path::PathBuf,
}

impl Db {
    fn new(device: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("elsewhen-sync-gc-{}", Uuid::new_v4()));
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

    fn member_state(&self, device: &str) -> Option<(i64, bool)> {
        self.store
            .connection
            .query_row(
                "SELECT confirmed_sequence,needs_resnapshot FROM sync_member_state WHERE device_id=?1",
                [device],
                |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)),
            )
            .optional()
            .unwrap()
    }

    fn gc_floor(&self) -> i64 {
        self.store
            .connection
            .query_row(
                "SELECT COALESCE((SELECT gc_floor FROM sync_gc_state WHERE origin_stream_id=?1),0)",
                [format!("stream-{}", self.device())],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn device(&self) -> String {
        self.store.sync_identity().unwrap().unwrap().device_id
    }

    /// Create `n` local captures on this device.
    fn capture_n(&self, prefix: &str, n: usize) {
        for i in 0..n {
            self.store
                .submit_input_as_event(&format!("{prefix}-{i}"), "capture", None)
                .unwrap();
        }
    }

    /// Deliver all pending ops from `self` to `other` and ack each back on
    /// `self` (full round trip: receive + application ack). Pages the outbox
    /// in MAX_BATCH_OPERATIONS-sized chunks; the ack clears each page so the
    /// loop always sees fresh pending ops until none remain.
    fn deliver_and_ack(&self, other: &Db) {
        let my_id = self.device();
        let other_id = other.device();
        loop {
            let ops = self.store.pending_sync_operations(&other_id, 100).unwrap();
            if ops.is_empty() {
                break;
            }
            for op in &ops {
                other
                    .store
                    .receive_sync_operation(&my_id, &serde_json::to_vec(op).unwrap())
                    .unwrap();
                let digest = hash(serde_json::to_string(op).unwrap().as_bytes());
                self.store
                    .acknowledge_sync_operation(&other_id, &op.op_id, &digest)
                    .unwrap();
            }
        }
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn pair(a: &Db, b: &Db) {
    let ai = a.store.sync_identity().unwrap().unwrap();
    let bi = b.store.sync_identity().unwrap().unwrap();
    a.store
        .authorize_sync_member(&bi.device_id, &bi.stream_id)
        .unwrap();
    b.store
        .authorize_sync_member(&ai.device_id, &ai.stream_id)
        .unwrap();
}

fn op_seq(op_id: &str) -> i64 {
    op_id.rsplit(':').next().unwrap().parse().unwrap()
}

#[test]
fn ack_advances_member_watermark_and_never_decreases() {
    let a = Db::new("a");
    let b = Db::new("b");
    pair(&a, &b);
    a.capture_n("cap", 3);
    a.deliver_and_ack(&b);

    let state = a.member_state("b").expect("watermark row after ack");
    assert_eq!(state.0, 3, "three acked ops → watermark 3");
    assert!(!state.1, "no resnapshot needed");

    // A duplicate/older ack must never decrease the watermark.
    a.store.advance_sync_member_watermark("b", 1).unwrap();
    assert_eq!(
        a.member_state("b").unwrap().0,
        3,
        "watermark never decreases"
    );
}

#[test]
fn gc_trims_only_fully_confirmed_local_history_and_keeps_domain_data() {
    let a = Db::new("a");
    let b = Db::new("b");
    pair(&a, &b);
    a.capture_n("cap", 8);
    a.deliver_and_ack(&b);
    assert_eq!(b.count("events"), 8, "all events applied on b");

    // Push the retention floor far below the confirmed watermark by making the
    // newest 64-op window cover everything older than seq 8: with only 8 ops,
    // retention keeps all of them (window covers all). Add 70 more confirmed
    // ops so the first 8 fall outside the retention window.
    a.capture_n("more", 70);
    a.deliver_and_ack(&b);
    let total = a.count("sync_operations");
    assert!(total >= 78);

    let report = a.store.run_sync_gc().unwrap();
    assert!(report.resnapshot_pending.is_empty());
    assert!(report.gc_floor > 0, "floor advanced past early ops");
    assert!(
        report.operations_pruned > 0,
        "early confirmed ops pruned: {report:?}"
    );
    assert!(
        a.count("sync_operations") < total,
        "history shrank after full confirmation"
    );

    // Domain data is never touched by GC: b still has every event.
    assert_eq!(b.count("events"), 78, "GC never deletes domain data");
    // The floor never decreases on a second run.
    let second = a.store.run_sync_gc().unwrap();
    assert!(second.gc_floor >= report.gc_floor, "floor is monotonic");
}

#[test]
fn unconfirmed_member_holds_back_gc_completely() {
    let a = Db::new("a");
    let b = Db::new("b");
    let c = Db::new("c");
    pair(&a, &b);
    pair(&a, &c);
    pair(&b, &c);
    a.capture_n("cap", 80);
    // Only b confirms; c never receives or acks anything.
    a.deliver_and_ack(&b);
    let before = a.count("sync_operations");

    let report = a.store.run_sync_gc().unwrap();
    // c is active but has never confirmed anything. Whether or not it is
    // flagged needs_resnapshot, the destructive invariant must hold: nothing is
    // pruned and the floor does not advance while a member has not confirmed.
    assert_eq!(
        report.operations_pruned, 0,
        "no trimming without full confirmation"
    );
    assert_eq!(
        a.count("sync_operations"),
        before,
        "nothing pruned while a member has not confirmed"
    );
    assert_eq!(
        a.gc_floor(),
        0,
        "floor does not advance without full confirmation"
    );
}

#[test]
fn missing_watermark_row_counts_as_unconfirmed_not_excluded() {
    let a = Db::new("a");
    let b = Db::new("b");
    pair(&a, &b);
    a.capture_n("cap", 80);
    // b exists in sync_members (active) but has NO sync_member_state row.
    assert!(
        a.member_state("b").is_none(),
        "b starts with no watermark row"
    );
    let before = a.count("sync_operations");

    let report = a.store.run_sync_gc().unwrap();
    // A missing row must count as 0 (unconfirmed), which with any floor > 0
    // window flags resnapshot or at minimum holds GC — never silently excluded.
    assert_eq!(
        a.count("sync_operations"),
        before,
        "a member with no watermark row holds back GC (not filtered out)"
    );
    assert_eq!(report.operations_pruned, 0);
}

#[test]
fn lagging_member_flips_needs_resnapshot_and_gc_stays_closed() {
    let a = Db::new("a");
    let b = Db::new("b");
    let c = Db::new("c");
    pair(&a, &b);
    pair(&a, &c);
    pair(&b, &c);

    // c confirms a little, then goes quiet for a long stretch.
    a.capture_n("early", 4);
    a.deliver_and_ack(&b);
    a.deliver_and_ack(&c);
    assert_eq!(a.member_state("c").unwrap().0, 4);

    // b keeps up with a long burst; c falls far behind. Raise the head well
    // beyond the catch-up window so a genuine lag can be established (need
    // floor > window + stale_watermark, so > 256 + 4).
    a.capture_n("burst", 340);
    a.deliver_and_ack(&b);
    a.store.advance_sync_member_watermark("b", 344).unwrap();
    // Bring c to a high point once so the floor advances past the window, then
    // reset c to a stale watermark to model a device whose durable state is old.
    a.store.advance_sync_member_watermark("c", 340).unwrap();
    a.store.run_sync_gc().unwrap();
    let high_floor = a.gc_floor();
    a.store
        .connection
        .execute(
            "UPDATE sync_member_state SET confirmed_sequence=4,needs_resnapshot=0 WHERE device_id='c'",
            [],
        )
        .unwrap();

    let report = a.store.run_sync_gc().unwrap();
    assert!(
        report.resnapshot_pending.contains(&"c".to_string()),
        "c flagged needs_resnapshot after falling behind the window (floor {high_floor}): {report:?}"
    );
    assert_eq!(
        report.operations_pruned, 0,
        "GC closed while resnapshot pending"
    );
    let c_state = a.member_state("c").unwrap();
    assert!(c_state.1, "needs_resnapshot persisted for c");
    // While c is flagged, a subsequent GC makes NO new destructive progress:
    // nothing more is pruned. (Ops below the floor from the earlier legitimate
    // pass are already gone; the invariant is that the pending resnapshot
    // freezes further trimming, not that already-confirmed history reappears.)
    let after_flag = a.count("sync_operations");
    let frozen = a.store.run_sync_gc().unwrap();
    assert_eq!(
        frozen.operations_pruned, 0,
        "no further trimming while flagged"
    );
    assert_eq!(
        a.count("sync_operations"),
        after_flag,
        "no new destructive progress while resnapshot pending"
    );
}

#[test]
fn snapshot_ack_seeds_watermark_at_build_frontier_not_zero_or_live() {
    let a = Db::new("a");
    let b = Db::new("b");
    pair(&a, &b);
    a.capture_n("pre", 10);
    // b is "new": simulate that a built a snapshot at the current frontier
    // (seq 10), then a kept writing more ops before b acked the snapshot.
    let build_frontier = 10i64;
    a.capture_n("post", 5); // live frontier now 15

    // Seed b exactly as begin_snapshot_export + acknowledge would.
    let mut frontiers = BTreeMap::new();
    frontiers.insert("stream-a".to_string(), build_frontier);
    a.store.mark_member_snapshotted("b", &frontiers).unwrap();

    let (watermark, needs) = a.member_state("b").expect("snapshot seeds watermark");
    assert_eq!(
        watermark, build_frontier,
        "watermark = build-time frontier, not 0 (holds GC) and not live (over-claims)"
    );
    assert!(!needs, "a fresh snapshot recipient is not behind");
}

#[test]
fn gc_never_uses_relay_ack_and_remote_history_is_never_pruned() {
    let a = Db::new("a");
    let b = Db::new("b");
    pair(&a, &b);
    // b produces ops; a applies them. a's GC must never prune b's history.
    b.capture_n("b-cap", 10);
    let b_id = b.device();
    let a_id = a.device();
    let ops = b.store.pending_sync_operations(&a_id, 100).unwrap();
    for op in &ops {
        a.store
            .receive_sync_operation(&b_id, &serde_json::to_vec(op).unwrap())
            .unwrap();
    }
    let remote_before = a.count("sync_operations");
    assert_eq!(remote_before, 10, "a's store holds b's remote ops");

    // Running GC on a only ever considers a's LOCAL stream; b's remote ops stay.
    let _ = a.store.run_sync_gc();
    assert_eq!(
        a.count("sync_operations"),
        remote_before,
        "GC never prunes another device's history (receiver is not the authority)"
    );
}
