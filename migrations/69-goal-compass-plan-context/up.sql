-- v69：航向校准提醒的计划层上下文（P4 主动复盘与偏离扩展）。
--
-- 步骤即 goals 行：当停滞的活跃步骤属于某个长程计划时，提醒需携带计划
-- 意图与归属，供横幅/卡片展示「计划『…』的步骤已停滞」，而不再把它读成
-- 孤立目标。goal_compass_nudges 是纯本地衍生提醒表（不入同步与快照，
-- 见 SNAPSHOT_EXCLUDED_TABLES），两列可空、旧行自然回填为 NULL。

ALTER TABLE goal_compass_nudges ADD COLUMN plan_id TEXT;
ALTER TABLE goal_compass_nudges ADD COLUMN plan_intent TEXT;
