# AI 为中心的目标拆解与长程计划 Roadmap（Intent → Plan → Step）

## 目标与归属

把目标功能的「翻译工作」从用户侧整体转移给 AI：用户只说一句意图（如「3 年内月收入 10 万」），AI 产出完整草案（现实性评估、假设清单、阶段拆解、第一步 goal），用户只做确认与纠偏。

- **问题定性（详见 [提案](../notes/proposed/product/2026-11-04-ai-centric-goal-decomposition.md)）：** 现行「认知型目标」功能正确但用不动——四切片填写、三态管理、名额经营全是用户侧成本；用户的心智形态是一句意图，缺的是「从意图到可执行」的翻译，而翻译正是 AI 该干的活。
- **需求锚点：** [FR-PES-005 目标与偏差检测](../requirements/product/FR-PES-005-目标与偏差检测.md)。
- **承接：** [认知型目标与长程航向对齐 Roadmap](2026-11-02-cognitive-goals-and-long-term-alignment-roadmap.md)（四切片与三态保留为「步骤」层词汇）；既有 [目标表设计与偏差检测](../notes/proposed/architecture/2026-09-28-goal-schema-and-deviation.md)；AI 侧「幕僚」定位落地见 [review 落地记录](../review_logs/2026-11-02/review_goal_first_menu_positioning.md)。

## 核心不变式（Invariants）

1. **意图不设上限：** 目标 / 计划 / 意图数量无上限，「名额已满」不出现在用户侧；用户随时可扔进任意数量的想法。
2. **注意力保护移驻并发步骤：** 同一时刻处于推进中的步骤数受限（缺省 3，不做可调设置）；这是 AI 的调度并发闸，用户无感——无空位先排队，空位出现 AI 从等待队列提议激活；同计划默认 1 步、按需并行占同一总名额。拆解粒度与形态（线性步骤 vs 并行子目标）由 AI 自决，用户可给期望数量。数据库触发器保留为内核兜底。
3. **确认制：** AI 起草一切（计划 / 步骤 / 推进 / 暂停），全部走 WriteConfirm 审阅卡；「AI 为中心」指起草与经营由 AI 承担，不指免确认落库。
4. **零打扰：** 单日闲聊零挂靠；主动提醒只走长程窗口（计划层复用 compass 机制）。
5. **本地优先 / 原始记录不可变 / 多端同步一致：** 计划与步骤全部进同步域（PlanRecord），快照与增量遵循既有裁决规则。

## 架构切片与实施计划

| 阶段 | 代号 | 内容 | 交付物与依赖 | 状态 |
|---|---|---|---|---|
| 阶段 0 | P0 | 契约冻结 | 数据模型、工具契约、UI 形态、待定问题定案 | **已完成**（2026-11-04，见提案「定案记录」） |
| 阶段 1 | P1 | 存储与协议 | goal_plans 表 + goals 扩展列（含存量清理迁移）+ PlanRecord 同步 + FRB 门面 | **已完成**（2026-11-04，证据见下） |
| 阶段 2 | P2 | AI 拆解（主对话入口） | 拆解提示词 + create_plan 提案 + 草案审阅卡 + 端到端 | **已完成**（2026-11-04 工程收口；真模型质量与真人验收留人工门，见落地记录） |
| 阶段 3 | P3 | 执行循环与计划看板 | advance/激活提议 + 并发闸调度 + 目标页计划看板 | **已完成**（2026-11-04 工程收口；真人验收留人工门，见落地记录） |
| 阶段 4 | P4 | 主动复盘与偏离扩展 | 计划层 compass + 复盘提议 | **已完成**（2026-11-04 工程收口——roadmap 四阶段全部完成；真人验收留人工门，见落地记录） |

## 详细阶段设计

### P0 — 契约冻结（已完成，2026-11-04）
- 三问定案（记录于提案「定案记录」）：① 软时间窗（可改假设，不判逾期）；② 并发与拆解形态 AI 自决，用户可给期望数量（线性步骤或并行子目标），总闸缺省 3 不做可调；③ 不存在无计划的独立目标（直接目标 = 单步计划），存量随迁移删除。
- 冻结：`goal_plans` 字段集、`goals.plan_id / step_index` 语义、提案工具名与审阅卡载荷、计划状态机（proposed/active/paused/completed）。
- 产出：提案 Status 已转 adopted；实施从 P1 开工。

