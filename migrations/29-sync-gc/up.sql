-- P4c: S03 lifecycle — per-member watermark, catch-up window, re-snapshot
-- request flags, and per-stream GC cursors.
--
-- Design invariants (see docs/roadmap/2026-10-07-multi-device-sync-handoff.md §17):
--  * Three distinct confirmations: relay accepted ≠ recipient applied ≠ GC
--    watermark. GC only ever uses the application-level watermark
--    (sync_member_state.confirmed_sequence), never the relay ack.
--  * All ACTIVE members participate in the watermark computation; a missing or
--    NULL watermark counts as 0 (not confirmed), so a lagging device always
--    holds back GC — NULLs are never filtered out.
--  * A member whose confirmed_sequence falls behind
--    (head_gc_floor - catch_up_window) flips needs_resnapshot=1. That is an
--    explicit protocol state; old offline ops are NEVER silently dropped while
--    any stream is in this state, and re-snapshot is arranged instead.
--  * Retention (SYNC_GC_MIN_RETAINED_OPERATIONS) is an additional holdback: GC
--    never trims the newest N applied local ops even when every member has
--    confirmed, so a very recent local capture always remains replayable.

-- One row per dataset member (local + remote), tracking how far each member
-- has durably applied this stream and whether it has fallen too far behind to
-- catch up incrementally.
CREATE TABLE sync_member_state (
    device_id TEXT PRIMARY KEY,
    confirmed_sequence INTEGER NOT NULL DEFAULT 0 CHECK(confirmed_sequence >= 0),
    needs_resnapshot INTEGER NOT NULL DEFAULT 0 CHECK(needs_resnapshot IN (0,1))
);

-- Monotonic per-origin-stream GC cursor. For the local stream this is the
-- highest applied sequence that has been confirmed by every active member AND
-- honoured the retention window; operations at or below this floor may be
-- pruned. A row is created lazily on first GC; the floor never decreases.
CREATE TABLE sync_gc_state (
    origin_stream_id TEXT PRIMARY KEY,
    gc_floor INTEGER NOT NULL DEFAULT 0 CHECK(gc_floor >= 0)
);

-- A snapshot recipient starts its application watermark at the local-stream
-- frontier captured when its snapshot was built (never at 0, which would hold
-- back GC, and never at the *current* frontier, which would over-claim).
ALTER TABLE sync_snapshot_exports ADD COLUMN local_watermark INTEGER NOT NULL DEFAULT 0;
