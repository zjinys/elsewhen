CREATE TABLE sync_operation_fragments (
    sender_device_id TEXT NOT NULL,
    op_id TEXT NOT NULL,
    key_version INTEGER NOT NULL CHECK(key_version > 0),
    payload_hash TEXT NOT NULL,
    total_bytes INTEGER NOT NULL CHECK(total_bytes > 0 AND total_bytes <= 16777216),
    chunk_count INTEGER NOT NULL CHECK(chunk_count > 0 AND chunk_count <= 64),
    assembled INTEGER NOT NULL DEFAULT 0 CHECK(assembled IN (0,1)),
    PRIMARY KEY(sender_device_id,op_id,key_version)
);
CREATE TABLE sync_operation_fragment_parts (
    sender_device_id TEXT NOT NULL,
    op_id TEXT NOT NULL,
    key_version INTEGER NOT NULL,
    chunk_index INTEGER NOT NULL CHECK(chunk_index >= 0 AND chunk_index < 64),
    content_hash TEXT NOT NULL,
    encrypted_envelope BLOB,
    PRIMARY KEY(sender_device_id,op_id,key_version,chunk_index),
    FOREIGN KEY(sender_device_id,op_id,key_version)
        REFERENCES sync_operation_fragments(sender_device_id,op_id,key_version)
);
CREATE TABLE sync_operation_fragment_outbox (
    recipient_device_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    op_id TEXT NOT NULL REFERENCES sync_operations(op_id),
    PRIMARY KEY(recipient_device_id,message_id),
    FOREIGN KEY(recipient_device_id,message_id)
        REFERENCES sync_wire_outbox(recipient_device_id,message_id)
);
CREATE INDEX sync_operation_fragment_outbox_operation ON sync_operation_fragment_outbox(op_id,recipient_device_id);
