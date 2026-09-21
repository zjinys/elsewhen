# Elsewhen 个人认知主循环 Roadmap

## 目标

把现有的事件、对话、Wiki、待办和内容导入能力连接成一个每天可使用、可验证的闭环：

```text
随手记录 → AI 理解 → 人物 / 项目 / 认识沉淀 → 每日回顾 / 后续行动
保存内容 → 阅读理解 → 观点 / 灵感 / 创作素材 → 采用或归档
```

本路线图接管 [旧版 Personal Event System Roadmap](2026-09-11-personal-event-system-roadmap.md) 的产品实施顺序。旧路线图继续作为早期范围和架构约束记录；其中原始事件不可变、AI 失败不阻塞 Capture、派生结果可重算等门禁继续有效。

对应架构决策见 [以个人认知主循环重组 Elsewhen](../notes/proposed/architecture/2026-09-17-personal-cognition-main-loop.md)。

## 当前基线

已经具备：

- SQLite 不可变事件和可恢复分析队列；
- Flutter 对话、Capture、知识库、待办和设置界面；
- OpenAI-compatible Provider、对话工具调用与确认后写入；
- Wiki 页面、修订、人物关系、来源 URL、标签和页内 AI 对话；
- 任意网页 / X 内容 / 文本导入；
- 派生产物的 `area / based_on / content_type` 在当前工作区开发中。

主要缺口：

- 对话消息与个人事件没有统一进入每日记录流；
- Flutter `trigger_analysis` 尚未真正处理分析任务；
- 没有“今天”和每日总结视图；
- 项目、人物主要是静态知识页，缺少带时间的事实与状态变化；
- 外部素材缺少完整的阅读、加工、采用状态；
- Bridge 集成测试会接触实际数据目录，需要先隔离。

## 执行状态

更新规则：每完成一个可独立验证的步骤，立即在对应 Phase 勾选，并在下方完成记录中追加验证证据。

- **当前阶段：** Phase 2.5 — 事件账本质量治理收尾
- **整体状态：** 进行中
- **下一步骤：** Phase 4C — 决策辅助第一版

## 整合执行路线（2026-09-20）

原有 Phase 编号描述产品能力层级；本节把 `todo.md` 中的方案按依赖重新排列，作为实际开发顺序。原则是先保证进入账本的数据可靠，再建立结构化用户模型，最后让模型参与决策辅助；Today UI 暂不提前上线。

### 已完成基线

- **Phase 0：安全与可观测性。** Bridge 测试隔离、分析队列状态、数据安全门禁完成。
- **Phase 1：统一输入。** Capture、主输入、对话和 URL 路由统一进入可追溯输入链路，幂等和确认边界完成。
- **Phase 2：分析闭环。** 非阻塞分析 worker、重试、重启恢复、统一 `event-analysis` 契约、详情 API 和 Bridge 验证完成。
- **Phase 3 数据层：每日概览契约。** `get_daily_overview`、`daily_reviews`、来源引用、显式生成和当天待办聚合完成；Today 页面暂缓。

### Step 1：Phase 2.5 — 事件账本质量治理

对应：`docs/notes/proposed/architecture/2026-09-19-event-recordability-filter.md`

- 对话层先判断是否值得记录；评价 AI、闲聊、元对话默认不写 `events`。
- **已完成第一步：** system prompt 与 `record_event` 工具描述已明确上述边界，用户显式要求记录时仍按意图执行。
- **已完成第二步：** 统一 `event-analysis` 契约保留 `recordable` 与 `kind`，旧 v1/v2 仅兼容读取；分析 prompt 区分 event / discussion / chitchat / meta，并明确 `projects` 只收录长期项目/产品/组织，不把动作或流程当项目。
- **已完成第三步：** 日流、每日概览和每日回顾生成输入均过滤最新分析中的 `recordable=false`；无分析或旧 v1 结果保持可记录，原始事件不删除。
- **已完成第四步：** migration v28 增加追加式人工记录性决策；消息时间线显示“记录 / 讨论 / 分析中”状态菜单，可纳入记录、作为讨论或重新分析。讨论决策优先于 AI 结果，从日流和每日回顾中撤出，并清理该事件产生的派生事实、关系和待确认动作；原始事件、消息和历史分析保留。隔离 Bridge 测试已覆盖真实往返。
- 明确记录时放宽判断，保留用户主动记录意图。
- 分析契约升级为兼容的 v2，增加 `recordable` 与 `kind`，为存量事件和 UI 过滤提供确定字段。
- 误记内容不物理删除：保留在 conversation，事件侧通过派生状态从时间线撤出。
- 复用现有分析队列做存量重新分析。

