# Agent Note: Elsewhen 的本地优先与状态边界

Status: proposed

## Problem

个人事件系统同时包含极低延迟的记录入口和不稳定的 AI 分析。如果 Capture 等待网络或让 AI 直接修改项目状态，网络故障、模型误判和规则变更会破坏用户信任。原始记录、派生理解和确定性提醒也需要可追溯边界。

## Proposal

采用单一 Rust `personald` daemon、本地 SQLite 和按需 Slint UI。Capture 事务只写不可变 Raw Event，随后以可恢复队列异步调用抽象 `AiProvider`。AI 只产生 schema 校验后的候选结果；Core 与 Rule Engine 决定是否落库、创建 Task/Attention 或要求 Clarification。Morning/Evening 从事实和派生状态动态计算，不建立第二套事实源。

## Alternatives considered

1. 使用 Electron/Tauri/浏览器常驻：生态成熟，但常驻资源和运行时复杂度与“随手记录”目标冲突。
2. 让 AI 直接调用数据库/规则：实现快，但无法稳定审计、回放和防止高风险误判。
3. 先做云同步或多用户：会扩大隐私、账号和一致性范围，不能先验证核心记录闭环。

## Consequences

收益是离线可用、原始事实可重放、Provider/Prompt 可替换、规则行为可测试。代价是需要维护 queue/retry、derived schema、跨平台 adapter 和较明确的状态机；MVP 的功能面必须保持克制。
