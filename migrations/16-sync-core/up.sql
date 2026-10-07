CREATE TABLE sync_replica (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    dataset_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    stream_id TEXT NOT NULL UNIQUE,
    next_sequence INTEGER NOT NULL DEFAULT 1 CHECK(next_sequence > 0),
    logical_clock INTEGER NOT NULL DEFAULT 0 CHECK(logical_clock >= 0),
    processor_device_id TEXT NOT NULL
);
CREATE TABLE sync_members (
    device_id TEXT NOT NULL,
    stream_id TEXT PRIMARY KEY,
    active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1))
);
CREATE TABLE sync_operations (
    op_id TEXT PRIMARY KEY,
    origin_device_id TEXT NOT NULL,
    origin_stream_id TEXT NOT NULL,
    origin_sequence INTEGER NOT NULL CHECK(origin_sequence > 0),
    payload TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    local INTEGER NOT NULL CHECK(local IN (0,1)),
    state TEXT NOT NULL CHECK(state IN ('applied','waiting','conflict','rejected')),
    error_code TEXT,
    UNIQUE(origin_stream_id, origin_sequence)
);
CREATE INDEX sync_operations_pending ON sync_operations(state, origin_stream_id, origin_sequence);
CREATE TABLE sync_frontiers (
    origin_stream_id TEXT PRIMARY KEY,
    applied_sequence INTEGER NOT NULL DEFAULT 0 CHECK(applied_sequence >= 0)
);
CREATE TABLE sync_outbox (
    op_id TEXT NOT NULL REFERENCES sync_operations(op_id),
    recipient_device_id TEXT NOT NULL,
    acknowledged INTEGER NOT NULL DEFAULT 0 CHECK(acknowledged IN (0,1)),
    PRIMARY KEY(op_id, recipient_device_id)
);
CREATE TABLE sync_receipts (
    op_id TEXT PRIMARY KEY,
    target_device_id TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    pending INTEGER NOT NULL DEFAULT 1 CHECK(pending IN (0,1))
);
CREATE TABLE sync_known_revisions (
    object_kind TEXT NOT NULL,
    object_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    PRIMARY KEY(object_kind,object_id,revision_id)
);
CREATE TABLE sync_checkpoint_operations (
    op_id TEXT PRIMARY KEY,
    origin_device_id TEXT NOT NULL,
    origin_stream_id TEXT NOT NULL,
    origin_sequence INTEGER NOT NULL CHECK(origin_sequence>0),
    payload_hash TEXT NOT NULL,
    UNIQUE(origin_stream_id,origin_sequence)
);
CREATE TRIGGER validate_sync_receipt BEFORE INSERT ON sync_receipts
WHEN NOT EXISTS(SELECT 1 FROM sync_operations WHERE op_id=NEW.op_id AND origin_device_id=NEW.target_device_id AND payload_hash=NEW.payload_hash AND state IN ('applied','conflict'))
 AND NOT EXISTS(SELECT 1 FROM sync_checkpoint_operations WHERE op_id=NEW.op_id AND origin_device_id=NEW.target_device_id AND payload_hash=NEW.payload_hash)
BEGIN SELECT RAISE(ABORT,'sync_receipt_has_no_durable_operation'); END;
CREATE TABLE sync_snapshot_target (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    snapshot_id TEXT NOT NULL
);
CREATE TABLE sync_snapshots (
    id TEXT PRIMARY KEY,
    source_device_id TEXT NOT NULL,
    key_version INTEGER NOT NULL,
    manifest BLOB,
    state TEXT NOT NULL DEFAULT 'staging' CHECK(state IN ('staging','applied','aborted'))
);
CREATE TABLE sync_snapshot_chunks (
    snapshot_id TEXT NOT NULL REFERENCES sync_snapshots(id),
    chunk_index INTEGER NOT NULL,
    encrypted_chunk BLOB NOT NULL,
    plaintext_hash TEXT NOT NULL,
    PRIMARY KEY(snapshot_id,chunk_index)
);
CREATE TABLE sync_object_heads (
    object_kind TEXT NOT NULL,
    object_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0,1)),
    PRIMARY KEY(object_kind, object_id)
);
CREATE TABLE sync_conflicts (
    op_id TEXT PRIMARY KEY REFERENCES sync_operations(op_id),
    object_kind TEXT NOT NULL,
    object_id TEXT NOT NULL,
    current_revision TEXT,
    reason TEXT NOT NULL
);
CREATE TABLE sync_wire_receipts (
    target_device_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    pending INTEGER NOT NULL DEFAULT 1 CHECK(pending IN (0,1)),
    PRIMARY KEY(target_device_id,message_id)
);
CREATE TABLE sync_wire_outbox (
    recipient_device_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    key_version INTEGER NOT NULL,
    encrypted_envelope BLOB NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    available_at_ms INTEGER NOT NULL DEFAULT 0,
    relay_id TEXT,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','acknowledged')),
    PRIMARY KEY(recipient_device_id, message_id)
);

CREATE TABLE sync_checkpoint_providers (
    revision_id TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    snapshot_id TEXT NOT NULL REFERENCES sync_snapshots(id)
);
CREATE TABLE sync_provider_versions (
    revision_id TEXT PRIMARY KEY,
    base_revision TEXT,
    sender_device_id TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    encrypted_envelope BLOB NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('applied','conflict'))
);
CREATE TABLE sync_provider_state (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    revision_id TEXT NOT NULL,
    base_revision TEXT
);
CREATE TABLE sync_provider_dirty (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    base_revision TEXT,
    revision_id TEXT NOT NULL
);
CREATE TABLE ai_provider_sync_order (
    config_id TEXT PRIMARY KEY REFERENCES ai_provider_configs(id) ON DELETE CASCADE,
    position INTEGER NOT NULL UNIQUE CHECK(position >= 0)
);

CREATE TABLE sync_message_order (
    message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    origin_stream_id TEXT NOT NULL,
    origin_sequence INTEGER NOT NULL,
    logical_clock INTEGER NOT NULL,
    input_message_id TEXT
);
