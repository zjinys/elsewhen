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
| C6 | **真机验收**：在真机上把库造成 `requires_repair`（不改代码、只删 `sync_authority*`），点新出口 → 状态回「同步未开启」→ 能重新准备本机并完成一次配对；同时确认日志里 `analysis worker failed: … Query returned no rows` 不再出现 | 截图 + 日志 + 库行数 | `[ ]` **真机未执行**：2026-10-27 13:47 设备不可达（`adb devices` 空、`ping 192.168.101.102` 100% 丢包、`adb connect …:40009` = `No route to host`）。已用**真实库副本 + 真实 API 路径**先行取证（见 C3 证据明细末行），真机两项（点出口 → 状态回「同步未开启」→ 重新准备本机并完成配对；日志无 `analysis worker failed` 刷屏）待设备回来补跑 |
| C7 | **D3 定位与修复**：先取真机证据（①`receipt.rs:99-107` 拒收分支加三字段诊断日志，定位 F13 是哪一身份不符并修；②手机 release 构建下 controller 的阶段/错误日志出口；必要时加一次性诊断日志并重跑），再按 §三之二 的判据修「交接完成后 UI 不收敛」；补用例覆盖「回执被拒 → 发送端不无限重推」与「阶段到终态 → UI 离开等待页」，均做变异验证 | F13 定位日志 + 现象前后截图（不重启）+ 用例红绿 | `[ ]` |。**改写条款**：本步骤的「修」不做预先承诺 —— 若定位结果推翻 §三之二 方向 0（F13），则本行必须在动手前重写（见 §五之二「完成定义」第 2 条）
| C8 | **D4 状态可见**：手机端加全局同步状态位（主界面状态条/角标 → 点击直达解锁），区分「待解锁积压（显示条数/字节）」与「网络延迟」；桌面端把 `sync_wire_outbox` 的 pending 投影成可读的「对端未确认 N 条」 | **完成（2026-10-27）**。①**口径先立**：`LocalSyncStateDto` 新增 `pending_wire_envelopes` + `pending_bytes`，由 `Store::pending_wire_backlog()`（`src/storage/sync/delivery.rs`）从 `sync_wire_outbox WHERE state='pending'` 取 `COUNT(*)` 与 `COALESCE(SUM(LENGTH(encrypted_envelope)),0)` —— `pending_operations` 是「本机还有多少操作没被回执」，新字段是「对端还没确认收到几条」，两者不可互替。真库实测当时是 `pending=12 条 / 58,590 B`、`acknowledged=94`，正好是「显示 64 条待确认但其实在等对端」那种误导的现场。②**手机状态位**：`ui/lib/sync/sync_backlog_notice.dart`（纯判定）+ `ui/lib/widgets/sync_backlog_banner.dart`（只在真有积压时出现，`locked` ⇒ 「待解锁积压 N 条 · X KB · 解开本机密钥后自动同步」且可点直达同步页；`local_ready` ⇒ 「同步延迟：对端未确认 N 条 · X KB，正在重试」且**不给假按钮**；`not_configured`/`requires_repair` ⇒ 不显示，各有自己的入口）。挂在 `main_screen.dart` 移动端主界面（`aiProviderSetupHint`→`CompanionRevisitHint` 之后），点击 `sidebarTabProvider.set(SidebarTab.sync)`。③**桌面**：`SyncBacklogLine`（`sync_tick_status.dart`）在同步页投递状态行下方显示「对端未确认 N 条 · X KB」，0 时不渲染。④**用例**：Rust `api::sync::invite::tests` 2 条（只算 pending / 空集不报错且 COALESCE 兜住 NULL）；Dart `ui/test/sync_backlog_banner_test.dart` 9 条（判定 4 + 手机状态位 3 + 桌面行 2）⇒ `fvm flutter test` **全仓 476 passed**。⑤**变异验证**：Rust ①去掉 `state='pending'` 过滤 ⇒ 红（`left (3,1029)` vs `right (2,30)`）；②去掉 `COALESCE` ⇒ 空集用例红；Dart A `actionable` 恒 true ⇒ 2 红；B 零积压也返回 notice ⇒ 2 红；C 锁定/就绪两类对调 ⇒ 2 红。全部还原后复绿。⑥**门禁**：`cargo test --all-targets` **976 passed / 0 failed**（首跑 975+1 failed 的 `ai::generation::…observable_run_checkpoints…` 是并行负载下的既有 flake：单跑 3/3 绿、模块级 9/9 绿、全量复跑 976 全绿）；`fvm flutter test` 476 passed；`fvm flutter analyze` 133 issues / **0 error**；`scripts/regen.sh` EXIT=0。⑦**文案待你报审**：本行四句用户可见文案为我起草（见上），改文案只需动 `sync_backlog_notice.dart` 的 `message` 与 `SyncBacklogLine` 一行。 | `[x]` 2026-10-27 |
| C9 | **解锁路径真机验证（独立于其它步骤，可最先做）**：全新身份 + 全新安装 + app 重启后，确认「解锁本机密钥」能成功（本轮首次走到成员设备状态，该路径**从未成功验证过**）；解锁后积压在一个轮询周期内灌入、`sync_wire_outbox` 的 pending 归零、两端内容一致 | 解锁前后 relay 信箱计数 + 桌面 pending 计数 + 两端同一条消息截图 | `[ ]` |

