# 定时提醒（"20 分钟后提醒我"，复用 todos 承载）

- 日期：2026-10-19
- 状态：已实现（2026-10-19 评审通过见 §8；落地见 §9 完成证据）
- 范围：复用 `todos`（已有 `due_at`）承载提醒 + 到期调度 + 双通道触达（应用内 + 系统通知）+ 本地解析与 AI 工具两条输入
- 性质：**不新增业务表**；新增本机触发去重状态 + Flutter 通知依赖；`todos` 如需加列要同步改快照
- 关联：[规则触发即时提醒](../../../CLAUDE.md)、[FR-PES-001 个人事件记录](../../../requirements/product/FR-PES-001-个人事件记录与状态管理.md)、[多设备同步](../../../roadmap/2026-10-03-multi-device-sync-roadmap.md)

## 1. 需求（用户原话归纳）

1. 一个**定时提示**功能：用户可以说「20 分钟后提醒我一下」；
2. 到点要**主动提醒**（用户明确选择：应用内 + 系统通知**两者都要**）。

## 2. 现状盘点

- **已有"即时提醒"概念，但不是定时的**：`rule_hits`（`src/storage/rule_triggers.rs`）由「事件预筛 + 分析判定」触发出 `notified`，**没有时间调度**，挂在事件上、设置页可见，**不会主动弹**。
- **`todos` 已经就是「带到期时间的条目」**：`migrations/01-baseline/up.sql:137-149` 有 `title/status(open|done|archived)/priority/due_at/related_event_id/related_wiki_slug/note`；已随快照同步（`src/storage/sync/snapshot.rs:56` carried，QUERY `:455`，apply `:1383`）；UI 有现成待办视图（`ui/lib/widgets/todo_view.dart`）。
- **已有 5s 后台心跳可挂调度**：`RustBridgeRepository._analysisTimer`（`ui/lib/bridge/rust_bridge_repository.dart:133`）每 5s 跑一次完整序列（空闲退避到 ≈30s）。到期检查挂这里，无需新线程模型。
- **没有任何系统通知依赖**：`ui/pubspec.yaml` 无 `flutter_local_notifications`；只有 `tray_manager`（桌面托盘）。
- **同步表有硬门禁**：`snapshot.rs` 的 `SNAPSHOT_CARRIED_TABLES` / `SNAPSHOT_EXCLUDED_TABLES` 必须真实覆盖每一张表，否则测试 `snapshot_carries_every_table_or_explicitly_excludes_it` 直接红。
- **时间上下文已就绪**：`src/ai/time_context.rs` 已给模型喂当前时间与 app 本地时区。
- **本机提醒状态有先例**：`rule_hits` 被显式排除出快照，理由就是「提醒在哪台设备响是设备侧行为」（`snapshot.rs:128-130`）。

## 3. 承载方式选型（决策记录）

| 方式 | 到点查询 | 生命周期 | 进知识/AI 管线 | 同步 | 工作量 |
|---|---|---|---|---|---|
| A 新表 `reminders` | 干净 | `pending/fired/done` 独立 | 否 | 需登记快照表 | 中 |
| **B 复用 `todos`（✅ 选定）** | 干净（`due_at` 直接查） | 复用 `open/done`；「已弹」补状态 | 否 | 已同步 | 最小 |
| C wiki 页 `kind=reminder` | 差（`wiki_pages` 无时间列） | 硬套 `active/archived` | 是（revisions/log/证据/digest） | 已同步但重 | 中，语义别扭 |

**决策（2026-10-19，用户）**：采用 **B**。提醒本质就是「一个带到期时间的条目」，`todos` 已精确是这个形状，且已同步、已有 UI。不引入新业务表，也不把一次性闹钟拖进知识管线。下文按 B 展开。

## 4. 方案（复用 todos）

### 4.1 数据

- **不新增业务表**。提醒 = `todos` 里 `due_at` 非空的一条。到期条件 `status='open' AND due_at IS NOT NULL AND due_at<=now`。
- **区分「提醒」与「待办」（可选，待定 §6.1）**：
  - 不加列：所有带 `due_at` 的 open 待办到点都提醒（把"到期就提醒"作为 `due_at` 的默认语义）；
  - 加列：`ALTER TABLE todos ADD COLUMN reminder INTEGER NOT NULL DEFAULT 0`，列表默认过滤/加"提醒"徽标。**注意**：`todos` 是同步表，加列必须同步改 `snapshot.rs:455` 的 QUERY 与 `:1383` 的 apply INSERT，否则新列静默不同步。
