-- Admin UI support: key/value settings (e.g. CWS watchlist extra IDs).
-- Source enabled/interval live in scout_sources and are user-sticky from now on:
-- upsertSource only refreshes name/spec_hash, first insert uses spec defaults.
CREATE TABLE IF NOT EXISTS scout_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
