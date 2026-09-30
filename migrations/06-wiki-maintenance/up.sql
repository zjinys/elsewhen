-- Versioned semantic observations; never authoritative knowledge or rule changes.
CREATE TABLE knowledge_semantic_issues (
    fingerprint TEXT PRIMARY KEY,
    page_id TEXT NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
    page_hash TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('conflict','outdated','duplicate')),
    description TEXT NOT NULL,
    snapshot_ids TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX knowledge_semantic_issue_page ON knowledge_semantic_issues(page_id);