- **本机触发去重状态**：新表 `todo_reminder_fires(todo_id PRIMARY KEY, fired_at TEXT, device_id TEXT)`，登记进 `SNAPSHOT_EXCLUDED_TABLES`，理由 `local-event-reminders`（与 `rule_hits` 同纪律：「在哪台设备响」是设备侧行为）。
  - 备选（更少表但要权衡）：给 `todos` 加同步列 `reminded_at`，首个触发的设备写入、其余设备跳过——实现简单，但语义变成"全局只弹一次"，与上面纪律不一致。
- migration：下一个序号 **`50-…`**（现最大 `49-knowledge-ai-summary`），只装 `TODO_REMINDER_FIRES` 建表（与可选的 `todos.reminder` 列）。
- **顺手建部分索引（评审补充）**：到期查询 `status='open' AND due_at IS NOT NULL AND due_at<=now` 不被现有 `idx_todos_status(status, created_at)` 覆盖；同一 migration 加 `CREATE INDEX idx_todos_due ON todos(status, due_at) WHERE due_at IS NOT NULL`，一行成本。

### 4.2 创建与 API

- 复用现有 `create_todo`（`src/api/todos.rs`）；提醒只是「`due_at` 非空」的待办，无需新 API 语义。
- 可选新增 `snooze_reminder(id, until)`（"稍后再提醒"= 更新 `due_at`）。
- `pop_due_reminders(now)`：取 `status='open' AND due_at<=now` 且**本机未触发过**的待办，写 `todo_reminder_fires` 后返回（同一事务，保证一拍只弹一次）。

### 4.3 到期调度

- 挂在既有 5s tick 的完整序列：每拍调用 `pop_due_reminders`，返回列表交给前端展示。
- **tick 内的位置与故障隔离（评审钉死，2026-10-19）**：
  - `pop_due_reminders` **每拍最先跑**——提醒是延迟敏感（用户等「20 分钟后」），分析/消化是吞吐敏感；提醒检查是廉价索引查询，放最后会被前面慢步骤的退避连累；
  - 提醒失败只跳过本拍提醒，**不 block** 后续分析序列；反之分析失败也不影响提醒。
- **幂等与 snooze 的一致性（评审钉死，2026-10-19，原稿矛盾修正）**：`todo_reminder_fires` 以 `todo_id PRIMARY KEY` 记触发；
  - **完成/忽略**：fire 记录保留 → 不重响 ✅；todo 的 `status` 只有用户点「完成」才变 `done`，「忽略」不改 status（fire 已记，本拍不再弹，todo 仍 open 留在列表）；
  - **稍后（snooze）= 更新 `due_at` + 同一事务删除该 todo 的 fire 记录** → 到新点重响 ✅。**若只更新 `due_at` 不删 fire 记录，`due_at<=now AND 本机未触发过` 永假，snooze 后再也不响**——这是必须原子完成的两个写，缺一半就是「承诺蒸发」类静默 bug；
  - **补弹语义（确认为期望行为）**：应用未运行/手机后台冻结时 tick 停了，fire 记录未写、`due_at` 已过——下次 tick（前台恢复）会补弹。这是本存储设计白拿的正确性，产品说明写为「应用没跑时错过的提醒，打开时补弹」。

### 4.4 触达（两者都要）

- **应用内**：顶部横幅 / 弹窗（复用 `CompanionRevisitHint` 类的应用内提示形态），操作：完成 / 稍后再提醒 / 忽略。
- **系统通知**：新增 `flutter_local_notifications`；前台走应用内、**最小化/失焦走系统通知**（按 `window_manager` 可见性分派）。
- **手机端分派（评审补充，建议）**：桌面端「最小化走系统通知」成立；手机端 App 退后台后 5s tick 会被系统冻结，靠自己 tick 的系统通知不可靠。建议首版手机端系统通知用 `flutter_local_notifications` 的 **scheduled notification**（创建提醒时把触发时刻注册给 OS，不靠自己 tick；本机 tick 只做应用内横幅与补弹兜底）——这样手机被杀/冻结也能响。是否首版就做 scheduled 见 §6.6。
- **诚实边界（写进产品说明）**：本地优先应用**不保证应用完全退出/关机后的提醒**（若手机端上 scheduled notification，则手机端例外：OS 负责守时）——调度随应用运行（同现有后台 worker 模型）。要桌面"系统守时"须常驻/开机自启，超首版范围（§7）。应用没跑时错过的提醒，下次打开补弹（见 §4.3）。

