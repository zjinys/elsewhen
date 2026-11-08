-- 用户声明的 MCP 服务关联意图；属于可同步的定义，不授予执行权限。
ALTER TABLE mcp_servers ADD COLUMN keywords TEXT NOT NULL DEFAULT '';
