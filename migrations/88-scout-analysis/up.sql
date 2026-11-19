CREATE TABLE scout_analysis_jobs (
 id TEXT PRIMARY KEY, study_slug TEXT NOT NULL, basis_token TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed','cancelled')),
 owner TEXT NOT NULL, lease_until INTEGER NOT NULL, attempts INTEGER NOT NULL DEFAULT 1,
 result_json TEXT, error TEXT, model TEXT, prompt_tokens INTEGER, completion_tokens INTEGER,
 created_at TEXT NOT NULL DEFAULT (datetime('now')), updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_scout_analysis_study ON scout_analysis_jobs(study_slug,updated_at DESC);
