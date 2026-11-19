-- Scout on D1. Same shape as the Python service: append-only snapshots,
-- findings reference their snapshot, one run row per tick per source.
CREATE TABLE IF NOT EXISTS scout_sources (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    spec_hash TEXT NOT NULL,
    interval_minutes INTEGER NOT NULL,
    last_run_at TEXT
);

CREATE TABLE IF NOT EXISTS scout_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL REFERENCES scout_sources(id),
    taken_at TEXT NOT NULL,
    data_json TEXT NOT NULL,
    item_count INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS scout_findings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL REFERENCES scout_sources(id),
    snapshot_id INTEGER NOT NULL REFERENCES scout_snapshots(id),
    kind TEXT NOT NULL CHECK (kind IN ('new','changed','gone')),
    item_key TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    detected_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_findings_source_detected
    ON scout_findings(source_id, detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_findings_key
    ON scout_findings(source_id, item_key, detected_at);

CREATE TABLE IF NOT EXISTS scout_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL REFERENCES scout_sources(id),
    status TEXT NOT NULL CHECK (status IN ('succeeded','failed')),
    started_at TEXT NOT NULL,
    finished_at TEXT,
    items_count INTEGER,
    findings_count INTEGER,
    error TEXT
);
CREATE INDEX IF NOT EXISTS idx_runs_source_started
    ON scout_runs(source_id, started_at DESC);
