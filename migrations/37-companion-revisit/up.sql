-- 闭环度量（决策辅助 D6）之二：低打扰情感性回访。
-- * companion_settings（singleton，沿用 rule_trigger_settings 写法）：
--   revisit_enabled       默认 1（打开应用时才可能回访；用户可一键关，
--                         关闭后 check 路径零查询零成本——零侵入验证的依据）
--   revisit_interval_days 回访频率下限（默认 7 天），CHECK >= 1 上限可配
--   last_revisit_at       上次实际回访时间；展示即记账，防止重复打扰
-- * 触发面红线：仅「打开应用时」的一次读面检查；不注册任何系统级推送、
--   不写通知通道、不做后台唤醒。素材来自既有事件与分析（被记住的事），
--   呈现为一条温和的回引，可以完全无视。

CREATE TABLE companion_settings (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    revisit_enabled INTEGER NOT NULL DEFAULT 1 CHECK(revisit_enabled IN (0,1)),
    revisit_interval_days INTEGER NOT NULL DEFAULT 7 CHECK(revisit_interval_days >= 1),
    last_revisit_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
INSERT INTO companion_settings (singleton, revisit_enabled, revisit_interval_days, created_at, updated_at)
VALUES (1, 1, 7, strftime('%Y-%m-%dT%H:%M:%fZ','now'), strftime('%Y-%m-%dT%H:%M:%fZ','now'));
