CREATE TABLE IF NOT EXISTS sync_processor_handoffs (
    roster_revision INTEGER PRIMARY KEY,
    dataset_id TEXT NOT NULL,
    from_device_id TEXT NOT NULL,
    to_device_id TEXT NOT NULL,
    from_sequence INTEGER NOT NULL CHECK (from_sequence >= 0),
    proof_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS sync_processor_state (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    state TEXT NOT NULL CHECK (state IN ('idle', 'relinquished', 'pending_takeover', 'active')),
    from_device_id TEXT,
    to_device_id TEXT,
    final_sequence INTEGER NOT NULL DEFAULT 0 CHECK (final_sequence >= 0),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

DROP VIEW sync_background_permission;
CREATE VIEW sync_background_permission AS
SELECT NOT EXISTS(SELECT 1 FROM sync_replica)
    OR EXISTS(
        SELECT 1 FROM sync_replica r
        JOIN sync_members m ON m.device_id=r.device_id AND m.stream_id=r.stream_id
        WHERE r.processor_device_id=r.device_id AND m.active=1
          AND NOT EXISTS(
              SELECT 1 FROM sync_snapshot_target t JOIN sync_snapshots s ON s.id=t.snapshot_id
              WHERE s.state='staging'
          )
          AND NOT EXISTS(
              SELECT 1 FROM sync_processor_state ps
              WHERE ps.singleton=1 AND ps.state IN ('relinquished', 'pending_takeover')
          )
    ) AS allowed;
