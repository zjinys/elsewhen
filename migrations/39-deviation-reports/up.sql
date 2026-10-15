-- 周期报告与行为偏差检测（FR-PES-005-04，管道产物）。
--
-- * 报告里的数字来自本地 SQL 实时聚合（R2 聚合层 query_event_stats 与
--   R1 周/月拼装同源口径），AI 只做偏差语义判定——统计本地算，偏差才用
--   AI（FR-PES-005 不变式 1）。
-- * 生成与检测是各自独立的状态：deviations_json 为 NULL 表示「尚未检测」，
--   是合法状态（AI 失败或 Provider 未配置不影响报告可读，不变式：AI 失败
--   不影响统计）。
-- * report_json 内联生成时刻的活跃目标快照（phase+content）：偏差判定与
--   呈现都可溯源到「当时的目标是什么」，目标事后被取代也不影响旧报告
--   的自我解释（不变式 6：可溯源）。
-- * 幂等：UNIQUE(period, period_type) 防重生成——补做逻辑可以放心地在
--   任意 tick 重试；period 取该报告覆盖周期的末日（周=周日、月=月末），
--   与「最近一个完整周期」的唯一对应关系一致。
-- * 未读/已读（read_at）由回顾页消费（R4），提醒形态只有未读小圆点一种。
-- * 管道产物纪律：排除于快照同步（本机回顾产物不跨设备复制，与
--   knowledge 管线产物同口径）。

CREATE TABLE deviation_reports (
    id TEXT PRIMARY KEY,
    period TEXT NOT NULL,
    period_type TEXT NOT NULL CHECK(period_type IN ('week','month')),
    generated_at TEXT NOT NULL,
    report_json TEXT NOT NULL,
    deviations_json TEXT,
    detected_at TEXT,
    read_at TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(period, period_type)
);

CREATE INDEX idx_deviation_reports_period
    ON deviation_reports(period_type, created_at DESC);
