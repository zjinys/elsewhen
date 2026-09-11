# FR-PES-ARCH-001: 本地事件系统技术设计

**版本**: v1.0  
**最后更新**: 2026-09-11  
**状态**: Design  
**关联需求**: [FR-PES-001](../product/FR-PES-001-个人事件记录与状态管理.md)

## 1. 架构概览

```text
elsewhen (Rust daemon)
├── platform adapter: global hotkey / window / notification
├── capture UI (Slint, on demand)
├── application core: event, entity, project, task, attention
├── SQLite repository + migrations
├── durable analysis queue + retry worker
├── AI provider adapter
├── deterministic rule engine
└── scheduler: morning / evening / reminders / maintenance
```

单一 daemon 常驻；UI 只在需要时创建。平台差异收口到 adapter，业务模块不依赖 Linux API。

## 2. 数据边界

### 2.1 Source of truth

`events.raw_text` 与用户记录时间是不可变事实。所有 AI 结果写入带版本和时间戳的 derived 表：`event_analyses`、`entity_mentions`、`project_links`、`rule_evaluations`。重新分析通过新 analysis run 生成结果，不覆盖原始文本。

### 2.2 核心关系

```text
Event ──< EntityMention >── Entity
Event ──< ProjectLink >── Project
Event ──< Task / Attention / Evidence
Rule ──< RuleEvaluation ──> Task / Attention
AI Run ──> Analysis Result
```

Event、Task、Attention、Evidence、Decision、AI Run 均需稳定 ID；Task/Attention 必须保留来源事件或规则引用。

## 3. 异步处理契约

```text
commit raw event
  → enqueue analysis job (idempotency key = event_id + analysis_version)
  → worker: classify → extract → resolve → detect state
  → core validates result
  → rule engine evaluates
  → transaction writes derived state and outbox notifications
```

队列至少支持 `pending/running/retry/succeeded/failed`、指数退避、最大重试和人工/CLI 重放。通知应通过 outbox 或等价机制避免数据库成功而通知丢失。

## 4. AI 与规则边界

`AiProvider` 提供统一调用接口，首版可实现一个 OpenAI-compatible provider；Core 不依赖厂商。AI 只能提出事实和候选变化，不能执行写库或发通知。Rule Engine 只消费经过 schema 校验的结果，负责硬规则、任务和 Attention。

建议置信度策略：高置信度仅自动执行低风险结构化；中置信度进入 suggestion；低置信度或高风险状态变更进入 clarification。阈值必须由测试校准，不固化为不可变产品事实。

## 5. 存储与运行时

- Linux 数据目录：`~/.local/share/personald/events.db`；配置：`~/.config/personald/config.toml`；状态/日志：`~/.local/state/personald/`。Windows/macOS 使用各自标准目录。
- SQLite 启用 WAL、foreign keys 和事务；schema 通过版本化 migration 管理。
- Scheduler 使用 Rust async timer/event loop，不引入大型调度框架。
- Slint 只负责 Capture、Attention、Morning、Evening；Search/Timeline、Settings 后置。

## 6. 安全与隐私

API key 从系统配置/环境读取，不进入事件表和日志。发送给 provider 的文本应经过可配置脱敏；所有外发调用记录 provider/model/prompt version。默认不自动采集其他应用内容。

## 7. 可观测性与测试

必须覆盖：快捷键误触发、SQLite 原子提交、队列重启恢复、AI 非法输出、规则幂等、低置信度 clarification、Morning/Evening 重算和全量 reprocess。日志不得打印 raw_text，除非显式 debug 配置。