当前边界：默认写入路径仍先保存不可变原始事件、再异步分类，以保证 Capture 不等待 AI；因此“写入前 AI 判断”不是默认门禁。当前可验证闭环是分析后过滤、人工纠正和存量重新分析。若未来要求写入前门禁，必须新增显式严格模式并单独评估对即时记录体验的影响。

退出条件（当前第一版）：用户可在消息上下文中区分事件/讨论、人工纠正并立即影响日流和长期事实；原始事件保持不可变；存量事件可重新分析；Bridge/Flutter 可区分事件与讨论。决策见 [事件可记录性纠正闭环](../notes/implemented/feature/2026-09-21-recordability-correction-loop.md)。

### Step 2：Phase 4A — 结构化用户模型地基（已完成）

对应：`docs/notes/proposed/architecture/2026-09-19-decision-support-loop.md` 中“剩余四件硬活”的前两项。

- 以现有 `relations`、network Wiki 页面和事件分析结果为基础，增加统一实体事实表达：`person / project / topic`。
- **已完成第一步：** migration v21 新增 `entity_facts`，事实必须携带实体类型、slug、文本、发生时间、置信度和来源事件；重复的同实体/事实/来源幂等合并并提升置信度。
- **已完成第二步：** Rust `list_entity_facts` API 与 Flutter Bridge 已生成，后续实体详情可直接读取时间线。
- 每条事实包含来源事件、发生时间、置信度和当前状态；原始事件永不覆盖。
- 新事件只做增量维护，更新 `last_seen` / confidence，不重复制造关系。
- 建立名称规范化、slug 和别名表，为人物 / 项目消歧做准备。

已完成：统一 `event-analysis`（旧 v1/v2 兼容读取）、`people/projects/activities` 语义分离；普通分析候选确认卡片；`@/#` 确定性识别；确认后人物/项目事实写入与真实事件回溯；同名实体阻止静默合并并支持候选卡片选择明确页面 slug；隔离 Rust 测试、Bridge 重新生成和 Flutter analyze 通过。提交包括 `6b47654`、`07dbfe3`、`1ed0ae9`、`c1684c1`。

退出条件：同一实体可以查询最近事实；重复分析幂等；所有事实可回到来源事件；旧 Wiki 页面继续可读。

### Step 3：基础设施并行项 — 归档对话清理（已完成）

对应：`docs/notes/proposed/architecture/2026-09-19-archived-conversation-deletion.md`

- 只允许删除已归档、非知识页会话。
- UI 二次确认；AI 指令走 `WriteConfirm`。
- 删除与 messages、pending actions 级联，禁止留下悬挂引用。

已完成：归档列表中的普通对话提供永久删除入口，UI 二次确认；后端拒绝删除未归档对话和知识页专用会话。提交 `aabf15b`，Flutter analyze 与 Rust/Bridge 构建通过。

该项不属于 Phase 4 业务模型，但应在事件过滤后完成，保证讨论内容可收纳、可清理。

### Step 4：Phase 4B — 实体确认与来源闭环

当前进度：已完成。候选确认与忽略、别名匹配和同名消歧、冲突事实提示与人工纠正、安全合并/撤销拆分、来源回溯和关联待办均已形成应用内闭环。

- 所有 AI 实体候选均必须确认；`@/#` 只提供确定性候选线索，不自动写入长期记忆。
- 人工确认、忽略、合并、拆分都保存为显式操作，不修改历史事件。
- 人物 / 项目详情展示事实时间线、来源、最近动态和关联待办。
- 将事件分析、关系提议和 Wiki network 统一到同一实体模型。

退出条件：实体消歧可解释；人工纠正后引用一致；候选不会越权写入长期记忆。

完成证据：migration v24-v27 增加合并审计、逐行快照和旧草案兼容迁移；Rust/API/Bridge 提供合并状态、合并和安全撤销；知识页仅允许选择同类型真实目标，旧页可查看目标并撤销，事实、别名、关系和待办引用即时刷新。冲突事实保留各自来源并允许人工删除派生事实。隔离测试覆盖合并、去重、撤销、再次合并、后续修改原子拒绝、旧 schema 迁移和原始事件保留；`cargo test --all-targets`（lib 127 + bin 114）、Flutter 39 项测试、Bridge 往返及 `git diff --check` 通过，Flutter analyze 无 error/warning（保留 36 条 info）。决策见 [安全实体合并与拆分](../notes/implemented/feature/2026-09-21-safe-entity-merge-and-split.md)。

