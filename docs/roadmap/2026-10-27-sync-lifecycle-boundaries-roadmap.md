# 同步生命周期边界 Roadmap（无出口状态 + 裸读单例 + UI 不收敛 + 状态不可见）

- 建立：2026-10-27
- 状态：**C0 未开始**（本文档建立时无任何代码改动）。**四条**缺陷：D1/D2 由人工清库暴露，D3 由第 3 次真机复跑暴露（11:15），**D4 由用户实际使用中暴露**（11:40，「桌面一直在写、手机看不到」）。四条同族 —— 状态机走到了边界/终点/暂停状态，却没有把这件事正确地**处理**（D1/D2）或正确地**呈现**（D3/D4）
- 起因：2026-10-27 彻底重置三处配对数据（桌面 `sync_*` 表 / Android 应用数据 / relay mailboxes）后重做配对，app 落进两个从未被走到的边界状态（一个**卡死不可操作**，一个**持续报错刷屏**）；随后配对终于跑通，又暴露第三个：**交接完成但界面不收敛**
- 上游 roadmap：[配对快照交接收敛性](2026-10-26-enrollment-snapshot-convergence-roadmap.md)（C5-A 已落地，真机复跑进行中）
- 口径：**根治，不打补丁**。四条缺陷的共性都是「把状态机的边界/终点当成了正常路径」：一条让状态无出口，一条让错误无领域语义，一条让终点不到达界面

## 使用规则（必须遵守）

1. 每完成一个步骤，**立即** `[ ]`→`[x]`，并在「证据」列写日期 + 可复核事实（测试名/命令输出/行号/实测日志），不写「已完成」这类空话。
2. 步骤失败或前提不成立时改 `[!]` 并写清阻塞原因，不静默跳过。
3. **新回归用例必须做变异验证**（改坏实现看用例是否变红）；**反向也成立**——变异后仍全绿说明该守卫没被覆盖，必须补用例。
4. **任何代码改动前先落本文档的对应步骤 + 申报范围**；范围外的改动单独提，不夹带。
5. 用户可见文案的断言必须随生产语义同步，不许为了过测而弱化断言。
6. 本文档是唯一进度权威；`docs/proposal-backlog.md` 的对应条目同步更新。

---

## 一、现场事实（2026-10-27 10:20–11:35，桌面真机 + relay）

复现前提：手工清空桌面库全部 `sync_*` 表（配对数据全丢），业务数据完好（`messages=594` / `conversations=79` / `chat_run_events=4294` / `ai_provider_configs=10`）。

| # | 事实 | 证据 |
|---|---|---|
| F1 | 清空后先只恢复 `sync_local_vault` + `sync_replica`（authochrity 仍空），桌面 UI **完全卡死**：状态卡显示「同步需要修复」，页面唯一入口是「加入现有数据集」 | 用户截图；`local_sync_settings.dart:146` |
| F2 | 此时「准备本机」按钮**不出现**，因为它只在 `!configured` 分支渲染 | `ui/lib/widgets/local_sync_settings.dart:131`（`configured = state != null && state.state != 'not_configured'`）、`:168`（`_buildPrepareRow`）、`:324`（按钮 label） |
| F3 | 状态计算把「有 vault 或 identity 但无 authority」钉成 `requires_repair`，**没有任何条件能让它离开该状态** | `src/api/sync/invite.rs:144-160`：`if !authority { if vault.is_some() \|\| identity.is_some() { "requires_repair" } else { "not_configured" } }` |
| F4 | 全 `ui/lib` 搜索「重置 / 重新准备 / 清除同步 / 撤销本机 / 离开同步 / 删除本机」→ **无任何同步身份重置入口** | `grep -rn` 命中项全部与同步无关（字体重置、会话窗口重置等） |
| F5 | 产品文案自认唯一出路是清应用数据 | `ui/lib/screens/settings_parts/security.dart:88`：「遗忘密码后无法找回，只能清除应用数据重置」 |
| F6 | 清空 `sync_local_vault` + `sync_replica` 后进 `not_configured`，「准备本机」出现，配对可重新开始 | 清库后我按 `invite.rs` 分支模拟：`vault=0 replica=0 authority=0` → `not_configured`；用户随后成功准备本机并配对到 `preparing_snapshot` |
| F7 | 未配置同步（`sync_local_vault` 空）时，桌面日志**每 tick 一条** `analysis worker failed: AnyhowException(Query returned no rows)`，持续到回填 vault 行才停 | `/tmp/desk_run.log`（恢复 vault 前后对比该行计数：有 → 0） |
| F8 | 错误无领域语义：`Query returned no rows` 是 rusqlite 原始文本，不带任何 `sync_*` 前缀 | `src/storage/provider_keys.rs:32-37` 裸 `query_row`；对照同表的正确写法 `src/storage/sync/vault.rs:69-73`（`.optional()?` + `.context("sync_vault_missing")`） |
| F9 | 同类裸读单例共 8 处（`sync_replica` ×5、`sync_local_vault` ×2、`sync_gc_state` ×1），其中只有 `provider_master_key` 会在**同步未初始化时**被非同步路径调用 | `capture.rs:126`/`:132`、`delivery.rs:97`、`receive.rs:61`、`snapshot/exporter.rs:73`、`membership/authority.rs:187`、`provider_conflicts.rs:166`、`gc.rs:92` |
| F10 | **交接数据层已完成，手机 UI 未收敛**：手机停在「等待成员授权与初始同步」+ 转圈（持续约 1 小时），但三条独立证据都指向已完成 —— ①桌面 `sync_snapshot_exports af56a582 state=acknowledged transport=blob blob_index=173`；②桌面 outbox `enrollment-complete:05a7273e…` 为 `acknowledged`（该回执只由 `complete_enrollment` 成功路径的 `queue_sync_wire_receipt` 产生，`src/storage/sync/enrollment.rs:379-386`）；③手机已拿到快照内容（知识库 8 条、2026-10-24/10-26 的历史对话、3+ 条 AI 提供商配置，而手机是 `adb uninstall` 后全新安装） | `/tmp/ph5.png`；桌面库查询 |
| F11 | **`force-stop` + 重启 app 后 UI 立刻正确**：显示「同步已暂停（本机密钥已锁定）· 本机 PEUMOO · 成员设备」，且再无「等待成员授权」字样 | `/tmp/ph5.png`（重启后）；重启前 `/tmp/ph2.png` 阶段为转圈页 |
| F12 | 同期手机 `adb logcat -d` 过滤 `flutter` 或 `sync` **零命中** —— 既无 `tick failed` 也无阶段推进日志，说明 controller 链既没在报错也没在推进（与下方同族前科一致） | `adb logcat -d` 输出 |
| F13 | **一条回执在卡住窗口内被丢弃**：桌面 `sync_wire_rejections` 有 1 行 `sync_receipt_identity_mismatch`，`first_seen 11:04:16`、`last_seen 11:04:17`、`attempts=3`、`dropped_at` 已置（=该回执被永久丢弃），`sender_mailbox_id` 非空（来自手机）。判定条件见 `src/storage/sync/receipt.rs:99-107`：`dataset_id` / `target_device_id` / `acknowledging_device_id` 三者任一不符即拒。**丢弃 = 发送端永远收不到这条 ack**，故这是 D3 的一个候选根因而非旁支。注意 `mail_id` 列存的是 relay 邮件 id、`message_id` 列存 `envelope-receipt-{hash(原信令 id)}`，**无法由该行反查是哪条信令** | 桌面库 `sync_wire_rejections` 查询 |
| F14 | **D4 现场（用户主动报障）**：11:19–11:35 期间桌面持续写入（`messages` 594 → 609 → 624），手机停在对话页显示的是 10-24/10-26 的**旧内容**，看不到任何新对话。用户原话「桌面端一直在增加内容（对话），但是我看手机上没有同步过去」 | 桌面库 `messages` 计数三次采样 + 用户口述 |
| F15 | 同一时刻 relay 手机信箱**堆着 13 封 / 42,445 B 无人取**，桌面 outbox 恒定 10 条 `op-*` `pending`（attempts 2–3、`available_at` 一直在「刚刚」）—— 即：数据早就到 relay 了，手机不消费、不回执 | relay 信箱查询 + `sync_wire_outbox` |
| F16 | **手机端唯一的提示藏在「同步」子页里**（「同步已暂停（本机密钥已锁定）」+「解锁本机密钥」按钮）；对话页 / 知识页 / 侧边栏**无任何横幅、角标、图标**表明同步已暂停。桌面端同样没有任何「对端未确认」的可见信号（10 条 pending 只在库里） | `/tmp/ph5.png`（同步子页）vs `/tmp/ph6.png`（对话页）；`local_sync_settings.dart` / `joiner_sync_pairing_settings.dart` 无全局态位 |
| F17 | **解锁后链路自证通畅**（反过来排除「同步实现坏了」）：11:34:13 手机信箱 13 → **0**、桌面信箱 1 → **11 封 / 12,327 B**（手机发来的）；11:35:23 桌面已消费回 1 封、手机信箱又被新推 13 封；`sync_wire_outbox` acknowledged 21 → 54 → 64，pending 恒为**最新**的 10 条（旧 1374~1383 已 ack，新 1384~1393 在途） | 三次 relay + 桌面库采样（11:19 / 11:34 / 11:35） |

### D2 的触发链（实测拼接）

```
Dart AI 分析 worker（每 tick）
  ui/lib/bridge/rust_bridge_parts/worker_reminders.dart:104-112
    → api.tickKnowledgeInsights() / 其他 ai activity
    → Store::read_provider_key()            src/storage/provider.rs:118 / :265 / :408、src/storage/sync/providers.rs:145
    → Store::provider_master_key()          src/storage/provider_keys.rs:32
    → query_row("SELECT revision FROM sync_local_vault WHERE singleton=1")   ← 无 .optional()，空表即 rusqlite 错误
  catch → debugPrint('analysis worker failed: $e')  → 下一 tick 重试
```

关键点：这条链**不经过** `sync_identity()` 的 `sync_not_initialized` 门（`capture.rs:5-12` 有 `.optional()`），所以同步未启用时它不是「跳过」，而是「每拍抛一次」。

---

## 二、缺陷 D1：`requires_repair` 是无出口状态

**用户路径**：本机曾有同步身份（vault + replica 存在）但授权/名册丢失 → UI 显示「同步需要修复」→ 用户既不能修复也不能重来，也不能邀请设备 → **只能手动清数据库**（本次就是这样绕过的）。

**为什么这不是「用户乱清库才有的怪状态」**——仓库里已有多个正常进入路径：

- `src/storage/sync/vault.rs:79` → `sync_vault_restored_revision_requires_repair`（从备份恢复数据库、revision 不匹配）
- `src/storage/sync/enrollment.rs:99` → `sync_enrollment_completion_missing_requires_repair`（设备交接写到一半）
- `src/sync/enrollment.rs:116` → `sync_enrollment_bootstrap_missing_requires_repair`（join 中断后 bootstrap 记录缺失）

这三条都是真机可达的（掉电、杀进程、恢复备份），不是运维臆想。

**判定「修好」的标准**（不接受「加个提示文案」）：

1. 用户在 `requires_repair` 状态下有**明确出口**，且出口语义可解释：「本机同步身份已损坏 → 清除同步身份（保留本地笔记）」；
2. 出口执行后状态机走到 `not_configured`，随后能正常「准备本机」；
3. 出口必须**显式二次确认**，且明确告知副作用（见 §六 关联发现：provider keys 会失效）；
4. 出口只在真正损坏时可用，不能顺手变成「一键重置」按钮。

---

## 三、缺陷 D2：未配置时裸读单例，错误无领域语义

