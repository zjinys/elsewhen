-- Existing pending inputs remain ordinary queued questions. A supplement is an
-- explicit, version-bound request and cannot be inferred from old messages.
ALTER TABLE chat_input_queue ADD COLUMN input_basis TEXT;
ALTER TABLE chat_input_queue ADD COLUMN context_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE chat_input_queue ADD COLUMN defer_reason TEXT;
CREATE INDEX chat_input_queue_supplements ON chat_input_queue(target_run_id,status,id);