### Step 5：Phase 4C — 决策辅助第一版

对应：`decision-support-loop.md` 的规则触发、上下文引用和轻量闭环度量。

- 新事件分析时按需检索相关人物、项目、规则和历史事实。
- 先输出“关联事实 / 可复用经验 / 可能后续行动”，不直接执行高影响写入。
- 明确行动仍进入待办并保留来源；提醒按高置信即时、低置信回顾分级。
- 记录用户接受、忽略或改写建议，为后续优化提供数据。

### Step 6：Phase 3 UI — Today 工作台（后置）

对应：`docs/notes/proposed/product/2026-09-20-today-daily-review-workbench.md`。

Today 只有在日期切换、来源跳转、每日回顾版本、行动区和实体事实可用后才进入 UI。它不是 Records 或 Inbox，也不新增重复的待办 Tab。具体设计和上线条件以该设计文档为准。

### Step 7：Phase 5/6 — 素材加工与主动回顾

在实体模型和来源引用稳定后，再推进素材阅读 / 派生产物、跨域检索、周回顾和低打扰主动陪伴。情绪、健康、财务、地点、身份和创作主题等维度先作为独立可见的事实类型演进，不提前建设全知画像。

### TODO 映射

| todo.md 文档 | 新路线位置 |
| --- | --- |
| 事件可记录性过滤 | Step 1：Phase 2.5 |
| 归档对话可删除 | Step 3：基础设施并行项 |
| 决策辅助闭环 | Step 2、4、5，贯穿 Phase 4～6 |
| Today 每日回顾工作台 | Step 6：Phase 3 UI 后置 |

## 实施原则

## 交付与验证节奏

后续每个功能都按可验证的纵向切片交付，不先完成一批无法在应用中操作的数据层。

- 每个切片同时包含必要的存储/API、Bridge、Flutter 入口和测试。
- 每完成一个步骤，必须能从应用界面触发并观察到结果；纯后端准备工作不能单独宣称完成。
- 如果完整 UI 仍需后续迭代，先提供最小可用入口，但不得提供没有真实数据闭环的空壳页面。
- 每次交付都明确说明：界面新增或改变了什么、从哪里进入、如何操作、预期看到什么、哪些内容尚未实现。
- 用户验证后再进入下一个切片；验证反馈优先于继续扩展范围。
- 完成标准必须覆盖真实闭环：用户输入/触发、必要确认、持久化、界面可见和来源核验；单独的 schema、API、Bridge 或末端展示不计为功能完成。
- 如果上游尚不能产生真实数据，不继续打磨下游空状态或展示细节，优先补齐生成与确认链路。
- 内部存储类型不得自动升级为用户导航概念；`topic` 保持内部实现，不设置“议题库”或专门创建入口。

- 每个阶段保持增量兼容，不先建长期无人使用的抽象层；
- 新流程必须兼容现有数据，优先增量迁移，不重写用户原始数据；
- 保存路径永不依赖 AI 或网络；分类、分析、日结均异步执行；
- AI 结果是候选或派生数据，确定性 Core 负责校验、幂等和状态变更；
- 每个阶段同时交付 schema / Rust API / Bridge / Flutter UI / 测试与对应文档；
- 在主循环完成前，不扩展云同步、多用户、自动外部采集和复杂 Dashboard。

## Phase 0：安全基线与可观测性

**目的：** 先保证后续迭代不会污染用户数据，并能看见异步任务发生了什么。

### 范围

- [x] 所有真实 Flutter Bridge 集成测试强制使用独立临时数据目录；
- [x] 清理依赖或写入个人数据库的测试，并以完整测试前后数据库哈希与记录数不变作为隔离门禁；
- [x] 为分析任务暴露最小状态：pending / running / retry / succeeded / failed；
- [x] Flutter 展示全局或记录级后台任务状态，不要求第一阶段展示完整分析内容；
- [x] 更新过时的 README、CLAUDE 和 NEXT_STEPS，使其反映当前能力。

### 退出证据

