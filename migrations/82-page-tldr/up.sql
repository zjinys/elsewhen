-- 页级 TL;DR 摘要索引：后台生成的结构化短摘要，检索第一跳返回目录级
-- 信息（标题 + TL;DR + 适用条件），深读原文走 get_wiki_page 分页。
-- content_hash 锚定页面版本：页面更新后摘要自动视为过期，等待重建。
CREATE TABLE page_tldr (
    page_id TEXT PRIMARY KEY REFERENCES wiki_pages(id) ON DELETE CASCADE,
    content_hash TEXT NOT NULL,
    summary TEXT NOT NULL CHECK(length(summary) > 0),
    model TEXT,
    created_at TEXT NOT NULL
);