**现象**：同步未配置 → 后台 AI worker 每 tick 抛 `AnyhowException(Query returned no rows)`，只 `debugPrint`、不中断、无限重试。用户看到的是日志噪声 + AI 功能静默不可用。

**两层问题（都要修）**：

1. **存储层**：单例行读取没有单一接缝。`provider_master_key` 裸读，而同表的 `load_local_vault` 用了 `.optional()? + context("sync_vault_missing")`——同一个仓库里两种写法并存，说明这是**漏改**而不是设计选择。
2. **worker 层**：`worker_reminders.dart:112` 的 catch 把「未配置」这种**终态**和「抢锁 BUSY」这种**瞬时**失败混为一谈，一律重试。终态应当退避或停止，否则日志永远在刷。

**判定「修好」的标准**：

1. 同步未配置时，`provider_master_key()` 返回领域错误（如 `sync_not_configured`）或 `None`，不再漏 rusqlite 文本；
2. 其余 7 处裸读单例统一走同一个接缝，行为一致（要么全 `optional` + 领域错误，要么全部经 `sync_identity()` 前置门禁）；
3. worker 对「未配置/未初始化」类错误**不再每 tick 重试**（降级为一次性提示或长退避），对瞬时错误保持现有重试；
4. 有回归用例覆盖：空表读单例 → 断言领域错误，worker 侧断言「终态不重试」。

---

## 三之二、缺陷 D3：交接完成后手机 UI 不收敛到终态

**现象**：数据层已完成（F10 三条证据），手机界面却一直停在「等待成员授权与初始同步」并转圈；无错误提示、无「重试/完成」出口；`force-stop` 后重启即恢复正确状态（F11）。

**已排除**：
- 不是传输没完成 —— relay 残留 0（手机信箱 `mail=0 blob=0 chunk=0`）、桌面导出 `acknowledged`；
- ~~不是手机拒收信令 —— 桌面 `sync_wire_rejections` 为空~~ **该条已撤回（2026-10-27 11:25 复核）**：当时查确实为空，但随后的复核发现 1 行 `sync_receipt_identity_mismatch`（11:04:16，attempts=3，`dropped_at` 已置），见 F13 —— 它落在卡住窗口内，因此「手机回执被丢弃」**是待查候选根因，不能排除**；
- 不是 UI 根本没有看门狗 —— `sync_enrollment_controller.dart:34-40` 已有 `stalledTick` 看门狗，且注释记录了 2026-10-24 同族事故（「界面停在最后一帧（例如『等待成员授权与初始同步 0/174』）」）。

**待定位方向**（按可能性排序，需真机证据而非读代码猜）：
0. **（新，最可疑）回执被 `sync_receipt_identity_mismatch` 永久丢弃**（F13）—— 丢 ack 会让发送端永远重推同一条，而接收端的阶段推进又依赖「对端已确认」，形成「数据都到位、阶段不动」的错配。查法：在 `receipt.rs:99-107` 拒收分支补一条带三个身份字段实际值的诊断日志，复现一次即可定位是哪一字段不符（尤其怀疑 `dataset_id`：本轮曾整体重建 dataset/installation）；
1. `_needsWork`/`states` 的收敛判据 —— 交接记录已 complete 但 `states` 仍含未完成项，导致 controller 继续 2s 轮询而 `phase` 永不为 `complete`；
2. tick 返回的 DTO 与 `joiner_sync_pairing_settings.dart:123-127` 的读取口径不一致（`controller.state` 为 null → 走默认「等待」分支）；
3. controller 停摆（`_schedule()` 未重排），此时应有 `tick stalled` 日志 —— 但 F12 显示零日志，该分支亦需确认日志是否只在 debug 变体输出（手机是 release AOT）。

**判定「修好」的标准**：
1. 一次全新配对，**不重启 app**，手机界面在交接完成后的一个轮询周期（≤2×interval）内自行切到终态；
2. 终态切换前若数据层已完成，界面不得继续展示「等待成员授权与初始同步」+ 转圈；
3. 有回归用例（Dart 单测或 controller 级用例）覆盖「states 变为已完成 → 界面离开等待页」，并做变异验证；
4. 真机复跑一次确认（含不重启路径）。

---

## 三之三、缺陷 D4：同步暂停/积压状态在主界面不可见

**现象**：手机密钥被锁定时，同步实际停止，但用户在主界面（对话/知识）看不到任何异常 —— 桌面在写、手机不动，只能靠「内容对不上」自己发现（F14）。用户为此主动报障一次。手机端唯一的提示在「同步」子页（F15、F16）。

**为什么这不是 D3 的重复**：
- D3 是**状态机到终点却没被推导出来**（数据 complete、UI 不 complete）；
- D4 是**状态机明确处于「暂停/积压」，界面不披露**（数据层没坏，UI 也没错，只是没说）；
- 两者修法不同：D3 修收敛逻辑，D4 加可见性。可以合并实现成「一个全局同步状态位」，但**判定标准必须分开写**，否则又会出现「D3 修完了所以同步一定看得见」这种假闭环。

**为什么必须修（而不是「用户自己去看同步页就行」）**：
1. 「暂停」是一个**需要用户动作才能解除**的状态（去输密码），不被披露就等于静默丢数据 —— 与「遗忘密码只能清除应用数据」（§一 F5）叠加后，用户可能连续几天以为在同步；
2. 同一族的 D1 已经证明过这个模式有多贵：一个不可见的边界状态直接导致用户走到「清库重来」；
3. 桌面侧也有对称问题：10 条 `op-*` 长期 pending 却不可见，用户无法区分「对方没解锁」和「网络差」（F15）。

**判定「修好」的标准**：
1. 手机端在**任一主界面**（对话 / 知识 / 回顾）都能看到同步处于「已暂停 / 待解锁」，且点击直达解锁入口；状态解除后提示在一个轮询周期内自行消失；
2. 「有待解锁造成的积压」与「单纯的网络延迟」在界面上**可区分**（例如显示积压条数/字节而不是笼统的「同步中」）；
3. 桌面端能看到「对端未确认」的条目数与最近一次确认时间（`sync_wire_outbox` pending 的可读投影，不得暴露密文/内部 id）；
4. 文案不含内部术语（`pending` / `relay` / `watermark`），且不制造焦虑（不是错误弹窗，是状态条）；
5. 有 UI 层用例覆盖「未解锁 → 主界面出现提示」「解锁 → 提示消失」，并做变异验证（仓内已有 106 个 `ui/test/*_test.dart`，沿用 `_widget_test.dart` 形状，不需要新基建）。**注意：`fvm flutter test` 不在既有四道门禁命令里**，C8 必须自己跑并把输出贴进证据列，否则新用例等于不存在。

---

## 四、步骤

| # | 步骤 | 证据 | 状态 |
|---|---|---|---|
| C0 | **定性验证**：为 F2/F3 写一条用例，构造 `authority` 空 + `vault`/`replica` 存在的库，断言 `get_local_sync_state()` == `requires_repair` 且**当前无任何 API 能把状态推回 `not_configured`**（即把 D1 钉成红用例）。为 D2 写一条用例：空 `sync_local_vault` 调 `provider_master_key()`，断言当前返回的错误是 rusqlite 文本（红） | 两条用例首跑**均已 FAILED（2026-10-27 12:47）**：①`storage::sync::tests::local_identity::a_half_configured_local_identity_has_no_exit_back_to_not_configured` 红在 `src/storage/sync/tests/local_identity.rs:89`，入口结果 `create_sync_vault` / `create_sync_join_vault` 均 `sync_vault_already_exists`、`initialize_sync_replica` 返回 Ok 但 authority 仍空 ⇒ 形状不变；②`storage::provider_keys::tests::unconfigured_master_key_read_reports_a_domain_error_not_rusqlite_text` 红在 `src/storage/provider_keys/tests.rs:559`，实际漏出 `Query returned no rows`。首跑汇总 `0 passed; 2 failed; 985 filtered`。D1 那条首次编写时因 `create_sync_vault` 不建 `sync_replica`（身份来自 `initialize_sync_replica`）而 `unwrap` 空值，改成先取 `vault.device()` 后才红在正确断言上 —— 记一笔，避免「用例红了就算钉住」 | `[x]`
| C1 | **D2 存储层**：抽出单例读取接缝（如 `Store::read_singleton(table)` 或 `require_*` 系列），`provider_master_key` 改为领域错误 `sync_not_configured`；其余 7 处裸读统一（范围限于 `sync_replica`/`sync_local_vault`/`sync_gc_state` 单例读，不动其它查询） | **完成（2026-10-27 13:10）**。新增接缝 `src/storage/sync/singleton.rs`：`singleton_value` 一个出口 + 5 个访问器（`replica_next_sequence` / `replica_logical_clock` / `local_vault_basis` / `local_vault_installation` / `local_vault_revision`），缺行一律 `sync_not_configured`。改造点 12 处：`capture.rs`×2、`delivery.rs`、`receive.rs`、`snapshot/exporter.rs`（视图连接）、`membership/authority.rs`、`provider_conflicts.rs`、`provider_keys.rs`×3，外加同缺陷但不在 `src/storage` 层的 `src/sync/runtime.rs:392`、`src/sync/datasets.rs:281`（同一批修，已申报）。`grep` 审计：两张单例表的裸读（无 `.optional()`）**13 处 → 1 处**，仅剩 `src/sync/transport_relay_smoke_tests/enrollment_ui.rs:48` 的测试辅助（故意 `.unwrap()` 断言存在）。用例 `storage::sync::tests::singleton_seam::*` 两条转绿（空库全领域错误 / 配置后读真值）；变异验证：把错误码换成 `sync_vault_missing` → 缺行用例红；把 `local_vault_basis` 的 revision 恒返 0 → 读真值用例红（`singleton_seam.rs:64`），均已还原。回归面：`cargo test --lib -- storage::sync provider_keys datasets` **190 passed / 1 failed**（唯一红是 C0 的 D1 用例，属预期红） | `[x]` |
| C2 | **D2 worker 层**：`worker_reminders.dart` 区分终态错误（`sync_not_configured`/`sync_not_initialized`/`sync_vault_missing`）与瞬时错误；终态不重试，记一次日志 | **完成（2026-10-27 13:35），实测前后对比**：新增 `ui/lib/sync/background_failure_policy.dart`（纯逻辑、时钟可注入）：终态错误码（`sync_not_configured`/`sync_not_initialized`/`sync_vault_missing`）→ 指数退避 2→15 min 封顶 + **同一错误码只记一次**；瞬时错误（抢锁 BUSY/网络）→ 原节奏原样重试。接线：`worker_reminders.dart` 的 `_wakeAnalysisWorker` 在退避窗口内直接返回、`catch` 只记一次、成功路径 `recordSuccess()`；`rust_bridge_repository.dart:126` 的 `localSync.onReady` 先 `reset()` 再唤醒（用户解锁/准备本机后立刻恢复，不等 15 min）。**实测**（真实库副本 `ELSEWHEN_DATA_DIR=/tmp/c2content`，清空 38 张 `sync_*` 表但保留 `messages=640`/`wiki_pages=134`，确保真走到 provider key 路径）：**前**（旧 Rust release lib + 新 Dart 策略）`analysis worker failed` **5 条 / 130 s**，内容全是 `AnyhowException(Query returned no rows)`；**后**（`cargo build --release` 1m20s 后）**1 条 / 200 s**，内容为 `AnyhowException(sync_not_configured)`。用例 `test/background_failure_policy_test.dart` 7 条全绿；变异验证：把 `isTerminal` 改成恒 false → 6 条红、去掉「同因只记一次」→ 1 条红、退避封顶放大到 1h → 1 条红（均已还原、复跑全绿）；`sync_worker_pause_bridge_test.dart` 仍绿；`fvm flutter analyze` 133 issues / 0 error（与基线一致） | `[x]` |
| C3 | **D1 出口**：新增重置本机的 Rust API + UI 入口（状态卡在 `requires_repair` 时出现「清除本机同步身份」，二次确认写明副作用），执行后清 `sync_replica`/`sync_local_vault`/`sync_authority*`/`sync_members`/`sync_member_keys`/游标/outbox/exports，**保留全部业务数据**；随后状态回 `not_configured` | 用例断言执行前后业务表行数不变、同步表归零、状态迁移 | `[x]` 2026-10-27 |