- 全量测试前后，真实数据目录的数据库文件哈希和记录数不变；
- 可以在 Flutter 中看到待处理、失败和可重试任务数量；
- Rust、Flutter 测试全绿，Flutter analyze 无 error / warning。

### 完成记录

- **2026-09-17 — Bridge 测试数据隔离：完成。** `init_bridge(database_path)` 现在真正设置后续 API 使用的数据目录；`RustBridgeRepository` 支持初始化时传入隔离目录；真实 Bridge 测试统一通过 `test/support/isolated_bridge.dart` 创建并清理临时数据库，Wiki 与设置测试改为自种数据，不再假设个人库已有内容。验证：完整 `fvm flutter test` 36 项通过；真实数据库测试前后 SHA-256 均为 `23ae80c302b8c344cecbfaa79e1f664dd80637f5c12b4c986430b8540cf5e29c`，`events/wiki_pages/conversations` 计数均保持 `18/19/45`。
- **2026-09-17 — 分析队列状态可观测：完成。** Rust `analysis_job_stats` 稳定聚合 pending / running / retry / succeeded / failed 五种状态，Bridge 新增 `get_analysis_job_stats`，Flutter 设置页“数据”标签新增“事件分析队列”概况，显示待处理、处理中、等待重试、已完成和失败数量。验证：新增 Rust 五状态聚合测试与真实 Bridge/UI 测试；全量 Rust `103 + 90` 项、Flutter `37` 项全部通过；`flutter analyze` 无 error/warning，保留 32 条既有 info。
- **2026-09-17 — 当前文档同步：完成。** README 补充个人认知主循环、现有 Flutter 能力、分析队列入口和 Bridge 重生成/测试隔离约束；CLAUDE 更新测试、Bridge 与当前产品方向说明；NEXT_STEPS 删除已完成但仍被列为待办的早期会话/消息任务，改为指向唯一 Roadmap。验证：文档链接与 `git diff --check` 通过。
- **2026-09-17 — Phase 0 完成。** 数据隔离、分析任务状态、Flutter 最小可见反馈和开发文档均已交付；全量 Rust/Flutter 回归通过，静态检查无 error/warning。进入 Phase 1。

## Phase 1：统一记录入口与输入路由

**目的：** 用户只管表达，系统可靠保存后再判断如何处理。

### 范围

- [x] 引入输入记录或等价的关联机制，记录一次用户提交的来源、原文及其产生的 event / message / source / todo ID；
- [x] 主输入接受普通文本、网址和明确行动指令；
- [x] 普通个人记录先落为不可变事件；需要继续交流时，同时关联会话消息；
- [x] Capture 保持直接写事件，但进入同一“今日记录”查询；
- [x] URL 输入走现有导入预览流程，保存后关联原始输入；
- [x] 为重复提交、AI 重试和工具重放定义幂等键；
- [x] 明确自动记录与确认边界：事实陈述可草拟为事件，高影响业务写入仍需确认。

### 数据与 API

- 增量 migration，不改变已有 `events.raw_text`；
- 提供 `submit_input`、`list_daily_entries` 或语义等价的业务 API；
- 旧的 `record_event`、`send_message` 保持兼容，逐步改为调用统一编排层。

### 退出证据

- 从主输入提交一条经历后，可同时继续对话，并且该经历只产生一条权威事件；
- Capture、主输入和历史事件都能由同一日流查询返回；
- 同一输入重试不会重复创建事件、素材或待办；
- Provider 未配置或断网时，输入仍立即保存。

### 完成记录