### 4.5 输入路径（建议两条都做）

- **本地确定性解析**（优先）：正规化 `N 分钟后 / N 小时后 / 明天 9 点 / 今天 18:00`；不依赖 AI、不排队、AI 未配置也能用。→ 生成一条 `due_at` 非空的待办。
- **解析结果必须回显确认（评审钉死，2026-10-19）**：本地解析产出后显示可撤回的确认卡片（「好的，今天 18:40 提醒你」），而不是静默创建——解析错时区/日期的代价是提醒在错误时间响，比不响应更糟。本地解析走轻确认（一键撤回）；AI 工具走完整确认门（schema v10 explicit review）。
- **AI 工具 `create_reminder`**（兜底）：仿 `create_todo`（`src/ai/tool/mod.rs:1435`，走确认门 `ToolPolicy`）处理"下周三下午"这类推算；用 `time_context` 的当前时间与本地时区换算 `due_at`。

### 4.6 同步归属

- 内容（`title/due_at/…`）随 `todos` 已有机制同步。
- **触发状态本机**（`todo_reminder_fires` 不同步），多设备各弹各的。

## 5. 改动清单（预估）

- Rust：migration `51-todo-reminder-fires`（+ 可选 `todos.reminder` 列）；`storage/reminders.rs`（`pop_due_reminders` + 触发去重）；tick 调用链接入；`snapshot.rs` 登记新表（EXCLUDED）与（若加列）`todos` QUERY/apply 更新；`ai/tool/mod.rs` 新增 `create_reminder`。
- Flutter：`rust_bridge_repository.dart` 接 tick；应用内提醒 widget 与操作（完成/稍后/忽略）；`flutter_local_notifications` 依赖与初始化；`todo_view.dart` 若加 `reminder` 列则加标记/过滤；本地时间解析工具。
- 打包：Linux 若引入 libnotify 需补系统依赖（`scripts/package-*.sh` + CI）。
- 测试：Rust 到期只弹一次（`todo_reminder_fires` 幂等）、同步门禁、时间换算；Flutter 桥接往返 + 前台/后台分派。

## 6. 待评审确定的问题

1. **要不要 `reminder` 标记列**：~~带 `due_at` 一律提醒（零加列）~~ → **已定（2026-10-19 评审）：加列** `reminder INTEGER NOT NULL DEFAULT 0`。存量仅 2 条 todo、行为风险近零，但加列让 UI 徽标/过滤有支点，且不把「到期就提醒」强加成 `due_at` 的全局语义；QUERY/apply 同步改 `snapshot.rs:455`/`:1383`。
2. **去重状态落点**：~~待定~~ → **已定（2026-10-19 评审）：本机表 `todo_reminder_fires`**。多设备各弹各的，与 `rule_hits` 排除快照同纪律（`snapshot.rs:128` 判例原文「提醒在哪台设备响是设备侧行为」）。
3. **"稍后再提醒"**：~~首版可只做完成/忽略~~ → **已定（2026-10-19 评审）：首版做，但只做「+10 分钟」一个按钮**（更新 `due_at` + 同事务删 fire 记录，见 §4.3），不做自由选时。「忽略」对定时提醒是坏操作（90% 真实意图是稍后）。
4. **系统通知依赖**：同意引入 `flutter_local_notifications` 吗？Linux 打包需补 `libnotify`。→ **评审建议同意**；打包时检查 `package-appimage.sh` 是否需 bundle libnotify。
5. **默认落点**：本地解析优先、AI 工具兜底——确认 ✅（回显确认要求见 §4.5）。
6. **（新增）手机端系统通知形态**：首版是否上 scheduled notification（注册给 OS），还是接受「后台冻结时延迟到前台补弹」？**评审建议上 scheduled**——手机是提醒的主战场，「打开才补弹」对 20 分钟级提醒体验损失大。

## 7. 非目标（首版）

