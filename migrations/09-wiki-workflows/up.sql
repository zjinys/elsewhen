-- Cheap change counters supplement (never replace) confirmation fingerprints.
ALTER TABLE wiki_pages ADD COLUMN knowledge_epoch INTEGER NOT NULL DEFAULT 0;
ALTER TABLE knowledge_dependencies ADD COLUMN basis_epoch INTEGER;
CREATE INDEX knowledge_dependency_upstream ON knowledge_dependencies(upstream_id,page_id);
CREATE INDEX knowledge_source_page_source ON knowledge_source_pages(source_id,page_id);
CREATE INDEX knowledge_page_source_snapshot ON knowledge_page_sources(snapshot_id,page_id);
CREATE INDEX knowledge_background_lookup ON knowledge_background_runs(task,strategy_version,input_key,started_at DESC);
CREATE INDEX knowledge_background_active ON knowledge_background_runs(task,status,started_at);
CREATE INDEX knowledge_refresh_due ON knowledge_refresh_jobs(status,available_at,requested_at);
CREATE INDEX wiki_derivative_parent ON wiki_pages(based_on,area,created_at DESC,id);
CREATE INDEX wiki_page_browse ON wiki_pages(area,last_seen_at DESC,id);
CREATE INDEX wiki_revision_page_time ON wiki_revisions(page_id,created_at DESC);
CREATE INDEX wiki_title_normalized ON wiki_pages(lower(trim(title)),updated_at DESC);

CREATE TRIGGER knowledge_epoch_page AFTER UPDATE OF content_md,status,opinion,source_event_ids ON wiki_pages
WHEN OLD.content_md IS NOT NEW.content_md OR OLD.status IS NOT NEW.status OR OLD.opinion IS NOT NEW.opinion OR OLD.source_event_ids IS NOT NEW.source_event_ids
BEGIN UPDATE wiki_pages SET knowledge_epoch=knowledge_epoch+1 WHERE id=NEW.id; END;
CREATE TRIGGER knowledge_epoch_metadata_insert AFTER INSERT ON knowledge_metadata
BEGIN UPDATE wiki_pages SET knowledge_epoch=knowledge_epoch+1 WHERE id=NEW.page_id; END;
CREATE TRIGGER knowledge_epoch_metadata_update AFTER UPDATE ON knowledge_metadata
WHEN OLD.applicable_when IS NOT NEW.applicable_when OR OLD.strength IS NOT NEW.strength
BEGIN UPDATE wiki_pages SET knowledge_epoch=knowledge_epoch+1 WHERE id=NEW.page_id; END;
CREATE TRIGGER knowledge_epoch_metadata_delete AFTER DELETE ON knowledge_metadata
BEGIN UPDATE wiki_pages SET knowledge_epoch=knowledge_epoch+1 WHERE id=OLD.page_id; END;
CREATE TRIGGER knowledge_epoch_source_insert AFTER INSERT ON knowledge_page_sources
BEGIN UPDATE wiki_pages SET knowledge_epoch=knowledge_epoch+1 WHERE id=NEW.page_id; END;
CREATE TRIGGER knowledge_epoch_source_delete AFTER DELETE ON knowledge_page_sources
BEGIN UPDATE wiki_pages SET knowledge_epoch=knowledge_epoch+1 WHERE id=OLD.page_id; END;
CREATE TRIGGER knowledge_epoch_dependency_insert AFTER INSERT ON knowledge_dependencies
BEGIN UPDATE wiki_pages SET knowledge_epoch=knowledge_epoch+1 WHERE id=NEW.page_id; END;
CREATE TRIGGER knowledge_epoch_dependency_update AFTER UPDATE OF basis ON knowledge_dependencies
WHEN OLD.basis IS NOT NEW.basis
BEGIN UPDATE wiki_pages SET knowledge_epoch=knowledge_epoch+1 WHERE id=NEW.page_id; END;
CREATE TRIGGER knowledge_epoch_dependency_delete AFTER DELETE ON knowledge_dependencies
BEGIN UPDATE wiki_pages SET knowledge_epoch=knowledge_epoch+1 WHERE id=OLD.page_id; END;
CREATE VIEW knowledge_stale_dependencies AS
WITH RECURSIVE stale(page_id) AS (
 SELECT d.page_id FROM knowledge_dependencies d JOIN wiki_pages p ON p.id=d.upstream_id
 WHERE d.basis_epoch IS NULL OR d.basis_epoch<>p.knowledge_epoch OR p.status='archived' OR p.opinion='reject'
 UNION SELECT d.page_id FROM knowledge_dependencies d JOIN stale s ON d.upstream_id=s.page_id
) SELECT page_id FROM stale;
ALTER TABLE knowledge_refresh_jobs ADD COLUMN review_only INTEGER NOT NULL DEFAULT 0;
ALTER TABLE knowledge_refresh_jobs ADD COLUMN dependency_epoch INTEGER;