- **2026-09-17 — 统一输入关联模型：完成。** migration v19 新增 `input_records`，保留用户原始提交、来源、路由状态、可选幂等键，以及 event / message / wiki page / todo 关联；该表只承担溯源关联，不复制各对象业务状态。存储层支持幂等创建和渐进补充路由结果，并校验状态词汇。验证：新增幂等、原文保留、事件关联和非法状态测试；全量 Rust `104 + 91` 项通过。
- **2026-09-17 — `submit_input` 普通文本业务 API：完成。** 存储层在单一 SQLite 事务内创建 input record、不可变 event 和 pending analysis job；相同幂等键返回原路由结果，不重复制造事件。Rust API 与 Flutter Bridge 已暴露 `submit_input`。验证：原子提交/幂等 Rust 测试通过；真实隔离 Bridge 测试确认首次统一提交增加一条事件、重复提交不增加事件。
- **2026-09-17 — 主对话输入接入：完成。** `ConversationRepository.sendMessage` 的生产路径改走 `submit_conversation_input`；单一事务同时创建用户消息、不可变 event、pending analysis job 和带双向 ID 的 input record，之后仍按原流程生成 AI 回复。知识页专用 AI 对话不经过此路径，避免把内容加工指令误记为个人事件。验证：Rust 对话统一提交测试确认一条消息对应一条事件且幂等；Flutter ConversationRepository/Enter/生成中/重试测试通过，并新增真实 Bridge 断言确认三条用户消息只产生三条事件和三个 pending job；静态检查无 error/warning。
- **2026-09-17 — Capture 与统一日流：完成。** Flutter Capture 通过 `recordUnifiedInput` 写入 input record + event，非 Rust 适配器保留 `recordEvent` 回退；新增 `list_daily_entries`，以 events 为权威全集并左连接 input records，使历史事件、Capture 和主对话输入在同一日历日查询中只出现一次。验证：Rust 日流测试覆盖旧事件、Capture、对话三种来源；真实 Bridge 测试确认三条事件全部可见，其中两条带 input 关联；Mock 适配器补齐统一输入接口后，完整 Flutter `38` 项回归通过；真实数据库测试前后 SHA-256 保持 `23ae80c302b8c344cecbfaa79e1f664dd80637f5c12b4c986430b8540cf5e29c`。
- **2026-09-17 — URL 输入路由与来源关联：完成。** 主对话输入识别独立 `http/https` 链接后，不创建对话消息或个人事件，而是先持久化 `url_import` input record，再复用既有推文/网页抓取与预览 tab；已存在素材直接关联，新增素材在用户点击保存后关联 `wiki_page_slug`，抓取失败保留原始输入并标记 failed。验证：真实隔离 Bridge 覆盖 `needs_confirmation → routed` 及无事件副作用；完整 Rust `107 + 94` 项、Flutter `38` 项通过；静态检查无 error/warning（保留 32 条 info）；真实数据库 SHA-256 保持 `23ae80c302b8c344cecbfaa79e1f664dd80637f5c12b4c986430b8540cf5e29c`。
- **2026-09-17 — 端到端提交幂等边界：完成。** Flutter 主输入为一次提交意图生成稳定 key，失败重试复用、成功后清除；普通对话与 URL 路由均把 key 传到事务写入层，避免“服务端已提交但客户端未收到响应”时重复创建 message、event、analysis job 或 input record。AI 回复重试只重跑 assistant 生成，不重写 user input；URL 保存继续按来源页和 input 关联收敛。决策与取舍记录在 [统一输入路由与幂等边界](../notes/implemented/architecture/2026-09-17-unified-input-routing-and-idempotency.md)。验证：真实 Bridge 使用同一 key 两次提交仅返回一条消息与一个分析任务；完整 Flutter `39` 项通过；`git diff --check` 通过，真实数据库 SHA-256 不变。
- **2026-09-17 — 自动记录与确认边界：完成。** Capture 与主对话个人陈述本地立即落为不可变 event；独立 URL 只创建 input record，必须在预览点击保存后才形成并关联素材；查询类工具可直接执行，新建待办、改名、归档、人物关系等业务写入继续沿用 pending action 确认门禁。统一入口只改变路由，不提升任何工具权限。验证：既有 Rust 测试覆盖 pending action 跨消息保留、确认后执行、拒绝后丢弃，以及 rename/archive/relation 工具的 confirm-gated 行为；Flutter 的发送成功/AI 失败测试确认 Provider 故障不回滚用户输入。
- **2026-09-17 — Phase 1 完成。** 普通文本、URL 和明确行动指令均可从主输入进入对应路径；Capture、对话与历史事件汇入同一日流；普通提交和 URL 提交具备端到端幂等与来源关联；保存路径不依赖 Provider，高影响写入仍需确认。进入 Phase 2。

## Phase 2：打通记录分析闭环

**目的：** 每条记录都能可靠经历“待理解 → 已理解 / 待重试”。

### 范围