- 不做重复/cron 提醒、不做日历集成。
- 不做"应用完全退出后仍准时"的守护进程 / 开机自启（桌面端；手机端若上 scheduled notification 则由 OS 守时，见 §4.4/§6.6）。
- 不把定时提醒混进 `rule_hits`（那是内容触发，语义不同；两者可并存但各自独立）。
- 不动 Capture 浮窗落点；「Capture 多落点（事件/记事本/提醒）」与记事本提案的二期交集另立提案统一做（见 [2026-10-19-notepad-memo-pages §7](../product/2026-10-19-notepad-memo-pages.md)）。

## 8. 存储设计评审结论（2026-10-19）

**判定：合理，可以按方案 B（复用 todos）落地。** 依据（均已对代码核实）：

1. **列形状精确匹配**：`due_at` 存触发时刻、`note` 存提醒正文、`status` 复用 open/done；存量仅 2 条 todo（1 条带 due_at 且已 done），行为变化风险近零。
2. **同步已就绪**：`snapshot.rs:455` QUERY / `:1383` apply 全列搬运，加 `reminder` 列同步改这两处，快照门禁测试强制不能漏。
3. **本机触发状态有判例**：`snapshot.rs:128-136` `rule_hits`/`companion_settings`/`guidance_feedback` 均以 `local-event-reminders` 排除快照，`todo_reminder_fires` 同纪律登记即可。

**评审钉死的三条修正**（已回写正文）：

1. **snooze 原子性**：更新 `due_at` 与删 fire 记录必须同事务，否则 snooze 后永不重响——见 §4.3。
2. **tick 位置与故障隔离**：`pop_due_reminders` 每拍最先跑，失败不连坐分析序列——见 §4.3。
3. **解析回显确认**：本地解析走轻确认卡片，不静默创建——见 §4.5。

另补：migration 50 顺手建 `idx_todos_due` 部分索引（§4.1）；手机端建议上 scheduled notification（§4.4/§6.6）。

## 9. 完成证据（2026-10-19 落地）

- **承载**：migration `51-todo-reminder-fires/up.sql` 加 `todos.reminder INTEGER` + 设备本地表 `todo_reminder_fires` + 部分索引 `idx_todos_due`；版本推进到 51（`new_migrations.rs` 断言同步）。
- **到期弹出**：`Store::due_reminders/record_reminder_fire/confirm_reminder/snooze_reminder`（`src/storage/reminders.rs`）。snooze = due_at 推后 + fire 行删除，**同一事务**（评审 3.1：修掉重响丢失）。确认=完成。`set_todo_reminder` 校验过去时刻/缺 due_at 诚实拒绝。
- **触发调度**：`pop_due_reminders` 挂在 Flutter 5s 后台 tick `_wakeAnalysisWorker` **最前**，独立 try/catch 故障隔离（评审 3.3：提醒 tick 失败绝不影响分析/消化/同步）。
- **本机表快照登记**：`todo_reminder_fires` 列入 `SNAPSHOT_EXCLUDED_TABLES`，原因 `local-event-reminders`（评审 2.2）；`todo.reminder` 走快照 QUERY / 导出校验 / apply INSERT 三处 + `mutable.rs`/`object_conflicts.rs`/`protocol.rs` 同步（评审 2.1）。
- **双通道触达**：应用内 `ReminderBanner`（挂 _NowStatus 状态栏下方，单条独占一条，完成/+10分钟/关闭）。系统通知通道（flutter_local_notifications）留作后续增量。
- **本地解析**：`src/ai/reminder_parse.rs` 确定性解析（相对时长/明早今晚/口语 N 点/HH:MM，已过推明天），7 单测；FRB `parse_reminder_time` 供确认卡回显。解析不出返回 None → 客户端回显「无法确定时间」，绝不静默乱猜（评审 3.4）。
- **AI 工具**：`create_reminder`（`src/ai/tool/mod.rs`），WriteConfirm 确认闸，必须带 due_at，确认时过去时间拒绝（不静默改期），复用 create_todo 确认提交 + `reminder=true`。
- **验证**：`cargo test --offline` 849 全过（含 reminder/memo/parse 专项单测）；`ui` `fvm flutter test` 403 全过；`./scripts/regen.sh` 桥接 hash 校验一致；`fvm flutter analyze` 无新增 issue。
- **提交**：`0665af1 feat: memo 记事本 + 定时提醒`（分支 `feat/memo-and-reminders-20261019`）。
