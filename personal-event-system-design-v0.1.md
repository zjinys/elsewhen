# Personal Event System — 产品与技术设计文档

> 文档版本：v0.1  
> 状态：Draft  
> 用途：作为 Codex / Claude Code 等 AI Coding Agent 的产品与实现基准  
> 首要平台：Linux  
> 目标平台：Linux / Windows / macOS

---

## 1. 产品定位

这是一个**极轻量、长期常驻、AI 驱动的个人事务记录与状态管理系统**。

它不是传统日记、Todo，也不是以聊天为中心的 AI 助手。

核心理念：

> 用户只负责记录“发生了什么”，AI 负责理解、整理、关联、发现风险、执行规则、生成提醒和每日回顾。

用户不应该为了记录而填写复杂表单、选择分类、维护标签。

### 核心循环

```text
想到一件事
    ↓
双击快捷键
    ↓
输入自然语言
    ↓
Enter
    ↓
本地立即保存
    ↓
Capture 窗口立即关闭
    ↓
AI 后台异步分析
    ↓
事件理解 / 关联 / 规则匹配
    ↓
必要时产生 Attention
    ↓
早晨生成 Today
    ↓
晚上生成 Today Review
```

---

# 2. 产品原则

## 2.1 Capture First

记录必须比整理容易。

用户只需要输入：

> 今天和 XX 沟通了 XX 项目的需求，对方说下周开始。

不要求用户填写：

- 类型
- 分类
- 项目
- 人物
- 标签
- 优先级

这些由 AI 后台推断。

---

## 2.2 原始记录永远是事实

用户输入的原始文本必须永久保存，不允许 AI 覆盖。

例如：

```text
RAW EVENT

2026-09-11 09:13

和老王聊了一下，他说那个项目下个月开始，
价格还没谈。
```

AI 后续可以产生：

```text
Structured Event

type: external_project
person: 老王
status: negotiation
project: unknown
```

但 Structured Event 属于 derived data。

原始记录永远是事实来源。

这样未来可以：

- 更换 AI 模型
- 修改 Prompt
- 修改分类规则
- 重新分析历史数据
- 使用本地模型

而不会丢失原始信息。

---

## 2.3 AI 不应该过度打扰

系统输出分三级：

### Normal

只记录，不打扰。

### Suggestion

AI 认为可能值得关注，但不需要立即打断用户。

### Hard Attention

命中明确规则，必须提醒。

例如：

```text
⚠ 需要注意

检测到一个新的对外合作项目。

当前尚未发现：
□ 书面需求确认
□ 合作条件确认
□ 费用 / 付款确认

建议在正式投入工作前完成确认。
```

只有 Hard Attention 才允许主动弹窗。

---

## 2.4 AI 不确定时不能自作主张

例如：

> 老王说那个项目可以开始搞了。

不能直接认定：

```text
project.status = started
```

如果置信度不足，应产生 Clarification：

```text
我理解为“XX 项目已经正式开始”，是否正确？

[是] [不是]
```

用户确认后再更新状态。

---

## 2.5 AI 不可用不能影响记录

网络断开、API 错误、模型不可用时：

```text
快捷键
→ 输入
→ 本地保存
→ 窗口关闭
```

必须正常完成。

AI 分析进入待处理队列，之后重试。

---

# 3. 产品形态

系统由一个极轻量的 Rust 常驻进程承担核心工作。

建议 executable：

```text
personald
```

后台不应该常驻：

- Chromium
- Electron
- WebView
- Node.js runtime
- GTK

UI 仅在需要时创建。

---

# 4. 技术方向

## 4.1 Core

使用 Rust。

负责：

- Global Hotkey
- Event Capture
- SQLite
- Event Queue
- AI Client
- Rule Engine
- Scheduler
- Notification
- Task / Attention 状态
- 系统生命周期

---

## 4.2 UI

第一版优先考虑 **Slint**。

原因：

- Rust 原生
- 跨平台
- 不依赖浏览器
- 不需要 Chromium
- 适合少量、定制化窗口