| C4 | **用例与变异验证**：C0 两条红用例转绿；每条新用例做变异验证（把修复改坏 → 必须变红；若仍绿则补负向用例） | **完成（2026-10-27）**。①C0 两条红用例均已转绿：`local_identity::a_half_configured_local_identity_has_no_exit_back_to_not_configured`（C3 出口补上后）、`provider_keys::tests::unconfigured_master_key_read_reports_a_domain_error_not_rusqlite_text`（C1 接缝补上后）。②逐条变异（改坏实现 → 必须变红）：C1 接缝 `singleton_value` 退回裸 `query_row` ⇒ D2 用例 + `singleton_seam::every_singleton_accessor_reports_sync_not_configured_on_an_empty_store` 双双红；C2 `isTerminal` 恒 false ⇒ 6 红、去掉「同因只记一次」⇒ 1 红、退避封顶 1h ⇒ 1 红；C3 去掉 authority 守卫 ⇒ 1 红、跳过清 `sync_replica` ⇒ 2 红、不清 `provider_key_secrets` ⇒ 1 红、准入闸门放行一切 ⇒ 1 红；Dart 侧 A 无条件显示入口 ⇒ 2 红、B 确认按钮不看勾选 ⇒ 1 红、C 删「Key 会失效」文案 ⇒ 1 红、D 清完不刷新 ⇒ 1 红。全部还原后复绿。③**教训已固化**：变异必须先确认「能编译再跑」—— 首版「从 38 元素数组里删一项」直接编译失败，什么都没验证到却看起来是绿的；后改成运行时把 `sync_replica` 换成 `sync_frontiers` 才真正变红。④**被判定为「未被覆盖」的变异**：无（本轮所有变异均至少打红一条用例；若日后出现全绿，按纪律必须补负向用例而不是放过）。 | `[x]` 2026-10-27 |
| C5 | **门禁四道全绿**：`cargo test --all-targets`、`cd relay && npm test`、`cd ui && fvm flutter analyze`、`scripts/regen.sh` | **完成（2026-10-27 15:0x）**，四条命令逐条贴输出：①`cargo test --all-targets` ⇒ `test result: ok. 974 passed; 0 failed; 20 ignored`（+9 条相对上轮 965，全部是 C1/C3 新增），`CARGO_EXIT=0`；②`cd relay && npm test` ⇒ `tests 49 / pass 49 / fail 0`，`RELAY_EXIT=0`；③`cd ui && fvm flutter analyze` ⇒ `133 issues found`、`grep -cE "^\s+error "` = **0**（与基线同：133 全是既有 warning/info，退出码因 warning 为 1，判据按「0 error」）；④`scripts/regen.sh` ⇒ `REGEN_EXIT=0`，末行「生成的 Dart/Rust bridge 与 target/release/libelsewhen.so 已保持一致」，跑完 `git status` 仅剩本文档改动（生成物与源码一致，无漂移）。**C3 修正后按纪律重跑**（改了 `reset.rs`/`repair.rs` 就必须重跑，不能沿用旧绿）：`cargo test --all-targets` ⇒ `974 passed; 0 failed; 20 ignored`、`EXIT=0`；`scripts/regen.sh` ⇒ `REGEN_EXIT=0` 且 `git status` 干净（无生成物漂移）。relay 与 analyze 两棵树自上次全绿后无改动，沿用 15:0x 结果 | `[x]` 2026-10-27 |
| C6 | **真机验收**：在真机上把库造成 `requires_repair`（不改代码、只删 `sync_authority*`），点新出口 → 状态回「同步未开启」→ 能重新准备本机并完成一次配对；同时确认日志里 `analysis worker failed: … Query returned no rows` 不再出现 | 截图 + 日志 + 库行数 | `[x]` **真机通过（2026-10-27 17:2x，设备恢复可达后）**。①造现场：`adb shell run-as dev.elsewhen.elsewhen_ui` 先 `cp elsewhen.db elsewhen.db.bak`，在副本上 `delete from sync_authority` 后写回，核对 `authority=0` / `vault=1`，`md5sum` 双向一致 `056e298ba79176659b892ab38d2d7e42`。②启动 app，同步页直接显示「同步需要修复」+ 可点的「清除本机同步身份」出口。③弹窗写明三条后果 + 「不删业务数据」，**未勾选时确认键禁用**；勾选 → 确认 → 回到「同步未开启」起点并给出重新配对入口，**全程未重启 app**（截图 c6c~c6f）。④库核对（`sqlite3` 逐表）：`sync_replica`/`sync_local_vault`/`sync_authority`/`sync_members`/`sync_gc_state`/`sync_wire_outbox`/`sync_wire_rejections`/`sync_snapshot_exports` **全 0**；业务数据 `messages=645`、`conversations=79`、`wiki_pages=134` **全留**。⑤**副作用声明**：本步把手机打回未配对（这是断言的一部分），⇒ D3 真机复核必须先重新配对（见 C7 ⑨ 与 §七 18:10 条） |
| C7 | **D3 定位与修复**：先取真机证据（①`receipt.rs:99-107` 拒收分支加三字段诊断日志，定位 F13 是哪一身份不符并修；②手机 release 构建下 controller 的阶段/错误日志出口；必要时加一次性诊断日志并重跑），再按 §三之二 的判据修「交接完成后 UI 不收敛」；补用例覆盖「回执被拒 → 发送端不无限重推」与「阶段到终态 → UI 离开等待页」，均做变异验证 | **定位器的代码侧已完成（2026-10-27），「定位」本身仍等真机**。①**诊断进数据，不进日志**：release 包拿不到调试日志，且拒收原因本来就会随 `sync_wire_rejections.last_error` 落库（`src/sync/engine.rs:498` 用 `format!("{err:#}")` 整条写入），所以把身份字段写进**错误文本**比加 `eprintln!` 有用得多。新增 `src/sync/protocol.rs::receipt_identity_mismatch(kind, fields)`：输出 `sync_receipt_identity_mismatch kind=receipt dataset expected=<本机> actual=<信封> target … ack …`，**只列真正不符的字段**；开头保留原错误码，既有检索/前缀匹配不变。两处调用点改造：`receipt.rs:99` 与 `engine.rs:709`（后者把「身份不符」与「这封信不该被回执」拆成两个分支 —— 混在一个 `ensure!` 里会让排查把两种原因都读成 identity_mismatch）。②**用例**：`storage::sync::tests::receipts_deletion::a_rejected_receipt_says_which_identity_field_disagreed` —— 冒充 ack ⇒ 文本点名 `ack expected=stranger actual=<peer>` 且**不含** dataset/target（相等项不许出现）；错 target ⇒ 点名 `target … actual=someone-else` 且不含 ack。**变异**：A 退回裸错误码 ⇒ 红；B 去掉「只列不符项」的过滤 ⇒ 红（实测文本里三个字段全出，正是要防的噪声）。均还原后复绿。③**门禁**：`cargo test --all-targets` **977 passed / 0 failed / 20 ignored**。④**仍缺（需真机）**：真机 logcat/库里读到 F13 的确切不符字段、按 §三之二 判据修「交接完成后 UI 不收敛」、以及「回执被拒 → 发送端不无限重推」「阶段到终态 → UI 离开等待页」两条用例。⑤**口径诚实**：`engine.rs` 新增的 `sync_receipt_not_receiptable` 分支目前**没有直接用例**（该分支此前也无覆盖），属既有缺口，本次未扩大。⑥**2026-10-27 16:12 真机取证（debug 包 + `run-as`）—— 定位结论推翻了 F13**：设备库 `sync_wire_rejections` **唯一一行** = `message_id=enrollment-complete:05a7273e…`、`last_error=sync_wire_kind_unsupported`、`attempts=3`、`first_seen 11:03:54` → `dropped 11:03:56`。**不是** `sync_receipt_identity_mismatch`（那条只出现在**桌面**库，是另一端的另一条历史记录，我此前把它当成了 F13 的现场）。机制：交接收尾时 owner 重投 `enrollment-complete:`（它没收到 joiner 的回执），而 joiner 的 enrollment 会话此刻**已经结束** → 这封信走 `src/sync/engine.rs` 的**普通**分派（`engine/enrollment.rs` 的会话分支已不匹配）→ 一路落到文件末尾的 `ensure!(message_id.starts_with("receipt-"), "sync_wire_kind_unsupported")`（`:753`）→ 被记成「信号种类不支持」，连拒 3 次后丢弃。危害三条：①**真话变假话**（我们完全支持这个信号，只是没有会话）；②被丢弃 ⇒ 对端继续重投直到自己超时；③拒收表被这种正常收尾噪声占满，真正的错误反而看不见。⑦**修法（与 D1~D4 同族：终点没被正确推导）**：普通分派里补一条「交接已完成后的迟到重投」——发送方**已是我名册里的 active 成员**且来的是 enrollment/snapshot 控制帧 ⇒ 幂等收尾（消费掉、静默不回退缓存），不再报 kind_unsupported；**发送方不是成员时仍然拒收**（否则会把真错误一起吞掉，正是「变异通过≠覆盖充分」的坑）。⑧用例（均做变异验证）：成员重投 `enrollment-complete:` ⇒ 返回 true、`sync_wire_rejections` 不增行、成员关系不变；非成员发 `enrollment-offer:` ⇒ 仍然拒收。⑨**连带结论**：D3「交接完成后手机 UI 不收敛」的最可能机制就是这条 —— 收尾回执永远到不了对端，手机等一个不会来的确认。真机复跑后按 §三之二 判据复核，若仍不收敛则修 UI 侧。⑩**修法已落地（2026-10-27）**：`src/sync/protocol.rs` 新增 `pub(crate) fn is_enrollment_control_id`（认 `membership:` / `enrollment-` / `snapshot-start:` / `snapshot-end:` 四类前缀）、`src/storage/sync/delivery.rs` 新增 `sync_member_is_active(device_id)`（`SELECT EXISTS(… sync_members WHERE device_id=?1 AND active=1)`），并在 `src/sync/engine.rs:753` 那条 `ensure!(message_id.starts_with("receipt-"), "sync_wire_kind_unsupported")` **之前**加幂等收尾分支：`if crate::sync::protocol::is_enrollment_control_id(&message_id) && store.sync_member_is_active(sender)? { return Ok(true); }`（返回 `true` 才会被 ack、才会从 relay 删掉）。⑪**用例与变异**：`engine::tests::a_late_enrollment_frame_from_a_member_is_retired_quietly_not_rejected`（断言 `accept()==true`、`sync_wire_rejections` 不增行、名册行数以 `members_before` 为基线不变）与 `a_late_enrollment_frame_from_a_stranger_is_still_rejected`（错误串含 `sync_wire_kind_unsupported`）；变异 A 删收尾分支 → 成员用例 FAILED、变异 B 去掉成员身份判定 → stranger 用例 FAILED，`cp` 还原后模块级 `cargo test --lib -- sync::engine sync::storage::sync::membership storage::sync::membership` = **45 passed / 0 failed**。⑫**真机复核完成（2026-10-28 10:10–10:55）**：桌面撤销死身份后重新「准备本机」，手机加入新数据集成功；重投的收尾帧不再进拒收表（D7 拒收在切换数据集后自愈清零，见 §六之二）；手机从 staging 走到快照 applied、名册 active、桌面侧导出 acknowledged —— ⑨ 的链路在真机走通。**注意**：⑤「第五步转圈」仍发生，但定性为 **C11（结构性 UI 终态缺失）**，与本行修法无关，已在 C11 单独立档并完成修复 | 定位器代码 + 用例红绿；**定位已完成（推翻了 F13）**；**修复已落地且用例+变异验证**；**真机复核已通过** | `[x]` 2026-10-28 |
| C8 | **D4 状态可见**：手机端加全局同步状态位（主界面状态条/角标 → 点击直达解锁），区分「待解锁积压（显示条数/字节）」与「网络延迟」；桌面端把 `sync_wire_outbox` 的 pending 投影成可读的「对端未确认 N 条」 | **完成（2026-10-27）**。①**口径先立**：`LocalSyncStateDto` 新增 `pending_wire_envelopes` + `pending_bytes`，由 `Store::pending_wire_backlog()`（`src/storage/sync/delivery.rs`）从 `sync_wire_outbox WHERE state='pending'` 取 `COUNT(*)` 与 `COALESCE(SUM(LENGTH(encrypted_envelope)),0)` —— `pending_operations` 是「本机还有多少操作没被回执」，新字段是「对端还没确认收到几条」，两者不可互替。真库实测当时是 `pending=12 条 / 58,590 B`、`acknowledged=94`，正好是「显示 64 条待确认但其实在等对端」那种误导的现场。②**手机状态位**：`ui/lib/sync/sync_backlog_notice.dart`（纯判定）+ `ui/lib/widgets/sync_backlog_banner.dart`（只在真有积压时出现，`locked` ⇒ 「待解锁积压 N 条 · X KB · 解开本机密钥后自动同步」且可点直达同步页；`local_ready` ⇒ 「同步延迟：对端未确认 N 条 · X KB，正在重试」且**不给假按钮**；`not_configured`/`requires_repair` ⇒ 不显示，各有自己的入口）。挂在 `main_screen.dart` 移动端主界面（`aiProviderSetupHint`→`CompanionRevisitHint` 之后），点击 `sidebarTabProvider.set(SidebarTab.sync)`。③**桌面**：`SyncBacklogLine`（`sync_tick_status.dart`）在同步页投递状态行下方显示「对端未确认 N 条 · X KB」，0 时不渲染。④**用例**：Rust `api::sync::invite::tests` 2 条（只算 pending / 空集不报错且 COALESCE 兜住 NULL）；Dart `ui/test/sync_backlog_banner_test.dart` 9 条（判定 4 + 手机状态位 3 + 桌面行 2）⇒ `fvm flutter test` **全仓 476 passed**。⑤**变异验证**：Rust ①去掉 `state='pending'` 过滤 ⇒ 红（`left (3,1029)` vs `right (2,30)`）；②去掉 `COALESCE` ⇒ 空集用例红；Dart A `actionable` 恒 true ⇒ 2 红；B 零积压也返回 notice ⇒ 2 红；C 锁定/就绪两类对调 ⇒ 2 红。全部还原后复绿。⑥**门禁**：`cargo test --all-targets` **976 passed / 0 failed**（首跑 975+1 failed 的 `ai::generation::…observable_run_checkpoints…` 是并行负载下的既有 flake：单跑 3/3 绿、模块级 9/9 绿、全量复跑 976 全绿）；`fvm flutter test` 476 passed；`fvm flutter analyze` 133 issues / **0 error**；`scripts/regen.sh` EXIT=0。⑧**真库副本核对（C3 教训照做：纯 fixture 覆盖不到真 schema）**：把线上库 `.backup` 到 `/tmp/c8real.db`，临时 `diag_backlog_on_real_db_copy`（已删，`grep -rn diag_backlog src`=0）用 `Store::open` 走一遍 `pending_wire_backlog()` ⇒ `(0, 0)`，与 `sqlite3` 直接算的 `count(*)=0 / sum=0` 一致；`sync_wire_outbox` 当时 `total=132 acknowledged=132`（523,903 B 已全部确认）—— 也就是说线上此刻确实没有积压，状态位此刻应当**不出现**，这是正确结果而不是「测不出来」。⑨**待你确认的口径**：状态位的条数按 `sync_wire_outbox` 的 `state='pending'` 计，不含 `state='acknowledged'` 的历史行（后者是审计，不是积压）。⑦**文案待你报审**：本行四句用户可见文案为我起草（见上），改文案只需动 `sync_backlog_notice.dart` 的 `message` 与 `SyncBacklogLine` 一行。⑩**C9 现场又暴露两个缺陷，已修（2026-10-27 16:2x–16:5x）**：缺陷 1「隐形」——`SyncBacklogBanner.initState` 只 `refresh()` 一次、此后再不重读，主界面状态位整整 8 分钟不出现（重启才立刻显示「对端未确认 1 条 · 2.7 KB」）；缺陷 2「文案重复」——渲染成 `对端未确认 1 条 · 2.7 KB（1 条），正在重试`（`_size` 又拼了一次条数）。修法：①`sync_backlog_notice.dart::_size` 只返回 `formatBacklogBytes(bytes)`，两句 `message` 整句重写为 `待解锁积压 $envelopes 条 · $_size，解开本机密钥后自动同步` / `同步延迟：对端未确认 $envelopes 条 · $_size，正在重试`；②`sync_setup_controller.dart` 新增 `attachBacklogSource(Listenable)` / `_onBacklogTick()` / `refreshBacklog()`（静默重读，只比 `state`/`pendingWireEnvelopes`/`pendingBytes`，值不变不 notify；`busy`/`joiner.active`/`_backlogRefreshing` 早退）、`dispose()` 首两行摘监听，并在 `rust_bridge_repository.dart` 构造函数里 `localSync.attachBacklogSource(syncDeliveryStatus)` —— 即状态类 DTO **必须挂到每拍 tick 都会 notify 的信号上**。⑪**用例与变异**：`ui/test/sync_backlog_live_refresh_test.dart` 3 条（0→1 自动浮出且用户可见文案整句精确比对；重复心跳 `stateReads>1` 但 `notifications==0`；`dispose` 后 `isAttached==false` 且不再读库）；变异 4 条：A 不注册监听 → 2 红、B 去掉「值不变就不 notify」→ 1 红、C `dispose` 不摘监听 → **首轮全绿**（`_disposed` 早退已挡住读库）⇒ 补 `_TickSignal.isAttached`（`hasListeners`）判据后红、D `_size` 拼回条数 → 2 红；均还原复绿。§十二 门禁：`fvm flutter test` **479 passed**、`cd ui && fvm flutter analyze` **133 issues / 0 error** | `[x]` 2026-10-27 |
| C9 | **解锁路径真机验证（独立于其它步骤，可最先做）**：全新身份 + 全新安装 + app 重启后，确认「解锁本机密钥」能成功（本轮首次走到成员设备状态，该路径**从未成功验证过**）；解锁后积压在一个轮询周期内灌入、`sync_wire_outbox` 的 pending 归零、两端内容一致 | 解锁前后 relay 信箱计数 + 桌面 pending 计数 + 两端同一条消息截图 | `[x]` **真机通过（2026-10-27 15:3x–15:5x，全程不重启 app）**。解锁路径本身在上一轮已走通（"成员设备" + 双向往来），本步按四问复核：①积压 1 → 状态位出现；②积压 1 → 0 → 状态位自行消失（15:54 起桌面端 → 15:56 消失）；③0 → 1 → 自行浮出（kill 桌面进程 → 手机发 `c9again`）；④锁定态 → 显示 `待解锁积压 1 条 · 2.6 KB，解开本机密钥后自动同步　去解锁 ›`，点「去解锁 ›」确实跳到同步页，解锁后切回「对端未确认」，再起桌面 → `messages 640→644`、积压清零、状态位消失。收尾核对：手机信箱 `relay_v2_mail` count = **0**；本地 `sync_wire_outbox` = `acknowledged|136` / `pending|1`；`sync_receipts` = 4 |
| C10 | **D5（C6 出口的下游缺陷，真机抓到）**：`selected_database_path` 把「三张身份表全空」误判成「身份变了」⇒ 走完 C6 出口的手机**下一次冷启动整机起不来**（`Failed to initialize / Bad state: 初始化存储失败: AnyhowException(Error resolving data directory: sync_selected_dataset_identity_changed)`）。修法：身份断言只在**表里有行**时生效（有行且值不同 = 真被换过 ⇒ 仍报 `sync_selected_dataset_identity_changed`）；三张表**全空** = 已合法退出同步的普通本地数据集 ⇒ 允许打开，业务数据照旧可见 | 见 §C10 证据明细 | **[x] 已落地并真机复核** |

