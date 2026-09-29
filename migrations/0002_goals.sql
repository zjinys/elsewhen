-- v2：目标表与活跃目标上限（FR-PES-005-01）
--
-- phase 是标签不是槽位，故刻意不加 UNIQUE(phase)：同一阶段可以有多条活跃目标，
-- 只要活跃总数不超过 3，且不要求三个阶段都有。
--
-- 活跃集上限 3 条，在数据库层强制而非 Rust 侧数一遍：应用层的 count-then-insert
-- 是 check-then-insert 竞态，本仓库已为此修过两处。触发器写法沿用 0001 里既有的
-- prevent_raw_event_mutation（RAISE(ABORT, ...)）。
--
-- 需要两个触发器：新增走 INSERT，但复活一条已归档目标是对既有行改 status，只挡
-- INSERT 会漏掉这条路径。UPDATE 那个的 WHEN 必须排除「状态没变」的情形
-- （OLD.status='active'，即编辑一条活跃目标的正文），否则正常编辑会被误计入上限。

CREATE TABLE goals (
  id TEXT PRIMARY KEY,
  content TEXT NOT NULL CHECK(length(trim(content)) > 0),
  phase TEXT NOT NULL CHECK(phase IN ('near','mid','long')),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','superseded')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  superseded_at TEXT
);

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
