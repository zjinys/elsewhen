-- chat_run_requests 的降级构成观测列（P1 底座验收口径 2：工具结果占位
-- 节省量在 chat_run_requests 里可观测）。request_budget 事件每次覆盖写，
-- 终态行记录的值 = 最后一次装配的占位化条数；NULL = 未触发预算降级。
ALTER TABLE chat_run_requests ADD COLUMN pruned_tool_results INTEGER;
