-- Successful collection is one atomic publication. Leases fence overlapping collectors.
ALTER TABLE scout_sources ADD COLUMN lease_token TEXT;
ALTER TABLE scout_sources ADD COLUMN lease_until INTEGER;
ALTER TABLE scout_snapshots ADD COLUMN run_key TEXT;
ALTER TABLE scout_snapshots ADD COLUMN spec_hash TEXT;
ALTER TABLE scout_snapshots ADD COLUMN interval_minutes INTEGER;
CREATE UNIQUE INDEX idx_snapshot_run_key ON scout_snapshots(run_key);
CREATE TABLE scout_commit_guard (valid INTEGER NOT NULL CHECK(valid=1));
CREATE TABLE scout_instance (id INTEGER PRIMARY KEY CHECK(id=1), instance_id TEXT NOT NULL);
INSERT INTO scout_instance VALUES(1, lower(hex(randomblob(16))));

CREATE TABLE scout_products (
  product_id TEXT PRIMARY KEY, platform TEXT NOT NULL, platform_key TEXT NOT NULL,
  title TEXT NOT NULL, payload_json TEXT NOT NULL,
  first_seen_at TEXT NOT NULL, last_seen_at TEXT NOT NULL,
  UNIQUE(platform, platform_key)
);
CREATE INDEX idx_products_latest ON scout_products(last_seen_at DESC, product_id);
CREATE TABLE scout_product_sources (
  product_id TEXT NOT NULL REFERENCES scout_products(product_id),
  source_id TEXT NOT NULL REFERENCES scout_sources(id),
  last_seen_at TEXT NOT NULL, present INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY(product_id, source_id)
);
CREATE INDEX idx_product_sources_source ON scout_product_sources(source_id, product_id);
CREATE TABLE scout_observations (
  snapshot_id INTEGER NOT NULL, product_id TEXT NOT NULL REFERENCES scout_products(product_id),
  source_id TEXT NOT NULL, item_key TEXT NOT NULL, taken_at TEXT NOT NULL,
  spec_hash TEXT NOT NULL, interval_minutes INTEGER NOT NULL,
  payload_json TEXT NOT NULL,
  PRIMARY KEY(snapshot_id, product_id)
);
CREATE INDEX idx_observations_product_time ON scout_observations(product_id, taken_at DESC, snapshot_id DESC);
CREATE INDEX idx_observations_item_history ON scout_observations(source_id,item_key,snapshot_id DESC);
CREATE INDEX idx_observations_source_time ON scout_observations(source_id, taken_at DESC);
-- Published snapshots from the legacy writer only. Unmatched/orphan snapshots are not baselines.
UPDATE scout_snapshots SET run_key='legacy:' || id
 WHERE EXISTS(SELECT 1 FROM scout_runs r WHERE r.source_id=scout_snapshots.source_id
 AND r.status='succeeded' AND r.finished_at=scout_snapshots.taken_at);
