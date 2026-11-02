-- 认知型目标长程偏离检测与航向校准（Phase G4，FR-PES-005-04）。
--
-- * 滑动窗口算法（过去 3~5 天），检测抓手停滞与精力脱靶；
-- * 频控与防烦纪律：同一目标偏离提醒至少间隔 7 天，用户点击「暂不打扰」后写入 dismissed_until 静默退避 7 天；
-- * 状态机：pending（待校准/呈现中） -> dismissed（暂不打扰） / realigned（已更新抓手或聚焦攻克已校准）。

CREATE TABLE goal_compass_nudges (
    id TEXT PRIMARY KEY,
    goal_id TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
    goal_content TEXT NOT NULL,
    current_focus TEXT NOT NULL,
    stalled_days INTEGER NOT NULL,
    recent_topics TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending', 'dismissed', 'realigned')),
    created_at TEXT NOT NULL,
    dismissed_until TEXT
);

CREATE INDEX idx_goal_compass_nudges_goal
    ON goal_compass_nudges(goal_id, created_at DESC);

CREATE INDEX idx_goal_compass_nudges_pending
    ON goal_compass_nudges(status, created_at DESC);
