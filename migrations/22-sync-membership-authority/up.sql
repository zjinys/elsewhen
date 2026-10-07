CREATE TABLE sync_authority (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    anchor TEXT NOT NULL,
    local_device TEXT NOT NULL,
    signed_roster BLOB
);
CREATE TABLE sync_authority_versions (
    revision INTEGER PRIMARY KEY CHECK(revision>0),
    digest TEXT NOT NULL
);
CREATE TABLE sync_member_keys (
    device_id TEXT PRIMARY KEY,
    stream_id TEXT NOT NULL UNIQUE,
    signing_key BLOB NOT NULL CHECK(length(signing_key)=32),
    active INTEGER NOT NULL CHECK(active IN (0,1))
);