### P1 — 存储与协议（已完成，2026-11-04）
- migration（追加式）：`goal_plans` 表（意图原文 / 愿景 / 假设 JSON / 状态 / 时间戳）；`goals` 增 `plan_id`、`step_index`（可空，向后兼容）。
- 存量清理：迁移删除既有独立 goals 行（不设兼容层；删除随同步语义覆盖多端）。
- Store：计划 CRUD、步骤挂接与排序、并发闸调度查询（活跃步骤计数 + 等待队列）。
- 同步：`PlanRecord` 新记录域（`#[serde(default)]` 兼容旧信封）；快照携带；冲突裁决复用名册规则。
- FRB 门面与 Dart 适配。
- 测试面：migration 校验（含存量清理后无孤立引用）、Store 计划 CRUD 与闸门调度查询、PlanRecord 往返与快照。
- **落地记录（2026-11-04）：**
  - migration 68（`migrations/68-goal-plans/up.sql`）：`goal_plans` 表 + `goals.plan_id/step_index` + 索引；存量清理按定案先解绑 `conversations.goal_id`，再清 `goal_compass_nudges` 与 `goals` 行（不设兼容层；混版窗口的旧目标复活风险已知并接受，见迁移注释）。
  - Store（`src/storage/plans.rs`）：计划 CRUD + 步骤挂接与排序（`COALESCE(step_index)` 末尾兜底）+ 闸门查询（`count_active_steps` / `list_waiting_steps`，唤醒复用既有 `reactivate_goal_with_focus`）+ `delete_plan`（步骤解绑保留）；写路径均在同一事务内捕获同步（`sync_record_plan`）。
  - 同步：`PlanRecord` 整行 + `PlanChanged` 变更（删除为全端物理删行）；冲突立案 / 「采纳远端」/「保留本端」全通路复用名册 CAS 规则，删除后编辑的候选 reason 为 `plan_deleted`。
  - 快照 v9：`Record::Plan` + goals 行携带计划两列；携带表 / 导出查询 / `domain_table` / apply 全接线；跨 v8/v9 显式隔离（新增 serde 变体必须 bump）。
  - FRB 与 Dart：`src/api/plans.rs`（PlanDto + 门面函数）、`GoalDto` 增 `plan_id/step_index`；`models/plan.dart`（GoalPlan/PlanStatus）+ `rust_bridge_parts/plans.dart`（RepoPlans 委托）+ Goal 模型扩展 + 对象冲突审阅计划文案；`scripts/regen.sh` 重生成后桥 hash 两侧一致（`-1437813334`）。
  - 门禁（2026-11-04）：`cargo test --all-targets` **1019 passed / 0 failed / 20 ignored**；`fvm flutter test` **521 passed / 0 failed**；`fvm flutter analyze` **0 error**。专项用例：`v68_goal_plans_migration_purges_legacy_goals_without_orphans`（v67 真造存量 → 迁移后无孤立引用）、`snapshot_carries_plans_with_steps_and_incremental_metadata`（快照携带 + PlanChanged 增量）、`src/storage/plans.rs` 6 例（CRUD / 闸门 / 删除解绑）。

### P2 — AI 拆解（主对话入口）（已完成，2026-11-04）
- 提示词：拆解方法论（里程碑由近及远 / 每步 1~2 周抓手 / 每步可验证判据）+ 可行性评估（结合知识库现状与 web_search）+ **假设清单**纪律（AI 凭什么这么拆，逐条可纠正）。
- 工具：`create_plan`（提案：计划全貌 + 第一步 goal）、`add_plan_steps`（追加拆解）；goal-management 技能包扩章。
- 审阅卡：草案可视化（意图原文 / 现实性评估 / 假设列表 / 里程碑 / 第一步 goal），接受或逐项纠正。
- 端到端：主对话一句话 → 草案 → 确认落库 → 目标页出现计划。
- 测试面：提案载荷校验、接受/纠正路径、无会话降级、红线（未确认零落库）。
- **落地记录（2026-11-04）：**
  - 工具与提示词（`src/ai/tool/plan_tools.rs` + goal-management 技能扩章）：`create_plan`（意图原文原话引用 / 愿景 / 现实性评估 / 假设清单（≤16 条，逐条可纠正）/ 里程碑步骤（1~12 步，由近及远、带判据））、`add_plan_steps`（追加接续）、`list_plans`（含推进 / 排队计数）；全部 WriteConfirm 确认制，方法论与「期望步数」纪律写进工具描述与技能说明。
  - 执行（`src/ai/tool/revision_tools.rs` + `src/ai/approval.rs`）：接受后单事务落库——计划整行 + 全部步骤目标（第一步按活跃名额激活、其余暂停排队，「同计划默认 1 步推进」；`store.create_plan_with_steps / append_plan_steps`）；复用审批索赔 / 租约 / 回执与审阅目标核验。
  - 逐项纠错（`src/storage/approvals.rs`）：`create_plan` 草案的 vision / assessment / assumptions / steps 可编辑（意图原文是身份锚，不可改）；纠正后审批 token 重新绑定，旧版本自然失效。
  - 审阅卡（`ui/lib/widgets/chat_proposal_parts/`）：`create_plan` / `add_plan_steps` 专属草案渲染（意图原文 / 评估 / 假设清单 / 里程碑步骤；列表标题回退到意图原文）；详情面板支持愿景与现实性评估的文本纠正。假设与步骤的**结构化逐条编辑**留作后续切片（当前由「拒绝 + 对话纠偏 + 重新提议」覆盖）。
  - 测试面与门禁：`cargo test --all-targets` **1024 passed / 0 failed / 20 ignored**；新增 5 例——计划存储 2（第一步激活 / 全局满额排队、追加序号接续）+ 工具确认流 3（未确认零落库 → 确认整计划落库 → 追加；逐项纠正 → token 重绑 → 接受采用纠正版；空步骤拒绝且不登记）；`fvm flutter test` **521 passed / 0 failed**、`fvm flutter analyze` 0 error。
  - 剩余人工门：真模型质量（「一句话 → 可用拆解」）与真人 GUI 验收沿用既有验收路径；「无会话降级」由构造保证（提案必须挂源会话）。

