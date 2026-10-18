-- MCP server 平台标签（方案：定义随人走，激活看平台）。
-- 1) platform 列：'all' | 'linux' | 'macos' | 'windows'。工具注册/调用只
--    激活 platform='all' 或匹配当前 OS 的行——stdio 命令是平台绑定事实
--    （npx / npx.cmd / PATH 差异），不再整表排除出同步域，而是按平台
--    分发：同一名 server 可有跨平台变体（同名不同标签）。
-- 2) 唯一约束从 name 落到 (name, platform)——SQLite 不能改约束，重建表。
-- 3) 旧行一律 platform='all'：现有 stdio 在移动端不激活（调用层已有
--    拒绝），现有 streamable-http 标 all 正好达成「桌面配好手机能用」。
-- 4) trusted 不随行传播：快照 apply 落表时强制置 0——URL 可以搭车，
--    信任是本机事实（本机环境、本机命令解析，豁免须由本机用户显式
--    声明，回到逐次审批）。headers_json 同理不入快照（凭据不离本机，
--    与 provider key 同一先例；接收端工具调用 401 是显式失败）。
CREATE TABLE mcp_servers_new (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    platform TEXT NOT NULL DEFAULT 'all'
        CHECK (platform IN ('all', 'linux', 'macos', 'windows')),
    transport TEXT NOT NULL CHECK (transport IN ('stdio', 'streamable-http')),
    command TEXT NOT NULL DEFAULT '',
    args_json TEXT NOT NULL DEFAULT '[]',
    env_json TEXT NOT NULL DEFAULT '{}',
    url TEXT NOT NULL DEFAULT '',
    headers_json TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    trusted INTEGER NOT NULL DEFAULT 0 CHECK (trusted IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (name, platform)
);

INSERT INTO mcp_servers_new
    (id, name, platform, transport, command, args_json, env_json,
     url, headers_json, enabled, trusted, created_at, updated_at)
SELECT
    id, name, 'all', transport, command, args_json, env_json,
    url, headers_json, enabled, trusted, created_at, updated_at
FROM mcp_servers;

DROP TABLE mcp_servers;
ALTER TABLE mcp_servers_new RENAME TO mcp_servers;
