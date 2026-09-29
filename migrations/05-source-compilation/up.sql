-- A worker must never auto-accept a proposal a user is currently reviewing.
-- Existing proposals remain manual decisions; only newly generated automatic
-- references can use the background application path.
ALTER TABLE knowledge_proposals ADD COLUMN origin TEXT NOT NULL DEFAULT 'manual'
    CHECK(origin IN ('manual','automatic'));