### P3 — 执行循环与计划看板（已完成，2026-11-04）
- 完成一步：AI 复盘（事实 vs 假设）→ 提议下一步 goal（`advance_plan_step` 提案）。
- 并发闸调度：等待队列 + 空位激活提议；满额场景零拒绝（复用既有「满额落暂停」语义，新步骤落暂停排队）。
- UI：「目标」页 = 计划看板（每计划一卡：意图 / 当前步 / 进度 / 下一步预告）；「直接建一条目标」= 自动包成单步计划（无独立目标概念）。
- 测试面：步骤完成 → 下一步提议、闸门调度（3 活跃 + 队列）、看板渲染与选中联动。
- **落地记录（2026-11-04）：**
  - 执行循环（`src/ai/tool/plan_tools.rs` + `revision_tools.rs`）：新工具 `advance_plan_step`（确认制）——完成当前步（`completed_step_goal_id`，校验确属本计划）+ 推进（`activate_goal_id` 激活队列步骤 / `new_step` 追加新步骤）；名额空出立即激活，满额保持排队并如实回报（零拒绝）；`list_plans` 输出补推进清单与等待名单（ID 可直接用于推进提案）；goal-management 技能补「先复盘（事实 vs 假设）再推进」纪律。
  - 并发闸：激活一律经 `count_active_goals < 3` 预检 + `reactivate_goal_with_focus`（触发器兜底）；「同计划默认 1 步推进」由 P2 创建语义延续。
  - 「无独立目标概念」：`Store::create_goal_as_single_step_plan`（计划意图 = 目标正文，步骤 1 = 该目标）；AI `create_goal` 执行与 FRB `create_goal` 门面统一改走包成路径（UI「新建」按钮零改动获得计划化）；技能话术同步（长期意图优先 `create_plan`，单步计划适合「先干起来」的念头）。
  - 计划看板（`ui/lib/providers/goal_provider.dart` + `widgets/goal_view.dart`）：左栏工作台顶部「长程计划」区——每计划一卡（意图 / 当前步 / 进度 x/y / 下一步预告），点击卡片经 `selectedGoalIdProvider` 选中当前步（无当前步则下一步），与主区详情 / AI 讨论联动；`planBoardProvider` 跟随目标清单自动刷新。**落地取舍**：按增量落地，看板是「目标」页里的计划视口，步骤级清单保留在卡片区下方（步骤即 goals 行，状态过滤 / 唤醒 / 编辑等既有操作面不变），而非全量替换。
  - 测试面：`cargo test --all-targets` **1027 passed / 0 failed / 20 ignored**；新增 4 例——advance 完成 + 激活队列步骤 → 追加即激活；create_goal 工具包成单步计划；单步包成（存储）满额排队；既有流程回归。`fvm flutter test` **522 passed / 0 failed**（新增看板渲染 + 点击选中联动 1 例）；`fvm flutter analyze` 0 error。
  - 剩余人工门：真模型质量（完成一步后的复盘与推进是否可读可信）与真人 GUI 验收沿用既有验收路径。

