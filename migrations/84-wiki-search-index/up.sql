-- 知识页词法索引（FTS5）。分词与写索引在 Rust 侧惰性完成
-- （src/storage/wiki/search.rs），这里只建表和标脏触发器。
--
-- wiki_fts：无内容表，只存倒排索引，不重复存正文。两列与检索的两种命中
--   对应：prominent = 标题 + 标签 + 适用条件，body = 正文。记号是字符二元组的
--   十六进制，由应用生成，所以用内置的 ascii 分词器即可。
-- wiki_search_docs：文档号 ↔ 页 id。文档号是整数主键，VACUUM 后不变；
--   wiki_pages 没有整数主键，它的隐式 rowid 会在 VACUUM 后改变，不能直接用。
-- wiki_search_dirty：待重建索引的页。触发器只往这里记页 id——纯 SQL、不碰
--   虚表、不调用应用函数，所以任何工具改库都不会因为索引报错，最多是索引
--   暂时落后（落后时检索回落到扫描路径）。
--
-- 三者都是本机派生物：不进快照、不同步，可以随时整体重建。
CREATE VIRTUAL TABLE wiki_fts USING fts5(
    prominent,
    body,
    content='',
    contentless_delete=1,
    tokenize='ascii'
);

CREATE TABLE wiki_search_docs (
    doc INTEGER PRIMARY KEY,
    page_id TEXT NOT NULL UNIQUE
);

CREATE TABLE wiki_search_dirty (
    page_id TEXT PRIMARY KEY
) WITHOUT ROWID;

-- 从命中页里取前 80 个时只需要这几列。有了它，筛选和排序只读索引，不必为
-- 每个命中页去读带着整段正文的行。
CREATE INDEX wiki_pages_search_filter ON wiki_pages(id, status, kind, opinion, last_seen_at);

-- 标脏写成「不存在才插入」，而不是 INSERT OR IGNORE：触发器里语句的冲突处理
-- 会被触发它的外层语句的 OR 子句覆盖（外层是 INSERT OR ROLLBACK 之类时，
-- 重复标脏会让页面写入本身失败）。不产生冲突就与外层怎么写无关。
CREATE TRIGGER wiki_search_page_insert AFTER INSERT ON wiki_pages
BEGIN
    INSERT INTO wiki_search_dirty(page_id) SELECT NEW.id
     WHERE NOT EXISTS (SELECT 1 FROM wiki_search_dirty WHERE page_id = NEW.id);
END;

CREATE TRIGGER wiki_search_page_update AFTER UPDATE OF id, title, tags, content_md ON wiki_pages
WHEN OLD.id IS NOT NEW.id OR OLD.title IS NOT NEW.title
  OR OLD.tags IS NOT NEW.tags OR OLD.content_md IS NOT NEW.content_md
BEGIN
    INSERT INTO wiki_search_dirty(page_id) SELECT OLD.id
     WHERE NOT EXISTS (SELECT 1 FROM wiki_search_dirty WHERE page_id = OLD.id);
    INSERT INTO wiki_search_dirty(page_id) SELECT NEW.id
     WHERE NOT EXISTS (SELECT 1 FROM wiki_search_dirty WHERE page_id = NEW.id);
END;

CREATE TRIGGER wiki_search_page_delete AFTER DELETE ON wiki_pages
BEGIN
    INSERT INTO wiki_search_dirty(page_id) SELECT OLD.id
     WHERE NOT EXISTS (SELECT 1 FROM wiki_search_dirty WHERE page_id = OLD.id);
END;

-- 适用条件属于 prominent 文本。
CREATE TRIGGER wiki_search_metadata_insert AFTER INSERT ON knowledge_metadata
BEGIN
    INSERT INTO wiki_search_dirty(page_id) SELECT NEW.page_id
     WHERE NOT EXISTS (SELECT 1 FROM wiki_search_dirty WHERE page_id = NEW.page_id);
END;

CREATE TRIGGER wiki_search_metadata_update AFTER UPDATE OF page_id, applicable_when ON knowledge_metadata
WHEN OLD.page_id IS NOT NEW.page_id OR OLD.applicable_when IS NOT NEW.applicable_when
BEGIN
    INSERT INTO wiki_search_dirty(page_id) SELECT OLD.page_id
     WHERE NOT EXISTS (SELECT 1 FROM wiki_search_dirty WHERE page_id = OLD.page_id);
    INSERT INTO wiki_search_dirty(page_id) SELECT NEW.page_id
     WHERE NOT EXISTS (SELECT 1 FROM wiki_search_dirty WHERE page_id = NEW.page_id);
END;

CREATE TRIGGER wiki_search_metadata_delete AFTER DELETE ON knowledge_metadata
BEGIN
    INSERT INTO wiki_search_dirty(page_id) SELECT OLD.page_id
     WHERE NOT EXISTS (SELECT 1 FROM wiki_search_dirty WHERE page_id = OLD.page_id);
END;

-- 现有页面全部待建索引：由后台 tick 与检索路径分批完成。
INSERT INTO wiki_search_dirty(page_id) SELECT id FROM wiki_pages;
