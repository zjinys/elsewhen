# Agent Note: 分析队列触发与失败重试边界

Status: implemented

## Problem

统一输入已经会产生持久化分析任务，但 Flutter 侧触发接口此前没有真正消费队列，记录只能停留在 pending。

## Decision

`trigger_analysis` 读取数据库中的激活 Provider，逐项 claim 任务并调用 OpenAI-compatible 接口。只有合法 JSON 对象才会写入 `event_analyses` 并将事件标记为 processed；无 Provider 明确返回 `no_provider`；Provider 错误、非法 JSON 和非对象 JSON 统一调用 `fail_analysis`，沿用指数退避和最终 failed 状态。每次调用最多处理当次可见队列中的 50 项，避免单次 Bridge 调用无限运行。

Flutter 进程持有一个串行 worker：Bridge 初始化后立即唤醒，事件或统一输入保存成功后只排入内存唤醒信号而不等待网络，5 秒周期唤醒负责 retry 的 `available_at` 到期。重复唤醒会合并；若一次 Rust 调用后仍有 pending 任务，worker 立即继续下一批。

分析结果使用 `event-analysis-v1` 稳定契约：固定 `schema_version`、`event_type`、`confidence`、`summary`、`clarifications`、`people`、`projects`、`follow_ups`，拒绝未知字段，限制 confidence 在 0..1，清洗字符串数组后再落盘。版本同时写入结果 JSON 和 `prompt_version`。

Bridge 初始化会把上次进程遗留的 running 任务重置为立即可执行的 retry。恢复不放在 `Store::open`，因为每个 API 都会打开独立连接；在那里恢复会把当前进程仍在执行的任务错误地重新 claim。

## Alternatives considered

- 让输入提交同步等待 AI：会把网络和 Provider 故障带入保存路径，违反本地优先约束。
- 只在 Flutter 内存中维护任务：应用退出后无法恢复，也无法观察 retry / failed 状态。
- 在每次 `Store::open` 时恢复 running：API 调用与 worker 并发时可能窃取仍在执行的任务，因此恢复限定在进程启动边界。
- 为每次保存启动一个独立分析调用：并发 Provider 请求和重复 claim 更难控制，因此 worker 串行执行并合并唤醒。
- 接受任意 AI 文本作为分析结果：无法稳定消费结构化派生数据，因此要求 JSON object。

## Consequences

保存路径只承担本地事务，Provider 延迟和故障不会阻塞输入；应用启动、批次积压、退避到期和异常退出后的 running 任务均能继续推进。代价是分析依赖 Flutter 进程存活，退出应用后不会在独立系统服务中执行；轮询最多带来约 5 秒的 retry 调度延迟。
