-- P4c follow-up: S03 lifecycle — automatic re-snapshot scheduling.
--
-- A member flagged needs_resnapshot (v29) asks for one re-snapshot delivery at
-- a time. The flag, the export row, and this request are the same durable
-- protocol state; a new snapshot for the same member is only staged after the
-- previous request completed (receipt applied) or was explicitly restarted
-- with a fresh nonce.
--
-- Invariants (see docs/roadmap/2026-10-07-multi-device-sync-handoff.md §17/§18):
--  * One in-flight re-snapshot per member: derived from the
--    building/ready snapshot_exports rows, so a crash mid-build can never
--    produce two concurrent deliveries for one recipient.
--  * request_nonce binds a request to the export id; a member that was
--    re-flagged while an old snapshot was still in flight gets a NEW nonce
--    and therefore a NEW export id — the old transfer cannot satisfy a
--    request issued after it (no cross-generation replay).
--  * roster_revision records which signed roster is embedded in an
--    authority-carrying re-snapshot. 0 means the snapshot carries no
--    authority frame (initial enrollment snapshots record 1+).

ALTER TABLE sync_snapshot_exports ADD COLUMN roster_revision INTEGER NOT NULL DEFAULT 0;

-- One durable re-snapshot request per member device. The nonce never
-- decreases for a device, so every new generation gets a fresh snapshot id
-- even after the previous request completed (completion only clears `open`,
-- keeping the nonce so an old export can never be confused with a new one).
-- The snapshot id is bound at begin_snapshot_export time (requests are
-- claimed before the export row exists, so no foreign key).
CREATE TABLE sync_resnapshot_requests (
    device_id TEXT PRIMARY KEY,
    snapshot_id TEXT NOT NULL,
    request_nonce INTEGER NOT NULL CHECK(request_nonce > 0),
    open INTEGER NOT NULL DEFAULT 1 CHECK(open IN (0,1))
);
