CREATE TABLE sync_local_vault (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    installation_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0),
    encrypted_secrets BLOB NOT NULL
);