第一阶段不要引入：

- React
- Vue
- Tauri
- Electron
- GTK

UI 类型只有：

1. Capture
2. Attention
3. Morning
4. Evening
5. Settings（后期）
6. Search / Timeline（后期）

---

# 5. 后台进程模型

第一版原则上只需要一个常驻进程：

```text
personald
│
├── hotkey listener
├── scheduler
├── sqlite
├── event queue
├── AI worker
├── rule engine
└── notification
```

UI 按需创建。

正常状态：

```text
personald
    │
    ├── 监听快捷键
    ├── 等待任务
    └── CPU 接近空闲
```

不应该为了 UI 保持浏览器或大型 GUI Runtime 常驻。

---

# 6. 全局快捷键

## 默认设计

默认触发方式：

> **双击 Left Ctrl**

示例：

```text
Ctrl down
Ctrl up

Ctrl down
Ctrl up
```

在规定时间窗口内完成，即触发 Capture。

---

## 6.1 配置化

快捷键系统必须抽象，不要把 Ctrl 写死。

建议支持：

```text
Double Left Ctrl
Double Right Ctrl
Double Left Shift
Double Right Shift
Double Left Alt
Double Right Alt
Double CapsLock
```

后续可以扩展 F13/F14 等键。

---

## 6.2 双击判定

建议默认：

```text
min_interval = 80ms
max_interval = 300ms
```

即两次完整 key press 的间隔：

```text
80ms <= interval <= 300ms
```

超过则视为普通按键。

参数应可配置。

---

## 6.3 防误触

必须避免：

```text
Ctrl+C
Ctrl+V
Ctrl+...
```

触发 Capture。

只有检测到同一个配置键独立完成两次 press/release，且没有其它修饰键参与，才触发。

---

# 7. Capture UI

Capture 是整个产品最重要的 UI。

目标：

> **像 Alfred 一样快。**

建议：

- 无传统标题栏
- 居中
- 获得焦点后立即可输入
- 单输入框
- Enter 提交
- Esc 取消
- 点击外部可取消
- 窗口尽快显示
- 提交后立即关闭

示意：

```text
┌──────────────────────────────────────────────┐
│                                              │
│ 和 XX 沟通了 XX 项目的需求，准备开始合作……   │
│                                              │
└──────────────────────────────────────────────┘
```

---

# 8. Capture 生命周期

```text
Hotkey
  ↓
Show Capture
  ↓
User Input
  ↓
Enter
  ↓
Validate non-empty
  ↓
Create Raw Event
  ↓
SQLite transaction commit
  ↓
Close Capture
  ↓
Return control to user
```

关键要求：

> **SQLite commit 成功后才认为记录成功。**

AI 分析不属于 Capture 的同步路径。

---

# 9. Event 数据模型

第一版核心对象：

```text
Event
Entity
Project
Task
Rule
Evidence
Attention
Decision
AI Run
```

---

# 10. Event

Event 是系统最基础的对象。

建议字段：

```text
Event
------------------------------
id
occurred_at
recorded_at
processed_at
raw_text
source
status
event_type
confidence
created_at
updated_at
```

### 三个时间必须分开

```text
occurred_at
```

事情实际发生时间。

```text
recorded_at
```

用户记录时间。

```text
processed_at
```

AI 处理时间。

例如：

> 昨天和客户聊了一个项目。

那么：

```text
occurred_at = yesterday
recorded_at = today
processed_at = today
```

---

# 11. Event Type

第一版不要设计成无限枚举。

建议：

```text
unknown
work
personal
project
communication
decision
purchase
financial
travel
family
idea
task
meeting
milestone
```

AI 可以输出更细的 subtype，但核心 type 保持稳定。

例如：

```text
type = project
subtype = external_collaboration
```

---

# 12. Entity

Entity 用于表示长期存在的人、组织、项目、地点等。

第一版：

```text
person
organization
project
place
product
```

建议字段：

