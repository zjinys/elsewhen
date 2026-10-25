-- 习惯页与问候计划的关联表（2026-10-22 会话拍板：wiki_pages 是唯一事实源，
-- 一个习惯可关联多张习惯页；页面一旦关联不再重复提炼，去重靠关联本身）：
-- 1) habit_page_links：页面 → 习惯计划的多对一关联，随多设备同步。
--    page_slug 唯一 = 一张习惯页最多属于一个习惯（去重约束）；
--    nudge_id 不唯一 = 一个习惯可以聚合多张页面的证据。
-- 2) 推断建议行 status='suggested'（自动推断、尚未确认）与 'active'（已确认）。
-- 3) 本表只记关联事实：归档/删除页面的清理由应用层在同一事务内删行 +
--    最后一条 active 关联解除时把 habit_nudges.enabled 置 0；表上不建触发器，
--    避免 snapshot apply / rename 等同步路径静默触发业务副作用。
CREATE TABLE habit_page_links (
    id         TEXT PRIMARY KEY,
    page_slug  TEXT NOT NULL,                 -- wiki_pages.slug（kind='habit'）
    nudge_id   TEXT NOT NULL,                 -- habit_nudges.id
    status     TEXT NOT NULL DEFAULT 'suggested' CHECK(status IN ('suggested','active')),
    source     TEXT NOT NULL DEFAULT 'auto' CHECK(source IN ('auto','manual')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_habit_page_links_page ON habit_page_links(page_slug);
CREATE INDEX idx_habit_page_links_nudge ON habit_page_links(nudge_id, status);
