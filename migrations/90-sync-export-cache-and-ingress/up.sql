-- A snapshot's encrypted bytes must outlive the read transaction that built
-- them. Upload retries never reconstruct a prefix from a newer database view.
ALTER TABLE sync_snapshot_exports ADD COLUMN cached_manifest BLOB;
CREATE TABLE sync_snapshot_export_chunks (
    snapshot_id TEXT NOT NULL REFERENCES sync_snapshot_exports(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL CHECK(chunk_index >= 0),
    encrypted_envelope BLOB NOT NULL CHECK(length(encrypted_envelope) > 0),
    PRIMARY KEY(snapshot_id, chunk_index)
);

-- Existing partial uploads have no immutable export cache. Enrollment rotates
-- aborted requests; the re-snapshot claimant also rotates uncached old uploads.
UPDATE sync_snapshot_exports SET state='aborted'
WHERE state='building' AND (blob_index>0 OR blob_id IS NOT NULL);

-- A trusted dataset owner can forward another member's operation. Its
-- application receipt returns to the forwarding hop that owns that delivery.
ALTER TABLE sync_operations ADD COLUMN received_from_device_id TEXT;
UPDATE sync_operations SET received_from_device_id=origin_device_id WHERE local=0;

-- Recover operations already inventoried by the owner before forwarding was
-- implemented, even if its signed roster does not change after this upgrade.
INSERT OR IGNORE INTO sync_outbox(op_id,recipient_device_id)
SELECT o.op_id,m.device_id FROM sync_operations o CROSS JOIN sync_members m
JOIN sync_replica r ON r.singleton=1 JOIN sync_authority a ON a.singleton=1
WHERE r.device_id=json_extract(CAST(a.signed_roster AS TEXT),'$.roster.owner.device_id')
  AND o.local=0 AND o.state IN ('applied','conflict') AND m.active=1
  AND m.device_id!=r.device_id AND m.device_id!=o.origin_device_id;

DROP TRIGGER validate_sync_receipt;
CREATE TRIGGER validate_sync_receipt BEFORE INSERT ON sync_receipts
WHEN NOT EXISTS(
    SELECT 1 FROM sync_operations WHERE op_id=NEW.op_id
      AND COALESCE(received_from_device_id,origin_device_id)=NEW.target_device_id
      AND payload_hash=NEW.payload_hash AND state IN ('applied','conflict')
)
 AND NOT EXISTS(
    SELECT 1 FROM sync_checkpoint_operations WHERE op_id=NEW.op_id AND payload_hash=NEW.payload_hash
      AND (origin_device_id=NEW.target_device_id OR NEW.target_device_id=(
          SELECT json_extract(anchor,'$.owner.device_id') FROM sync_authority WHERE singleton=1
      ))
)
BEGIN SELECT RAISE(ABORT,'sync_receipt_has_no_durable_operation'); END;
