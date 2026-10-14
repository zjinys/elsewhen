-- S05：wiki slug 足迹。页面行被物理删除后 head 仍保留，但 slug→id 映射
-- 已丢失；wiki 专属会话的引用锚等待判定需要「该 slug 曾真实出现过」的
-- 事实，不允许推断（同名新页不得自动收养旧引用）。
CREATE TABLE IF NOT EXISTS sync_wiki_slug_seen (
    slug TEXT PRIMARY KEY,
    seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
