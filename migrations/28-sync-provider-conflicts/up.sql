-- P4b Provider sync conflict review: a conflicted remote revision must be
-- dismissible when the user explicitly keeps the local configuration.
--
-- The v16 CHECK constraint on sync_provider_versions.state only allows
-- ('applied','conflict'); SQLite cannot ALTER a CHECK constraint, so the table
-- is rebuilt with a 'dismissed' state. Rows keep their encrypted envelope:
-- a dismissed revision stays recognized (re-delivery is a duplicate, not a
-- fresh conflict), and its ciphertext is never reinterpreted as applied.
PRAGMA legacy_alter_table=ON;
CREATE TABLE sync_provider_versions_new (
    revision_id TEXT PRIMARY KEY,
    base_revision TEXT,
    sender_device_id TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    encrypted_envelope BLOB NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('applied','conflict','dismissed'))
);
INSERT INTO sync_provider_versions_new(revision_id,base_revision,sender_device_id,content_hash,encrypted_envelope,state)
SELECT revision_id,base_revision,sender_device_id,content_hash,encrypted_envelope,state FROM sync_provider_versions;
DROP TABLE sync_provider_versions;
ALTER TABLE sync_provider_versions_new RENAME TO sync_provider_versions;
PRAGMA legacy_alter_table=OFF;
