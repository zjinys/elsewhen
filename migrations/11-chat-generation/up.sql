CREATE TABLE chat_runs (
    id TEXT PRIMARY KEY,
    request_key TEXT NOT NULL UNIQUE,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    input_message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    input_hash TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('preparing','running','cancelling','completed','failed','cancelled','interrupted')),
    phase TEXT NOT NULL DEFAULT 'preparing',
    executor_id TEXT NOT NULL,
    lease_until TEXT NOT NULL,
    last_sequence INTEGER NOT NULL DEFAULT 0,
    request_count INTEGER NOT NULL DEFAULT 0,
    provider_basis_json TEXT NOT NULL DEFAULT '[]',
    response_text TEXT NOT NULL DEFAULT '',
    error_kind TEXT,
    error_text TEXT,
    final_message_id TEXT REFERENCES messages(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX chat_one_active_run ON chat_runs(conversation_id)
WHERE status IN ('preparing','running','cancelling');
CREATE INDEX chat_run_history ON chat_runs(conversation_id,created_at DESC,id);
CREATE INDEX chat_run_input ON chat_runs(input_message_id,created_at DESC);
CREATE INDEX chat_run_expiry ON chat_runs(status,lease_until);

CREATE TABLE chat_run_events (
    run_id TEXT NOT NULL REFERENCES chat_runs(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY(run_id,sequence)
);

CREATE TABLE chat_run_requests (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES chat_runs(id) ON DELETE CASCADE,
    provider_name TEXT NOT NULL,
    model TEXT,
    status TEXT NOT NULL CHECK(status IN ('running','completed','failed','cancelled','interrupted')),
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    usage_source TEXT NOT NULL DEFAULT 'unknown',
    started_at TEXT NOT NULL,
    finished_at TEXT
);
CREATE INDEX chat_run_request_run ON chat_run_requests(run_id,started_at,id);

-- One authoritative capture per legacy chat input. New main-stream inputs
-- already have input_records/event links; this covers model-tool captures.
-- A deleted event leaves a tombstone rather than permitting silent recreation.
CREATE TABLE chat_input_effects (
    input_message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    event_id TEXT REFERENCES events(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL
);