- [x] 实现 Flutter `trigger_analysis`，复用现有 claim / complete / fail / retry 机制；
- [x] 保存后非阻塞唤醒后台 worker，应用重启后能继续处理积压；
- [x] 为分析结果定义稳定版本，至少提取：记录类型、简短摘要、人物、项目 / 主题、可能的后续事项；
- [x] 提供分析详情 API，暴露状态、结构化结果、来源与错误，供 Today 等来源视图按需展示；
- [x] 不设置独立 Records / Inbox 导航：分析失败由设置中的队列状态承载，明确行动进入待办，普通记录进入 Today；
- [x] 所有候选人物、项目、待办不由分析 worker 自动写入；后续只在具体上下文中按确定性合并和确认策略落库。

### 退出证据

- 新记录保存后可观察到状态从 pending 走到 succeeded；
- 模拟超时、非法 JSON、应用退出后重启，记录不丢失且任务能够重试；
- 分析版本升级可以重跑派生结果，不改变原始事件；
- 用户能从分析结果跳到对应人物、项目或待办。

### 完成记录

- **2026-09-18 — 分析队列处理器：完成。** `trigger_analysis` 读取当前激活 Provider，逐项 claim 持久化队列任务，并将合法 JSON 对象写入 `event_analyses`、标记事件为 `processed`；无 Provider 返回 `no_provider`，Provider 错误或非法 JSON 通过 `fail_analysis` 进入 retry / failed 状态，原始事件保持不变。补充了离线 StubProvider 测试，覆盖多任务成功、重复调用不重处理、非法 JSON 和 Provider 故障。验证：`cargo test`（109 个 lib 测试及全部目标通过）、`git diff --check` 通过；真实数据库 SHA-256 保持 `23ae80c302b8c344cecbfaa79e1f664dd80637f5c12b4c986430b8540cf5e29c`。本次 `./regen.sh` 因宿主 FVM 缓存只读而无法运行，Bridge 生成文件此前已与公开 API 同步，后续环境可写时需重跑确认。
- **2026-09-18 — 非阻塞分析 worker 与重启恢复：完成。** Flutter Bridge 初始化后立即唤醒串行 worker，事件、Capture 和对话统一输入保存成功后只合并唤醒信号，不等待 Provider；5 秒周期唤醒覆盖 retry 到期，pending 超过 Rust 单次 50 项上限时自动续批。Bridge 启动边界把上次进程遗留的 running 任务恢复为可立即 claim 的 retry，恢复不放在每次 `Store::open`，避免抢占当前进程仍在执行的任务。隔离 Bridge teardown 会停止周期 worker 后再删除临时库。验证：新增 running 恢复 Rust 测试；`./regen.sh` 成功并重建匹配动态库；`cargo test --all-targets`（lib 110 + bin 97）和 Flutter 39 项全部通过；`flutter analyze` 无 error/warning（32 条既有 info）；`git diff --check` 通过。
- **2026-09-18 — 稳定结构化分析 schema：完成。** `event-analysis-v1` 现在要求固定 schema_version、event_type、confidence、summary、clarifications、people、projects、follow_ups 八个字段，拒绝未知字段、错误类型、空摘要和越界 confidence；字符串数组 trim、去空并按首次出现去重。schema_version 同时写入 JSON 和 `prompt_version`，为后续按版本重算保留边界。验证：离线 StubProvider 覆盖合法结果、规范化、非法 JSON/非对象、confidence 越界、未知字段和 Provider 失败；`./regen.sh` 成功并同步动态库，`git diff --check` 通过。
- **2026-09-18 — 用户入口边界收敛：完成。** 撤销 Records / Inbox 一级导航：单独浏览原始记录价值不足，而把所有分析候选做成 Inbox 又与待办重叠并制造人工清理负担。明确行动继续进入待办；分析失败由设置中的队列状态承载；普通记录与来源详情进入 Phase 3 Today；人物/项目候选仅在具体上下文中确认。底层 `get_event_analysis_detail` 保留，原始事件和模型结果仍可追溯。验证：Flutter 导航/UI 回归测试 6 项通过；`flutter analyze` 无 error，仅保留既有 info/style 提示。
- **2026-09-18 — 一级导航与待办位置收敛：完成。** 一级导航只保留对话与知识库两个主要内容域；待办不再占用 Tab，移动到侧栏底部常用工具入口，以固定尺寸面板承载添加、勾选、编辑和删除。“今天”暂不以只有记录列表的半成品页面上线，等日期切换、日结、来源引用和行动关联形成完整场景后再进入导航。验证：Flutter 导航/UI 回归测试 6 项通过；`flutter analyze` 无 error/warning，保留 32 条既有 info；`git diff --check` 通过。
- **2026-09-18 — Phase 2 完成。** 持久化分析队列、非阻塞 worker、重启恢复、严格 `event-analysis-v1`、重试与错误可观测、来源详情 API 均已交付；保存路径不依赖 Provider，分析候选不会越权写入人物、项目或待办。最终验证：`cargo test --all-targets` 全部通过；完整 Flutter 39 项通过；`flutter analyze` 无 error/warning（32 条既有 info）；`git diff --check` 通过。进入 Phase 3，UI 信息架构暂冻结。

