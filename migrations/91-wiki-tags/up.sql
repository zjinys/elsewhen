-- Canonical tag vocabulary and page membership. `wiki_pages.tags` remains the
-- wire/storage compatibility column; triggers keep this normalized index in sync.
CREATE TABLE wiki_tags (
    name TEXT PRIMARY KEY COLLATE NOCASE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE wiki_page_tags (
    page_id TEXT NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
    tag_name TEXT NOT NULL COLLATE NOCASE REFERENCES wiki_tags(name) ON DELETE CASCADE,
    PRIMARY KEY (page_id, tag_name)
);

CREATE INDEX idx_wiki_page_tags_tag ON wiki_page_tags(tag_name);

INSERT OR IGNORE INTO wiki_tags(name, created_at, updated_at)
SELECT DISTINCT trim(j.value), datetime('now'), datetime('now')
FROM wiki_pages p, json_each(
    CASE WHEN json_valid(p.tags) THEN p.tags ELSE '[]' END
) j
WHERE trim(j.value) <> '';

INSERT OR IGNORE INTO wiki_page_tags(page_id, tag_name)
SELECT p.id, trim(j.value)
FROM wiki_pages p, json_each(
    CASE WHEN json_valid(p.tags) THEN p.tags ELSE '[]' END
) j
WHERE trim(j.value) <> '';

CREATE TRIGGER wiki_pages_tags_index_insert
AFTER INSERT ON wiki_pages
BEGIN
    INSERT OR IGNORE INTO wiki_tags(name, created_at, updated_at)
    SELECT trim(j.value), datetime('now'), datetime('now')
    FROM json_each(CASE WHEN json_valid(NEW.tags) THEN NEW.tags ELSE '[]' END) j
    WHERE trim(j.value) <> '';
    INSERT OR IGNORE INTO wiki_page_tags(page_id, tag_name)
    SELECT NEW.id, trim(j.value)
    FROM json_each(CASE WHEN json_valid(NEW.tags) THEN NEW.tags ELSE '[]' END) j
    WHERE trim(j.value) <> '';
END;

CREATE TRIGGER wiki_pages_tags_index_update
AFTER UPDATE OF tags ON wiki_pages
WHEN OLD.tags IS NOT NEW.tags
BEGIN
    DELETE FROM wiki_page_tags WHERE page_id = NEW.id;
    INSERT OR IGNORE INTO wiki_tags(name, created_at, updated_at)
    SELECT trim(j.value), datetime('now'), datetime('now')
    FROM json_each(CASE WHEN json_valid(NEW.tags) THEN NEW.tags ELSE '[]' END) j
    WHERE trim(j.value) <> '';
    INSERT OR IGNORE INTO wiki_page_tags(page_id, tag_name)
    SELECT NEW.id, trim(j.value)
    FROM json_each(CASE WHEN json_valid(NEW.tags) THEN NEW.tags ELSE '[]' END) j
    WHERE trim(j.value) <> '';
END;