| C11 | **D3 的第二根因（重新配对后当场看到）：joiner 加入流程的第五步「等待成员授权与初始同步」永远不会打勾**。真机 2026-10-28 10:10 现场：前四步全 ✔、第五步持续转圈，而同一时刻 staging 库里 `sync_snapshots.state='applied'`、`sync_replica=1`/`sync_local_vault=1`/`sync_members=3`（本机 active）、桌面端该导出已 `acknowledged` ⇒ **初始同步其实早已完成**。结构性根因：`Phase` 枚举（`src/sync/runtime/joining.rs:24-41`）**根本没有「完成」这个终态**，`joinerStageFor` 最大只映射到 4（`ui/lib/widgets/sync_pairing_steps.dart:128-135`），而 `SyncPairingSteps` 把 `i == current` 渲染成无限转圈（同文件 `:36-41`）⇒ 只要相位落在 `awaiting_membership`/`enrollment`/`enrollment_confirmation` 任一个，最后一步就永远是「进行中」。真正的下一步藏在**另一张卡** `DatasetSwitchCard` 的「进入数据集」上（`ui/lib/widgets/dataset_switch_card.dart`），用户没有任何线索知道要点它 —— 2026-10-28 10:41 由我代点后 `workspace.selected_id` 才从 `d1bdfdba…` 变为 `39512723…`，force-stop 后冷启动才进入新数据集。**修法申报**：①Rust 补终态 `Phase::Joined`（名字 `joined`），判据 = 快照 `applied` 且本机成员 `active`（「终点推导」，不再等一个不会来的信令），在**状态轮询**（`joining::state`）里幂等推进 —— 不放在 `joining_install.rs` 的收尾，因为那时快照还没应用；②Dart `joinerStageFor` 把 `joined` 映射到 `steps.length`（五步全 ✔、无转圈）；③**顺带发现**：`joiner_sync_pairing_settings.dart:177` 的 `if (phase == 'complete')` 是**死代码** —— `Phase::name()` 从不产出 `complete`，所以那张带「进入数据集」的卡从来没显示过，这正是用户「除了转圈什么都没得点」的直接原因；改成 `joined` 才真正接上。④owner 侧 `ownerPairingStage` 增加 `deviceJoined` 入参（来源 = enrollment 视图 `phase == 'complete'`），到终点时返回 5。**范围（2026-10-28 11:05 追加申报）**：`src/sync/runtime/joining.rs`、`ui/lib/widgets/sync_pairing_steps.dart`、`ui/lib/widgets/joiner_sync_pairing_settings.dart`，**追加** `ui/lib/widgets/owner_sync_pairing_settings.dart` —— owner 侧是同一处缺陷（步骤表也是 5 行、`ownerPairingStage` 最大只返回 4 ⇒ 第五步「新设备接入」同样永不完成），故同批修；`joining_install.rs` 经复核**无需改动**（终态推导放在状态轮询里，`enrollment_available` 显式保留 `Joined`，不截断交接握手）。**不碰**协议帧、mail/blob 传输、DB schema。用例需覆盖「相位到终态 → 五步全 ✔ 且不出现转圈」，并做变异验证 | **完成（2026-10-28 14:4x，`24bf7f6`，已 push）**，见 §C11 证据明细 | `[x]` 2026-10-28 |

