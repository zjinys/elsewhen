CREATE TABLE personal_memory_extraction_state (
    id INTEGER PRIMARY KEY CHECK(id=1),
    last_message_rowid INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
INSERT INTO personal_memory_extraction_state(id,last_message_rowid,updated_at)
VALUES(1,0,strftime('%Y-%m-%dT%H:%M:%fZ','now'));
