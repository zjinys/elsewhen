# Agent Note: 分析队列触发与失败重试边界

Status: implemented

## Problem

统一输入已经会产生持久化分析任务，但 Flutter 侧触发接口此前没有真正消费队列，记录只能停留在 pending。

## Decision

`trigger_analysis` 读取数据库中的激活 Provider，逐项 claim 任务并调用 OpenAI-compatible 接口。只有合法 JSON 对象才会写入 `event_analyses` 并将事件标记为 processed；无 Provider 明确返回 `no_provider`；Provider 错误、非法 JSON 和非对象 JSON 统一调用 `fail_analysis`，沿用指数退避和最终 failed 状态。每次调用最多处理当次可见队列中的 50 项，避免单次 UI 调用无限运行。

## Alternatives considered

- 让输入提交同步等待 AI：会把网络和 Provider 故障带入保存路径，违反本地优先约束。
- 只在 Flutter 内存中维护任务：应用退出后无法恢复，也无法观察 retry / failed 状态。
- 接受任意 AI 文本作为分析结果：无法稳定消费结构化派生数据，因此要求 JSON object。

## Consequences

队列现在可从 Bridge 主动推进，失败任务和原始事件都可恢复；处理上限让接口适合交互式唤醒，但持续积压仍需后续后台 worker 或应用启动时唤醒机制处理。