## Phase 3：“今天”与每日回顾

**目的：** 建立每天打开 Elsewhen 的默认场景。

### 范围

- 一级导航增加“今天”，作为默认主页；
- 时间流聚合当天事件、重要用户输入、分析状态和由它们产生的待办；
- 支持按日期切换，默认只加载必要窗口；
- 生成可重算的每日总结：做过的事、想法 / 决定、涉及的人和项目、未完成跟进；
- 日结必须引用来源记录，用户修正保存为显式修订，不覆盖来源；
- 对没有 Provider 或分析未完成的情况提供纯事实降级视图。

### 第一版明确不做

- 图表化生产力评分；
- 情绪打分和未经用户请求的心理判断；
- 自动推送大量提醒；
- 周报、月报等多周期报表。

### 退出证据

- 用户只使用 Capture 和主输入，也能在“今天”看到完整记录；
- 日结中的每条结论都能跳回至少一条来源；
- 删除并重算日结不会改变事件、人物、项目和待办；
- 无 AI 时仍能按时间可靠回顾当天事实。

### 完成记录

- **2026-09-18 — 每日回顾版本与来源契约：完成。** migration v20 新增追加式 `daily_reviews` 与 `daily_review_sources`，重算创建新版本且不覆盖事件或旧回顾；`daily-review-v1` 要求每条结论携带至少一个目标日期内的来源事件。`get_daily_overview` 在无回顾时返回当天事实与空 review，在有回顾时只暴露通过 schema 和来源校验的最新版本。Flutter Bridge 已生成 `DailyOverviewDto`、`DailyReviewDto` 和条目 DTO，但尚未新增页面。决策见 [每日回顾版本与来源契约](../notes/implemented/architecture/2026-09-18-daily-review-source-contract.md)。验证：存储版本/跨日来源测试与 schema 逐条引用测试通过；`./regen.sh` 成功并同步 release 动态库。
- **2026-09-19 — 每日概览行动关联：完成。** `DailyOverviewDto` 在事实列表和可选最新回顾之外，返回当天相关的未归档待办：事件关联待办或当天到期待办；无 AI 回顾时仍可直接使用事实与行动数据。验证：Bridge 重新生成并重建 release 动态库；每日回顾定向 Rust 测试通过；`flutter analyze` 无 error/warning，保留 32 条既有 info。
- **2026-09-19 — 每日概览 Flutter Bridge 验证：完成。** `RustBridgeRepository.getDailyOverview` 接入生成绑定；隔离真实 Bridge 验证日期、统一日流事实、无 AI 回顾降级和无关联待办结果，测试不接触个人数据库。验证：`flutter test test/bridge_integration_test.dart` 通过。
- **2026-09-19 — 每日回顾写入边界：完成。** 新增 `save_daily_review` API，仅接受 `daily-review-v1`、目标日期内的来源事件和每条结论的来源引用；回顾继续追加版本，不提供覆盖更新。生成绑定与 release 动态库已同步。验证：Rust schema/来源定向测试通过，`git diff --check` 通过。
- **2026-09-20 — 每日回顾显式生成：完成。** 新增 `generate_daily_review`，仅在用户或后续明确流程触发时读取当天事实和已完成分析，调用 Provider，严格校验 `daily-review-v1` 后追加版本；无 Provider 返回 `no_provider`，无事实返回 `no_entries`，Provider/解析失败不写半成品。Flutter 仓库层已接入方法，但尚未绑定页面或自动任务。验证：离线 StubProvider 生成测试通过；`./regen.sh` 成功并同步 release 动态库。

## Phase 4：人物、项目与长期记忆

**目的：** 从静态 Wiki 页面升级为有证据、有时间变化的个人记忆。

### 范围

