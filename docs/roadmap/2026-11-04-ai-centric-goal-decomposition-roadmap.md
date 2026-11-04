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
| 阶段 2 | P2 | AI 拆解（主对话入口） | 拆解提示词 + create_plan 提案 + 草案审阅卡 + 端到端 | 未开始 |
| 阶段 3 | P3 | 执行循环与计划看板 | advance/激活提议 + 并发闸调度 + 目标页计划看板 | 未开始 |
| 阶段 4 | P4 | 主动复盘与偏离扩展 | 计划层 compass + 复盘提议 | 未开始 |

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

### P2 — AI 拆解（主对话入口）
- 提示词：拆解方法论（里程碑由近及远 / 每步 1~2 周抓手 / 每步可验证判据）+ 可行性评估（结合知识库现状与 web_search）+ **假设清单**纪律（AI 凭什么这么拆，逐条可纠正）。
- 工具：`create_plan`（提案：计划全貌 + 第一步 goal）、`add_plan_steps`（追加拆解）；goal-management 技能包扩章。
- 审阅卡：草案可视化（意图原文 / 现实性评估 / 假设列表 / 里程碑 / 第一步 goal），接受或逐项纠正。
- 端到端：主对话一句话 → 草案 → 确认落库 → 目标页出现计划。
- 测试面：提案载荷校验、接受/纠正路径、无会话降级、红线（未确认零落库）。

### P3 — 执行循环与计划看板
- 完成一步：AI 复盘（事实 vs 假设）→ 提议下一步 goal（`advance_plan_step` 提案）。
- 并发闸调度：等待队列 + 空位激活提议；满额场景零拒绝（复用既有「满额落暂停」语义，新步骤落暂停排队）。
- UI：「目标」页 = 计划看板（每计划一卡：意图 / 当前步 / 进度 / 下一步预告）；「直接建一条目标」= 自动包成单步计划（无独立目标概念）。
- 测试面：步骤完成 → 下一步提议、闸门调度（3 活跃 + 队列）、看板渲染与选中联动。

### P4 — 主动复盘与偏离扩展
- 计划层 compass：步骤停滞/偏离 → 计划层提醒（沿用 3~5 天窗口与频控）。
- 复盘提议：从事件/对话信号判断步骤是否达成，提议推进或调整假设。
- 测试面：偏离判定复用与计划层降频、提议不落库（确认制）。

## 验收场景（Validation Scenarios）

1. **一句话立项：** 主对话输入「3 年内月入 10 万」→ 产出含现实性评估与假设清单的草案；确认后计划与第一步 goal 落库，目标页可见。
2. **意图不设限：** 连续提出 6 个意图全部成功建档（超额步骤排队），全程零「名额已满」。
3. **并发闸：** 活跃步骤达到上限时新步骤自动排队；某步完成后 AI 提议激活队列中优先级最高者。
4. **纠错路径：** 草案中任一条假设被用户改写后，采纳版本落库且后续步骤按新假设生成。
5. **存量清理：** 迁移删除既有 goals 行（不设兼容层）；升级库与新库行为一致，无孤立引用。
6. **红线：** 未确认的草案/推进提议零落库；单日闲聊不触发计划话题。
7. **期望步数：** 意图里带「用 5 步实现」时，拆解结果恰为对应数量（步骤或子目标形态由 AI 定）。
