-- v68：AI 为中心的意图拆解——长程计划（Plan）实体与步骤挂接（P1）。
--
-- * goal_plans：长程计划整行（意图原文 / 愿景 / 假设 JSON（含软时间窗）/ 状态）。
--   计划本身不占活跃名额（「目标不设上限、注意力保护移驻并发步骤」，2026-11-04 定案）；
--   状态机 proposed/active/paused/completed——proposed 由审阅卡承载，落行即从 active 起步。
-- * goals.plan_id / step_index：步骤挂接（可空）。「无计划的独立目标」概念已取消：
--   直接建一条目标 = 单步计划，由应用层包成。
-- * 存量清理（定案：不做历史兼容）：先解绑目标讨论会话的 goal_id 外键
--   （migration 67 的 conversations.goal_id，无 CASCADE），再清 compass 提醒与 goals 行。
--   多端各自升级时各自执行；混版窗口期旧端可能把旧目标整行同步回来，风险已知并接受
--   （roadmap P1 备注）。
--
-- 注：同步记账里陈旧的 goal 头（sync_object_heads）交由既有 GC 回收，这里不动。

CREATE TABLE goal_plans (
  id TEXT PRIMARY KEY,
  intent_text TEXT NOT NULL CHECK(length(trim(intent_text)) > 0),
  vision TEXT NOT NULL DEFAULT '',
  assumptions TEXT NOT NULL DEFAULT '[]',
  status TEXT NOT NULL DEFAULT 'active'
    CHECK(status IN ('proposed','active','paused','completed')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

ALTER TABLE goals ADD COLUMN plan_id TEXT;
ALTER TABLE goals ADD COLUMN step_index INTEGER;

CREATE INDEX idx_goals_plan ON goals(plan_id, step_index);

-- 存量清理：解绑引用 → 清提醒 → 清目标。
UPDATE conversations SET goal_id = NULL WHERE goal_id IS NOT NULL;
DELETE FROM goal_compass_nudges;
DELETE FROM goals;