| C12 | **provider key 重封迁移（§六方案 b 便宜版，用户已确认）**：C3 出口若本进程 runtime session 仍持有旧 vault master（未锁定），把 `provider_key_secrets` 全部解密、明文暂存**进程内迁移槽**（不落库/不进 DTO/不进日志）；随后同一进程内 `confirm_key`/joiner `install_key` 建好新 vault 后，用新 master + 新 AAD 重封回 `provider_key_secrets`。旧 master 不可用（已 `lock_local_sync`/进程重启后）时回退现状（出口删密文），且 UI 确认弹窗文案按「是否可自动迁移」区分：「API Key 将自动保留（自动重封）」vs「API Key 会失效，需要重新填写」。**范围（实现时修正）**：`src/storage/provider_keys.rs`（`open_provider_keys_with` 解密 + `reseal_staged_provider_keys` 重封 + `seal_provider_key_with_master`）、`src/storage/sync/reset.rs`（出口在**删复制表之前**解密暂存 —— AAD 依赖 `sync_identity`，删表后组装不出来）、`src/sync/runtime.rs`（迁移槽 + `provider_keys_source_master`/`provider_keys_migratable` + owner `confirm_key_inner` 触发重封）、`src/api/sync/repair.rs`/`sync.rs`（lock 前取 master + `provider_keys_migratable()` API）。**joining_install.rs 经复核无需改动**：joiner 的 key 来自 provider bundle，不走 C3 出口。UI：`ui/lib/sync/sync_repair_api.dart`（接口加 `providerKeysMigratable`）+ `ui/lib/widgets/local_sync_settings.dart`（文案/勾选双分支，判定抛错回退保守）。**不碰**：AAD 结构、schema、快照协议。用例与变异见 §C12 证据明细 | **完成（2026-10-28 16:xx）**，见 §C12 证据明细 | `[x]` 2026-10-28 |
> 顺序理由：C0 先把两条缺陷钉成红用例（否则修完无法证明修的是什么）；C1/C2 是 D2 的两层，C3 是 D1；C4 收口；C5/C6 是门禁与真机。**C7 是唯一根因未确证的步骤**（其余步骤的根因都已用库/日志实测钉死），因此它被拆成「定位 → 修」，并带回写条款；**C9 建议提前到最前面单独跑** —— 它不依赖任何代码改动，却决定了后面所有真机验收能否进行（本轮用户实际卡住的正是这一环）。

#### C10 证据明细

| ｜证据｜ | ｜内容｜ |
|---|---|---|
| 现场 | 手机 2026-10-27 17:08 截图：`Failed to initialize` / `Bad state: 初始化存储失败: AnyhowException(Error resolving data directory: sync_selected_dataset_identity_changed)` —— app **连主界面都进不去** |
| 根因 | `src/sync/datasets.rs::selected_database_path`（唯一启动校验）用「`sync_replica.dataset_id` ∧ `sync_local_vault.installation_id` ∧ 已 applied 快照」三条 `EXISTS` 当身份一致判据；C6 出口按 `storage/sync/reset.rs::REPLICATION_TABLES` 把这三张表**合法清空** ⇒ 三条全 false ⇒ 被当成「身份被换」 |
| 修法 | 把「表全空」与「表有行但值不同」拆开：`left_sync = replica.is_none() && vault.is_none() && !applied` ⇒ 放行（已退出同步的普通本地数据集）；否则逐条 `ensure!` 原断言（真被换过仍报 `sync_selected_dataset_identity_changed`）。`state='ready'`/`published_snapshot`/文件存在性三条**不动** |
| Rust 用例 | `sync::datasets::tests` 新增 3 条：`a_dataset_that_left_sync_still_opens_on_the_next_start`（走**真实出口** `clear_local_sync_identity()`，前置按 C6 现场先清 authority；断言 `selected_database_path == Some(path)` 且 `events` 仍为 1）、`a_partially_cleared_identity_is_still_a_startup_error`（只删 `sync_local_vault` ⇒ 仍报身份不符）、`a_changed_installation_or_unapplied_snapshot_is_still_a_startup_error`（改 installation / 快照回 `staging` ⇒ 仍报） |
| 变异验证 | A `left_sync = true`（去掉全部身份断言）⇒ 3 条红（含既有 `startup_never_falls_back_…`）；B `left_sync = false`（回到旧行为）⇒ 正向用例红；C 删掉 vault 那一条 `ensure!` ⇒ 2 条红。均还原后 13 passed |
| 真实库副本端到端（2026-10-28 09:14） | `/tmp/phone/after.db` = 走完 C6 出口的**真机库副本**（`sync_replica/sync_local_vault/sync_snapshots/sync_snapshot_target/sync_members` 全 0，`messages=645`、`wiki_pages=134`）；按手机布局搭 `/tmp/c10/{sync-workspace.db,datasets/d1bdfdba…/elsewhen.db}`（`selected_id` 指向它、`state='ready'`）。临时 diag 用例：新逻辑 `DIAG ok path=… events=232 wiki_pages=131`；把 `left_sync` 改回 `false` 同一份数据 ⇒ `DIAG err sync_selected_dataset_identity_changed`（**精确复现手机上的那句错**）。diag 用例跑完即删（`grep -c diag_real_phone` = 0） |
| 门禁 | `cargo test --all-targets` ⇒ **982 passed / 0 failed / EXIT=0**（979 + 新增 3） |
| 真机复核（2026-10-28 09:24–09:31，PEUM00） | `./scripts/build-apk.sh --debug` ⇒ `app-debug.apk` 216,965,743 B（md5 `3e417bfdf387d6e487ede540e4166e84`）→ `adb install -r` Success（数据与配对不丢）→ `am start` 后**直接进主界面**（不再有 `Failed to initialize`），会话里 10-27 的旧消息照旧在。取证库（`files/datasets/d1bdfdba…/elsewhen.db`，`am force-stop` 后 `exec-out run-as cat`）：`messages=645`、`conversations=79`、`events=232`、`wiki_pages=134`、`sync_replica/sync_local_vault/sync_authority/sync_members` 全 0。同步页显示「同步未开启」+「准备本机」+「加入现有数据集（通过 Key / 扫码）」，**没有**「同步需要修复」 |

#### C11 证据明细

| # | 证据 | 事实 |
|---|---|---|
| 1 | Rust 修法落地 | `src/sync/runtime/joining.rs`：`Phase::Joined`（`name()=>"joined"`）；`joined_locally(store)` = `local_sync_membership_active()` **且** `EXISTS(SELECT 1 FROM sync_snapshots WHERE state='applied')`；`terminal_phase(phase, joined)` 只在三个中间相位（`awaiting_membership`/`enrollment`/`enrollment_confirmation`）时推进、幂等；`advance_terminal` 挂在 `joining::state`（每拍状态轮询重推导；`staging_store` 探测失败=「现在还不是终态」，不当错误抛） |
| 2 | 截止时间豁免 | `deadline_passed(phase, deadline)` 抽出并在豁免集合里加 `Joined`：终态不被渲染成「邀请已过期」 |
| 3 | 握手不截断 | `joining_install.rs::enrollment_available` 显式保留 `Phase::Joined` 并注明理由：终态只是界面的终点，迟到重投/收尾回执仍要走 `enroll_tick`（C7 ⑩ 的教训反过来用） |
| 4 | 死代码修复 | `joiner_sync_pairing_settings.dart` 的 `if (phase == 'complete')` → `'joined'`（`Phase::name()` 从不产出 `complete` ⇒ 那张「进入数据集」卡在真机从未显示）；`ownerPairingStage` 增加 `deviceJoined` 入参，owner 卡按 enrollment 视图 `phase=='complete'` 传入 |
| 5 | Dart 映射 | `joinerStageFor('joined') => 5`（= `steps.length`，五步全 ✔）；`SyncPairingSteps` 文档写明 `current == steps.length` 表示全部完成、不渲染转圈与 ◀ |
| 6 | Rust 用例 | `sync::runtime::joining::tests::a_join_is_complete_on_local_facts_not_on_a_confirmation_frame`：空库 ✗ → 只有快照 ✗（名册没装=没被授权）→ 名册激活 ✓ → `terminal_phase` 对三个中间相位全到 `Joined` 且幂等、对 `Ready`/`AwaitingApproval`/`InstallationConfirmation` 不动 → `deadline_passed(Joined)=false` → 撤名册后立刻 ✗。模块级 `cargo test --lib -- sync::runtime::joining` **13 passed** |
| 7 | Rust 变异 | A `terminal_phase` 恒返原相位 ⇒ 红；B `joined_locally` 只看快照不看名册 ⇒ 红；C `deadline_passed` 去掉 `Joined` 豁免 ⇒ 红。均 `cp` 还原后复绿 |
| 8 | Dart 用例 | 新增 `ui/test/sync_pairing_terminal_test.dart` 4 条：`joinerStageFor('joined')==5`（并钉住 0/4/-1 既有值）、`ownerPairingStage(deviceJoined)` 4→5、joiner/owner 双端渲染终态（`check_circle` 恰 5 个、`CircularProgressIndicator`/`radio_button_unchecked`/`◀` 全 0）。既有 `sync_dataset_switch_test.dart` 的 joiner complete 用例相位名 `'complete'`→`'joined'`（它在验一段死代码，这次一并纠正）；该文件 4 条随同全绿 |
| 9 | Dart 变异 | A `joinerStageFor('joined')` 退回 4 ⇒ 终态渲染用例红；B `ownerPairingStage` 无视 `deviceJoined` ⇒ owner 用例红；C joiner 卡条件退回 `'complete'` ⇒ switch 用例红。均还原后复绿 |
| 10 | **顺带修掉的测试基建泄漏** | `relay && npm test` 每轮驱动 Rust 冒烟 `end_to_end_send_fetch_ack_against_real_worker`，其 `elsewhen-relay-blob-snapshot-<uuid>` 目录**从不清理**：`/tmp` 里积了 **183 个目录 ≈ 2.5 GB**，把 tmpfs（16 GB）堆到 **99%**，`fvm flutter test` 全量编译直接 `No space left on device (errno 28)` —— 第一轮 `EXIT=124` 假超时、第二轮 `-1 [E]` 全是它。修法：`transport_relay_smoke/mod.rs` 给 `snapshot_dir` 加 `Drop` 清理 guard（panic 路径也回收）；复跑 relay **49/49**、`ls /tmp/elsewhen-relay-blob-snapshot-*` = **0** |
| 11 | 门禁（五道全绿） | `cargo test --all-targets` **983 passed / 0 failed** EXIT=0；`cd relay && npm test` **49/49** fail=0 EXIT=0；`cd ui && fvm flutter analyze` **133 issues = 0 error**；`cd ui && fvm flutter test` **+483 All tests passed!** EXIT=0（基线 479 + 新 4）；`scripts/regen.sh` EXIT=0「已保持一致」 |

