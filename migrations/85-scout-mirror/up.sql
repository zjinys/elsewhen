-- Scout 情报消费端（提案 2026-11-15-watcher-data-collection-and-analysis §4）：
-- scout 服务 findings 的本地镜像 + 本地设置。两张表都是本机派生物：
-- 不进同步快照（findings 是外部世界的观测，不是个人事件/知识），
-- 服务端（CF Workers D1）才是权威数据源，本地表可随时整体清空重建。
--
-- scout_findings_mirror：/findings 响应的镜像 + read_at。
--   read_at 归 Elsewhen 管（服务端没有已读概念）；detected_at 保留服务端
--   时钟（跨设备展示一致）。id 用服务端 findings.id（跨拉取批次稳定，
--   支持「拉到已有 id 就跳过」的增量幂等）。
-- scout_settings：scout base url / token 引用（{{secret:name}}）/ 拉取间隔 /
--   启用 / 增量水位。单行表（id=1），行不存在 = 未配置。

CREATE TABLE scout_findings_mirror (
    id INTEGER PRIMARY KEY,            -- 服务端 scout_findings.id
    source_id TEXT NOT NULL,
    snapshot_id INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('new','changed','gone')),
    item_key TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    detected_at TEXT NOT NULL,         -- 服务端 ISO 时间戳，原样保留
    fetched_at TEXT NOT NULL DEFAULT (datetime('now')),
    read_at TEXT
);

CREATE INDEX idx_scout_findings_detected
    ON scout_findings_mirror(detected_at DESC);
CREATE INDEX idx_scout_findings_unread
    ON scout_findings_mirror(read_at) WHERE read_at IS NULL;
CREATE INDEX idx_scout_findings_source_item
    ON scout_findings_mirror(source_id, item_key);

CREATE TABLE scout_settings (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    base_url TEXT NOT NULL,
    token_secret TEXT,                 -- 命名密钥代号（{{secret:name}} 形态引用）
    poll_interval_minutes INTEGER NOT NULL DEFAULT 15
        CHECK(poll_interval_minutes >= 5),
    enabled INTEGER NOT NULL DEFAULT 1,
    last_polled_at TEXT,               -- 本地水位（增量拉取 since 参数）
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
