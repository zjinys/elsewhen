-- 外网搜索（web_search）与 DoH/ECH 网络层配置。
--
-- 设计要点：
-- * 两张表分离：network_settings 是「DoH 作用域」这种网络层决策，web_search_configs
--   是搜索引擎本身——DoH 作用域（将来可扩成 global，影响 AI provider 等所有外发请求）
--   在语义上不属于搜索配置，故单独建表。
-- * 两张表都是单行（singleton=1 约束），与 provider_key_policy 同一写法。
-- * 没填 doh_uri = 不启用 DoH+ECH（退回系统直连，即现状）；填了 = 仅搜索走 DoH+ECH。
--   「全局」作用域本版只在 doh_scope 字段预留，UI 暂不开放。
-- * web_search_configs.enabled 默认 0：外网搜索默认关闭，需用户在设置页显式开启。

CREATE TABLE network_settings (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    -- 自定义 DoH 服务地址（https://.../dns-query 或线格式端点）。空串 = 不启用 DoH。
    -- 仅自定义、无内置默认：内置的 Cloudflare/Google 等在国内多被 SNI 封锁，
    -- 用户必须自己提供一个可用的 DoH（参见 docs/notes/proposed/2026-09-27-web-search-tool.md）。
    doh_uri TEXT NOT NULL DEFAULT '',
    -- DoH 作用域：'search_only'（仅外网搜索）| 'global'（所有外发请求，本版预留未实现）。
    doh_scope TEXT NOT NULL DEFAULT 'search_only' CHECK(doh_scope IN ('search_only','global')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE web_search_configs (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    -- 默认关闭：不连任何外部搜索服务，除非用户显式开启。
    enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0,1)),
    -- 引擎标识，首版仅 'tavily'；抽象层（WebSearchEngine trait）允许后补其它引擎。
    engine TEXT NOT NULL DEFAULT 'tavily',
    -- 引擎 API 端点（Tavily 默认 https://api.tavily.com/search）。
    base_url TEXT NOT NULL DEFAULT 'https://api.tavily.com/search',
    -- 引擎 API key（明文存储，与 ai_provider_configs.api_key 同一处理级别）。
    api_key TEXT NOT NULL DEFAULT '',
    max_results INTEGER NOT NULL DEFAULT 5,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
