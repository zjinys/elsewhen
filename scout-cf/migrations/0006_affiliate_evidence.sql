-- Supplemental evidence never participates in collection hashes or observations.
CREATE TABLE scout_affiliate_checks (
  product_id TEXT PRIMARY KEY REFERENCES scout_products(product_id),
  document_json TEXT,
  next_check_at TEXT NOT NULL,
  checked_at TEXT,
  outcome TEXT,
  lease_token TEXT,
  lease_until INTEGER
);
CREATE INDEX idx_affiliate_due ON scout_affiliate_checks(next_check_at, product_id);