#### C3 证据明细
| ｜证据｜ | ｜内容｜ |
|---|---|
| 存储出口 | `src/storage/sync/reset.rs`：`REPLICATION_TABLES`（38 张，显式列出）+ `clear_local_sync_identity_on()` 两道闸门（必须写事务内 = `sync_reset_requires_transaction`；authority 必须为空 = `sync_reset_requires_broken_identity`）+ 清 `provider_key_secrets`（密文 AAD 绑死旧 `dataset_id`+`installation_id`，留着只会显示「有 key 用不了」），**不动** `provider_key_policy` 与 `ai_provider_configs` 结构 |
| API 出口 | `src/api/sync/repair.rs::clear_local_sync_identity()`；准入闸门抽成纯函数 `ensure_reset_applicable(&state)`（只有 `requires_repair` 放行，否则 `sync_reset_not_applicable`）；状态判定抽成 `invite::local_sync_state_for(config, store)` 让「显示的状态」与「出口的准入」用同一份实现 |
| Rust 用例 | `storage::sync::tests::local_identity`：`a_half_configured_local_identity_has_no_exit_back_to_not_configured`（C0 的红用例，**现已转绿**）、`clearing_a_half_configured_identity_returns_to_not_configured`、`clearing_is_refused_while_the_local_identity_is_healthy`、`clearing_keeps_business_data_and_drops_undecryptable_provider_keys`、`every_replication_table_is_covered_by_the_reset_list`、`api::sync::repair::tests::only_the_repair_state_may_clear_the_local_identity` ⇒ `cargo test --lib -- local_identity repair::` **6 passed / 0 failed** |
| Rust 变异验证 | ①去掉 authority 守卫 → `clearing_is_refused…` 红；②运行时把 `sync_replica` 换成 `sync_frontiers`（跳过清身份）→ `clearing_a_half_configured…` + C0 用例双双红（首版变异改数组元素导致 `[&str; 38]` 编译失败、什么都没验证到，已记为教训：**变异必须先确认能编译**）；③不清 `provider_key_secrets` → `clearing_keeps_business_data…` 红；④准入闸门改成放行一切 → `only_the_repair_state…` 红。均还原后复绿 |
| UI 入口 | `ui/lib/widgets/local_sync_settings.dart`：`state == 'requires_repair'` 时出现代价说明 + 「清除本机同步身份」按钮；二次确认弹窗逐条列副作用（退出数据集 / **Provider API Key 会失效需重填** / 未同步改动不补发 / **笔记对话知识库不删**），且必须勾选「我已知晓…」才解锁「清除并重新配对」；成功后 `controller.refresh()` |
| Dart 用例 | `ui/test/sync_repair_exit_test.dart` 4 条：出口只在 `requires_repair` 出现；`local_ready` 下**不出现**（负向）；未勾选时确认按钮 `onPressed == null` 且「取消」后 `repair.calls == 0`（负向）；勾选后 `calls == 1` 且状态刷新成 `not_configured`、入口消失 ⇒ `fvm flutter test test/sync_repair_exit_test.dart` **4 passed** |
| Dart 变异验证 | A 无条件显示入口 → 2 条红；B 确认按钮不看勾选 → 1 条红；C 删掉「Key 会失效」文案 → 1 条红；D 清完不刷新控制器 → 1 条红。均还原后复绿 |
| 真实库副本端到端（2026-10-27 13:53，`/tmp/c6desk` = 线上库副本，先 `DELETE FROM sync_authority`+`sync_authority_versions` 造 `requires_repair`） | 临时诊断（`tests/diag_reset.rs`，取证后已删）在真库上走完整 API 路径：`BEFORE state=requires_repair dataset=Some(0903216e…) device=Some(9625222c…)` → `AFTER state=not_configured dataset=None device=None`；`SYNC_TABLES total=38 non_empty_after=[]`（全部清零）；`conversations=79`、`messages=640`、`wiki_pages=134`、`ai_provider_configs=10` 全部保留、`provider_key_secrets=0`。**这条真库跑出了单测跑不出的两个 bug**：①`require_background_quiescent_for_sync` 要求调用方已在领域写事务内，API 层裸调直接 `sync_requires_domain_transaction` —— 已把该检查移进存储层的同一事务；②逐表 `DELETE` 撞 `FOREIGN KEY constraint failed`（复制表之间互有外键，删除顺序不可能对所有库状态成立）—— 改为事务内 `PRAGMA defer_foreign_keys = ON`，把检查推迟到 COMMIT（那时复制表已全空），该 pragma 在 COMMIT 后自动失效。 |

---

## 五、范围申报（改动前必须确认）

- **C1/C2 只动存储层单例读取 + 一个 Dart worker 的错误分类**，不碰快照/传输/relay 协议。
- **C3 与 C8 是仅有的两处新增用户可见界面**（C3 重置按钮 + 确认弹窗；C8 全局同步状态位），文案均需另行报审。
- **C8 只读已有状态做投影**，不新增协议字段、不改 relay、不改快照格式；若实现时发现需要新的原生 API，先回到本行申报再动。
- **C9 零代码改动**（只做真机验证与数据采样）。
- **C10 只动 `src/sync/datasets.rs::selected_database_path` 的身份断言（+ `src/sync/datasets/tests.rs`）**：不改 catalog schema、不改 `reset.rs` 的清表清单、不改启动回落规则。判据是把「表全空」与「表有行但值不同」拆开，不是放宽成「什么都不查」。
- 不在本 roadmap 内：provider key 的重加密/迁移（见 §六）、配对快照传输本身的任何改动（归上游 roadmap）。

---

## 五之二、完成定义（这张表回答「全绿之后还剩什么」）

**C0~C9 全 `[x]` 的含义是**：四条缺陷（D1/D2/D3/D4）的现场现象不再复现、各自有红→绿用例与变异验证兜底、四道门禁全绿、并在真机上验收过。**它不等于「同步功能没有其它问题」**，因为：

| 事项 | 全绿后是否解决 | 说明 |
|---|---|---|
| D1 `requires_repair` 卡死 | ✅ 是 | 有出口 + 副作用提示，判定标准 4 条锁死 |
| D2 未配置裸读单例刷屏 | ✅ 是 | 领域错误 + worker 不再重试 |
| D3 交接完成 UI 不收敛 | ⚠️ **定位已完成，修复已落地；真机复核仍待重新配对** | 定位结论**推翻了 F13**（`sync_receipt_identity_mismatch` 不是手机侧现场，那是桌面库里的另一条历史记录）：真因是交接收尾时 owner 没收到回执而重投 `enrollment-complete:`，此时 joiner 的 enrollment 会话已结束 → 该帧落到 `engine.rs:753` 的普通分派末尾被误判为 `sync_wire_kind_unsupported`，连拒 3 次后丢弃 ⇒ **收尾回执永远到不了对端，手机只能等一个不会来的确认**。修法已按该结论落地（`is_enrollment_control_id` + `sync_member_is_active` 幂等收尾，用例+双向变异全红后还原，见 C7 ⑩⑪）。**真机复核已通过（2026-10-28 10:10–10:55，C7 ⑫）**：重配对全程走通，收尾帧不再被误拒，两端最终收敛（手机 `rejections=0 / receipts=66 / pending=0`，桌面 `ack=211 / pending=0`，两端 `messages=649`）。第五步转圈是另一条独立缺陷 **C11**，已单独修复（`24bf7f6`） |
| D4 状态不可见 | ✅ 是 | 跨主界面状态位 + 积压条数 + 桌面「未确认 N 条」 |
| D5 C6 出口导致下一次冷启动整机起不来（C10） | ✅ 是（真机复核通过） | 这是 C6 出口的**下游**缺陷：清身份合法地把三张身份表清空，而启动校验把「全空」当成「身份被换」⇒ 冷启动硬失败。教训与 D1 同族：**给出口就得把出口之后的第一条启动路径也走一遍**，否则出口只是把「局部坏状态」换成「整机不可用」 |
| D6 重加入后旧数据集无清理出口（**新立档，待决策**） | ❌ **否** | 每次加入都 `create_staging` 新建目录 + 新 `installation_id`（`sync/datasets.rs:136`，且「永不收养已有目录」是 fail-closed 约束），而 `discard_staging` 明写 `ensure!(state=='staging')`、UI 也只有 `discardSyncJoinStaging` ⇒ 退出同步→重新加入后，旧数据集（实测 133 MB）变成**未选中且永远删不掉**的孤儿。三种候选：①自动删旧（危险，可能与「未选中」语义冲突）②给「清理未使用的数据集」入口 ③复用同 `dataset_id` 的旧目录（要动 fail-closed 约束，须单独评审）。**我未擅自实现** |
| 「解锁本机密钥」能否成功 | ✅ 是（C9 专项验证） | 本轮是**首次**走到成员设备状态，该路径从未成功验证过。F17 只证明了「有流量」，没证明「解锁动作本身可用」 |
| **provider API Key 全部失效** | ❌ **否，且需要你决策 + 手动重填** | 见 §六。`provider_key_secrets=0`、10 条 `ai_provider_configs.api_key` 全 NULL ⇒ AI 功能在两端都不可用，直到你从各家后台重新取 key。这是「选 B 换速度」的既定代价，**不是 bug，但必须有人做** |
| relay 侧累计写入字节 | ❌ 否（口径缺口） | 属上游 roadmap C4-4 ⑤，需第 4 次复跑专项采样 |
| `provider:` 一类信令的 ack 语义 | ✅ 已澄清 | 「永不 ack」的旧判断已被推翻（见上游 C4-4 ⑥ 纠正），无需动作 |

**结论口径**：本 roadmap 解决的是「状态机的边界与终点没有被正确处理/呈现」。它**不**承诺同步协议本身无缺陷 —— 协议层的收敛性归上游 [配对快照交接收敛性](2026-10-26-enrollment-snapshot-convergence-roadmap.md)。

---

## 六、关联发现：provider key 随身份失效（⇒ **已收敛为 C12，方案 b**）

**换同步身份 ⇒ 旧 sealed provider keys 全部永久失效，且无任何提示。**

- 现象：重建身份后进入 `preparing_snapshot`，每拍报 `AnyhowException(provider_key_authentication_failed)`，配对无法继续（2026-10-27 10:39 实测）。
- 根因：`provider_key_context()` 把 `dataset_id` + `installation_id` + provider id/url/type 当 AAD（`src/storage/provider_keys.rs:40-62`）；身份一换 AAD 全变，ChaCha20-Poly1305 认证必然失败（`:215`）。
- 后果：`provider_key_secrets` 的 10 条密文变成永久无法解密的死数据；`ai_provider_configs.api_key` 早被写成 `NULL`，明文不存在，**用户只能去各服务商后台重新取 key**。而且快照设计上必含 provider keys（`src/storage/sync/snapshot/records.rs:80-81`）、UI 确认框只有「共享所有 Provider 配置和 API Key」一途（无「不共享」选项），所以只要桌面上有 sealed key，快照导出就必然撞上这个错误。
- 与 D1 的关系：**D1 的「清除同步身份」出口必然踩到它**，所以 §四 C3 的确认弹窗必须写明「已有 Provider API Key 将失效，需要重新填写」，否则就是给出一个会静默破坏数据的出口。
- ~~待决定的选项：(a) 出口里顺手把 key 作废并提示；(b) 提供「重封」迁移；(c) 允许用户选择「快照不含 provider keys」。~~ **已收敛（2026-10-28，用户选定方案 b 的便宜版）**，见下。
- **方案收敛的关键事实（逐条复核过代码）**：①key 死亡的真正边界是 **vault 重建**（master 每个 installation 一把）。owner 侧**不走 C3 出口**的重新配对（撤销成员 + 现有 vault，`runtime.rs` `prepare_key` `creating=false`）本就不丢 key；joiner 重新加入经 provider bundle 从 owner 拿回全部 key（`src/storage/sync/providers.rs`）。②**唯一死穴是 C3 出口**（`clear_local_sync_identity_on` 删 `sync_local_vault` ⇒ 旧 master 永久不可用 ⇒ `reset.rs:92` 删密文）。③快照传输时 key 本来就以明文进 bundle（信封加密，key 材料不离本机的约束不破坏）——`sealed at rest` 防的只是「本机 DB 单独被偷」，该保留。④所以「重封」不需要复杂迁移：只需**在 C3 出口瞬间，若本进程 runtime session 仍持有旧 master（未锁定），把 `provider_key_secrets` 明文解出暂存进程内；随后 `confirm_key` 建好新 vault 时用新 master + 新 AAD 重封回库**。旧 master 不可用（已锁定/进程重启后）时回退现状：删密文。明文永不落库、不进 DTO、不进日志。
- 对应申报步骤：**§四 C12**。
- **当前实际状态（2026-10-27 11:40 实测）**：用户已选 B（丢 key 换速度）—— `provider_key_secrets` 已清空为 0 条、`provider_key_policy` 保留 1 行（sealed 标记）、`ai_provider_configs` 10 行结构（base_url/type/模型）完好但 `api_key` 全为 NULL。**⇒ AI 功能在桌面与手机两端都处于不可用状态，直到逐条重新填 key。** 这是一笔待办，不是待修 bug；若将来要做 (a) 或 (c)，必须同时给出「如何让用户知道哪些 provider 需要重填」的界面，否则等于把问题从「静默失效」挪到「静默缺失」。

