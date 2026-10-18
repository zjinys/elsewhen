-- 远程 MCP server（streamable-http 传输）：
-- 1) transport 放宽到 'stdio' | 'streamable-http'（SQLite 不能改 CHECK，重建表）
-- 2) 新列 url：http 传输的端点；headers_json：透传的鉴权/自定义 header
--    （Bearer token 等；明文存放，与 api_key 同级敏感面，不进同步域）
CREATE TABLE mcp_servers_new (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    transport TEXT NOT NULL CHECK (transport IN ('stdio', 'streamable-http')),
    command TEXT NOT NULL DEFAULT '',
    args_json TEXT NOT NULL DEFAULT '[]',
    env_json TEXT NOT NULL DEFAULT '{}',
    url TEXT NOT NULL DEFAULT '',
    headers_json TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO mcp_servers_new
    (id, name, transport, command, args_json, env_json, url, headers_json, enabled, created_at, updated_at)
SELECT
    id, name, transport, command, args_json, env_json, '', '{}', enabled, created_at, updated_at
FROM mcp_servers;

DROP TABLE mcp_servers;
ALTER TABLE mcp_servers_new RENAME TO mcp_servers;
