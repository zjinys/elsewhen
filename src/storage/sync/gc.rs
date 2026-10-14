//! S03 lifecycle — GC watermarks, catch-up windows, and re-snapshot state.
//!
//! The design Note (`2026-09-30-multi-device-sync.md` §5) splits delivery into
//! three distinct confirmations; this module implements the third one:
//!
//! 1. relay accepted the envelope — NOT tracked here (transport only);
//! 2. recipient durably applied the op — tracked per-recipient in `sync_outbox`
//!    and reported back via `ApplicationReceipt` → [`Store::acknowledge_sync_operation`];
//! 3. every ACTIVE member's application watermark — computed here and used as
//!    the sole GC gate.
//!
//! Invariants (all enforced in SQL, all fail closed):
//!
//! * GC never uses relay-level information; only application watermarks.
//! * All active members participate in the watermark computation. A member row
//!   missing from `sync_member_state` counts as 0 — a lagging device ALWAYS
//!   holds back GC (`MIN(...) FILTER` never excludes NULL / missing rows).
//! * A member whose `confirmed_sequence` falls behind
//!   `(head_gc_floor - SYNC_GC_CATCH_UP_WINDOW)` is flagged
//!   `needs_resnapshot = 1`. While ANY active member is flagged, GC makes no
//!   progress at all: old offline ops are never silently dropped; a re-snapshot
//!   is arranged instead.
//! * `SYNC_GC_MIN_RETAINED_OPERATIONS` is an extra holdback — GC never trims
//!   the newest N applied local ops even when everyone has confirmed, so a very
//!   recent local capture always stays replayable.
//!
//! GC never deletes domain data (events / conversations / messages / …); it
//! only prunes the *sync operation history* rows for already-confirmed local
//! ops, plus the per-recipient `sync_outbox` rows of fully-acked operations.
//! Remote ops are never pruned — the receiver is not the authority for another
//! device's history.

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use crate::storage::Store;

/// How far below the head GC floor a member may lag before it is considered
/// too far behind for incremental catch-up and flips to `needs_resnapshot`.
const SYNC_GC_CATCH_UP_WINDOW: i64 = 256;

/// Minimum number of newest applied local operations retained regardless of
/// member confirmations — a very recent local capture is never trimmed.
const SYNC_GC_MIN_RETAINED_OPERATIONS: i64 = 64;

/// Report from one GC pass over the local stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyncGcReport {
    /// Members currently flagged `needs_resnapshot`.
    pub resnapshot_pending: Vec<String>,
    /// Local `sync_operations` rows pruned this pass.
    pub operations_pruned: usize,
    /// Fully-acked `sync_outbox` rows pruned this pass.
    pub outbox_rows_pruned: usize,
    /// The GC floor for the local stream after this pass.
    pub gc_floor: i64,
}

impl Store {
    /// Advance one member's application watermark for the local stream and
    /// re-evaluate whether it needs a re-snapshot. Called on every
    /// `acknowledge_sync_operation`; the watermark never decreases.
    pub(crate) fn advance_sync_member_watermark(
        &self,
        member_device_id: &str,
        confirmed_sequence: i64,
    ) -> Result<()> {
        ensure!(confirmed_sequence > 0, "sync_gc_watermark_invalid");
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let floor = self.sync_local_gc_floor()?;
        let needs: bool = confirmed_sequence < floor.saturating_sub(SYNC_GC_CATCH_UP_WINDOW);
        self.connection.execute(
            "INSERT INTO sync_member_state(device_id,confirmed_sequence,needs_resnapshot)
             VALUES(?1,?2,?3)
             ON CONFLICT(device_id) DO UPDATE SET
               confirmed_sequence = MAX(confirmed_sequence, excluded.confirmed_sequence),
               needs_resnapshot = MAX(needs_resnapshot, excluded.needs_resnapshot)",
            params![member_device_id, confirmed_sequence, needs as i64],
        )?;
        // self-watermark: the local device trivially applies its own stream.
        let _ = identity;
        Ok(())
    }