```text
Entity
----------------
id
type
name
aliases
metadata
created_at
updated_at
```

---

# 13. Event 与 Entity

一个 Event 可以关联多个 Entity。

例如：

> 今天和老王沟通 XX 项目。

关系：

```text
Event
 ├── Person: 老王
 └── Project: XX 项目
```

不要把人物名称永久写死在 Event JSON 中。

应该有 Entity Resolution：

```text
"老王"
"王总"
"王XX"
```

如果 AI 判断可能是同一个人，但置信度不足，不要自动合并。

---

# 14. Project

Project 是一个长期事务容器。

状态建议：

```text
unknown
planned
negotiating
started
active
blocked
paused
completed
cancelled
```

Project 可以关联：

```text
Events
Tasks
Evidence
People
Rules
Decisions
```

---

# 15. Task

Task 是明确需要执行的事情。

例如：

```text
获取 XX 对项目需求的书面确认
```

字段：

```text
id
title
description
status
priority
due_at
source_event_id
project_id
rule_id
created_at
completed_at
```

状态：

```text
open
in_progress
completed
dismissed
```

重要原则：

> AI 可以创建 Task，但 Task 必须有来源。

例如：

```text
source = rule
source_rule_id = external-project-start
```

或者：

```text
source = user
source_event_id = ...
```

---

# 16. Evidence

Evidence 是本系统非常重要的概念。

它表示：

> 支撑某个事务事实的外部证据。

例如：

```text
微信聊天截图
邮件
PDF
合同
报价单
对方确认消息
文件
```

第一版可以先只保存 metadata，不必实现完整附件系统。

建议：

```text
Evidence
----------------
id
type
title
uri
hash
created_at
event_id
```

后续可以支持：

```text
file
image
email
message
url
document
```

---

# 17. Rule

Rule 是系统区别于普通日记软件的核心。

Rule 不应该只是 Prompt，而应该是结构化规则。

例如：

```yaml
id: external-project-start

name: 对外合作项目启动

trigger:
  event:
    type: project
    subtype: external_collaboration
    status: started

actions:
  - type: require_evidence
    title: 获取书面需求确认

  - type: create_task
    title: 明确合作条件

  - type: create_task
    title: 明确费用及付款方式
```

---

# 18. Rule 三个等级

### System Rule

系统内置规则。

### Personal Rule

用户自己定义的规则。

### Suggested Rule

AI 根据历史发现提出，但必须经过用户确认后才能成为正式 Rule。

例如：

```text
过去 3 次项目中，你都在合作条件确认前开始执行。

是否建立规则：

“所有对外合作项目，在正式执行前必须完成书面合作条件确认。”

[建立规则]
[忽略]
```

---

# 19. Attention

Attention 表示：

> AI 认为用户现在应该注意某件事情。

等级：

```text
info
suggestion
warning
critical
```

只有：

```text
warning
critical
```

才可能主动弹出窗口。

建议字段：

```text
id
level
title
message
event_id
rule_id
status
created_at
resolved_at
```

状态：

```text
open
snoozed
resolved
dismissed
```

---

# 20. Attention 与 Task 的区别

非常重要。

### Attention

> 你应该注意这件事情。

### Task

> 你需要执行这个动作。

例如：

```text
Attention:
XX 项目已经开始，但尚未确认合作条件。

Tasks:
□ 获取书面需求确认
□ 确认费用
□ 确认交付标准
```

一个 Attention 可以产生多个 Task。

---

# 21. AI 分析流程

每一个 Event 保存后：

```text
Raw Event
    ↓
AI Analysis
    ↓
Intent / Event Type
    ↓
Entity Extraction
    ↓
Entity Resolution
    ↓
Project Resolution
    ↓
State Detection
    ↓
Rule Evaluation
    ↓
Task / Attention
```

---

# 22. AI 输出不要直接修改数据库

AI 应输出一个结构化 Analysis Result。

示意：