CREATE TABLE knowledge_topic_plan_pages (
 plan_id TEXT NOT NULL REFERENCES knowledge_topic_plans(id),
 page_id TEXT NOT NULL REFERENCES wiki_pages(id),
 role TEXT NOT NULL CHECK(role IN ('input','output')),
 before_status TEXT,
 after_state TEXT,
 PRIMARY KEY(plan_id,page_id)
);
CREATE INDEX knowledge_topic_plan_page ON knowledge_topic_plan_pages(page_id,plan_id);
ALTER TABLE knowledge_topic_plans ADD COLUMN undone_at TEXT;
INSERT OR IGNORE INTO knowledge_topic_plan_pages(plan_id,page_id,role)
SELECT t.id,p.id,'input' FROM knowledge_topic_plans t,json_each(t.plan,'$.bases') b
JOIN wiki_pages p ON p.slug=json_extract(b.value,'$[0]');
INSERT OR IGNORE INTO knowledge_topic_plan_pages(plan_id,page_id,role)
SELECT plan_id,new_page_id,'output' FROM knowledge_topic_replacements;

CREATE TABLE knowledge_reading_states (
 page_id TEXT PRIMARY KEY REFERENCES wiki_pages(id) ON DELETE CASCADE,
 state TEXT NOT NULL CHECK(state IN ('unread','read','valuable','adopted','archived')),
 updated_at TEXT NOT NULL
);
CREATE INDEX knowledge_reading_state_filter ON knowledge_reading_states(state,page_id);
CREATE TABLE knowledge_artifact_versions (
 page_id TEXT PRIMARY KEY REFERENCES wiki_pages(id),
 parent_id TEXT NOT NULL REFERENCES wiki_pages(id),
 content_type TEXT NOT NULL,
 version INTEGER NOT NULL,
 instruction TEXT,
 model TEXT,
 strategy TEXT,
 created_at TEXT NOT NULL,
 UNIQUE(parent_id,content_type,version)
);
INSERT INTO knowledge_artifact_versions(page_id,parent_id,content_type,version,created_at)
SELECT d.id,p.id,COALESCE(d.content_type,'AI 加工'),
 row_number() OVER(PARTITION BY p.id,COALESCE(d.content_type,'AI 加工') ORDER BY d.created_at,d.id),d.created_at
FROM wiki_pages d JOIN wiki_pages p ON p.slug=d.based_on WHERE d.area='derivative';
CREATE TABLE knowledge_artifact_adoptions (
 parent_id TEXT NOT NULL REFERENCES wiki_pages(id),
 content_type TEXT NOT NULL,
 page_id TEXT NOT NULL REFERENCES wiki_pages(id),
 revision_id TEXT NOT NULL REFERENCES wiki_revisions(id),
 adopted_at TEXT NOT NULL,
 PRIMARY KEY(parent_id,content_type)
);
CREATE TABLE suggestion_feedback (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 message_id TEXT NOT NULL REFERENCES messages(id),
 conversation_id TEXT NOT NULL REFERENCES conversations(id),
 decision TEXT NOT NULL CHECK(decision IN ('accepted','ignored','rewritten','cleared')),
 suggestion TEXT NOT NULL,
 rewrite TEXT,
 created_at TEXT NOT NULL
);
CREATE INDEX suggestion_feedback_message ON suggestion_feedback(message_id,id DESC);
CREATE INDEX suggestion_feedback_conversation ON suggestion_feedback(conversation_id,id DESC);
CREATE TABLE knowledge_snapshot_metrics (
 snapshot_id TEXT PRIMARY KEY REFERENCES knowledge_snapshots(id),
 chars INTEGER NOT NULL
);
INSERT INTO knowledge_snapshot_metrics SELECT id,length(content_md) FROM knowledge_snapshots;
CREATE TRIGGER knowledge_snapshot_metrics_insert AFTER INSERT ON knowledge_snapshots
BEGIN INSERT INTO knowledge_snapshot_metrics VALUES(NEW.id,length(NEW.content_md)); END;
ALTER TABLE knowledge_background_runs ADD COLUMN input_snapshot TEXT GENERATED ALWAYS AS
 (CASE WHEN instr(input_key,':')>0 THEN substr(input_key,1,instr(input_key,':')-1) ELSE input_key END) VIRTUAL;
CREATE INDEX knowledge_background_snapshot ON knowledge_background_runs(task,strategy_version,input_snapshot,started_at DESC);
CREATE VIEW knowledge_current_sources AS
SELECT s.id,s.source_id,s.title,s.version,s.captured_at,m.chars,
 COALESCE((SELECT p.slug FROM knowledge_source_pages k JOIN wiki_pages p ON p.id=k.page_id WHERE k.source_id=s.source_id ORDER BY p.id LIMIT 1),'') AS slug,
 (COALESCE(o.opinion,'')<>'reject' AND EXISTS(SELECT 1 FROM knowledge_source_pages k JOIN wiki_pages p ON p.id=k.page_id WHERE k.source_id=s.source_id AND p.status<>'archived')) AS usable
FROM knowledge_sources o JOIN knowledge_snapshots s ON s.source_id=o.id
JOIN knowledge_snapshot_metrics m ON m.snapshot_id=s.id
WHERE s.version=(SELECT MAX(version) FROM knowledge_snapshots WHERE source_id=o.id);
CREATE TABLE knowledge_generations (
 conversation_id TEXT NOT NULL REFERENCES conversations(id),
 content_hash TEXT NOT NULL,
 instruction TEXT NOT NULL,
 model TEXT,
 strategy TEXT NOT NULL,
 PRIMARY KEY(conversation_id,content_hash)
);

CREATE INDEX wiki_page_recent ON wiki_pages(last_seen_at DESC,id);