### 六之二、D7（2026-10-28 真机新抓到，**待用户决定是否本轮修**）：录入期间的 op 帧被当成「名册未激活」拒收

- 现场：手机加入（`39512723…` staging）过程中，桌面投来的 11 条 `op-…` + 1 条 `provider:d29e0c2d-…` 全部落进 `sync_wire_rejections`，`last_error=sync_local_membership_inactive`、`attempts=1`、`first_seen=last_seen=1793153179432`；而同一时刻在**同一份库**上用代码里的原样 SQL（`local_sync_membership_active`）查询返回 **1** —— 即「拒绝是记录当时判定的，之后名册才装好」。
- 危害：桌面把 11 条 pending 空转了 **9 次**（`attempts=9`，`available_at_ms` 一路推后），期间手机主界面显示「同步延迟：对端未确认 1 条 · 10 KB，正在重试」。但那 12 条业务操作**没有丢**：切换进新数据集后 `pull_page` 游标为 0 重新扫描、被接受后 `forget_wire_rejection` 清掉拒收行 —— 实测 10:43 手机 `rejections=0 / receipts=66 / pending=0`、桌面 `acknowledged=211 / pending=0`、两端 `messages=649`，**已完全收敛**。
- 定性：这是「自愈但代价高」的形状，与 C7 ⑩ 的收尾帧同一族 —— `sync_local_membership_inactive` 应是**推迟**信号而不是**拒收**信号。候选修法：照 `snapshot-start:` 的既有做法，在 `local_sync_membership_active()==false` 且发送方**在配对名单里**时返回「defer」（不消费、不记拒收），下一拍再取。
- 待用户决定：本轮修（接在 C11 之后）／另立一轮／只记录不动。

---

### 六之三、D8（2026-10-29 真机新抓到，**本轮已修**）：同内容假冲突 + 审阅错误无重拉出口

- 现场：手机重装后以新身份（`f8de02aa`）向桌面推送 provider 快照，base 指向旧链头 `7cb300cc`，而桌面 head 已前进 ⇒ `apply_encrypted_provider_bundle` 按 `base_revision != head` 判 conflict（只看版本链、不比对内容）。**两条冲突内容逐项完全一致**（截图核对），第二条还是同一快照的重复推送（base 指向第一条 conflict 行）。用户点「保留本地」报 `provider_conflict_local_changed（本地配置已变化，请刷新后重新审阅）`——审阅窗口内后台同步推进了 head，且 UI 文案让「刷新」却没有刷新按钮。
- 修法（两层）：
  1. **Rust 根治假冲突**（`src/storage/sync/providers.rs`）：链条分叉但 bundle 内容与本地完全一致（含明文 Key、激活项、failover 序；`updated_at` 是本地写入时间戳，归一化后剥离）⇒ 直接 applied 并推进 head，**修订链自愈**；内容有任何实质差异仍走人工审阅。关键不变量不放松：信封仍需可信成员密钥解开，「不覆盖独立本地 vault」的保障在于内容逐字节一致时覆盖无意义。
  2. **Flutter 审阅出口**（`ui/lib/sync/sync_provider_conflicts_controller.dart`、`ui/lib/widgets/sync_conflict_section.dart`）：裁决遇双版本绑定失败（`provider_conflict_local_changed` / `_revision_mismatch` / `_not_pending`）时**自动重拉列表**（后台随时推进 head，人工刷新往往又慢一步），错误文案改「请重新审阅下方最新列表」；`SyncConflictSection` 加可选 `onRetry`（错误态显示「重新拉取」ActionChip，busy 时禁用；三类冲突区共用，另两区不传即不显示）。
- 验证：Rust 新用例 `identical_content_on_forked_chain_applies_without_manual_review`（同内容异 base → applied + head 前进 + 链治愈 + 审阅列表空；对照段：同形态异内容 → 仍 Conflict 且 head 不动）+ 变异（删免审分支 → 红）；Dart 新用例 2 条（裁决遇版本漂移自动重拉、重试按钮触发 refresh）+ 变异 2（删自动重拉 → 红；重试按钮 onPressed 恒 null → 红）。

### 六之四、D9（2026-10-29 真机新抓到，**本轮已修**）：摘要历史线性校验把合法分叉判死

- 现场：用户 AI 回复开头反复出现「〔系统标注：本轮背景摘要不可用…〕」。取证库（`chat_run_events`）得真实错误：`摘要历史存在分支或不明确父链，未合并不同路径`。触发现场是会话 `93196d4f` 里同父两条 `habit_checkin`（06:03:48 / 06:04:00，同挂 06:00 习惯问候之下）——同父多子 = 合法分叉。
- 根因：`validate_history_path`（`src/storage/chat_summaries.rs`）要求「带父链消息的前一条必须正好是它的父」（严格相邻单链）。这是把旧世界「null-parent 线性历史」的假设带进了显式父链时代：多端同步与并发回复（两设备同时发言、同一条习惯提醒打两次卡）落库后必然分叉，排序器（`sync_message_order.logical_clock`）给出的全序已保证确定的合并路径，两条分支都是真实发生的历史。
- 修法：断言放宽为「父存在且位置在子之前」（悬空 / 倒挂 / 自指 / 环 / 重复 / 跨会话全部仍拒），放弃不构成摘要正确性必要条件的「严格相邻」。摘要节点的 span 校验（`validate_history_spans`：边界前全覆盖、无重叠空洞）走扁平有序列表，不受影响。
- 验证：新用例 `explicit_parent_edges_stay_causally_ordered`（合法链过、同父双 checkin 过、悬空父拒、倒挂拒、环拒）+ 变异（恢复相邻断言 → 同父 checkin 用例红）。用户库 06:54 那轮失败随下一轮摘要自动重做消失，无需清数据。

### C12 证据明细（2026-10-28）

| # | 证据 |
|---|---|
| 1 | Rust 新增两条用例（`src/storage/sync/tests/local_identity.rs`）：`provider_keys_survive_an_identity_reset_when_the_old_master_is_available`（旧 master 可用：密封 → 出口暂存 → 新 vault → 重封 → 新 master 读回明文一致 → 槽不重复消费）与 `provider_keys_stay_dropped_when_the_old_master_is_unavailable`（回退：密文删除、重封空操作） |
| 2 | Rust 变异验证（均先确认能编译再跑，改后还原复绿）：A 出口忽略 `migration_master`（恒 None）→ 成功用例红；B 重封用翻转的 master 字节（`broken[0] ^= 0xFF`）→ 红；C `take_staged_provider_keys` 改成 `get` 不 `remove`（可重复消费）→ 「再取一次槽」断言红 |
| 3 | 实现中真 bug 由用例抓出：迁移暂存最初放在删复制表**之后**，AAD 组装报 `provider_key_identity_missing` —— 已改为删表**之前**（C12 的一个非显然约束：AAD 依赖即将被删的身份表）；重封接口最初想复用 `provider_master_key()`（session 还没确认）报 `provider_key_locked` —— 改为显式传新 master |
| 4 | Dart 新增两条用例（`ui/test/sync_repair_exit_test.dart`）：「自动保留」分支（migratable=true 时文案/勾选切换、无「会失效」字样）+ 负向「判定抛错回退保守文案」 |
| 5 | Dart 变异验证：A UI 恒走保守分支 → 「自动保留」用例红；B 判定抛错时误报可迁移 → 首轮**全绿（覆盖不足）**，按纪律补负向用例后 → 红，还原复绿 |
| 6 | 五道门禁（2026-10-28）：`cargo test --all-targets` **985 passed / 0 failed**；`cd relay && npm test` **49/49**；`fvm flutter analyze` **133 issues = 0 error**（中途 134 有一条新 `use_build_context_synchronously` info，加 `mounted` guard 后回基线）；`fvm flutter test` **+485 All tests passed**（479 基线 + C11 4 + C12 2）；`scripts/regen.sh` EXIT=0 |
| 7 | **边界与不解决**：迁移只保「同进程、未锁定」的 C3 出口；已锁定（`lock_local_sync`）或进程重启后的出口仍回退删密文（密文在 master 不可用时本来就解不开）。救不回本次已删的 10 个 key —— 用户仍需手动重填。owner 非 C3 的重新配对与 joiner 路径本就不丢 key（owner 用现有 vault，joiner 走 provider bundle），无需迁移 |

---

## 七、进度记录

- **2026-10-29 15:xx D8 修复完成（同内容假冲突 + 审阅错误无重拉出口，未 push）**：修法与验证见 §六之三。五道门禁全绿：cargo **987** / relay **49** / analyze **0 error** / flutter test **+490** / regen EXIT=0。
- **2026-10-29 D9 修复完成（摘要历史线性校验把合法分叉判死，未 push）**：修法与验证见 §六之四。五道门禁全绿：cargo **987** / relay **49** / analyze **0 error** / flutter test **+490** / regen EXIT=0。

- **2026-10-28 15:30 C12 申报（provider key 重封迁移）**：方案 b 便宜版已在 §六收敛并逐条复核代码事实（见上）；确认唯一死穴是 C3 出口、重封可在同进程内完成（明文暂存迁移槽），**尚未改动任何代码**。
- **2026-10-28 16:xx C12 完成**：重封迁移 + 双分支 UI + Rust 2 用例 3 变异 + Dart 2 用例 2 变异（其一暴露覆盖不足，补负向用例后打红）全部落地，五道门禁全绿（cargo 985 / relay 49 / analyze 0 error / flutter test +485 / regen EXIT=0），见 §C12 证据明细。**10 个已删 key 无法救回，仍需手动重填**。

