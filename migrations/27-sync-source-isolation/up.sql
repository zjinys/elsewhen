-- S01 source isolation: allow a single poisoned/forged operation to be
-- quarantined per origin stream without rolling back the whole receive or
-- blocking later in-order operations from the same device.
--
-- The v16 CHECK constraint on sync_operations.state does not include
-- 'isolated', so the table is rebuilt. SQLite cannot ALTER a CHECK constraint.
-- legacy_alter_table is required so the RENAME below does not rewrite the
-- validate_sync_receipt trigger's stored reference to sync_operations (which
-- would resolve to the mid-rebuild name and break receipt validation).
PRAGMA legacy_alter_table=ON;
CREATE TABLE sync_operations_new (
    op_id TEXT PRIMARY KEY,
    origin_device_id TEXT NOT NULL,
    origin_stream_id TEXT NOT NULL,
    origin_sequence INTEGER NOT NULL CHECK(origin_sequence > 0),
    payload TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    local INTEGER NOT NULL CHECK(local IN (0,1)),
    state TEXT NOT NULL CHECK(state IN ('applied','waiting','conflict','rejected','isolated')),
    error_code TEXT,
    UNIQUE(origin_stream_id, origin_sequence)
);
INSERT INTO sync_operations_new(op_id,origin_device_id,origin_stream_id,origin_sequence,payload,payload_hash,local,state,error_code)
SELECT op_id,origin_device_id,origin_stream_id,origin_sequence,payload,payload_hash,local,state,error_code FROM sync_operations;
DROP TABLE sync_operations;
ALTER TABLE sync_operations_new RENAME TO sync_operations;
PRAGMA legacy_alter_table=OFF;
CREATE INDEX sync_operations_pending ON sync_operations(state, origin_stream_id, origin_sequence);
