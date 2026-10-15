CREATE TABLE sync_snapshot_exports (
    id TEXT PRIMARY KEY,
    recipient_device_id TEXT NOT NULL,
    key_version INTEGER NOT NULL CHECK(key_version > 0),
    state TEXT NOT NULL CHECK(state IN ('building','ready','acknowledged','aborted'))
);
CREATE TABLE sync_snapshot_export_wires (
    snapshot_id TEXT NOT NULL REFERENCES sync_snapshot_exports(id),
    recipient_device_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    PRIMARY KEY(recipient_device_id,message_id),
    FOREIGN KEY(recipient_device_id,message_id)
        REFERENCES sync_wire_outbox(recipient_device_id,message_id)
);
CREATE INDEX sync_snapshot_export_wires_snapshot ON sync_snapshot_export_wires(snapshot_id);
CREATE TABLE sync_relay_cursors (
    mailbox_id TEXT PRIMARY KEY,
    after_id TEXT NOT NULL
);
