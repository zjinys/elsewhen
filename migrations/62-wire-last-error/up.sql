-- `deliver` 状态机需要「为什么在退避」（R6-1/R6-2）。
--
-- 现在一次失败只留下痕迹：attempts 涨了、available_at_ms 往后挪了。没有任何
-- 地方能回放**原因**，于是 UI 只能说「同步中」，排查要人工对着四列拼图。
-- 这一列存的是**分类码**（不是错误原文，也不是响应体）：
--   transport（5xx/网络/超时）、integrity（摘要/AEAD 不符）、
--   protocol（413 一类不该发生的实现错位）、relay_quota（507，另有 blocked_reason）。
ALTER TABLE sync_wire_outbox ADD COLUMN last_error TEXT NOT NULL DEFAULT '';
