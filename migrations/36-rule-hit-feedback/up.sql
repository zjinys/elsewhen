-- 闭环度量（决策辅助 D6）之一：提醒采纳反馈。
-- * rule_hits 增两列：用户对一条已通知/已聚合提醒的反馈。
--   helpful        提醒了且真有帮助（采纳）
--   not_relevant   提醒了但不对题（降噪信号，只记录不外推）
--   NULL           尚未反馈（默认；历史与无反馈行为零改动）
-- * 反馈只记在本地命中行上（rule_hits 本就排除于快照），只记录不推断：
--   没有任何自动拉伸/自动关规则的逻辑消费它，仅面板计数展示。
-- * 可清：feedback 置回 NULL 即清除（toggle 语义）。

ALTER TABLE rule_hits ADD COLUMN feedback TEXT
    CHECK(feedback IS NULL OR feedback IN ('helpful','not_relevant'));
ALTER TABLE rule_hits ADD COLUMN feedback_at TEXT;

CREATE INDEX idx_rule_hits_feedback ON rule_hits(feedback) WHERE feedback IS NOT NULL;