- **2026-10-27 10:50** 建立本文档。D1/D2 均有现场实测证据（见 §一）。C0 尚未开始，无代码改动。
- **2026-10-27 11:15** 上游 roadmap 第 3 次真机复跑**配对成功**（桌面对端确认、手机「成员设备」、数据完整落地、relay 残留 0）。同时暴露本文档 **D3**（交接完成后手机 UI 不收敛，需重启 app 才显示终态，见 §一 F10~F12 与 §三之二）：数据层与 UI 层出现了一次真实的一致性断裂，故新开 C7；D3 与 D1/D2 同源 —— 都是「状态机已经走到终点，但没有把终点正确地呈现/推导出来」。
- **2026-10-27 11:25 复核纠正**：上一版本文档与上游 roadmap 里有两处判断被真机数据推翻，已就地改写而非另起备注 —— ①「`provider:` 永不 ack」不成立（8 条全 acknowledged；`in_scope` 只约束 enrollment 路径，普通 pull 路径白名单含 `provider:`）；②§三之二「已排除：手机没拒收信令」撤回，并新增 F13（11:04:16 一条 `sync_receipt_identity_mismatch` 回执被丢弃），升为 D3 的首要候选根因（§三之二 方向 0）。教训与本文档主线一致：**没把该 bug 的每条处理路径都验证一遍，就不能把「某路径不处理」写成「永远不处理」**。
- **2026-10-27 11:40 补全（D4 立档 + 完成定义）**：用户报「桌面一直在写、手机看不到」，实测得 F14~F17 → 新增 **D4（同步暂停/积压状态在主界面不可见）**、§三之三、步骤 **C8**；并新增 **C9（解锁路径真机验证）**、§五之二 **完成定义**（逐条写明全绿后解决了什么、没解决什么 —— 其中 D3 明确「定位前不得打勾」、provider key 需人工重填、relay 写入字节仍是口径缺口）。本轮仍**零代码改动**，只动文档。
- **2026-10-27 11:45 现场旁证**：解锁后双向流量自证（F17），`messages` 594→624、outbox ack 21→64；手机信箱 13→0→13 循环、桌面信箱 1→11→1 ⇒ 协议层与传输层未坏，坐实 D3/D4 都属「状态呈现」而非「同步失效」。
- **2026-10-27 13:12 C1 完成**：单例读取接缝落地，`provider_master_key` 在同步未配置时返回 `sync_not_configured`（C0 的 D2 红用例已转绿）。同时确认 `sync_gc_state` 的唯一读点（`gc.rs:92`）本来就是 `.optional()? + 默认 0`、按 `origin_stream_id` 而非 singleton 读，语义正确，未纳入改造。
- **2026-10-27 13:36 C2 完成**：worker 层终态分类 + 退避落地，实测 5 条/130 s → 1 条/200 s。**踩坑记一笔**：桌面 bundle 只重建 Flutter 侧时，`target/release/libelsewhen.so` 仍是旧逻辑 ——我第一次对比就撞上（旧 Rust + 新 Dart），差点把「Rust 修复没生效」误判成「策略没起作用」。这次反过来把它当成了「前」基线；以后凡测桌面行为，先确认 release lib 已重建。

- **2026-10-27 15:56 C9 真机通过**：四问全绿（积压 1→出现、1→0 自行消失、kill 桌面后 0→1 自行浮出、锁定态显示 `待解锁积压 1 条 · 2.6 KB…` 且「去解锁 ›」真跳同步页），解锁后 `messages 640→644`、信箱残留 0。**方法学一笔**：这一轮没有重启 app —— 因为 D4 的价值就在「不重启也能看见」，重启会绕过待测行为。
- **2026-10-27 16:55 C8 真机暴露两缺陷并已修**：①状态位 8 分钟不出现（`initState` 只读一次，挂在不会反复 tick 的时机上）；②文案把条数说了两遍。修法见 C8 ⑩；新增 3 条 Dart 用例 + 4 条变异。**教训升级为纪律**：*状态类 DTO 不能只在 `initState` 读一次，必须挂到每拍都会 notify 的信号上*；*同一个数字在用户可见文案里只说一次*；*不可观测的守卫（`dispose` 是否摘监听）要用「订阅是否还挂着」取证，而不是看有没有副作用*。
- **2026-10-27 17:25 C6 真机通过**：只删 `sync_authority`（改副本再写回，md5 双向一致）即复现 `requires_repair`，出口可用、未勾选时确认键禁用、执行后回到「同步未开启」且业务数据一行不少（`messages=645`/`conversations=79`/`wiki_pages=134`）。**代价**：手机被故意打回未配对，D3 真机复核因此排在重新配对之后。
- **2026-10-27 18:05 C7 定位完成，并推翻了 F13**：为拿确凿现场，把 release 包换成 `--debug` 包（release 也是 debug keystore 签名 ⇒ `adb install -r` 直接覆盖，数据与配对都不丢，`run-as` 立刻可用）。**关键取证纠正**：手机真实库不在 `files/elsewhen.db`（1.46 MB 空壳，`messages=0`），而在 `files/datasets/<dataset-uuid>/elsewhen.db`（本轮 133,926,912 B）；取错库会得出「设备上什么都没发生」的相反结论。查得唯一那行拒收：`message_id=enrollment-complete:05a7273e…`、`attempts=3`、`11:03:54 → 11:03:56`、`last_error=sync_wire_kind_unsupported` ⇒ 真因是**收尾帧落到普通分派末尾被误判为「种类不支持」**，不是 F13 的身份不符。修法与用例见 C7 ⑩⑪。
- **2026-10-27 18:10 环境事实（挡住 D3 真机复核的那堵墙）**：本机桌面 GUI 自动化不可用，三层原因都实测过 —— ①`xdotool` 报 `Authorization required`（`DISPLAY=:0` 但没有可用 Xauthority，且 app 是 Wayland 客户端，Xwayland 下看不到它）；②KWin 6.8.1 脚本通道：`loadScript` 返回 id 但 `isScriptLoaded` 恒 false、脚本不执行、`writeConfig` 不写 `kwinrc`；③`ydotool` 之前的「能用」是**假象**：`pp` 不在 `input` 组、`/dev/uinput` 是 `root:input 0660` ⇒ daemon 建不出虚拟设备，所有注入都是空操作（改用 `sudo ydotoold` 后 `/dev/input/event22` 才出现），换成 root daemon 后**绝对坐标映射仍对不上**（`mousemove -a` 到面板时钟点不出日历、点输入框打不进字），根因是 ydotool 的 ABS 量程与 3440×1440 屏幕无对应关系。**⇒ 配对必须由人手点两下（桌面「同步」页 → 生成邀请码；手机 → 加入），我负责观察手机侧是否收敛。**
- **2026-10-27 18:30 终局门禁（C7/C8 修复后重跑，五道全绿）**：①`cargo test --all-targets` ⇒ `test result: ok. 979 passed; 0 failed; 20 ignored`（EXIT=0，184 s）；②`cd relay && npm test` ⇒ `tests 49 / fail 0`（EXIT=0）；③`cd ui && fvm flutter analyze` ⇒ `133 issues found` = **0 error** / 45 warning / 88 info（EXIT=1 是既有 warning 导致，判据是 0 error）；④`cd ui && fvm flutter test` ⇒ `+479 All tests passed!`（EXIT=0）；⑤`scripts/regen.sh` ⇒ `生成的 Dart/Rust bridge 与 target/release/libelsewhen.so 已保持一致`（EXIT=0）。**坑记一笔**：把 `fvm flutter test` 放进后台子 shell 里跑会只留一行 `EXIT=1`、日志空白（无 TTY），必须前台跑才有真实输出 —— 别把「空日志 + 非零」当成用例红了。
- **2026-10-28 09:20 真机抓到 D5（C10）**：用户反馈 app 启动即 `Failed to initialize / sync_selected_dataset_identity_changed`。这是 **C6 出口的下游缺陷**：出口合法清空三张身份表，而启动校验把「全空」当「身份被换」⇒ 整机不可用。已按 C10 修（只拆「全空」与「值不同」，不放松其它断言），3 条新用例 + 3 条变异 + 真机库副本双向对照（新逻辑 ok / 旧逻辑精确复现那句错）。教训：**给出口必须把出口之后的第一条启动路径也走一遍** —— C6 当时只在同进程里验证了「回起点」，没验证「冷启动」。
- **2026-10-28 09:45 C10 收口（五道门禁复跑全绿）**：`cargo test --all-targets` 982 passed / 0 failed / EXIT=0；`cd relay && npm test` 49/0 EXIT=0；`cd ui && fvm flutter analyze` 133 issues = **0 error**（EXIT=1 是既有 warning）；`cd ui && fvm flutter test` +479 All tests passed EXIT=0；`scripts/regen.sh` EXIT=0。真机（PEUM00）覆盖安装后 app 直接进主界面、同步页「同步未开启」、业务数据 645/79/232/134 全在。
- **2026-10-28 10:10–10:55 D3 真机复核（重新配对后）—— 定位到第二个根因，C11 立档**：桌面撤销死身份 `477898ac…`、重新「准备本机」后手机加入成功。**录入期间**手机有 11 条 `sync_local_membership_inactive` 拒收（见 §六之二 D7），切换数据集后自愈。关键现场：10:10 手机停在「等待成员授权与初始同步」第五步转圈，但库里快照已 `applied`、名册已装（本机 active）—— 即 D3 的剩余症状**不是同步没完成，而是 UI 永远不肯说完成**（结构原因见 C11）。10:41 我代点另一张卡的「进入数据集」，`workspace.selected_id` 由 `d1bdfdba…` 变 `39512723…`；force-stop + 冷启动后进入新数据集，10:43 两端彻底收敛（手机 `rejections=0/receipts=66/pending=0`、桌面 `acknowledged=211/pending=0`、两端 `messages=649`），同步页显示「同步已开启 · 本机：PEUM00 · 成员设备」。**教训**：*「同步没完成」与「界面不肯说完成」是两件事，验收必须分开取证（库里终态字段 + UI 帧各取一次）*；*一张卡上给出的下一步，不能让用户去另一张卡上找*。
- **2026-10-28 10:55 新立 C11（D3 第二根因）+ §六之二 D7**：范围已在 §四 C11 行申报，**未改动任何代码**，等用户确认后动手。
- **2026-10-28 14:45 C11 完成（`24bf7f6`，已 push）**：终态 `Phase::Joined` 由本地事实（快照 applied + 本机名册 active）在每拍状态轮询里幂等推导；joiner 卡死代码条件 `'complete'`→`'joined'`，「进入数据集」卡从此真正可达；owner 侧同缺陷同批修（`deviceJoined`）。Rust 用例 1 条 + 变异 3、Dart 用例 4 条 + 变异 3，全红后还原复绿。**顺带修掉一个测试基建泄漏**：relay 冒烟测试的 `elsewhen-relay-blob-snapshot-*` 目录从不清理，183 个 ≈ 2.5 GB 把 `/tmp` 堆到 99%，`flutter test` 全量因此 `No space left on device` 假死 —— 加 `Drop` guard 后 relay 49/49、残留 0。**教训**：*门禁「假超时」先查磁盘，别急着怀疑用例*；*每轮测试都新建临时目录的代码，必须有同生命周期内的清理路径*。五道门禁：cargo **983** / relay **49** / analyze **0 error** / flutter test **+483** / regen EXIT=0（数字见 §C11 证据明细 #11）。
- **2026-10-28 10:05 D5 真机复核通过 + 新立档 D6**：debug 包覆盖安装后手机直接进主界面（09:29 截图），取证库 `messages=645`/`conversations=79`/`events=232`/`wiki_pages=134`、同步表全 0、同步页「同步未开启」。同时用户指出我把「两端都是未开启」说错了 —— 实测桌面 `sync_replica/vault/authority` 仍 1/1/1、名册 2 人 PEUM00 仍 active、outbox 有 11 条指向已死身份的 pending（`attempts` 已到 203）⇒ 那是「对着死人写信」，撤销成员时会按 `membership/members.rs:106-115` 清掉。并因重加入会产生第二份数据集，新立 **D6**（旧数据集无清理出口）。**教训**：*出口类改动只验「本机」，却对外宣称「两端」——涉及两端状态时必须两边各查一次库再说*。

