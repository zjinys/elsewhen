ALTER TABLE knowledge_background_runs ADD COLUMN source_slug TEXT;
ALTER TABLE knowledge_background_runs ADD COLUMN source_title TEXT;
ALTER TABLE knowledge_background_runs ADD COLUMN source_version INTEGER;
ALTER TABLE knowledge_background_runs ADD COLUMN detail TEXT;
ALTER TABLE knowledge_background_runs ADD COLUMN retry_at TEXT;
ALTER TABLE knowledge_background_runs ADD COLUMN strategy_version TEXT NOT NULL DEFAULT 'legacy';

CREATE TABLE knowledge_source_readings (
    snapshot_id TEXT NOT NULL REFERENCES knowledge_snapshots(id),
    strategy_version TEXT NOT NULL,
    level INTEGER NOT NULL,
    part INTEGER NOT NULL,
    start_char INTEGER NOT NULL,
    end_char INTEGER NOT NULL,
    summary TEXT NOT NULL,
    quotes_json TEXT NOT NULL,
    is_root INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(snapshot_id,strategy_version,level,part)
);

ALTER TABLE knowledge_proposals ADD COLUMN base_state TEXT;
ALTER TABLE knowledge_maintenance_reviews ADD COLUMN revision_id TEXT REFERENCES wiki_revisions(id);
CREATE TABLE knowledge_review_decisions (
    proposal_id TEXT PRIMARY KEY REFERENCES knowledge_proposals(id),
    original_content TEXT NOT NULL,
    original_applicable TEXT NOT NULL,
    selected_parts TEXT NOT NULL,
    issue_ids TEXT NOT NULL,
    created_at TEXT NOT NULL,
    revision_id TEXT REFERENCES wiki_revisions(id)
);

CREATE TABLE knowledge_refresh_jobs (
    page_id TEXT PRIMARY KEY REFERENCES wiki_pages(id) ON DELETE CASCADE,
    requested_at TEXT NOT NULL,
    generation INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    available_at TEXT NOT NULL,
    detail TEXT
);

CREATE TABLE knowledge_topic_plans (
    id TEXT PRIMARY KEY,
    mode TEXT NOT NULL,
    plan TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    resolved_at TEXT
);
CREATE TABLE knowledge_topic_replacements (
    old_page_id TEXT NOT NULL REFERENCES wiki_pages(id),
    new_page_id TEXT NOT NULL REFERENCES wiki_pages(id),
    plan_id TEXT NOT NULL REFERENCES knowledge_topic_plans(id),
    PRIMARY KEY(old_page_id,new_page_id)
);

CREATE TRIGGER knowledge_source_refresh AFTER INSERT ON knowledge_snapshots
BEGIN
    INSERT INTO knowledge_refresh_jobs(page_id,requested_at,available_at)
    SELECT DISTINCT p.id,NEW.captured_at,NEW.captured_at FROM wiki_pages p
    JOIN knowledge_page_sources k ON k.page_id=p.id
    JOIN knowledge_snapshots old ON old.id=k.snapshot_id
    WHERE old.source_id=NEW.source_id AND old.id<>NEW.id AND p.kind NOT IN ('source','note')
    ON CONFLICT(page_id) DO UPDATE SET requested_at=excluded.requested_at,
        generation=knowledge_refresh_jobs.generation+1,status='pending',attempts=0,available_at=excluded.available_at,detail=NULL;
END;

-- Cover sources that changed before this worker existed.
INSERT OR IGNORE INTO knowledge_refresh_jobs(page_id,requested_at,available_at)
SELECT DISTINCT p.id,strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now')
FROM wiki_pages p JOIN knowledge_page_sources k ON k.page_id=p.id
JOIN knowledge_snapshots s ON s.id=k.snapshot_id
WHERE p.kind NOT IN ('source','note') AND s.version < (SELECT MAX(version) FROM knowledge_snapshots WHERE source_id=s.source_id);

ALTER TABLE ai_provider_configs ADD COLUMN context_window INTEGER;
