# FR-PES-001: 个人事件记录与状态管理

**版本**: v1.0  
**最后更新**: 2026-09-11  
**变更类型**: FEATURE  
**状态**: Draft  
**来源**: `personal-event-system-design-v0.1.md`

## 1. 产品目标

Personal Event System（`personald`）是一个本地优先、长期常驻的个人事务记录与状态管理系统。用户可以快速记录“发生了什么”，也可以在连续对话中补充上下文、查询历史和推进事项；系统异步完成事件理解、实体/项目关联、规则匹配、提醒和每日回顾。

核心闭环：

```text
快速录入或打开对话 → 自然语言输入 → 本地提交 → 后台分析 → 规则/任务/提醒 → Morning/Evening 回顾
```

## 2. 目标用户与范围

### 2.1 目标用户

需要低摩擦记录工作、项目、沟通、决定和个人事务，并希望系统持续发现待办与风险的单用户。

### 2.2 MVP 范围

- Linux 桌面端单用户。
- 全局快捷键唤起 Capture 窗口。
- 独立的连续对话界面，支持类似 IM 的多轮消息、上下文和历史会话。
- SQLite 保存不可变 Raw Event。
- 后台 AI 分析、实体/项目识别和置信度输出。
- 确定性 Rule Engine 生成 Task、Evidence Requirement 和 Attention。
- 每日 Morning、Evening 页面与系统通知。
- AI 失败重试及历史事件重新分析。

### 2.3 非目标

移动端、云同步、多用户协作、账号体系、复杂 Dashboard、自动读取邮件/微信/屏幕/录音、RAG/向量库、本地大模型和自动外部操作。

## 3. 功能需求

### FR-PES-001-01 Capture

1. 默认快捷键为双击 Left Ctrl；间隔默认 `80ms <= interval <= 300ms`，必须可配置。
2. 仅同一配置键完成两次独立 press/release 且无其它修饰键参与时触发，不能误触发 `Ctrl+C` 等组合键。
3. Capture 窗口按需创建，获得焦点后可立即输入；Enter 提交、Esc/点击外部取消。
4. 输入为空或仅空白时不得创建事件。
5. 只有 SQLite 事务提交成功才向用户视为“记录成功”；提交后立即关闭窗口。
6. Capture 同步路径不得等待网络或 AI。

### FR-PES-001-01B 连续对话

1. 系统必须提供类似 IM 的对话界面，用户可在同一会话中连续发送多条消息。
2. 会话消息按顺序持久化，至少记录 `conversation_id`、`message_id`、角色、文本、时间和处理状态。
3. 对话必须保留当前会话上下文；用户无需重复说明刚刚提到的人、项目和事件。
4. 用户可新建会话、查看历史会话、切换会话和继续未完成会话。
5. 用户消息中明确描述的事实或事件可生成 Raw Event，并保留消息与事件的来源关联；普通问答不应强制生成事件。
6. 助手回复失败、超时或 AI 不可用时，用户消息仍须本地保存，并显示可重试状态。
7. 对话界面不得阻塞快速 Capture；两者共享事件、分析和记忆数据，但入口和交互状态相互独立。

### FR-PES-001-02 Raw Event

每条事件必须保存 `id`、`raw_text`、`occurred_at`、`recorded_at`、`source`、`status`、`created_at`、`updated_at`。`occurred_at`（事情发生时间）、`recorded_at`（记录时间）和 `processed_at`（分析时间）必须分开。Raw Event 永久保留，AI 不得覆盖。

### FR-PES-001-03 AI Analysis

事件提交后进入可恢复队列。AI 输出必须是结构化 Analysis Result，至少包含 event type/subtype、facts、entities、候选 state changes、confidence、clarifications。Core 负责校验和应用，AI 不得直接写业务状态。

AI 不可用、超时、限流或输出非法 JSON 时，事件仍保持可用，分析任务进入 `pending/retry/failed` 并记录 provider、model、prompt version、错误和时间。

### FR-PES-001-04 Entity / Project

支持 `person`、`organization`、`project`、`place`、`product` 实体及别名。实体解析低置信度时不得自动合并。Project 状态至少支持 `unknown/planned/negotiating/started/active/blocked/paused/completed/cancelled`。

### FR-PES-001-05 Rule / Task / Attention

- Rule 分为 `system`、`personal`、`suggested`；Suggested Rule 必须经用户确认后生效。
- Rule Engine 根据已确认的结构化事实匹配规则，不由 AI 直接决定提醒。
- Task 必须可追溯到用户事件或规则，状态为 `open/in_progress/completed/dismissed`。
- Attention 表示“应该注意”，Task 表示“需要执行”；一个 Attention 可生成多个 Task。
- Attention 等级为 `info/suggestion/warning/critical`；仅 `warning/critical` 可主动弹窗，状态为 `open/snoozed/resolved/dismissed`。
- 影响项目状态、合同/付款等高风险推断，置信度不足时必须生成 Clarification，不得自动变更。

### FR-PES-001-06 Daily Views

Morning 默认 08:00，聚合开放 Task、Attention、活动项目、即将发生事件、逾期项和近期决定；Evening 默认 21:30，总结当日事件、完成项、项目变化、未完成项和可确认的规则建议。两者均为动态视图，不是独立事实表。

### FR-PES-001-07 Reprocess

必须支持按事件和全量重新分析。重新分析只替换 derived data（Analysis、实体解析、分类、规则结果），不得修改 Raw Event。

## 4. 验收场景

| 场景 | 输入/条件 | 预期 |
|---|---|---|
| 普通记录 | “把 F429 的 USART 改完了” | 本地保存，识别为 work，不弹窗 |
| 外部项目 | “决定开始合作” | 识别项目启动，规则生成 Attention 与确认任务 |
| 不确定状态 | “项目可以开始搞了” | 生成 Clarification，不直接设为 started |
| AI 不可用 | 网络断开后记录 | Raw Event 保存，任务可恢复重试 |
| 快速录入 | 在其它应用中双击 Left Ctrl，输入事件并回车 | Capture 立即提交并关闭，生成独立 Raw Event，不等待 AI |
| 连续对话 | 在同一会话连续发送“刚才提到的项目”“把它记成待办” | 保留上下文，消息持久化，必要时生成可追溯 Event/Task |
| 对话失败 | 助手请求超时 | 用户消息保留，显示可重试状态，不影响后续对话 |
| Morning | 到达 08:00 | 展示开放风险、任务和活动项目 |
| Evening | 到达 21:30 | 展示当日回顾，不要求填写日报 |

## 5. 非功能需求

- 本地优先：无网络时 Capture 仍可用。
- 轻量：无任务时不持续轮询、不持续网络请求；不常驻 Chromium/Electron/WebView/Node/GTK。
- 可恢复：崩溃后 Raw Event 不丢失，pending job 和 scheduler 状态可恢复。
- 可升级：Provider、Prompt、Rule 独立版本化。
- 隐私：原始文本和凭据默认只存本机；外发 AI 前需有明确配置边界。
