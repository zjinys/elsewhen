//! S03 lifecycle — automatic re-snapshot scheduling for members flagged
//! `needs_resnapshot` (see `gc.rs` for the flagging/watermark semantics).
//!
//! A re-snapshot is a *durable request*, not an in-memory decision:
//!
//! 1. `run_sync_gc` flags a lagging member `needs_resnapshot = 1` and freezes
//!    GC. Nothing here un-freezes it by dropping history.
//! 2. [`Store::claim_sync_resnapshot`] turns the flag into exactly one
//!    in-flight export per member. The request row survives restarts; the
//!    `request_nonce` rotates whenever a new export must be built, so the
//!    derived snapshot id is never reused across requests and an old transfer
//!    can never satisfy a request issued after it.
//! 3. The runtime stages the snapshot (with a fresh authority roster frame)
//!    under that id; the ordinary durable wire outbox owns upload retries.
//!    Relay TTL only drops transport copies — the export row and the request
//!    are the durable state, and un-acked wires are re-sent.
//! 4. [`Store::finish_sync_resnapshot_requests`] completes a request only when
//!    the export *matching the current request id* is acknowledged: the flag
//!    clears and the member watermark re-seeds at the build-time frontier
//!    (via `mark_member_snapshotted`), which is what un-freezes GC.
//!
//! Fail-closed rules:
//! * A member that is not flagged, is revoked, or already has a building/ready
//!   export gets no new export (`None`), so ticks are idempotent.
//! * Completion never touches a request whose current id is not acknowledged —
//!   a member re-flagged after its last ack keeps its new request open.
//! * Receivers import snapshots only into a fresh staging database; a live
//!   non-empty device rejects the pieces (they expire at the relay as
//!   transport copies). Consuming a re-snapshot for repair is the restore
//!   flow's job, not this module's.

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use crate::storage::Store;
use crate::sync::protocol::validate_id;

/// A claimed re-snapshot: build exactly one export under this id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResnapshotClaim {
    pub member_device_id: String,
    pub snapshot_id: String,
}

/// Read-only per-member re-snapshot state for status reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResnapshotState {
    pub member_device_id: String,
    /// Current request's export id, if a request exists.
    pub snapshot_id: Option<String>,
    /// `building`/`ready` while the matching export is in flight; `None` when
    /// the flag is set but no export has been staged yet (or the last attempt
    /// aborted and the next tick will claim a fresh nonce).
    pub export_state: Option<String>,
}

