-- Generated context is independent of original messages and never authorizes a
-- business action. Leaves and merge nodes share exact source-span identities.
CREATE TABLE chat_summary_nodes (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    basis_hash TEXT NOT NULL,
    spans_json TEXT NOT NULL,
    children_json TEXT NOT NULL,
    strategy TEXT NOT NULL,
    provider_basis TEXT NOT NULL,
    model TEXT,
    body_json TEXT NOT NULL,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    elapsed_ms INTEGER NOT NULL CHECK(elapsed_ms >= 0),
    created_at TEXT NOT NULL,
    UNIQUE(conversation_id,basis_hash,strategy,provider_basis)
);
CREATE INDEX chat_summary_nodes_conversation ON chat_summary_nodes(conversation_id,created_at);
