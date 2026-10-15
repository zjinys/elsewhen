-- Task metadata is tied to the saved message, never a second copy of its text.
CREATE TABLE chat_page_tasks (
    input_message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL,
    task_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(conversation_id, idempotency_key)
);
CREATE INDEX chat_page_tasks_conversation ON chat_page_tasks(conversation_id);
