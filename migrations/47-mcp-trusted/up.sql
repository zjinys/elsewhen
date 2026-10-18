-- MCP server 免审批开关：trusted=1 时该 server 的工具调用直接执行，
-- 不再创建 pending-action 审批卡。默认 0（保持逐次审批）。
-- 风险提示在设置页明示：MCP server 是第三方代码，模型可能被诱导发起
-- 破坏性调用，开启即视为用户自担风险。
ALTER TABLE mcp_servers ADD COLUMN trusted INTEGER NOT NULL DEFAULT 0 CHECK (trusted IN (0, 1));