- 明确实体：person / project / topic；明确带时间事实与实体的关联；
- 人物、项目详情增加事实时间线、最近动态、未完成行动和来源；
- 项目状态变化建模为带时间和证据的事实，不直接覆盖成一段 Markdown；
- 对别名、同名人物和可能重复项目提供合并 / 拆分流程；
- 多条事实稳定支持后，AI 才提出长期认识、习惯、约束或洞察候选；
- 现有 Wiki 页面继续作为可读摘要，内容可由结构化事实辅助更新。

### 退出证据

- 任一人物或项目可以回答“最近发生了什么”，且每项有来源和时间；
- 同一事实重复分析不会增加重复关系或重复状态；
- 人工纠正实体后，相关时间线和引用一致更新；
- 长期认识能说明由哪些事实支持，并能撤销或重算。

## Phase 5：素材阅读与加工流水线

**目的：** 让收藏的内容从“存下来了”走到“真正被使用”。

### 范围

- 素材区独立展示外部原文，不与个人记忆混在同一默认列表；
- 状态至少包含：待读、已读、值得使用、已采用、归档；
- 派生产物类型至少包含：摘要、观点、灵感、脚本 / 文案；
- 派生产物保留 `based_on`、生成指令、模型 / 版本、创建时间和采用状态；
- 支持从一个原文生成多个版本、比较并选定采用版本；
- “与我有什么关系”必须检索个人记录 / 项目后再生成，并明确引用两侧来源；
- 只有用户认可的外部观点才能进入个人长期记忆。

### 退出证据

- 导入文章后可以完成“阅读 → 总结 → 提炼观点 → 生成文案 → 标记采用”；
- 原文在任何加工操作后保持不变；
- 每个派生产物都能返回原文，且多个版本不会互相覆盖；
- 素材列表可以按状态找到尚未阅读和已经采用的内容。

## Phase 6：检索、主动回顾与产品收敛

**目的：** 在主循环稳定后，让累积数据可被自然调用。

### 范围

- 跨事件、人物、项目、素材和派生产物的统一搜索；
- 基于来源的问答，答案必须展示引用；
- 周回顾 / 项目回顾由日流和事实动态生成；
- 只对明确规则或用户选择的事项主动提醒；
- 评估旧“对话 / 知识库”入口是否降级为上下文视图，删除重复导航和过时兼容层；
- 完成性能、备份恢复、迁移和跨平台验收。

### 退出证据

- 用户能从自然语言问题定位到原始记录和派生结论；
- 周回顾不建立第二套事实源，并可完整重算；
- 主导航不存在两个用途重叠的记录入口；
- 数据备份恢复后，来源关系、修订和状态保持一致。

## 阶段依赖

```text
Phase 0 安全基线
    ↓
Phase 1 统一输入
    ↓
Phase 2 分析闭环
    ↓
Phase 3 今天 / 日结
    ├──────────────┐
    ↓              ↓
Phase 4 个人记忆   Phase 5 素材加工
    └──────┬───────┘
           ↓
Phase 6 检索与收敛
```

Phase 4 与 Phase 5 在 Phase 3 完成后可以并行，但两者必须复用相同的来源引用、修订和幂等机制。

## 全程交付门禁

- **数据安全：** migration 前后数据量和关键引用可校验，禁止破坏性重建真实数据库；
- **离线可靠：** 保存路径不发网络请求，AI 故障有明确降级；
- **可追溯：** 派生结论、实体事实、日结和创作产物都能定位来源；
- **可重算：** Prompt 或模型变化可以生成新版本，不覆盖原始数据；
- **写入边界：** AI 提案必须经过 schema 校验，高影响操作需要确认；
- **测试：** Rust 单元 / 集成测试、Flutter widget / bridge 测试和 migration 测试随阶段同步增加；
- **文档：** 非平凡架构或行为决定同步更新 Agent Note、需求和开发文档。

## 成功指标

主循环完成后，不以页面数量或 AI 调用次数衡量成功，而观察以下行为：

- 用户能否每天以极低成本持续记录；
- 用户能否可靠回答“今天 / 最近在做什么”；
- 人物和项目是否随着记录自然积累，而非依赖手工维护 Wiki；
- 待跟进事项是否有来源且不会被遗漏或重复制造；
- 收藏内容是否产生被采用的观点、脚本或文案，而不只是停留在列表中；
- 任意 AI 结论出错时，用户是否能找到来源、修正并安全重算。