> 顺序理由：C0 先把两条缺陷钉成红用例（否则修完无法证明修的是什么）；C1/C2 是 D2 的两层，C3 是 D1；C4 收口；C5/C6 是门禁与真机。**C7 是唯一根因未确证的步骤**（其余步骤的根因都已用库/日志实测钉死），因此它被拆成「定位 → 修」，并带回写条款；**C9 建议提前到最前面单独跑** —— 它不依赖任何代码改动，却决定了后面所有真机验收能否进行（本轮用户实际卡住的正是这一环）。

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
- 不在本 roadmap 内：provider key 的重加密/迁移（见 §六）、配对快照传输本身的任何改动（归上游 roadmap）。

---

## 五之二、完成定义（这张表回答「全绿之后还剩什么」）

**C0~C9 全 `[x]` 的含义是**：四条缺陷（D1/D2/D3/D4）的现场现象不再复现、各自有红→绿用例与变异验证兜底、四道门禁全绿、并在真机上验收过。**它不等于「同步功能没有其它问题」**，因为：

| 事项 | 全绿后是否解决 | 说明 |
|---|---|---|
| D1 `requires_repair` 卡死 | ✅ 是 | 有出口 + 副作用提示，判定标准 4 条锁死 |
| D2 未配置裸读单例刷屏 | ✅ 是 | 领域错误 + worker 不再重试 |
| D3 交接完成 UI 不收敛 | ⚠️ **取决于 C7 的定位结果** | 目前根因未确证。若最终定位不是 F13（回执被丢弃），而是例如两端 `dataset_id` 实际不一致、或 controller 停摆，则 C7 的修法要整段重写，**本行不能在定位前打勾**。假闭环的样子是：「C7 做了点事、现象没再复现、于是打勾」——那是把偶发当修复 |
| D4 状态不可见 | ✅ 是 | 跨主界面状态位 + 积压条数 + 桌面「未确认 N 条」 |
| 「解锁本机密钥」能否成功 | ✅ 是（C9 专项验证） | 本轮是**首次**走到成员设备状态，该路径从未成功验证过。F17 只证明了「有流量」，没证明「解锁动作本身可用」 |
| **provider API Key 全部失效** | ❌ **否，且需要你决策 + 手动重填** | 见 §六。`provider_key_secrets=0`、10 条 `ai_provider_configs.api_key` 全 NULL ⇒ AI 功能在两端都不可用，直到你从各家后台重新取 key。这是「选 B 换速度」的既定代价，**不是 bug，但必须有人做** |
| relay 侧累计写入字节 | ❌ 否（口径缺口） | 属上游 roadmap C4-4 ⑤，需第 4 次复跑专项采样 |
| `provider:` 一类信令的 ack 语义 | ✅ 已澄清 | 「永不 ack」的旧判断已被推翻（见上游 C4-4 ⑥ 纠正），无需动作 |

**结论口径**：本 roadmap 解决的是「状态机的边界与终点没有被正确处理/呈现」。它**不**承诺同步协议本身无缺陷 —— 协议层的收敛性归上游 [配对快照交接收敛性](2026-10-26-enrollment-snapshot-convergence-roadmap.md)。

---

## 六、关联发现（**待定，不纳入本轮**，需用户决定）

**换同步身份 ⇒ 旧 sealed provider keys 全部永久失效，且无任何提示。**

