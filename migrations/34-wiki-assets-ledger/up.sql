-- wiki_assets 瘦身为台账（§31b）：图片字节不再落库，改由快照的文件段
-- 直接从发送端磁盘搬运、接收端直接落盘；数据库只记文件名片
-- （rel/sha256/mime/size），供完整性核对与清理记账。
CREATE TABLE wiki_assets_ledger (
    rel TEXT PRIMARY KEY,
    sha256 TEXT NOT NULL,
    mime TEXT NOT NULL,
    size INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
INSERT INTO wiki_assets_ledger(rel,sha256,mime,size,created_at)
    SELECT rel,sha256,mime,size,created_at FROM wiki_assets;
DROP TABLE wiki_assets;
ALTER TABLE wiki_assets_ledger RENAME TO wiki_assets;