```json
{
  "event_type": "project",
  "subtype": "external_collaboration",
  "confidence": 0.94,

  "facts": [
    {
      "text": "双方已经沟通项目需求",
      "confidence": 0.96
    },
    {
      "text": "项目准备启动",
      "confidence": 0.82
    }
  ],

  "entities": [
    {
      "type": "person",
      "name": "XX",
      "confidence": 0.98
    },
    {
      "type": "project",
      "name": "XX项目",
      "confidence": 0.91
    }
  ],

  "state_changes": [
    {
      "target": "project",
      "state": "started",
      "confidence": 0.82
    }
  ],

  "suggestions": [],
  "clarifications": []
}
```

Core 根据结果决定是否真正写入。

---

# 23. AI 与 Rule Engine 必须分离

不要让 AI 自己决定：

> “这个事情需要签合同。”

正确流程：

```text
AI
 ↓
识别：
external_collaboration
project_started
 ↓
Rule Engine
 ↓
匹配：
external-project-start
 ↓
生成：
Task
Attention
Evidence Requirement
```

这样规则才是确定性的。

---

# 24. AI 置信度

建议所有重要推断都有 confidence。

例如：

```text
0.95+
```

可以自动执行低风险结构化。

```text
0.80 - 0.95
```

可以进入建议状态。

```text
< 0.80
```

对于影响 Project 状态、Rule 触发等重要操作，应考虑要求用户确认。

具体阈值在实现阶段通过测试调整，不要作为永久产品事实。

---

# 25. Morning

每天早晨自动生成一个页面。

建议默认时间：

```text
08:00
```

时间可配置。

Morning 不应该是普通 Todo List。

它回答：

> **今天什么最值得关注？**

内容来源：

```text
Open Tasks
Open Attention
Active Projects
Upcoming Events
Recent Decisions
Overdue Tasks
AI Insights
```

示意：

```text
Good morning

今天值得关注

🔴 XX 项目
合作已经开始，但尚未完成书面需求确认。

[处理]

🟡 XX 客户
报价发送 3 天，尚未收到回复。

[跟进]

📌 进行中
F429 项目
当前阶段：USART 调整
```

---

# 26. Evening

每天晚上自动出现。

建议默认时间：

```text
21:30
```

时间可配置。

它不是要求用户写日报。

它自动总结：

```text
今天发生了什么？
哪些事情完成了？
哪些项目发生变化？
哪些事情仍未完成？
AI 发现了什么？
```

示意：

```text
Good evening

今天记录了 18 件事情。

工作
✓ F429 USART 调整
✓ XX 项目需求沟通

项目变化
XX 项目
需求沟通 → 准备启动

未完成
⚠ 书面需求确认

AI 发现
最近 3 个项目都出现了：
“先执行，后确认合作条件”。

是否建立长期规则？
```

用户可以直接关闭，不需要填写内容。

---

# 27. Morning / Evening 不是独立数据

页面内容必须动态生成。

不要保存：

```text
Morning Page
Evening Page
```

作为唯一事实。

应该根据：

```text
Events
Projects
Tasks
Attention
Rules
Decisions
```

动态生成。

这样历史数据发生修正时，页面也可以重新计算。

---

# 28. Scheduler

后台 scheduler 负责：

```text
Morning schedule
Evening schedule
Task reminder
Attention reminder
AI retry
Periodic maintenance
```

第一版不要引入大型任务调度框架。

Rust 内部使用简单 async timer / event loop 即可。

---

# 29. SQLite

第一版使用 SQLite。

建议数据目录遵循各平台标准目录。

Linux 示例：

```text
~/.local/share/personald/events.db
~/.config/personald/config.toml
~/.local/state/personald/
```

不要强制所有平台都使用 Linux 路径。

---

# 30. AI Provider

第一版不要把 AI Provider 写死。

定义统一接口：

```text
AiProvider
```

例如：

```text
OpenAIProvider
AnthropicProvider
CompatibleOpenAIProvider
LocalProvider
```

第一版可以只实现一个 Provider。

但 Core 不应该依赖具体厂商。

