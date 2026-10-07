CREATE TABLE message_response_links (
    message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    input_message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE
);
CREATE INDEX message_response_links_input ON message_response_links(input_message_id);
CREATE TRIGGER validate_message_response_link BEFORE INSERT ON message_response_links
WHEN NOT EXISTS(
    SELECT 1 FROM messages reply JOIN messages input ON input.id=NEW.input_message_id
    WHERE reply.id=NEW.message_id AND reply.role='assistant' AND input.role='user'
      AND reply.conversation_id=input.conversation_id
)
BEGIN SELECT RAISE(ABORT,'message_response_input_invalid'); END;
CREATE TRIGGER immutable_message_response_link BEFORE UPDATE ON message_response_links
BEGIN SELECT RAISE(ABORT,'message_response_input_immutable'); END;
-- Backfill only an explicit recorded run basis, never infer ownership by time.
INSERT INTO message_response_links(message_id,input_message_id)
SELECT r.final_message_id,r.input_message_id FROM chat_runs r
JOIN messages reply ON reply.id=r.final_message_id AND reply.role='assistant'
JOIN messages input ON input.id=r.input_message_id AND input.role='user'
WHERE r.status='completed' AND reply.conversation_id=input.conversation_id;
