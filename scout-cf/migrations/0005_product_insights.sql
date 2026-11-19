-- Rebuildable, versioned discovery index. Public observations remain the source of truth.
CREATE TABLE scout_product_insights (
  product_id TEXT NOT NULL REFERENCES scout_products(product_id),
  snapshot_id INTEGER NOT NULL,
  as_of TEXT NOT NULL,
  changed_at TEXT NOT NULL,
  observed_at TEXT NOT NULL,
  valid_until TEXT NOT NULL,
  reasons_json TEXT NOT NULL,
  PRIMARY KEY(product_id,snapshot_id)
);
CREATE INDEX idx_insights_discovery ON scout_product_insights(changed_at DESC,product_id,as_of);
CREATE INDEX idx_insights_product_time ON scout_product_insights(product_id,as_of DESC,snapshot_id DESC);
CREATE TABLE scout_product_aliases (
  alias_id TEXT PRIMARY KEY,
  product_id TEXT NOT NULL REFERENCES scout_products(product_id)
);
CREATE INDEX idx_product_aliases_target ON scout_product_aliases(product_id);

CREATE INDEX idx_observations_product_snapshot ON scout_observations(product_id,snapshot_id DESC);