---

# 31. AI 调用原则

AI 调用属于后台任务。

正确：

```text
save_event()
    ↓
queue_analysis()
    ↓
return_to_user()
```

错误：

```text
save_event()
    ↓
wait AI
    ↓
show result
```

---

# 32. AI 失败处理

例如：

```text
AI timeout
AI unavailable
invalid JSON
rate limit
network error
```

全部进入：

```text
analysis_status = pending / retry
```

原始 Event 不受影响。

AI Run 应记录：

```text
provider
model
prompt_version
started_at
finished_at
status
error
```

---

# 33. Prompt Versioning

Prompt 是系统的一部分，也需要版本化。

例如：

```text
event-analysis-v1
entity-resolution-v1
morning-summary-v1
evening-summary-v1
```

AI Run 记录对应版本。

未来 Prompt 升级后，可以重新分析历史数据。

---

# 34. 第一版必须支持重新分析

例如：

```text
reprocess event 18273
```

或者：

```text
reprocess all events
```

原始事件不变。

只重新生成：

```text
AI Analysis
Entities
Classification
Rule evaluation
```

---

# 35. MVP

不要一次实现全部功能。

## MVP-1：Capture

必须完成：

- Rust daemon
- Linux
- Double Ctrl
- Capture Window
- SQLite
- Raw Event
- Enter 保存
- Esc 关闭

验收标准：

> 用户可以在任何应用中双击 Ctrl，输入一句话，Enter 后事件被可靠保存。

---

## MVP-2：AI Analysis

增加：

- AI Provider
- Event Classification
- Entity Extraction
- Project Detection
- Confidence
- Analysis Queue

验收：

> 输入自然语言后，后台可以产生结构化 Event Analysis。

---

## MVP-3：Rule

增加：

- Rule schema
- Rule matching
- Task
- Attention
- Hard Attention popup

验收：

> 输入“和 XX 开始合作项目”，能够识别外部合作，并触发预定义规则。

---

## MVP-4：Morning / Evening

增加：

- Scheduler
- Morning Page
- Evening Page
- Notification
- 页面生成

验收：

> 每天早晨自动看到今天应该关注的事情，晚上自动看到今天发生的事情。

---

# 36. 第一版明确不做

为了避免项目失控，MVP 不做：

- 移动端
- 云同步
- 多用户
- 团队协作
- 在线账号系统
- 社交
- 复杂 Dashboard
- 完整知识库
- 自动读取所有系统数据
- 自动读取邮件
- 自动读取微信
- 自动截图
- 自动录音
- 本地大模型
- RAG
- 向量数据库

第一阶段只证明：

> **“随手记录 → AI 理解 → Rule → Attention → 每日回顾”这个闭环是否真的有价值。**

---

# 37. 非功能要求

## NFR-001 轻量

后台不得依赖：

- Electron
- Chromium
- WebView
- Node.js
- GTK

---

## NFR-002 低资源

无任务时：

- CPU 应接近空闲
- 不持续轮询
- 不持续网络请求
- 不持续唤醒 AI

---

## NFR-003 本地优先

事件必须先本地持久化。

AI 是增强能力，不是数据存储依赖。

---

## NFR-004 数据可靠

Capture 提交成功的定义：

```text
SQLite transaction committed
```

---

## NFR-005 可恢复

程序崩溃后：

- Raw Event 不丢失
- pending AI job 可恢复
- scheduler 状态可恢复

---

## NFR-006 可升级

AI Prompt、Provider、Rule 都应该可以独立升级。

---

# 38. 推荐项目结构

建议初始：