### P4 — 主动复盘与偏离扩展（已完成，2026-11-04）
- 计划层 compass：步骤停滞/偏离 → 计划层提醒（沿用 3~5 天窗口与频控）。
- 复盘提议：从事件/对话信号判断步骤是否达成，提议推进或调整假设。
- 测试面：偏离判定复用与计划层降频、提议不落库（确认制）。
- **落地记录（2026-11-04）：**
  - 计划层上下文：迁移 69（`goal_compass_nudges` 增 `plan_id`/`plan_intent` 可空列；纯本地衍生表，不入同步与快照，旧行回填 NULL）。`evaluate_goal_compass` 在停滞步骤属于计划时携带计划意图；横幅与工作台卡片改为展示「计划『…』· 步骤『…』」，脱离计划的目标回落原文案。
  - 计划层降频：`has_recent_compass_nudge_for_plan`（7 天窗口，含退避期）——同计划换一条停滞步骤也不连环打扰；与既有步骤级 7 天频控两级叠加。
  - 复盘提议：沿用 P3 的 `advance_plan_step` 确认制（先复盘「事实 vs 假设」，再完成当前步 / 激活队列步骤 / 追加新步骤）；不新增任何自动落库路径，**提议零落库红线保持**（AI 从事件与对话信号判断达成后走对话内提案）。
  - 测试面：`cargo test --all-targets` **1029 passed / 0 failed / 20 ignored**（新增：v69 迁移用例、计划层上下文与计划级降频用例）；`fvm flutter test` **522 passed / 0 failed**；`fvm flutter analyze` 0 error；`scripts/regen.sh` 后桥 hash 两侧一致。
  - 剩余人工门：真模型质量（提醒可读性与复盘话术）与真人 GUI 验收沿用既有验收路径。

## 验收场景（Validation Scenarios）

1. **一句话立项：** 在「目标」页的 AI 立项工作室（未选中目标时的主区）输入「3 年内月入 10 万」→ 产出含现实性评估与假设清单的草案；确认后计划与第一步 goal 落库，目标页可见。
2. **意图不设限：** 连续提出 6 个意图全部成功建档（超额步骤排队），全程零「名额已满」。
3. **并发闸：** 活跃步骤达到上限时新步骤自动排队；某步完成后 AI 提议激活队列中优先级最高者。
4. **纠错路径：** 草案中任一条假设被用户改写后，采纳版本落库且后续步骤按新假设生成。
5. **存量清理：** 迁移删除既有 goals 行（不设兼容层）；升级库与新库行为一致，无孤立引用。
6. **红线：** 未确认的草案/推进提议零落库；单日闲聊不触发计划话题。
7. **期望步数：** 意图里带「用 5 步实现」时，拆解结果恰为对应数量（步骤或子目标形态由 AI 定）。

## 入口修订（2026-11-05，用户要求）

**AI 立项 / 拆解入口整体移入「目标」页**：不再把用户带去对话页预填主对话草稿，也不再作为普通对话出现在会话清单与对话界面里。

- **目标工作室**：目标页主区未选中目标时即「AI 立项工作室」——保留会话（`goal-studio`：固定 id、`goal_id` 为空、`assistant_mode='goal_mentor'`、标题「目标工作室」），跨端收敛到同一行；`list_conversations` 显式排除该 id，故不进普通对话清单、不进任何对话界面。
- **入口**：左栏「和 AI 拆解 / 立新目标」与航向校准「聚焦讨论」改为写入目标页 AI 面板的预填草稿（`goalPanelSeedProvider`，绝不自动发送），并停在「目标」tab；目标讨论面板与工作室面板同源（`GoalChatPanel.studio`）。
- **提示词**：`system_context` 新增「目标工作室」幕僚分支（现实性评估 → 假设清单 → `create_plan` 草案 → 之后 `advance_plan_step` / `add_plan_steps` 经营；确认制与提议零落库红线不变），并让工作室与目标讨论一样不记个人事件。
- **无 schema 变更**：工作室就是普通 `conversations` 行；同步校验本就只约束 `goal_id ⇒ goal_mentor`（单向），`goal_mentor` + 空 `goal_id` 合法。
- **测试面**：`cargo test --all-targets` **1030 passed / 0 failed / 20 ignored**（新增 `goal_studio_conversation_is_reserved_hidden_and_idempotent`：保留 id 幂等、不在普通对话清单、模式与绑定正确、归档后静默取回）；Flutter 侧更新立项入口 / 空态主区 / 工作室发言三类用例。
