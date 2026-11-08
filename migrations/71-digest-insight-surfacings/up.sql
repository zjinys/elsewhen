-- v71：知识库消化洞见浮现（surfacing digest insights）——高价值提炼在
-- 主对话流以轻量卡片浮现并闭环（提案 2026-11-05-surfacing-digest-insights）。
--
-- 设计要点：
-- - 候选登记（pending）在本机库内排队：仅 insight（带行动建议）/ 新成立的
--   habit 页 / 识别出的 recurring_cost 页入池；同一页面终身最多浮现 1 次；
-- - 每日最多浮现 1 条（本地自然日 surfaced_date），07:00–22:00 之外静默；
-- - 仅 owner 主设备插入消息（副设备经同步直接收到 digest_insight 消息）；
-- - 本表对齐 habit_nudge_fires 的「本机台账」定位：不进快照、不参与同步；
--   跨设备动作产物（规则/待办）由各自的一等公民同步承担；
-- - 卡片消息 kind='digest_insight'，ref='surfacing:<id>'，内容自含卡片文本，
--   副设备据此渲染与就地重建本机台账行（同 id，前向兼容将来如需同步）。

CREATE TABLE digest_insight_surfacings (
    id TEXT PRIMARY KEY,
    insight_id TEXT,
    page_slug TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('insight','habit','recurring_cost')),
    title TEXT NOT NULL,
    observation TEXT NOT NULL,
    action_suggestion TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending','surfaced','acted','dismissed','expired')),
    surfaced_at TEXT,
    surfaced_date TEXT,
    message_id TEXT,
    action_type TEXT CHECK(action_type IN ('rule','habit_nudge','todo','dismiss')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_digest_insight_surfacings_date
    ON digest_insight_surfacings(surfaced_date, status);
CREATE INDEX idx_digest_insight_surfacings_slug
    ON digest_insight_surfacings(page_slug);
CREATE INDEX idx_digest_insight_surfacings_status
    ON digest_insight_surfacings(status, created_at);
