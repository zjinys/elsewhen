-- MCP 只读免审批白名单（事故 2026-10-19 P0 建议的落地前提）：
-- allow_readonly_auto=1 时，该 server 下 annotations.readOnlyHint=true 的
-- 工具调用跳过逐次审批直接执行。红线：不盲信上游标注——开关必须由用户
-- 按server 显式打开（默认 0），可信执行仍留痕（事件）。
-- 本机策略列，与 trusted/headers 同理不进快照同步域。
ALTER TABLE mcp_servers ADD COLUMN allow_readonly_auto INTEGER NOT NULL DEFAULT 0;
