-- 页级 TL;DR 修正（迁移 82 的后续）。
--
-- 1. basis：摘要生成时实际读到的范围（full / head-outline / reading-root），
--    让「只读了开头」的摘要可被识别，不冒充全文概括。
-- 2. 82 版的行来自只读开头的生成，并且候选没有排除记事本、孵化中与已否定
--    的页；整表清空，由后台按新规则重建。
-- 3. 页面正文是原地 UPDATE：旧行留在表里但 content_hash 不再匹配，读不出来
--    也不会被回填选中（回填只找「没有行」的页）。正文变化时删行，回填才会
--    重新生成。触发器是纯 SQL，覆盖全部写路径（含同步应用与快照导入）。
-- 4. 退避账本：retry_at 之前回填不再碰这一页。两种来源——
--    生成失败（attempts 递增、记 failed_at，按次数拉长退避），以及页面当前
--    不可引用（attempts 不变，一小时后再核对）。没有它，每个 tick 都会对
--    同两页重复付费请求，或逐页重复核对同一批不合格的页。
ALTER TABLE page_tldr ADD COLUMN basis TEXT NOT NULL DEFAULT 'head-outline';
DELETE FROM page_tldr;

CREATE TABLE page_tldr_backoff (
    page_id TEXT PRIMARY KEY REFERENCES wiki_pages(id) ON DELETE CASCADE,
    content_hash TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts >= 0),
    failed_at TEXT,
    retry_at TEXT NOT NULL
);

CREATE TRIGGER page_tldr_expire AFTER UPDATE OF content_md ON wiki_pages
WHEN OLD.content_md IS NOT NEW.content_md
BEGIN
    DELETE FROM page_tldr WHERE page_id = NEW.id;
    DELETE FROM page_tldr_backoff WHERE page_id = NEW.id;
END;
