CREATE TABLE scout_sync_cursor (
 id INTEGER PRIMARY KEY CHECK(id=1), instance_id TEXT NOT NULL, after_id INTEGER NOT NULL DEFAULT 0,
 upper_id INTEGER, products_json TEXT NOT NULL DEFAULT '[]', products_fetched_at TEXT
);
-- External observations are reconstructible; research is stored as human wiki pages
-- through existing synchronized, revisioned writes, never in this disposable cache.