- 现象：重建身份后进入 `preparing_snapshot`，每拍报 `AnyhowException(provider_key_authentication_failed)`，配对无法继续（2026-10-27 10:39 实测）。
- 根因：`provider_key_context()` 把 `dataset_id` + `installation_id` + provider id/url/type 当 AAD（`src/storage/provider_keys.rs:40-62`）；身份一换 AAD 全变，ChaCha20-Poly1305 认证必然失败（`:215`）。
- 后果：`provider_key_secrets` 的 10 条密文变成永久无法解密的死数据；`ai_provider_configs.api_key` 早被写成 `NULL`，明文不存在，**用户只能去各服务商后台重新取 key**。而且快照设计上必含 provider keys（`src/storage/sync/snapshot/records.rs:80-81`）、UI 确认框只有「共享所有 Provider 配置和 API Key」一途（无「不共享」选项），所以只要桌面上有 sealed key，快照导出就必然撞上这个错误。
- 与 D1 的关系：**D1 的「清除同步身份」出口必然踩到它**，所以 §四 C3 的确认弹窗必须写明「已有 Provider API Key 将失效，需要重新填写」，否则就是给出一个会静默破坏数据的出口。
- 待决定的选项：(a) 出口里顺手把 key 作废并提示；(b) 提供「重封」迁移（需要旧 master，即先解锁旧 vault，实现成本高）；(c) 允许用户选择「快照不含 provider keys」。
- **当前实际状态（2026-10-27 11:40 实测）**：用户已选 B（丢 key 换速度）—— `provider_key_secrets` 已清空为 0 条、`provider_key_policy` 保留 1 行（sealed 标记）、`ai_provider_configs` 10 行结构（base_url/type/模型）完好但 `api_key` 全为 NULL。**⇒ AI 功能在桌面与手机两端都处于不可用状态，直到逐条重新填 key。** 这是一笔待办，不是待修 bug；若将来要做 (a) 或 (c)，必须同时给出「如何让用户知道哪些 provider 需要重填」的界面，否则等于把问题从「静默失效」挪到「静默缺失」。

---

## 七、进度记录

- **2026-10-27 10:50** 建立本文档。D1/D2 均有现场实测证据（见 §一）。C0 尚未开始，无代码改动。
- **2026-10-27 11:15** 上游 roadmap 第 3 次真机复跑**配对成功**（桌面对端确认、手机「成员设备」、数据完整落地、relay 残留 0）。同时暴露本文档 **D3**（交接完成后手机 UI 不收敛，需重启 app 才显示终态，见 §一 F10~F12 与 §三之二）：数据层与 UI 层出现了一次真实的一致性断裂，故新开 C7；D3 与 D1/D2 同源 —— 都是「状态机已经走到终点，但没有把终点正确地呈现/推导出来」。
- **2026-10-27 11:25 复核纠正**：上一版本文档与上游 roadmap 里有两处判断被真机数据推翻，已就地改写而非另起备注 —— ①「`provider:` 永不 ack」不成立（8 条全 acknowledged；`in_scope` 只约束 enrollment 路径，普通 pull 路径白名单含 `provider:`）；②§三之二「已排除：手机没拒收信令」撤回，并新增 F13（11:04:16 一条 `sync_receipt_identity_mismatch` 回执被丢弃），升为 D3 的首要候选根因（§三之二 方向 0）。教训与本文档主线一致：**没把该 bug 的每条处理路径都验证一遍，就不能把「某路径不处理」写成「永远不处理」**。
- **2026-10-27 11:40 补全（D4 立档 + 完成定义）**：用户报「桌面一直在写、手机看不到」，实测得 F14~F17 → 新增 **D4（同步暂停/积压状态在主界面不可见）**、§三之三、步骤 **C8**；并新增 **C9（解锁路径真机验证）**、§五之二 **完成定义**（逐条写明全绿后解决了什么、没解决什么 —— 其中 D3 明确「定位前不得打勾」、provider key 需人工重填、relay 写入字节仍是口径缺口）。本轮仍**零代码改动**，只动文档。
- **2026-10-27 11:45 现场旁证**：解锁后双向流量自证（F17），`messages` 594→624、outbox ack 21→64；手机信箱 13→0→13 循环、桌面信箱 1→11→1 ⇒ 协议层与传输层未坏，坐实 D3/D4 都属「状态呈现」而非「同步失效」。
- **2026-10-27 13:12 C1 完成**：单例读取接缝落地，`provider_master_key` 在同步未配置时返回 `sync_not_configured`（C0 的 D2 红用例已转绿）。同时确认 `sync_gc_state` 的唯一读点（`gc.rs:92`）本来就是 `.optional()? + 默认 0`、按 `origin_stream_id` 而非 singleton 读，语义正确，未纳入改造。
- **2026-10-27 13:36 C2 完成**：worker 层终态分类 + 退避落地，实测 5 条/130 s → 1 条/200 s。**踩坑记一笔**：桌面 bundle 只重建 Flutter 侧时，`target/release/libelsewhen.so` 仍是旧逻辑 ——我第一次对比就撞上（旧 Rust + 新 Dart），差点把「Rust 修复没生效」误判成「策略没起作用」。这次反过来把它当成了「前」基线；以后凡测桌面行为，先确认 release lib 已重建。
