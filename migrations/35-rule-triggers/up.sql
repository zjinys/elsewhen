-- 规则触发引擎（决策辅助 D1）：
-- * rules 增补结构化触发字段。历史自由文本规则不受影响（两列均 NULL，
--   语义 = 不参与事件预筛，仅保留注入对话/分析上下文的既有行为）。
-- * rule_hits 记一次「事件 × 规则」命中及处置：
--   pending   预筛命中，等待分析调用判定
--   notified  LLM 判定适用且高置信 → 即时提醒（事件旁标记）
--   aggregated LLM 判定适用但低置信 / 用户偏好为 review_only → 每日回顾聚合
--   declined  LLM 判定不适用，或用户偏好为 off
-- * 预筛零依赖：关键词匹配用大小写折叠 + 原文包含，不引入 regex。
-- * UNIQUE(rule_id, event_id)：同一事件对同一规则只有一条记录，事件重放
--   （重复捕获幂等、重分析）不会重复提醒。
--
-- 隐私与打扰红线：命中记录只挂本地事件,不外发；即时提醒默认开、用户可
-- 整体关（rule_trigger_settings.mode）。off 时预筛根本不跑，零成本。

ALTER TABLE rules ADD COLUMN trigger_scene TEXT
    CHECK(trigger_scene IS NULL OR trigger_scene IN ('capture','conversation','daily_review'));
ALTER TABLE rules ADD COLUMN trigger_keywords TEXT
    CHECK(trigger_keywords IS NULL OR json_valid(trigger_keywords));

CREATE TABLE rule_hits (
    id TEXT PRIMARY KEY,
    rule_id TEXT NOT NULL REFERENCES rules(id) ON DELETE CASCADE,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending','notified','aggregated','declined')),
    confidence REAL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(rule_id, event_id)
);
CREATE INDEX idx_rule_hits_event ON rule_hits(event_id);
CREATE INDEX idx_rule_hits_status ON rule_hits(status, created_at);

-- 提醒偏好（单行，沿用 network_settings/provider_key_policy 的 singleton 写法）：
-- mode = 'instant'（默认：高置信即时提醒 + 其余回顾聚合，分级防打扰）
--      | 'review_only'（即时提醒全关，全部进每日回顾聚合）
--      | 'off'（彻底关闭：预筛不跑、不建命中记录）
CREATE TABLE rule_trigger_settings (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    mode TEXT NOT NULL DEFAULT 'instant' CHECK(mode IN ('instant','review_only','off')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
INSERT INTO rule_trigger_settings (singleton, mode, created_at, updated_at)
VALUES (1, 'instant', strftime('%Y-%m-%dT%H:%M:%fZ','now'), strftime('%Y-%m-%dT%H:%M:%fZ','now'));
