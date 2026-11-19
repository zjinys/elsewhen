-- Local incomplete edits never enter synchronized knowledge or background analysis.
CREATE TABLE scout_research_drafts (
 study_key TEXT PRIMARY KEY, document_json TEXT NOT NULL,
 updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
