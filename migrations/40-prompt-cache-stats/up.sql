-- 记录 provider 前缀缓存命中（prompt_cache_hit_tokens），让"上下文前缀
-- 稳定化 + 渠道缓存质量"可观测。deepseek 报 prompt_cache_hit_tokens，
-- OpenAI 报 prompt_tokens_details.cached_tokens，解析层统一取数。
-- 无命中数据（老网关/估算）时保持 NULL，不与 0 混淆。

ALTER TABLE chat_run_requests ADD COLUMN prompt_cached_tokens INTEGER;
