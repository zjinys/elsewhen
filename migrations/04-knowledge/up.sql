-- Imported originals are immutable snapshots. Wiki pages remain their existing
-- presentation; compiled pages cite an exact snapshot, never a moving URL.
CREATE TABLE knowledge_sources (
    id TEXT PRIMARY KEY,
    identity TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    locator TEXT,
    opinion TEXT CHECK(opinion IN ('endorse', 'reject')),
    created_at TEXT NOT NULL
);
CREATE TABLE knowledge_snapshots (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES knowledge_sources(id),
    version INTEGER NOT NULL CHECK(version > 0),
    title TEXT NOT NULL,
    content_md TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    captured_at TEXT NOT NULL,
    UNIQUE(source_id, version)
);
CREATE TRIGGER knowledge_snapshot_no_update BEFORE UPDATE ON knowledge_snapshots
BEGIN SELECT RAISE(ABORT, 'source_snapshot_is_immutable'); END;
CREATE TRIGGER knowledge_snapshot_no_delete BEFORE DELETE ON knowledge_snapshots
BEGIN SELECT RAISE(ABORT, 'source_snapshot_is_immutable'); END;
CREATE TABLE knowledge_source_pages (
    page_id TEXT PRIMARY KEY REFERENCES wiki_pages(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL REFERENCES knowledge_sources(id)
);
CREATE TABLE knowledge_page_sources (
    page_id TEXT NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
    snapshot_id TEXT NOT NULL REFERENCES knowledge_snapshots(id),
    PRIMARY KEY(page_id, snapshot_id)
);
CREATE TABLE knowledge_metadata (
    page_id TEXT PRIMARY KEY REFERENCES wiki_pages(id) ON DELETE CASCADE,
    applicable_when TEXT NOT NULL DEFAULT '',
    strength TEXT NOT NULL DEFAULT 'reference' CHECK(strength IN ('reference','method','rule')),
    confirmed_at TEXT
);
CREATE TABLE knowledge_proposals (
    id TEXT PRIMARY KEY,
    dedupe_key TEXT NOT NULL UNIQUE,
    page_id TEXT REFERENCES wiki_pages(id) ON DELETE CASCADE,
    target_slug TEXT NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    content_md TEXT NOT NULL,
    applicable_when TEXT NOT NULL DEFAULT '',
    snapshot_ids TEXT NOT NULL DEFAULT '[]',
    event_ids TEXT NOT NULL DEFAULT '[]',
    base_hash TEXT,
    reason TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','accepted','rejected')),
    created_at TEXT NOT NULL,
    resolved_at TEXT
);
CREATE INDEX knowledge_proposals_page ON knowledge_proposals(page_id,status);
CREATE TABLE knowledge_maintenance_reviews (
    fingerprint TEXT PRIMARY KEY,
    page_id TEXT NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
    resolution TEXT NOT NULL,
    created_at TEXT NOT NULL
);
-- Audit records distinguish candidates supplied to a model from its validated
-- citations. Payloads preserve page excerpts and exact source versions.
CREATE TABLE knowledge_usage (
    id TEXT PRIMARY KEY,
    task TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    candidates_json TEXT NOT NULL,
    cited_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX knowledge_usage_owner ON knowledge_usage(task,owner_id,created_at);
CREATE TABLE knowledge_background_runs (
    id TEXT PRIMARY KEY,
    task TEXT NOT NULL,
    input_key TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('running','succeeded','failed')),
    started_at TEXT NOT NULL,
    finished_at TEXT,
    error TEXT,
    result_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX knowledge_background_task ON knowledge_background_runs(task,started_at);
