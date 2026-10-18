-- MCP 服务器（用户自定义外部工具源，与 skills 同层级的使用者扩展能力）。
-- transport 首期只放 stdio（本地子进程，jsonrpc-2.0）；streamable-http 待
-- 实测后追加同列 CHECK 演进。命令行以 args_json/env_json 落数组/对象，
-- 避免 shell 拼接歧义。CRM 前缀命名 mcp__<server>__<tool> 由运行时拼接，
-- 表内不落合成名。
CREATE TABLE mcp_servers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    transport TEXT NOT NULL DEFAULT 'stdio' CHECK(transport IN ('stdio')),
    command TEXT NOT NULL,
    args_json TEXT NOT NULL DEFAULT '[]',
    env_json TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