```text
personald/
├── Cargo.toml
├── README.md
├── docs/
│   ├── product.md
│   ├── architecture.md
│   ├── event-model.md
│   ├── rule-engine.md
│   └── ai/
│       ├── event-analysis.md
│       ├── entity-resolution.md
│       ├── morning.md
│       └── evening.md
│
├── src/
│   ├── main.rs
│   ├── config/
│   ├── core/
│   │   ├── event.rs
│   │   ├── entity.rs
│   │   ├── project.rs
│   │   ├── task.rs
│   │   ├── rule.rs
│   │   ├── evidence.rs
│   │   └── attention.rs
│   │
│   ├── storage/
│   ├── hotkey/
│   ├── scheduler/
│   ├── ai/
│   ├── rules/
│   ├── notifications/
│   └── ui/
│       ├── capture/
│       ├── attention/
│       ├── morning/
│       └── evening/
│
└── migrations/
```

实际 crate / module 划分可以根据 Rust 生态和平台实现调整。

---

# 39. 第一版验收场景

## Scenario 1：普通记录

输入：

> 今天上午把 F429 的 USART 部分改完了。

结果：

```text
Event created
type = work
```

不弹窗。

---

## Scenario 2：外部项目

输入：

> 今天和 XX 沟通了 XX 项目的需求，决定开始合作。

结果：

```text
Project detected
External collaboration detected
Project status = started
```

触发：

```text
Attention
```

并创建：

```text
Task:
获取书面需求确认

Task:
明确合作条件
```

---

## Scenario 3：AI 不确定

输入：

> 老王说项目可以开始搞了。

AI 不能确定正式启动。

产生：

```text
Clarification
```

而不是直接修改 Project 状态。

---

## Scenario 4：AI 服务不可用

输入：

> 今天买了一台显示器。

结果：

```text
Raw Event saved
```

AI 失败。

用户仍然可以继续工作。

恢复网络后：

```text
pending event
→ retry
→ analysis
```

---

## Scenario 5：Morning

早晨自动显示：

```text
XX 项目
书面需求确认仍未完成

XX 客户
等待回复 3 天

F429
正在进行
```

---

## Scenario 6：Evening

晚上自动显示：

```text
今天记录 12 件事情

完成：
...

推进：
...

未完成：
...

AI 发现：
...
```

---

# 40. 第一阶段真正要验证的事情

这个系统成功与否，不取决于：

- UI 有多漂亮
- AI 模型多强
- 功能有多少

第一阶段真正要验证的是：

> **用户是否愿意每天持续按一下快捷键，把事情告诉系统。**

如果：

```text
双击 Ctrl
→ 说一句
→ Enter
```

足够自然，用户每天会产生大量高价值原始数据。

一旦原始数据积累起来：

```text
Event
    ↓
Project
    ↓
Relationship
    ↓
Decision
    ↓
Rule
    ↓
Attention
    ↓
Personal Knowledge
```

这个系统才会逐渐产生长期价值。

因此第一版必须极度克制。

**先把 Capture 做到“没有任何心理负担”，再让 AI 慢慢变聪明。**

---

# 41. 给 Coding Agent 的实现约束

实现时请遵循以下原则：

1. 先实现本地事件记录闭环，再实现 AI。
2. 不为了未来扩展提前引入大型框架。
3. 所有关键业务状态都必须可持久化、可恢复。
4. Raw Event 不允许被 AI 覆盖。
5. AI 输出必须经过 Core / Rule Engine 决定后才能修改业务状态。
6. UI 与业务逻辑解耦。
7. Linux 优先，但不要写死 Linux 特有的数据模型。
8. 跨平台差异放在 platform adapter 层。
9. 不要为了实现页面而引入 Web 技术栈。
10. 每个阶段完成后提供可运行的最小版本，不要一次性实现所有功能。

---

# 42. 建议的实现顺序

```text
Phase 1
Linux daemon
+
Double Ctrl
+
Capture
+
SQLite

↓

Phase 2
Event Queue
+
AI Provider
+
Structured Analysis

↓

Phase 3
Entity / Project

↓

Phase 4
Rule Engine
+
Task
+
Attention

↓

Phase 5
Morning

↓

Phase 6
Evening

↓

Phase 7
Search / Timeline

↓

Phase 8
Windows / macOS
```

核心原则：

> **每个 Phase 都必须能够独立运行和验证。**
