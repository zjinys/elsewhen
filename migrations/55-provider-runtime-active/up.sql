-- AI provider 的运行时落点（本机会话事实）与用户显式激活分离。
--
-- 背景：对话链路 failover 成功后会把落点写进 ai_provider_configs.is_active
-- （set_active_ai_provider_config_for_sync(mark_sync=false)），但这破坏了
-- provider bundle 的明文稳定性——同一 revision_id 下 active_id/failover_order
-- 漂移，对端早已 ack 的信封永远等不来匹配内容，每个同步 tick 都命中
-- 「acknowledged re-seal skipped」幂等防线，且该落点变更永远无法同步。
--
-- 语义修正：is_active 只表达用户显式激活（进同步域）；运行时 failover
-- 落点落本表（singleton 单行，本机事实，永不入 bundle）。读取侧
-- （list_ai_provider_configs_for_runtime）按「运行时落点 > 用户显式激活>
-- 手动排序 > 创建序」給出候选次序，所有消费方拿到打头项即生效落点。
CREATE TABLE ai_provider_runtime_active(
  singleton INTEGER PRIMARY KEY CHECK(singleton=1),
  config_id TEXT NOT NULL
);
