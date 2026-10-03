-- Review versions invalidate approvals after any content edit, including edits
-- through older code paths. Existing pending records remain unapproved.
ALTER TABLE pending_actions ADD COLUMN review_version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE rules ADD COLUMN review_version INTEGER NOT NULL DEFAULT 1;

CREATE TRIGGER pending_action_review_version AFTER UPDATE OF args_json,action ON pending_actions
WHEN OLD.args_json IS NOT NEW.args_json OR OLD.action IS NOT NEW.action
BEGIN
    UPDATE pending_actions SET review_version=OLD.review_version+1 WHERE id=NEW.id;
END;

CREATE TRIGGER rule_review_version AFTER UPDATE OF content ON rules
WHEN OLD.content IS NOT NEW.content
BEGIN
    UPDATE rules SET review_version=OLD.review_version+1 WHERE id=NEW.id;
END;

-- A durable claim prevents a second execution even if the process exits
-- between a business commit and its receipt. Such claims require inspection,
-- not automatic replay. Snapshots retain rejected rule drafts after deletion.
CREATE TABLE chat_proposal_decisions (
    proposal_kind TEXT NOT NULL CHECK(proposal_kind IN ('action','rule')),
    proposal_id TEXT NOT NULL,
    conversation_id TEXT REFERENCES conversations(id) ON DELETE CASCADE,
    proposal_version INTEGER NOT NULL,
    approval_token TEXT NOT NULL,
    decision TEXT NOT NULL CHECK(decision IN ('accept','reject')),
    status TEXT NOT NULL CHECK(status IN ('executing','succeeded','declined','failed','uncertain')),
    executor_id TEXT NOT NULL,
    lease_until TEXT NOT NULL,
    action TEXT NOT NULL,
    args_json TEXT NOT NULL,
    result TEXT NOT NULL DEFAULT '',
    started_at TEXT NOT NULL,
    finished_at TEXT,
    PRIMARY KEY(proposal_kind,proposal_id)
);
CREATE INDEX chat_proposal_decision_conversation
ON chat_proposal_decisions(conversation_id,started_at DESC);
CREATE INDEX chat_proposal_execution_lease
ON chat_proposal_decisions(status,lease_until);