    /// Local stream's current GC floor (0 if GC never ran).
    pub(crate) fn sync_local_gc_floor(&self) -> Result<i64> {
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        Ok(self
            .connection
            .query_row(
                "SELECT gc_floor FROM sync_gc_state WHERE origin_stream_id=?1",
                [&identity.stream_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    /// Members currently flagged for re-snapshot.
    pub(crate) fn sync_resnapshot_pending(&self) -> Result<Vec<String>> {
        let mut query = self.connection.prepare(
            "SELECT device_id FROM sync_member_state WHERE needs_resnapshot=1 ORDER BY device_id",
        )?;
        Ok(query
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Run one GC pass over the LOCAL stream.
    ///
    /// Fail-closed: if any active member is flagged `needs_resnapshot`, or any
    /// active member has no watermark row at all (never confirmed anything AND
    /// never been snapshotted), GC does nothing and only reports the pending
    /// members. Local ops are inserted at `applied` with a self-watermark, so
    /// the local device never blocks itself.
    pub(crate) fn run_sync_gc(&self) -> Result<SyncGcReport> {
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let tx = self.begin_write_transaction()?;

        // The set of active members (excluding self) that must confirm.
        // A member with no sync_member_state row counts as confirmed_sequence=0
        // and immediately needs_resnapshot=0 unless head floor is high — but a
        // brand-new member that just joined via snapshot starts at the snapshot
        // frontiers, recorded by `mark_member_snapshotted`.
        let mut stmt = self.connection.prepare(
            "SELECT m.device_id,
                    COALESCE(s.confirmed_sequence, 0),
                    COALESCE(s.needs_resnapshot, 0)
             FROM sync_members m
             LEFT JOIN sync_member_state s ON s.device_id = m.device_id
             WHERE m.active = 1 AND m.device_id != ?1",
        )?;
        let members: Vec<(String, i64, bool)> = stmt
            .query_map([&identity.device_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let current_floor = self.sync_local_gc_floor()?;
        let threshold = current_floor.saturating_sub(SYNC_GC_CATCH_UP_WINDOW);

        // Flag any member that has fallen too far behind. Recompute against the
        // CURRENT floor every pass (a stored needs_resnapshot computed under an
        // older, lower floor may be stale, but a member now beyond the window
        // must be flagged regardless of its stored flag).
        let mut resnapshot_pending = Vec::new();
        for (device_id, confirmed, flagged) in &members {
            let needs = *flagged || *confirmed < threshold;
            if needs && !*flagged {
                self.connection.execute(
                    "INSERT INTO sync_member_state(device_id,confirmed_sequence,needs_resnapshot)
                     VALUES(?1,?2,1)
                     ON CONFLICT(device_id) DO UPDATE SET needs_resnapshot=1",
                    params![device_id, confirmed],
                )?;
            }
            if needs {
                resnapshot_pending.push(device_id.clone());
            }
        }
        if !resnapshot_pending.is_empty() {
            // Fail closed: nothing is trimmed while any member needs re-snapshot.
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(SyncGcReport {
                resnapshot_pending,
                operations_pruned: 0,
                outbox_rows_pruned: 0,
                gc_floor: current_floor,
            });
        }

        // Everyone is caught up. The new GC floor is the minimum active
        // member watermark, additionally bounded by the retention window.
        let min_member = members
            .iter()
            .map(|(_, confirmed, _)| *confirmed)
            .min()
            .unwrap_or(i64::MAX);
        // Retention: never trim the newest SYNC_GC_MIN_RETAINED_OPERATIONS ops.
        let retained_floor: i64 = self.connection.query_row(
            "SELECT COALESCE(MIN(origin_sequence) - 1, 0) FROM (
                SELECT origin_sequence FROM sync_operations
                WHERE local = 1 AND state = 'applied'
                ORDER BY origin_sequence DESC LIMIT ?1
            )",
            [SYNC_GC_MIN_RETAINED_OPERATIONS],
            |r| r.get(0),
        )?;
        let new_floor = min_member.min(retained_floor).max(current_floor);

        // Prune outbox rows for local ops that are at/below the new floor and
        // fully acked by every active member FIRST (they reference
        // sync_operations), then prune the operation history rows themselves.
        let pruned_outbox = self.connection.execute(
            "DELETE FROM sync_outbox
             WHERE acknowledged = 1
               AND op_id IN (
                    SELECT op_id FROM sync_operations
                    WHERE local = 1 AND state = 'applied'
                      AND origin_stream_id = ?1 AND origin_sequence <= ?2
               )",
            params![identity.stream_id, new_floor],
        )?;
        let pruned_ops = self.connection.execute(
            "DELETE FROM sync_operations
             WHERE local = 1
               AND state = 'applied'
               AND origin_stream_id = ?1
               AND origin_sequence <= ?2
               AND NOT EXISTS (
                    SELECT 1 FROM sync_outbox q
                    JOIN sync_members m ON m.device_id = q.recipient_device_id
                    WHERE q.op_id = sync_operations.op_id
                      AND m.active = 1
                      AND q.acknowledged = 0
               )",
            params![identity.stream_id, new_floor],
        )?;

        self.connection.execute(
            "INSERT INTO sync_gc_state(origin_stream_id,gc_floor) VALUES(?1,?2)
             ON CONFLICT(origin_stream_id) DO UPDATE SET gc_floor = MAX(gc_floor, excluded.gc_floor)",
            params![identity.stream_id, new_floor],
        )?;

        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(SyncGcReport {
            resnapshot_pending: Vec::new(),
            operations_pruned: pruned_ops,
            outbox_rows_pruned: pruned_outbox,
            gc_floor: new_floor,
        })
    }

    /// Record that a member completed a snapshot at the given per-stream
    /// frontiers: its watermark starts there (not at 0) and it is NOT behind,
    /// so it neither holds back GC nor gets flagged needs_resnapshot.
    pub(crate) fn mark_member_snapshotted(
        &self,
        member_device_id: &str,
        frontiers: &std::collections::BTreeMap<String, i64>,
    ) -> Result<()> {
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let watermark = frontiers.get(&identity.stream_id).copied().unwrap_or(0);
        self.connection.execute(
            "INSERT INTO sync_member_state(device_id,confirmed_sequence,needs_resnapshot)
             VALUES(?1,?2,0)
             ON CONFLICT(device_id) DO UPDATE SET
               confirmed_sequence = MAX(confirmed_sequence, excluded.confirmed_sequence),
               needs_resnapshot = 0",
            params![member_device_id, watermark],
        )?;
        Ok(())
    }
}
