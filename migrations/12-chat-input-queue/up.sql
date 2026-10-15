-- Queue entries reference saved inputs; no second copy of message text.
CREATE TABLE chat_input_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    input_message_id TEXT NOT NULL UNIQUE REFERENCES messages(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','withdrawn','started','injected')),
    mode TEXT NOT NULL DEFAULT 'next' CHECK(mode IN ('next','steer')),
    target_run_id TEXT REFERENCES chat_runs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES chat_runs(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX chat_input_queue_fifo ON chat_input_queue(conversation_id,status,id);

-- Explicit continuation grants a short dispatch lease. A process restart cannot
-- turn an old persisted boolean into permission to replay work automatically.
CREATE TABLE chat_queue_dispatch (
    conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
    paused INTEGER NOT NULL DEFAULT 1 CHECK(paused IN (0,1)),
    owner TEXT,
    lease_until TEXT,
    updated_at TEXT NOT NULL
);
