-- v65：认知型目标扩展（切片字段 + 三态生命周期 + 活跃目标上限）
--
-- 1. 扩展字段：新增认知切片为什么做 (why)、当前抓手 (current_focus)、已知卡点 (blocker)、判定标准 (done_criteria)
-- 2. 状态扩展：status 枚举由 ('active','superseded') 升级为 ('active','paused','completed')
-- 3. 平滑升级：历史 status = 'superseded' 自动迁移为 'paused'（保留历史切片以备随时读档唤醒）
-- 4. 触发器继承：保留活跃目标最多 3 条的硬性约束（status = 'active' <= 3）

CREATE TABLE goals_new (
  id TEXT PRIMARY KEY,
  content TEXT NOT NULL CHECK(length(trim(content)) > 0),
  phase TEXT NOT NULL CHECK(phase IN ('near','mid','long')),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','paused','completed')),
  why TEXT,
  current_focus TEXT,
  blocker TEXT,
  done_criteria TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  superseded_at TEXT
);

INSERT INTO goals_new (
  id, content, phase, status,
  why, current_focus, blocker, done_criteria,
  created_at, updated_at, superseded_at
)
SELECT
  id, content, phase,
  CASE WHEN status = 'superseded' THEN 'paused' ELSE status END,
  NULL, NULL, NULL, NULL,
  created_at, updated_at, superseded_at
FROM goals;

DROP TABLE goals;
ALTER TABLE goals_new RENAME TO goals;

CREATE INDEX idx_goals_status ON goals(status);

CREATE TRIGGER goals_cap_active_insert
BEFORE INSERT ON goals WHEN NEW.status = 'active'
BEGIN
  SELECT CASE WHEN (SELECT COUNT(*) FROM goals WHERE status='active') >= 3
    THEN RAISE(ABORT, 'active_goal_limit_reached') END;
END;

CREATE TRIGGER goals_cap_active_update
BEFORE UPDATE OF status ON goals
WHEN NEW.status='active' AND OLD.status <> 'active'
BEGIN
  SELECT CASE WHEN (SELECT COUNT(*) FROM goals WHERE status='active') >= 3
    THEN RAISE(ABORT, 'active_goal_limit_reached') END;
END;