impl Store {
    /// Claim the re-snapshot for one flagged member, or return `None` when no
    /// new export may be built now (not flagged, revoked, self, or an export
    /// is already building/ready). Idempotent across ticks and restarts.
    pub(crate) fn claim_sync_resnapshot(&self, member: &str) -> Result<Option<ResnapshotClaim>> {
        validate_id(member)?;
        let tx = self.begin_write_transaction()?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        if member == identity.device_id {
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(None);
        }
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
            [member],
            |r| r.get(0),
        )?;
        let flagged: bool = self
            .connection
            .query_row(
                "SELECT needs_resnapshot FROM sync_member_state WHERE device_id=?1",
                [member],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .map(|v| v != 0)
            .unwrap_or(false);
        if !active || !flagged {
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(None);
        }
        let in_flight: Option<String> = self
            .connection
            .query_row(
                "SELECT id FROM sync_snapshot_exports
                 WHERE recipient_device_id=?1 AND state IN ('building','ready')
                 ORDER BY rowid DESC LIMIT 1",
                [member],
                |r| r.get(0),
            )
            .optional()?;
        if in_flight.is_some() {
            // One in-flight delivery per member; the wire outbox owns retries.
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(None);
        }
        // Idempotent replay: an OPEN request with NO export row at all
        // (claimed but never built) returns the same claim, not a new nonce.
        // A CLOSED row only carries the nonce forward so the next generation
        // gets a fresh id and can never be confused with the completed one.
        let current: Option<(String, i64, i64)> = self
            .connection
            .query_row(
                "SELECT snapshot_id,request_nonce,open FROM sync_resnapshot_requests WHERE device_id=?1",
                [member],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let nonce: i64 = match &current {
            Some((id, nonce, open)) => {
                if *open == 0 {
                    nonce + 1
                } else {
                    let state: Option<String> = self
                        .connection
                        .query_row(
                            "SELECT state FROM sync_snapshot_exports WHERE id=?1",
                            [id],
                            |r| r.get(0),
                        )
                        .optional()?;
                    match state.as_deref() {
                        // Claimed but never built, or the build aborted:
                        // resume the SAME request id. The rebuilt export is
                        // byte-identical under a reused id, so a partially
                        // delivered old transfer can never be confused with
                        // a new generation. begin_snapshot_export refuses an
                        // id under a different request, so this cannot mix
                        // generations.
                        None | Some("aborted") => *nonce,
                        // Any other state means the request moved on —
                        // rotate for a new generation.
                        _ => nonce + 1,
                    }
                }
            }
            None => 1,
        };
        let snapshot_id = format!("resnap-{member}-{nonce}");
        self.connection.execute(
            "INSERT INTO sync_resnapshot_requests(device_id,snapshot_id,request_nonce,open)
             VALUES(?1,?2,?3,1)
             ON CONFLICT(device_id) DO UPDATE SET
               snapshot_id=excluded.snapshot_id,
               request_nonce=excluded.request_nonce,
               open=1",
            params![member, snapshot_id, nonce],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(Some(ResnapshotClaim {
            member_device_id: member.to_string(),
            snapshot_id,
        }))
    }

    /// Complete every request whose *current* export id is acknowledged: clear
    /// the member's `needs_resnapshot` flag and re-seed its watermark at the
    /// build-time frontier pinned on that export. Returns how many requests
    /// were completed. A request whose id was rotated after re-flagging is
    /// never satisfied by an older acknowledged export.
    pub(crate) fn finish_sync_resnapshot_requests(&self) -> Result<usize> {
        let tx = self.begin_write_transaction()?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let mut stmt = self.connection.prepare(
            "SELECT r.device_id, e.local_watermark
             FROM sync_resnapshot_requests r
             LEFT JOIN sync_snapshot_exports e
               ON e.id = r.snapshot_id AND e.recipient_device_id = r.device_id
             WHERE r.open = 1 AND e.state = 'acknowledged'
             ORDER BY r.device_id",
        )?;
        let completed: Vec<(String, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (device, watermark) in &completed {
            let mut frontiers = std::collections::BTreeMap::new();
            frontiers.insert(identity.stream_id.clone(), *watermark);
            self.mark_member_snapshotted(device, &frontiers)?;
            // Close the request but KEEP the row: the nonce must survive so
            // the next generation gets a fresh id.
            self.connection.execute(
                "UPDATE sync_resnapshot_requests SET open=0 WHERE device_id=?1",
                [device],
            )?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(completed.len())
    }

    /// Read-only state of every flagged member for status surfaces.
    pub(crate) fn sync_resnapshot_states(&self) -> Result<Vec<ResnapshotState>> {
        let mut stmt = self.connection.prepare(
            "SELECT s.device_id, r.snapshot_id,
                    (SELECT e.state FROM sync_snapshot_exports e
                     WHERE e.recipient_device_id = s.device_id
                       AND e.state IN ('building','ready')
                     ORDER BY e.rowid DESC LIMIT 1)
             FROM sync_member_state s
             LEFT JOIN sync_resnapshot_requests r
               ON r.device_id = s.device_id AND r.open = 1
             WHERE s.needs_resnapshot = 1
             ORDER BY s.device_id",
        )?;
        Ok(stmt
            .query_map([], |r| {
                Ok(ResnapshotState {
                    member_device_id: r.get(0)?,
                    snapshot_id: r.get(1)?,
                    export_state: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
