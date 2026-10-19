# 定时提醒（"20 分钟后提醒我"，复用 todos 承载）

- 日期：2026-10-19
- 状态：proposed（方案待评审；承载方式已定为**复用 `todos`**）
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

### 4.2 创建与 API

- 复用现有 `create_todo`（`src/api/todos.rs`）；提醒只是「`due_at` 非空」的待办，无需新 API 语义。
- 可选新增 `snooze_reminder(id, until)`（"稍后再提醒"= 更新 `due_at`）。
- `pop_due_reminders(now)`：取 `status='open' AND due_at<=now` 且**本机未触发过**的待办，写 `todo_reminder_fires` 后返回（同一事务，保证一拍只弹一次）。

### 4.3 到期调度

- 挂在既有 5s tick 的完整序列：每拍调用 `pop_due_reminders`，返回列表交给前端展示。
- 幂等由 `todo_reminder_fires` 兜底：同一条只弹一次，直到用户"完成/稍后/忽略"。

### 4.4 触达（两者都要）

- **应用内**：顶部横幅 / 弹窗（复用 `CompanionRevisitHint` 类的应用内提示形态），操作：完成 / 稍后再提醒 / 忽略。
- **系统通知**：新增 `flutter_local_notifications`；前台走应用内、**最小化/失焦走系统通知**（按 `window_manager` 可见性分派）。
- **诚实边界（写进产品说明）**：本地优先应用**不保证应用完全退出/关机后的提醒**——调度随应用运行（同现有后台 worker 模型）。要"系统守时"须常驻/开机自启，超首版范围（§7）。

### 4.5 输入路径（建议两条都做）

- **本地确定性解析**（优先）：正规化 `N 分钟后 / N 小时后 / 明天 9 点 / 今天 18:00`；不依赖 AI、不排队、AI 未配置也能用。→ 生成一条 `due_at` 非空的待办。
- **AI 工具 `create_reminder`**（兜底）：仿 `create_todo`（`src/ai/tool/mod.rs:1435`，走确认门 `ToolPolicy`）处理"下周三下午"这类推算；用 `time_context` 的当前时间与本地时区换算 `due_at`。

### 4.6 同步归属

- 内容（`title/due_at/…`）随 `todos` 已有机制同步。
- **触发状态本机**（`todo_reminder_fires` 不同步），多设备各弹各的。

## 5. 改动清单（预估）

- Rust：migration `50-todo-reminder-fires`（+ 可选 `todos.reminder` 列）；`storage/reminders.rs`（`pop_due_reminders` + 触发去重）；tick 调用链接入；`snapshot.rs` 登记新表（EXCLUDED）与（若加列）`todos` QUERY/apply 更新；`ai/tool/mod.rs` 新增 `create_reminder`。
- Flutter：`rust_bridge_repository.dart` 接 tick；应用内提醒 widget 与操作（完成/稍后/忽略）；`flutter_local_notifications` 依赖与初始化；`todo_view.dart` 若加 `reminder` 列则加标记/过滤；本地时间解析工具。
- 打包：Linux 若引入 libnotify 需补系统依赖（`scripts/package-*.sh` + CI）。
- 测试：Rust 到期只弹一次（`todo_reminder_fires` 幂等）、同步门禁、时间换算；Flutter 桥接往返 + 前台/后台分派。

## 6. 待评审确定的问题

1. **要不要 `reminder` 标记列**：带 `due_at` 的待办一律到点提醒（零加列），还是加列区分/过滤？
2. **去重状态落点**：本机表 `todo_reminder_fires`（推荐，语义一致）还是 `todos.reminded_at`（更简、全局只弹一次）？
3. **"稍后再提醒"**：要做吗（更新 `due_at`）？首版可只做完成/忽略。
4. **系统通知依赖**：同意引入 `flutter_local_notifications` 吗？Linux 打包需补 `libnotify`。
5. **默认落点**：本地解析优先、AI 工具兜底——确认。

## 7. 非目标（首版）

- 不做重复/cron 提醒、不做日历集成。
- 不做"应用完全退出后仍准时"的守护进程 / 开机自启。
- 不把定时提醒混进 `rule_hits`（那是内容触发，语义不同；两者可并存但各自独立）。
