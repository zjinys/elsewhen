# 配对快照交接收敛性 Roadmap（mail 传输 → blob 唯一数据面）

- 建立：2026-10-26
- 状态：**C4-4 / C5-2 真机验收通过（2026-10-27 11:08）—— 配对完成、数据完整落地、relay 残留 0**。C5-A 已全部落地（2026-10-26 17:05）；此前两轮真机复跑依次暴露附加发现 ⑥（快照被 `sync_snapshot_frontier_not_covered` 永久拒收，C5-A-8 已修）与本轮新记的 D3（交接完成后手机 UI 不自动落地，需重启 app，已转入 `2026-10-27-sync-lifecycle-boundaries-roadmap.md`）。本节唯一未闭环项：relay 侧累计写入字节未采样（见 C4-4 ⑤ 口径缺口），信令面「`provider:` 永不 ack」这一度判断**已被真机推翻**（见 C4-4 ⑥，10-27 11:25 纠正）
- 起因：真机配对失败现场 —— 桌面端 174 项分片全部「已上传」，Android 端永远停在「等待成员授权与初始同步」
- 上游 roadmap：[relay v3 数据面重构](2026-10-24-relay-v3-dataplane-roadmap.md)（R0~R7 已完成）
- 上游事故档案：[enrollment 快照传输死锁](../notes/incidents/2026-10-24-enrollment-snapshot-transfer-deadlock.md)（§九 = 本轮 C5-A）
- 口径：**根治，不打补丁**。最终结论：三条不变式在「几百封信 + 整份重发兜底」这个形状上补不齐 → 按 C5-A 删掉 mail 快照路径，blob 成为唯一数据面（mail 只留信令面）

## 使用规则（必须遵守）

1. 每完成一个步骤，**立即** `[ ]`→`[x]`，并在「证据」列写日期 + 可复核事实（测试名/命令输出/行号/relay 库实测值），不写「已完成」这类空话。
2. 步骤失败或前提不成立时改 `[!]` 并写清阻塞原因，不静默跳过。
3. **新回归用例必须做变异验证**（改坏实现看用例是否变红），否则不算完成。
4. **任何代码改动前先落本文档的对应步骤 + 申报范围**；范围外的改动单独提，不夹带。
5. **真机验收以 relay 信箱实测为准**，不以 UI 文案为准（本次 174 项「已完成」就是 UI 假象）。
6. **本文档是唯一进度权威**；`docs/proposal-backlog.md` 与方案 note 的进度行同步更新。
7. 时限规则：2026-10-26 24:00 前 C0~C4 未全部 `[x]`，**立即转 C5**，不再追加调试轮次。

---

## 一、已确证事实（2026-10-26 10:15 现场）

三元对照：桌面日志 `~/.local/share/elsewhen/agent-debug.log` ↔ 本地库 `~/.local/share/elsewhen/elsewhen.db` ↔ 线上 relay 信箱只读副本（`/tmp/relay-inspect/`，从 Tower `/mnt/cache/appdata/elsewhen-relay/data/mailboxes/` 拷贝，不碰活库）。

| # | 事实 | 证据 |
|---|---|---|
| F1 | 快照导出 `6d1d2a8d`（接收方 `59cfd26d` = 手机）**state=`ready` 且从未 acknowledged**，`local_watermark=241`、`roster_revision=16`、`transport=mail` | `sqlite3 elsewhen.db "select * from sync_snapshot_exports"` 8 行，其中 6 行为悬空 `ready` |
| F2 | 该导出 176 条 wire 全部 `pending`，每封 `attempts=19` | `sync_wire_outbox`：`59cfd26d` pending=186（seq 1116–1302），acknowledged 仅 2（`membership:16`、`enrollment-offer`） |
| F3 | 桌面**每拍推 11 封**、约 3–5 s 一拍，且**成功推送零日志**（`Ok(id) =>` 分支无 `wire_log`） | `src/sync/engine.rs:292`；`due_sync_wire_envelopes` 的 8 MiB 字节上限切断在 11×699,701；`available_at_ms` 按 11 行一组向前滚动 |
| F4 | relay 手机信箱实测：38 封/18 MB → **75 s 后只剩 12 封/80 KB**（`id>190` 全被删） | Tower `/tmp/relay-inspect/t1|t2/663bd317*.db`：`select count(*),min(id),max(id),sum(bytes)` |
| F5 | 删不掉的 12 封是 **`op-*`×10（seq 1116–1125）+ `provider:*`（1126）+ `snapshot-end:*`（1302）**，从 09:25 挂到现在 | `select seq_hint,state,attempts,message_id from sync_wire_outbox … order by seq_hint` |
| F6 | 被秒删的是全部 `snapshot:6d1d2a8d:{0..173}` 分片；而接收方只有 `accepted==true` 才 `ack` | `src/sync/runtime/enrollment/flow.rs:471-478` |
| F7 | 手机 UI 的进度 = `COUNT(*) FROM sync_snapshot_chunks WHERE snapshot_id=record.snapshot.id`，显示 0/174 | `src/sync/runtime/enrollment/view.rs:154`；UI `ui/lib/widgets/joiner_sync_pairing_settings.dart:152-169` |
| F8 | 导出只有**收到最终应用回执**才批量退休 176 条 wire，没有逐分片回执、没有超时推进 | `src/storage/sync/delivery.rs:362 acknowledge_snapshot_export`（源码注释已写明「否则整条导出永不被确认，两端都停在『等待成员授权与初始同步』」） |
| F9 | 桌面信箱残留一封 10-24 wipe 前的旧配对信（sender `30461e62` / seq 0 / 828 B），本会话被重扫 1239 次 | `agent-debug.log`：`pull-mail … from 30461e62 … accepted false` 1239 条；`sync_relay_cursors` 该信箱 `after_id=0` |

**F6 与 F7 并存即矛盾**：分片被 ack 说明接收方 `accept` 返回 `Durable`（`stage_snapshot_chunk` 落库成功），但进度读出 0。C0 必须定性这一条，否则后续所有修复都是猜。

### 一·补 11:50 复核（直接读活 relay 库，推翻 F4/F5/F6 的解释）

`/tmp/relay-inspect/` 副本已不存在，改为**只读拷贝活库到 Tower `/tmp`**（`cp *.db* /tmp/…` 后再 `sqlite3`，不碰活文件）。

| # | 事实 | 证据 |
|---|---|---|
| F10 | 手机信箱**现在**是 **186 封 / 121,262,810 B（121 MB）**，不是 F4/F5 记的 12 封/80 KB。`min(id)=2, max(id)=3080` | Tower `/tmp/phone-mail.db`：`select count(*),sum(bytes),min(id),max(id) from relay_v2_mail` |
| F11 | 其中 `id<200` 只有 12 封（`op-*`×10 seq 1116–1125、`provider:` 1126、`snapshot-end:` 1302 / id **190**）；**其余 174 封全是 `snapshot:6d1d2a8d:*` 分片（每封 699,695~699,698 B），id 2940–3080** | 同上，`where seq_hint between 1128 and 1301` |
| F12 | 所以 **F6 的结论是错的**：分片没有被秒删，它们一直在信箱里（id 变新 = 被删后由发送端重推重新插入）。真正的矛盾是「174 片全在信箱、`snapshot-end` 也在、接收方一片都没落盘」 | F11 + F7（`sync_snapshot_chunks` 计数 0） |
| F13 | **507 配额不是本次主因**：线上 `RELAY_MAILBOX_QUOTA_BYTES=536870912`（512 MiB，`skills/relay-deploy/config.env`），121 MB 远未打满；代码默认 50 MiB 那条只影响没设 env 的部署 | `config.env` 实测 + `server.mjs:363/374` |
| F14 | `enrollment-offer:6f4199e2…`（seq 1303）**acknowledged** → 接收方 `select_enrollment_snapshot` 成功过 → `record.snapshot` 非空、staging 目标已建。所以「快照还没选中」不能解释 174 片全不收 | `sync_wire_outbox` 实测：`enrollment-offer` acknowledged，`membership:16` acknowledged |
| F15 | `snapshot-end`（2239 B，id 190，`created_at` 与 09:25 同批）从 09:25 挂到现在未被 ack、也未被 drop | Tower 实测 id=190 行 |
| F16 | 发送端每拍重推 11 封、`attempts` 已到 21~23，但 `state` 仍 `pending`。**注意：分片 wire 只有「最终应用回执」能退休（F8），所以 `pending` 本身不证明对方没收到**；证明「对方没收到」的是 F11/F15（relay 上还在 = 没 ack） | `sync_wire_outbox` + `flow.rs` 的 `accepted` 才 ack |

**结论：C0 要定性的只有一件事** —— 接收方对这 174 片 + `snapshot-end` 的 `transfer::accept` 到底返回什么（`Waiting`？哪条 `ensure!`？）。`Waiting` 会静默不 ack 不报错，`Err` 会静默（`looks_like_dataplane_envelope` 分支只响一次）。两者在真机上都表现为「永远卡住 + relay 上信越堆越多」。

**C0-1 的复现方式改为复用生产链路测试骨架**（比重写一套 UI 驱动可靠）：`src/sync/transport_relay_smoke_tests/enrollment_ui.rs::run_with_fault_proxy` 已经是「真 relay + 两个全新 data dir + 真 `enrollment::advance`/`confirm` 循环」的生产路径，只是快照体只有 ~1.26 MB（≈6 片）。给它加一个 `repeat` 参数，把快照体放大到 ≈174 片（`"large shared original ".repeat(3_000_000)` ≈ 63 MB 明文），并把 joiner 每拍的 `phase/received_chunks/total_chunks` 与 `advance` 的**错误串**打出来。申报范围：该文件加参数化 + 新增一个 `#[ignore]` 的 C0 复现用例（**不改生产代码**）。

## 二、根本原因判断

**主因是算法（状态机缺边），不是数据。**数据只是触发器。

mail 传输沿用旧模型「一次把 174 封全塞进 outbox，靠 relay 幂等 + 重发兜底」，**从未具备 blob 传输（R4）已有的三条不变式**：

| 不变式 | blob 传输（R4） | mail 传输（现状） |
|---|---|---|
| ① 有界在途 | `WINDOW_CHUNKS=10`，在途 ≤ 一窗 | 无窗口，174 封全 `pending`；11 封/拍只是 8 MiB 字节上限的**副作用** |
| ② 有进度反馈 | 逐窗 `ack upto`，发送端据此推进 | 无逐片回执，只能等「最终应用回执」一次性退休 |
| ③ 有停滞推进 | `window_wait_ms` 超时后留在 `building`，下一拍从 `index` 续 | 导出 `ready` 后无任何推进路径；停滞 = 永久 |

三条全缺 ⇒ 任一环节丢一次回执，系统**没有任何路径能重新收敛**。这是设计缺口，不是运气问题。

---

## C0 复现定性（先做，不改仓库代码）

目标：把 F6/F7 的矛盾、以及「`snapshot-end` 为什么不被 accept」这两条从猜测变成事实。手机上 `run-as`/`adb root` 均不可用、release 包 stderr 不进 logcat，所以只能本地复现。

| # | 步骤 | 状态 | 证据 |
|---|---|---|---|
| C0-1 | 本地起临时 relay（`PORT=8899`、空 `DATA_DIR=/tmp/conv-relay`、`NO_PROXY` 已设），两个全新 data dir 实例走**生产** `enrollment::advance`/`confirm` 链路，快照体放大到 ≈174 片，`ELSEWHEN_DEBUG=1` 收 joiner 侧 `phase/received_chunks/错误串` | `[x]` | 2026-10-26 12:40。**结果：本地复现「通过」，不是失败**。`run_scaled` 新增（`src/sync/transport_relay_smoke_tests/enrollment_ui.rs`，仅测试代码）：`Bulk::Spread{count:1500, bytes:30_000}` = 45 MB 明文 → **347 片**，log `[c0] +83s joiner chunks=341/347 → +95s 347/347 local_applied=true → +99.7s joiner phase=complete`，`test result: ok`，耗时 101.84 s。命令：`ELSEWHEN_RELAY_TEST_URL=http://127.0.0.1:8899 cargo test --lib sync::transport_relay_smoke_tests::enrollment_ui::c0_large_mail_snapshot_repro -- --ignored --exact --nocapture`（日志 `/tmp/conv-repro.log`）。**推论：mail 快照传轮算法在干净起点、347 片规模下能收口（比现场 174 片更大），所以事故不是“算法上不了规模”** |
| C0-1b | 首次复现误用单条超大事件（`repeat(3_000_000)` = 63 MB 单记录）→ 直接撞 `sync_snapshot_record_too_large`，且 3 次后进 **`sync_enrollment_snapshot_attempt_limit` 终态死锁**（打印中持续 24 s 不变） | `[x]` | 日志 `/tmp/conv-repro.log`（第一次跑）。**副产品发现**：导出准备失败 3 次后进 `sync_enrollment_snapshot_attempt_limit`，无任何退避/重试入口 → 该相位永久卡死，与现场「`ready` 悬挂导出永不取销」同类。已拆为独立待办（见 §二·补 的 F17） |
| C0-2 | 定性「分片已 ack 但 `sync_snapshot_chunks` 计数为 0」 | `[!]` | **受阻：无手机访问**。`adb devices` 为空，且本轮新增证据推翻了这个提法本身：F11 实测 174 片**全部还在信箱未被 ack**，Id 190→2940+ 的新 id 是「被 ack 删除后由发送端重推重新插入」的结果，不是「秒删」。所以「ack 了但计数 0」不成立；成立的是「片子一直在信箱里反复重写，接收方一片都没落盘」。**无法再缩小到具体 `ensure!` 的直接原因：joiner 侧日志拿不到**（release 包 stderr 不进 logcat、无 `run-as`） |
| C0-3 | 定性 `snapshot-end` 被拒的精确原因 | `[!]` | **受阻：同上**。已排除的部分：`snapshot-end` 落在 `accept_snapshot` 之前的两条 `ensure!`（`sync_enrollment_snapshot_identity_mismatch` / `sync_enrollment_manifest_changed`，`src/sync/engine/enrollment.rs` snapshot 分支）；`publish_and_receipt` 内部**不抛 `ensure!`**（`publish_snapshot` 只返 bool）→ 排除「被 publish 内部拒」。且 `loan_flow.rs` 已有守门用例覆盖乱序（manifest 先到、最后一片到齐才发布），说明旧口径的乱序死锁已修 → **现场是另一条未覆盖的路径**，无设备日志无法收敛 |
| C0-4 | 写事故档案 | `[ ]` | 未开始（预算耗尽前未动工） |

**C0 定论（置信分层）**

1. **已证伪（高置信）**：
   - 「mail 快照路径根本跑不通 / 上不了百片规模」 → **假**。347 片本地生产链路 101 s 收口（C0-1）。
   - 「F6 分片被秒删」 → **假**。174 片实测仍在信箱（F11/F12）。
   - 「507 配额是本次主因」 → **假**。线上 512 MiB，占用 121 MB（F13）。
   - 「快照没被选中」 → **假**。`enrollment-offer` seq 1303 已 acknowledged（F14）。
   - 「`publish_and_receipt` 内部 `ensure!` 拒收 `snapshot-end`」 → **假**（代码路径已核）。
2. **已证（高置信）：结构性根源是「无逐片退休 + 无在途窗口」**，与 C1 的判断一致，且有现场量化证据：
   - **发送端把同一批 pending wire 反复重发**：分片 wire 唯一的退休途径是最终应用回执（`snapshot-end` 回执），逐片 ack 不退休 wire → 每拍重发 → 现场 `attempts=21~23`，174 片 × ~700 KB × 21 ≈ **2.5 GB 重复上传**；relay 侧 121 MB 常驻。
   - relay `/send` 的幂等短路（同内容零写入）因此**永远命中不到**：分片被接收方 ack 删除后，发送端重推变成「新插入」→ 新 id（190 → 2940…3080）→ 接收方再 ack → 再删 → 再推，**没有收敛点**。
   - 接收方每拍只处理 relay 单页（`RESPONSE_BYTES=16 MiB` ⇒ ~22 片/拍），而发送端持续重写整个分片集合，进度很容易被“重新填满 174 片”掩盖；一旦 App 被冻结/息屏（`sync_keep_alive` 只在交接忙碌窗口持有），进度就停在那里，`snapshot-end`（id 190）永远等不到「全片已落盘」那一刻。
   - 排队头部还有 11 封接收方在快照期**故意不接受**的 `op-*`/`provider:`（快照期不得夹带业务增量），它们占住 `ORDER BY available_at_ms,rowid LIMIT 16` 的头部槽位，形成头部堵塞。
3. **未证（需要设备）**：joiner 侧 `transfer::accept` 对这 174 片与 `snapshot-end` 的具体返回（`Waiting` / 哪条 `ensure!`）。这一层**不影响修复方向**：三条不变式无论命中哪条都必需（无窗口+无退休必然导致反复重传与头部堵塞）。

**因此 C1 的设计输入已经足够，不再等设备日志。**

**C0 验收**：C0-2、C0-3 各有一行「哪个函数、哪条 `ensure!`、什么输入」的确证结论；结论与本 roadmap §二的判断冲突时，先改本文档再动代码。

## C1 补齐三条不变式（核心根治）

> **事后注（2026-10-26 17:05）**：本节 C1-1/C1-2 的落点（逐片回执、`confirm_snapshot_chunk`、`snapshot_allowed` 放行分片信封、回执上限抬到 32）**已随 C5-A 整条删除**。它们在本节写作时确实是 mail 路径唯一能收敛的形状，但后来被两个事实推翻：①这条路径的吞吐与上线量（0.38 MB/s、2.67×）在任何形状下都不合格；②即使逐片退休，blob 数据面已有的「窗口 + 逐窗 ack + 空闲预算」也是同一份需求的更短解。于是正确处置是删掉整条路径（C5-A），而不是把 blob 的不变式逐条搬进信箱。C1 的三条不变式现由 blob 数据面天生提供；本节保留原文作为**决策痕迹**，不再描述当前代码。

申报范围：`src/sync/engine.rs`、`src/sync/engine/snapshot.rs`、`src/sync/engine/enrollment.rs`、`src/storage/sync/wire.rs`、`src/storage/sync/delivery.rs`、`src/storage/sync/snapshot/*`、`src/sync/runtime/enrollment/flow.rs`、`src/sync/runtime/enrollment/view.rs`、`migrations/64-*`（仅当需要新列）。

### C1 设计定稿（2026-10-26 13:20，实现前的最后一道校对）

**为什么不能“直接给每片发回执”** —— 读码确认的核心陷阱：

`accept_snapshot` 的 chunk 分支（`engine/snapshot.rs:644-668`）**从不** `queue_sync_wire_receipt`，唯一的回执是 manifest 到齐后的 `publish_and_receipt`（subject = `snapshot-end:{id}`）。而 owner 侧收到 `snapshot:{id}:{n}` 形式的回执时，现有代码走的是 `acknowledge_snapshot_export`（`engine/enrollment.rs:468` 的 `snapshot_subject` 分支），而该函数（`storage/sync/delivery.rs:362`）会：

1. `acknowledge_sync_wire(subject)` —— 只退休那**一封**；
2. `UPDATE sync_wire_outbox SET state='acknowledged' WHERE message_id IN (SELECT … sync_snapshot_export_wires …)` —— **把整份导出的所有分片 wire 一次性标成已确认**；
3. `UPDATE sync_snapshot_exports SET state='acknowledged'`；
4. `mark_member_snapshotted` 播种 watermark。

所以若直接让每片回执走这条路，**第一片的回执就会宣布整份快照完成**，后续 173 片永远不再发送（`outgoing_scope` 的 `snapshot_allowed` 随之为假）。这就是“逐片确认”一直没做的真正原因，不是忘了。

**由此确定的三条不变式落点**（均为「沿用现有机制」而非新造语义）：

| 不变式 | blob 路径已有的做法 | mail 路径本次补法 |
|---|---|---|
| ① 有界在途 | `WINDOW_CHUNKS=10` + 每窗 `blob_ack` | 接收端每片 `queue_sync_wire_receipt`；发送端用**新的** `confirm_snapshot_chunk` 只退休**单片** wire（不碰 export state / watermark）。pending 集合随进度真减 → 重传归零 |
| ② 进度反馈 | `highest` 水位 | owner 侧 `received_chunks` 不再恒 0（现为假象），改为「已 acknowledged 的分片 wire 数」；joiner 侧 `COUNT(*) sync_snapshot_chunks` 不变 |
| ③ 停滞推进 | `WINDOW_WAIT_MS=20s` 超时重拉 | 分片 wire 重发次数达阀值仍无回执 → `block_sync_wire` 落 `blocked_reason`（复用现有状态机，不新开列），面板可表态而不是默默空转 |

**关键约束（不得违反）**：`snapshot-end:{id}` 的回执仍走 `acknowledge_snapshot_export`，语义不变 —— 那是唯一能播种 watermark / 推进 GC 的时机。逐片确认**绝不能**提前触发它。

**回执批量上限**：`receipt_subjects` 现 `ids.truncate(8)`、`due_enrollment_wires` 的 `ensure!(messages.len() <= 8)`。174 片的回执若每拍只走 8 条（还含 3~5 条固定项）→ 需 35~58 拍才能拍完。本步把上限提到 `32`（回执本身是 ~200B 控制帧，32 条 ≈ 6 KB，远低于 8 MiB 预算），让收敛从「数分钟」降到「几十秒」。两处上限必须同时改，否则 `sync_enrollment_wire_scope_invalid`。


| # | 步骤 | 状态 | 证据 |
|---|---|---|---|
| C1-1 | **①有界在途**：接收端每片 `queue_sync_wire_receipt`；发送端新增 `Store::confirm_snapshot_chunk` 只退休**单片** wire（不碰 export state / watermark）。未新开窗口常量：pending 集合随逐片退休而真减，叠加现有「16 封 / 8 MiB」与 `resend_spacing_ms` 轮转即构成有界窗口；另开常量反而会与字节预算重叠 | `[x]` | 2026-10-26 13:40。`src/storage/sync/delivery.rs::confirm_snapshot_chunk`（校验 subject 属本快照 + `sync_snapshot_export_wires` 存在性，再走 `acknowledge_sync_wire`）；`src/sync/engine/snapshot.rs` chunk 分支补 `queue_sync_wire_receipt`（与 `stage_snapshot_chunk` 同事务）。**变异验证已做**：把 owner 侧分片分支改回 `acknowledge_snapshot_export` → `chunk_triggered_publication_still_delivers_a_receipt_and_completes` 在「单片回执绝不能把整份导出置成已交付」变红；恢复后 18/18 绿 |
| C1-2 | **②有进度反馈**：复用 `EnvelopeReceipt`（subject = 分片真实 `message_id`），未新增控制帧。不需要「跳号清单」—— 逐片回执天然精确到片。owner `view.rs` 里那条「对端已回执的分片信封数」查询**本就写了但被架空**（没有逐片回执时恒为 0），现在真生效 | `[x]` | 端到端 347 片：修复前 `owner chunks=0/347` 而恒 0；修复后 `[c0] +116s owner phase=complete chunks=347/347`。回执批量上限 `receipt_subjects::truncate(8→32)` 与 `due_enrollment_wires::messages.len()<=32` 两处同步改 |
| C1-3 | **③有停滞推进**：原规划的「`ready` 超时 → 重建导出（新 id）+ 重发 offer + 接收方 abort 旧 staging」**未做**。2026-10-26 15:50 试过一个「`attempts>=8` 且已上 relay 的快照线落 `blocked_reason`」的轻量替身，**当场回滚**：它只写标签不改节奏（`resend_spacing_ms` 已把重发压到 5 分钟），却带来新常量、魔法阈值和两处重复调用点 —— 属于「加了机制没有加能力」。当前仍只能依靠「发送端 backoff 重发 + 接收端逐片重试（`stage_snapshot_chunk` 幂等）」 | `[ ]` | 未实现，且明确不做轻量替身。**影响**：接收端陷入持续拒收时，发送端只会无限重发而不是换新导出，quota 最终会被 507 撞满；这与 C2 的「3 次必表态」是两件不同的事（C2 管接收端丢弃，③ 管发送端换路）。判定「是否需要」放在 C4-4 真机结果之后：真机一次跑通则不引入重建路径（YAGNI），再卡住才做 |
| C1-4 | CI 守卫：已补一条并做变异验证（见 C1-1）；「停滞重建」用例随 C1-3 未做而缺失；「窗口上限」因未新开常量改由 C1-1 的窗口论证 + 既有字节预算用例覆盖 | `[!]` | 部分完成：`chunk_triggered_publication_still_delivers_a_receipt_and_completes` 已扩写为逐片确认守卫（含「除那一封外仍 pending」断言）。全量 `cargo test --lib sync::` = **286 passed, 0 failed, 4 ignored** |

**C1 落地结论（2026-10-26 13:45）**

- ① 有界在途：**已根治**。分片 wire 现在逐片退休，不再等最终回执 —— 这正是现场 `attempts=21~23`（≈2.5 GB 重传）的直接成因。
- ② 进度反馈：**已生效**，且是「本就有查询、被缺机制架空」自动恢复 —— 不是加旁路。
- ③ 停滞推进：**未完成**（原规划的自动重建导出）。剩余风险：接收端持续拒收时仍会无限重发。
- 端到端回归：`c0_large_mail_snapshot_repro`（347 片 / 45 MB 明文 / 真 relay）**117 s 收口**，`test result: ok`。比修复前（101 s）略慢，慢的就是新增的 347 条回执控制帧 —— 换来的是重传归零。

**C1 验收**：197 片模拟快照（事故同规模 174 片 + 余量）端到端 ≤ 5 分钟；人为丢弃任意一次最终回执，系统在 ≤ 2 分钟内自愈完成；relay 侧该导出的在途未确认片数峰值 ≤ 窗口。

## C2 静默丢弃改为显式表态

申报范围：`src/sync/runtime/enrollment/flow.rs`、`src/storage/sync/wire.rs`（拒收计数复用）。

| # | 步骤 | 状态 | 证据 |
|---|---|---|---|
| C2-1 | `flow.rs` 末尾的 `else { false }`（本机信箱里既不路由、也不是数据面信封的邮件）改为**显式 `/drop`** 并写 `sync_wire_rejections`；`ack` 语义是「已应用」，这里必须用 `drop`（R5 签名域分离的意义） | `[x]` | 2026-10-26 15:20。`src/sync/runtime/enrollment/flow.rs::pull_transfer` 扫描循环改为三态 `enum Disposition { Applied, Waiting, Rejected(anyhow::Error) }`，**删除**进程内的 `transfer_drop_registry()`（`OnceLock<Mutex<HashSet>>`）—— 计数改为落库（`Store::record_wire_rejection`，PK `(mail_id,mailbox_id)`），达 `engine::MAX_WIRE_REJECTIONS`(=3) 即 `client.drop`（签名域 `drop`≠`ack`）后 `mark_wire_rejection_dropped`；应用成功则 `forget_wire_rejection`。非数据面载荷（`looks_like_dataplane_envelope` == false）落 `sync_enrollment_foreign_frame`，本机信箱但非对端落 `sync_enrollment_unroutable_mail`。**变异验证四项**：(a) `else` 改回 `Disposition::Waiting` → 两个用例全红；(b) `attempts >= MAX` 改 `>` → drop 断言红；(c) 删掉 `mark_wire_rejection_dropped` → `dropped_at` 断言红；(d) 把 `return Err` 挪到推游标之前 → C2-2 用例红。恢复后 `sync::runtime::enrollment::tests` = 20 passed |
| C2-2 | 扫描光标必须能推进：任何一封邮件处理后（accept / drop）都要让 `after_id` 越过它，不允许出现「某封永远排第一」 | `[x]` | 2026-10-26 15:20。用例 `src/sync/runtime/enrollment/tests/scan_drops.rs::a_rejected_mail_advances_the_scan_cursor_before_the_error_bubbles`（假 relay 单页 1 封 + `has_more=true`，唯一一封不可路由 → 本拍报错但 `sync_relay_cursors.after_id` 已越过它）；`unroutable_mail_is_counted_dropped_and_does_not_block_the_peer` 断言「同一页里排在残留信后面的 peer 分片照常应用并 ack、分片真落盘（`sync_snapshot_chunks`=1）、残留信 3 圈后只在 `/drop` 出现而**不在** `/ack`」 |

**C2 验收**：`sync_relay_cursors.after_id=0` 的僵死状态不再出现；桌面日志中同一 `subject` 的 `pull-mail` 连续出现 ≤ 3 次。

## C3 可观测性（本次排障被误导的直接原因）

申报范围：`src/sync/engine.rs`、`src/sync/engine/deliver.rs`、`src/sync/runtime/enrollment/view.rs`、UI 只读 DTO（若需）。

| # | 步骤 | 状态 | 证据 |
|---|---|---|---|
| C3-1 | `push` **成功也要有痕**：每拍一行 `push-summary`（成败都打），**不要**每封一行（11 封/4 s 会刷爆）；严格守四字段契约 `event/subject/bytes/detail` | `[x]` | 2026-10-26 15:35。判断抽成 `engine::push_summary_line_opt`（`uploaded==0 && failed==0` → `None`，否则 `Some(push_summary_line(..))`），**两条推送路径共用**：`engine::push_due` 与 `engine::enrollment::push_due`（后者原来一行日志都没有，且没累加 `uploaded_bytes`，本次补上，失败分支也补了逐封 `wire_log("push", …)` 分类码）。用例 `push_summary_is_emitted_for_a_clean_successful_tick`（样例：`[wire] event=push-summary subject=peer-b bytes=8192 detail=envelopes 5 failed 0`）与 `push_summary_is_silent_when_the_tick_attempted_nothing`。**变异验证**：把守卫改回「只有 failed>0 才打」→ 前一个用例红。四字段契约由既有 `stderr_log_lines_only_come_from_wire_line` 静态扫描 + `push_summary_reports_ciphertext_bytes_not_envelope_count` 锁住 |
| C3-2 | 进度口径统一：同一个「已落盘片数」既能被 UI 读、也能被发送端读；owner 端「174 项已完成」必须基于 `state='acknowledged'` 而不是「relay 分配了 id」，否则就是假象（F2/F7 的教训） | `[x]` | 2026-10-26 15:35。口径本就分两侧且**都是落盘/确认态**，无需新列：owner `src/sync/runtime/enrollment/view.rs` 的 `received_chunks` = `COUNT(*) FROM sync_wire_outbox … state='acknowledged'`（relay 只分配 id 不算）；joiner = `COUNT(*) FROM sync_snapshot_chunks`（落盘才算），两侧完成态都等于 `total_chunks`。唯一按「relay 已分配 id」判的是 **joiner 的 completion 回执是否送达**，那是链路终点、永不被 ack（代码注释已写明，非假象）。回归覆盖：`loan_flow.rs:395`（owner 起始 0）、`:441`（半边 → `received_chunks == half` 且 `< total`）、`:451`（joiner 同值）；端到端 347 片实测两侧都到 `347/347`（C1-2 证据行） |
| C3-3 | 把 F3 的坑写进 `docs/roadmap/2026-10-24-relay-v3-dataplane-roadmap.md` 页尾进度记录 + 事故档案：「推送成功无日志」会让排障把『在推但对方不要』误判为『没在推』 | `[x]` | 2026-10-26 16:10。两处都落：①[relay-v3 roadmap](2026-10-24-relay-v3-dataplane-roadmap.md) 页尾进度记录末条「下游反馈：mail 传输缺三条不变式，且『推送成功无日志』害了排障」；②[事故档案 §八](../notes/incidents/2026-10-24-enrollment-snapshot-transfer-deadlock.md) 新增「第三轮」章节，含 §8.1 现场事实表（F3/F8/F9/F12 四条可直接复核）+ §8.2 三条教训（只记失败＝把成功当默认值 / 副作用指标被当健康指标 / 静默跳过不算表态） |

**C3 验收**：只看 `agent-debug.log` 就能判断「本拍推了几封、成几封、失败分类」，不需要反推 outbox 时间戳。

## C4 数据清理 + 真机验收

**顺序强制：C1~C3 通过后才做 C4**，否则清了还会重演。

| # | 步骤 | 状态 | 证据 |
|---|---|---|---|
| C4-1 | 桌面侧 drop 那封 `30461e62` 残留信（10-24 wipe 前配对残留，sender `cred_hex` 已置空）；清理后确认该信箱 `count=0` | `[ ]` | 待填：relay `ddb17544…db` 实测 `relay_v2_mail` 行数 |
| C4-2 | 作废 6 条悬空 `ready` 导出（`c01f1dad`/`890446fd`/`2d4d0584`/`dcadf22c`/`f3c700a5`/`14ff58f1`）；确认只剩当前配对在用的一条 | `[ ]` | 口径已改（2026-10-26 C5-A）：这 6 条全是 `transport='mail'`，清理已写进升级路径（见 C5-A-6 迁移 64 第 2 段），不再手工改线上库；待真机升级后跑 `select id,state,transport from sync_snapshot_exports` 复核行数 |
| C4-3 | 重建手机导出后清掉 `59cfd26d` 的 186 条 pending wire；确认 pending=0 | `[ ]` | 待填：`select state,count(*) from sync_wire_outbox group by 1` |
| C4-4 | **真机跑通一次完整配对**（桌面 + Android，全新邀请）：手机 UI 从「等待成员授权与初始同步」走到完成，且 relay 侧实测：写入总量 ≤ 1.05×、传输完成后残留 0 | `[x]` | **第 3 次真机复跑（2026-10-27 11:00–11:08）通过**：①桌面 `sync_snapshot_exports` `af56a582 state=acknowledged transport=blob blob_index=173 local_watermark=0 roster_revision=2`，日志阶段链 `preparing_snapshot → awaiting_snapshot_selection → sending_snapshot → awaiting_receipt → complete`（`commit-invite role=owner rev=7`）；②手机 UI 从「等待成员授权与初始同步」变为「同步已暂停（本机密钥已锁定）· 本机 PEUMOO · 成员设备」（`/tmp/ph5.png`，重启 app 后生效，见 D3）；③**数据完整性实测**（手机是 `adb uninstall` 后全新安装，`firstInstallTime 10:01:25`，内容只可能来自快照）：知识库 8 条（饭后散步 / anvil 项目 / 家庭情况 / 关于知识付费…）、对话历史含 2026-10-24「饭后散步习惯」与 10-26「又是新的一周」两条真实消息、AI 提供商 3+ 条（`deepseek-flash@fengwind` / `glm-5.3-flash@kunyou.asia` / `deepseek-flash@deepseek`）；④**残留 0**：relay 手机信箱 `mail=0 blob=0 chunk=0`、`relay_v3_blob` 与 `relay_v3_blob_chunk` 全 0 行；桌面信箱仅 1 封 827 B `seq_hint=0` 外来旧帧（非本会话产物）；⑤**写入量**：桌面 `[wire] event=blob-upload subject=af56a582 bytes=9069001 detail=chunks 173 blob 81f2441c` —— 上传事件**仅 1 条**、无重传、`grep -ci 'blob_open|blob_get_window|relay blob'` = **0**，信令侧 7 条 outbox 信封 ⇒ 估算 **1.00×**；**口径缺口（诚实标注）**：relay 侧累计字节计数本轮未采样（传输窗口内未轮询），严格 ≤1.05× 判定需第 4 次复跑专项采样补齐；⑥**先前的一条判断被真机推翻（留档）**：10:39–11:04 观察到 outbox `provider:efddc1af` 长期 `pending`、手机同步页显示「待确认 1」，当时定性为「`src/sync/engine/enrollment.rs:130-133` 的 `in_scope` 白名单不含 `provider:` ⇒ 该前缀永不 ack」。11:19 复核推翻：**8 条 `provider:*` 全部 `acknowledged`**（`efddc1af` attempts=5、`relay_id=…14`），因为该白名单只约束 enrollment 扫描路径，普通 pull 路径 `src/sync/engine.rs:706-709` 明确把 `provider:` 列入可回执前缀；期间的 pending 只是**手机密钥锁定**期间的暂态（手机不消费即不回执）。教训：把「某个路径不处理 X」外推成「X 永远不会被处理」是过度归纳，需验证所有处理路径|
| C4-5 | 门禁：`cargo test --all-targets` 全绿、relay `node --test` 全绿、`fvm flutter analyze` 0 error、改公开 API 后 `scripts/regen.sh` 桥 hash 一致 | `[x]` | **2026-10-27 09:35（C5-A-8 之后重跑，四道门禁全绿）**：`cargo test --all-targets` **965 passed / 0 failed / 20 ignored**（EXIT=0，+3 = 本轮三条新用例）；`cd relay && npm test` **49 tests / 49 pass / 0 fail / 0 skipped**（59 s，含 P2 32 s / P3 26 s）；`cd ui && fvm flutter analyze` **133 issues / 0 error**；`scripts/regen.sh` EXIT=0「生成的 Dart/Rust bridge 与 target/release/libelsewhen.so 已保持一致」。上一轮（2026-10-26 17:05，C5-A 收口时）的数字留档：`cargo test --all-targets` **962 passed / 0 failed / 20 ignored**（206.55 s，EXIT=0）；`cd relay && npm test` **49 tests / 49 pass / 0 fail / 0 skipped**（58 s，注意必须走 `npm test` —— 裸 `node --test` 会误跑需 URL 的 e2e 脚本）；`cd ui && fvm flutter analyze` **133 issues / 0 error**（全是既有 info/warning，含 `third_party/appflowy_editor`）；`scripts/regen.sh` EXIT=0「生成的 Dart/Rust bridge 与 target/release/libelsewhen.so 已保持一致」 |

**C4 验收**：C4-4 一次通过且可重复（连续两次全新配对都成功），才允许把本 roadmap 标为完成。

## C5 止损预案（触发即执行，不再追加调试轮次）

**触发条件**：2026-10-26 24:00 时 C4-4 仍未 `[x]`；或 C1 任一不变式被证明在现有协议形状下无法可靠实现。

两个预案都要求用户当场拍板选一个，选完**当日内**执行完，不留半吊子状态。

### C5-A（首选，保留功能）：mail 传输快照下线，只保留 blob 传输

- 依据：blob 路径（R4）已有窗口收敛、逐窗 ack、峰值与总量解耦、`close` 归零；mail 路径补不齐就是它不该存在。
- 动作：`SnapshotTransport` 只产出 `blob`；`/status.capabilities` 缺 `blob` 即拒绝初始同步并报 `sync_protocol_unsupported`；删除 `sync_snapshot_export_wires` 的 mail 分片路径与 `stage_piece` 的 mail 分支。
- 代价：首次配对要求对端支持 `/blob/*`（本仓两端同版本即满足）。

### C5-B（更彻底）：删除初始快照交接

- 动作：新成员只以「只读增量」加入，不做历史导入；删 `sync_snapshot_exports`/`sync_snapshot_export_wires`/`sync_snapshots`/`sync_snapshot_target`/`sync_snapshot_chunks`、`engine/snapshot.rs`、`storage/sync/snapshot/*`、UI `SyncSnapshotProgress`。
- 代价：新设备看不到历史数据，只能从加入时刻起同步。

| # | 步骤 | 状态 | 证据 |
|---|---|---|---|
| C5-0 | 触发判定并请用户二选一 | `[x]` | 2026-10-26 15:50 **提前触发**（不等 24:00）：用户质疑「同步设计过于复杂、低效」成立的部分被量化 —— mail 分片路径相对 blob 是 **0.38 MB/s vs 4.1 MB/s**（C0-1 45 MB/117 s vs R4 121.7 MB/29.5 s）、每 256 KiB 明文上线 **≈700 KB（2.67×）vs ≈350 KB（1.33×）**、且 mail 专属分支 33 处对 blob 25 处。**用户选择 C5-A**（保留功能、下线 mail 快照）。判定依据另有一条硬事实：`SnapshotTransport::from_env()` 默认 `mail` 且全仓无任何地方设 `SYNC_SNAPSHOT_TRANSPORT` → 生产一直在跑需要手工补三条不变式的旧路径，而 blob 早已完工 |
| C5-A-1 | **单一数据面**：删 `SnapshotTransport` 枚举与 `from_env`/`parse`/`as_str`；`begin_snapshot_export` 不再收 transport 参数（列固定写 `blob`）。relay 无 `/status.capabilities.blob` 或拿不到上传上下文 → `sync_protocol_unsupported`（不再「宁可慢也退回信件」） | `[x]` | 2026-10-26 16:40。枚举与三方法全删（`grep -rn "enum SnapshotTransport\|from_env\|SYNC_SNAPSHOT_TRANSPORT" src/` = 0），改 `delivery.rs:24 pub(crate) const SNAPSHOT_TRANSPORT: &str = "blob"`；`begin_snapshot_export` 收 `BeginExport{local_watermark,roster_revision,logical_clock}`、建行处显式写入（`delivery.rs:207`、`import.rs:82`），读时校验 `import.rs:389`（`transport != SNAPSHOT_TRANSPORT` → `sync_snapshot_transport_unsupported`）。能力探测：`runtime.rs:171 relay_supports_blob` 问本机信箱 `/status`、按 `base_url` 缓存、无 `blob` 即拒；`snapshot_upload`（`runtime.rs:32`）拿不到上下文 → `bail!("sync_protocol_unsupported")`；`blob_open`/`put` 的 404 单列 `RelayBlobUnavailable`（`blob.rs:526`）并在 `blob.rs:127` 转同一错误码。用例：`capability_tests.rs` 3 例（无能力→拒绝【用例名改成 `relay_without_blob_capability_is_refused`，旧名 `…_falls_back_to_mail` 已不成立】、有能力→走数据面、探测失败不缓存）+ `transport_isolation.rs:312` 断言 `sync_protocol_unsupported` |
| C5-A-2 | **删 mail 分片发送路径**：`stage_snapshot_inner` 的 `while next_encrypted_chunk → stage_piece` 循环、`stage_piece` 的 `finish=false` 分支、`stage_snapshot_export_payload` 的 `snapshot:{id}:{n}` 前缀分支；只剩 manifest（`snapshot-end:{id}`）与 authority（`membership:*`）进 outbox | `[x]` | 2026-10-26 16:40。`grep stage_piece src/` 只剩 `snapshot.rs:371` 一句历史注释；`while next_encrypted_chunk` 0 命中。分片改走 `upload_blob_chunks`（`snapshot.rs:204`）调 `BlobSink::put`，信令只剩 `announce_blob_snapshot`（`snapshot.rs:288`，`snapshot-start:{id}` + `{version,blob_id}`）与 `stage_manifest`（`snapshot.rs:373`）。运行期结果：快照在信箱里从 **176 封**降到 **2 条信令**（`loan_flow.rs:510` 断言「只有一条回执：manifest 那条」；relay 侧 `blob-import: 2 / blob-upload: 1` 的 wire 计数见 C5-A-7） |
| C5-A-3 | **删逐片回执与其对偶**：`confirm_snapshot_chunk`、`accept_snapshot` 的 chunk 分支、`envelope-receipt-` 里的 `chunk` 分支、`receipt_subjects` 的 `snapshot:{id}:` 白名单、`limit 8→32` 的两处抬高（回退到 8）、`due_enrollment_wires` 的 `LIKE 'snapshot:%'` 分支 | `[x]` | 2026-10-26 16:40。`confirm_snapshot_chunk` 0 命中；收端对 `snapshot:{id}:{n}` 分片信封回 `sync_snapshot_transport_unsupported`（`snapshot.rs:616`）；白名单只剩 `membership:`/`enrollment-`/`snapshot-start:`/`snapshot-end:`/`envelope-receipt-`（`enrollment.rs:130-141` + `wire.rs:254-262` 两处必须一致），`outgoing_scope` 显式 push `snapshot-end:{id}` 并 `truncate(8)`；每拍预算从 16 回落到 `limit=3`（`wire.rs:265`）。**一处偏离原计划（更严）**：`due_enrollment_wires` 的 ready 闸门收窄成只对 `snapshot-end:` 生效（`wire.rs:275-281`）—— 名单与开场信令被导出 ready 挡住会造成环形等待（见下方附加发现 ④） |
| C5-A-4 | **可测性接缝**（新增但净减）：`trait BlobSink{open,state,put}` / `trait BlobSource{get_window,ack,close}` 两个三方法接口；生产 `RelaySink`/`RelaySource`，测试 `MemBlob`（共享内存数据面）。理由：删掉 mail 路径后单测没有「无 relay 也能跑快照」的通道，否则只能靠 `#[ignore]` 冒烟用例，等于把窗口/ack/发布时序的 CI 覆盖一起删掉 | `[x]` | 2026-10-26 16:40。`blob.rs`：`BlobSink`/`BlobSource` 各 3 方法（`RelaySink:461`/`RelaySource:495`，含 `Arc` 透传实现，`BlobUpload{sink:Box<dyn BlobSink>}`、`BlobDownload{source:&dyn BlobSource}`）；`mem_blob.rs`（213 行）双向实现 + `truncate`/`restore`/`live_chunks`/`close_status` 三个诊断口（供「缺片不得发布」「relay 回收后不得再 ack」类用例造事故）；生产/测试分叉用 `runtime::DownloadPlane` + `#[cfg(test)] thread_local use_mem_blob_plane()`（`runtime.rs:86`），**生产二进制里不存在这两个符号** |
| C5-A-5 | **用例回收**：mail 分片专属用例删除（`chunk_triggered_publication_still_delivers_a_receipt_and_completes`、`snapshot_transport` 的 4 例中 2 例、`partial_export_is_not_uploadable…` 等按新语义改写），并在 `MemBlob` 上补「缺片不得发布 / 落盘后才 ack / 只有 snapshot-end 能退休导出」三条守卫（含变异验证） | `[x]` | 2026-10-26 16:50。回收：`snapshot_transport.rs` 从 6 例重写为 4 例（`+18/-100`，删除的两例是「分片信封续传」）；`engine/tests.rs` 的窗口流控用例改成 blob 语义（`+317/-105`，含 `partial_export_is_not_uploadable_and_resumes_after_restart`）；新增 `blob_snapshot_publishes_only_when_complete_and_completes_on_the_manifest_receipt`（缺片不发布 + manifest 回执才收口）、`snapshot_start_is_receipted_by_the_receiver_and_retired_by_the_sender`、`only_the_manifest_can_retire_an_export`、`snapshot_start_before_the_offer_stages_chunks_without_publishing`。**变异验证 3/3 被抓住**：①`import.rs::publish` 的 `count < manifest.chunks` 改成永不成立 → `loan_flow` 两条快照用例 FAILED；②去掉 `delivery.rs` 收尾的 `acknowledge_sync_wire` → `only_the_manifest_can_retire_an_export` FAILED；③删掉 `accept()` 的 `None if is_start => {}` 分支 → 新用例红在 `reference: expected a durable enrollment message`（`tests/mod.rs:282`），恢复后 8/8 绿。**诚实缺口**：「先落盘后 ack」目前是代码顺序 `snapshot.rs:474-479`（先 commit 再 `ack`）加 `MemBlob.ack` 即删的语义守着，没找到可行的失败注入点（本地写不会失败），未做变异验证 |
| C5-A-6 | **迁移 64**：删掉存量 `transport='mail'` 的导出/导入与其 wire（含 6 条悬空 `ready`，即原 C4-2 的动作）——它们在新代码里永不可达，留着就是永久悬空；接收端 `sync_snapshots.transport='mail'` 的 staging 一并清 | `[x]` | 2026-10-26 16:40。`migrations/64-drop-mail-snapshot/up.sql`（79 行，五段）：①`snapshot:{id}:{n}` 形状的信封与导出登记先子后父地删；②在途 mail 导出（`building/ready/aborted`）经临时表把信封 id 抄出来再删（外键顺序）；③接收侧 `sync_snapshot_target`→`sync_snapshot_chunks`→`sync_snapshots` 按依赖顺序清；④四个触发器把「忘了写 `transport`」变成写入即 `RAISE(ABORT,'sync_snapshot_transport_removed')`（SQLite 改不了列 DEFAULT，而这两张表被外键与八个 `sync_*_claim_*` 触发器引用，不能重建表）；⑤`applied`/`acknowledged` 的历史行**不动**。用例：`new_migrations_tests.rs:113` 断言目录名在表里、`:145` 断言 `user_version==64`（迁移目录数 = 版本号双向不变式） |
| C5-A-7 | 门禁 + 回填 | `[x]` | 2026-10-26 17:05。见 C4-5（同一份门禁数据）。净值 `git diff --stat` = **45 文件 +1967/−946**（其中 `src/` 生产代码 **+867/−477**、`docs/` +112/−1，余下是测试与两份 relay 故障代理），另新增 4 个未跟踪文件（`src/sync/mem_blob.rs` 213、`src/sync/runtime/enrollment/tests/scan_drops.rs` 381、`migrations/64-drop-mail-snapshot/up.sql` 79、本 roadmap）。**不改口径地说「净减」**：源码行数是净增的（新增主要是接缝、能力探测与「为什么这么写」的注释），真正减少的是**运行期形状** —— 快照在信箱里 176 封 → 2 条信令、每拍预算 16 → 3、分片回执的代码路径整条消失。relay 侧实测（P2 fault-proxy 用例）：**修前 220 s 跑不完 → 修后 16 s 绿**、`chunks=11/11 local_applied=true remote_applied=true` |
| C5-A-8 | **修快照被永久拒收**（真机复跑暴露的第六个缺陷，见附加发现 ⑥）：`publish_snapshot` 的 frontier 校验不能假设「序号从 1 连续保留」——发送端 GC 删掉已确认前缀后，行数永远小于序号。改为「行数 ≤ 序号，且快照给出的操作是一段**连续序号**、上界正好落在声明的 frontier 上」，全被压缩（`count==0`）放行；完整性仍由 `bytes/records/digest` 兜底 | `[x]` | 2026-10-27 09:20。**真机证据链**：①手机 logcat（`adb logcat -d \| grep flutter`）唯一一条交接日志就是 `[sync-pairing] enrollment: tick failed: AnyhowException(sync_snapshot_frontier_not_covered)`；②桌面 owner 库按**同一口径**自检 —— `sync_frontiers` 里 `be73b530…=241`、`0c5ae5d3…=18`，而 `SELECT COUNT(*),MIN,MAX FROM (sync_operations WHERE state IN ('applied','conflict') UNION ALL sync_checkpoint_operations)` 对 `be73b530` 只有 **64** 行（min=178 max=241），即**桌面导出的快照天生无法被任何接收端接受**；③于是 `snapshot-end:` 每次 accept 都 Err（进 `sync_wire_rejections`）→ 达上限后 drop（手机信箱 seq 1319 已不在，桌面 outbox 仍 `pending attempts=8`）→ manifest 从未落库 → `staged_snapshot_manifest` 恒 `None` → 片被拉完 ack 光后每拍 `window.from=None` → `break`，手机停在 174/174。**修法落点**：`src/storage/sync/snapshot/import.rs:516-534`（`contiguous` 判据）。**回归用例两条并做变异验证**：`garbage_collected_history_still_publishes_a_snapshot`（还原成 `count == *sequence` → FAILED，报 `sync_snapshot_frontier_not_covered`）；`a_frontier_beyond_the_transferred_operations_is_rejected`（把判据改成 `true` → FAILED）。定向跑：`cargo test --lib storage::sync::snapshot` 17 passed、`cargo test --lib storage::sync::snapshot::tests::lifecycle` 6 passed、`cargo test --lib sync::runtime::enrollment` 22 passed |
| C5-2 | 真机重跑配对（若为 A）或验证「只读增量加入」（若为 B） | `[x]` | **第 3 次复跑验收通过（2026-10-27 11:08）**；下面 ①~④ 是达成过程的事实链：①手机侧 `install.sh --no-build --relaunch` 覆盖安装成功且复核通过 —— 本地/设备 `libelsewhen.so` md5 均为 `1af2c1a36f5618abb89d19fae239ceae`、最大迁移版本两侧都是 `64-drop-mail-snapshot`、冷启动进程 30091 存活、界面正常（截图见对话）；②桌面 owner 侧进程加载的正是当前代码 —— `/proc/<pid>/maps` 指向 `target/release/libelsewhen.so`（inode 81733581 与磁盘一致 = 不是被替换掉的旧 inode），文件 mtime 08:40:38 晚于**全部** `src/`+`migrations/` 文件的 mtime（`find src migrations -newer` 命中 0），两端 `snapshot-start:` 字面量计数同为 2；③手机界面停在「等待主设备批准」（3/5），等待桌面点批准后才会进入快照交接；④**第 1 次交接已跑起来并停在 174/174**（根因见附加发现 ⑥，已修）。待填：装上新 APK 后的 relay 信箱实测（写入 ≤1.05×、完成后残留 0） | ⑤**第 3 次复跑（2026-10-27 11:08）验收通过**：配对完成（桌面 `af56a582 acknowledged`、手机「成员设备」）、数据完整落地、relay 残留 0（手机信箱 mail/blob/chunk 全 0，桌面信箱仅 1 封非本会话的 827 B 旧帧）、上传事件仅 1 条 `bytes=9069001 chunks=173` 且无重传无 blob 错误；relay 侧累计写入字节未采样，为该口径唯一缺口。

### C5-A 实施中的附加发现（同批次内全部修掉）

把 blob 变成唯一数据面之后，`relay && npm test` 的两个真门禁用例（P2 `enrollment-runtime`、P3 `network-runtime`）才真正跑到快照交接全过程 —— 它们把五个**会让真机永久转圈**的缺陷一次露出来。这五条不是测试脚手架的问题，是交接协议本身的互等：

| # | 缺陷 | 症状 / 证据 | 修法 |
|---|---|---|---|
| ① | **fault proxy 丢掉 `x-blob-*` 响应头** | joiner 侧 `fetch blob … from=0 -> from=None count=10` 重复 94 次：relay 回了 10 片，客户端却判「什么都没有」，每拍空转。窗口游标在 `x-blob-from/-count/-next` 头上（为省 base64 膨胀不走 JSON），而两份测试代理只转发 body | 两份代理（`test/enrollment-runtime.test.mjs`、`standalone/join-e2e.test.mjs`）改为复制全部响应头（排除 `content-length`/`content-encoding`/`transfer-encoding`）；**并加生产防御**：`blob_get_window` 有片无游标是协议违约，必`ensure!(chunks.is_empty() \|\| start >= 0, "relay blob window missing cursor")` —— 静默空窗等于把事故现场复刻一遍 |
| ② | **owner 每拍 `stage_offer` 早退** | `sync_enrollment_snapshot_not_ready` 每拍一次：`owner_offer` 要求 `enrollment_export_state ∈ {ready, acknowledged}`，而 blob 上传跨好几拍，此时还是 `building` | `flow.rs`：先查 `enrollment_export_state`，`ready\|acknowledged` 才 `stage_offer`，否则本拍照常消化已 ack 分片、下一拍再取 |
| ③ | **`snapshot-start:` 早于快照选取被挡** | 开场上形成死锁：offer 要 exports ready ← ready 要全部分片 ack ← ack 要接收端先拉 ← 拉要收到 `snapshot-start`。原逻辑「`record.snapshot is None` 一律 Waiting」 | `accept()` 改 `match &record.snapshot`：`Some` 校验身份；`None if is_start => {}`（放行，授权由 `begin_blob_snapshot_import` 自带的两道闸）；`None => Waiting`。新用例 `snapshot_start_before_the_offer_stages_chunks_without_publishing` + 变异验证 |
| ④ | **名单线被「导出 ready」挡死** | `due_enrollment_wires` 原来对整条快照线都要求 `ready`，于是 joiner 拿不到 roster → 选不了快照 → 不 ack → 导出永不 ready（与 ③ 同形） | 闸门收窄：`message_id NOT LIKE 'snapshot-end:%' OR …` —— 只有 manifest 等 ready，`membership:`/`snapshot-start:` 立即发 |
| ⑥ | **快照被 `sync_snapshot_frontier_not_covered` 永久拒收：GC 一跑，frontier 的校验口径就和库里的数据对不上**（2026-10-27 真机复跑暴露，P2/P3 都覆盖不到 —— 它们的发送端从没跑过 GC） | 手机 logcat 只有一条交接日志：`[sync-pairing] enrollment: tick failed: AnyhowException(sync_snapshot_frontier_not_covered)`。桌面 owner 库按同一口径自检：`sync_frontiers` 里 `be73b530…=241`，而 `sync_operations(applied\|conflict) ∪ sync_checkpoint_operations` 对这条流只有 **64** 行（min=178 max=241，`sync_gc_state.gc_floor=177`）—— 差值就是 `run_sync_gc` 删掉的已确认前缀。因此接收端的 `count == sequence` 断言**必然**失败：`snapshot-end:` 每次 accept 都 Err → 进 `sync_wire_rejections` → 达上限 drop（手机信箱里 seq 1319 已消失，桌面 outbox 仍 `pending attempts=8`）→ manifest 从未落库 → 片被 174 片拉完并 ack 光之后每拍 `get_window(from=174)` 都是 `from=None` → `break`，于是永久停在 174/174 | `import.rs` 的 frontier 校验改成「行数 ≤ 序号，且该流的操作是一段**连续序号**（`count == max-min+1`）、上界正好落在声明的 frontier；`count==0`（历史全部被压缩）放行」。快照完整性本来就由 `bytes/records/digest` 兜底，这里只需挡住「越界」和「留洞」。回归用例两条（`garbage_collected_history_still_publishes_a_snapshot`、`a_frontier_beyond_the_transferred_operations_is_rejected`），均已变异验证 |
| ⑤ | **面板一拍扣满 20 s 窗口** | `advance` 是一次 HTTP 调用，却把 `WINDOW_WAIT_MS=20_000` 扣在窗口上；当时 P2 跑 220 s 就是被这个拍长拖的 | `flow.rs::ENROLLMENT_WINDOW_WAIT_MS = 2_000`（交接期一拍最多等 2 s 分片 ack，传不完留 `building` 下拍续传）；另把 `upload_blob_chunks` 的 deadline 从「一窗一个」改成「`acked` 前进就续期」的空闲预算（否则一条稳态流被拆成一堆拍） |

**这一节的方法论**：六条里四条是**环形前置条件**（A 等 B、B 等 A），一条是**协议字段被中间层吃掉**（fault proxy 丢 `x-blob-*`），一条是**校验口径依赖了一个没人维护的前提**（⑥：`frontier == ops 行数` 只在「从没跑过 GC」的库上成立；`run_sync_gc` 是后来加的，它打破了前提却没有同步改校验）。前者是「谁都等谁来动」，⑥ 是「谁都动不了」—— 共同点是**局部看都对、整体永不前进**，而单测全绿，因为每个函数各司其职。真正把它们露出来的是把整条交接链路（含故障注入）当门禁跑：`cargo test --all-targets` 在没有 `ELSEWHEN_RELAY_TEST_URL` 时根本跑不到这条路。

**⑥ 的另一条教训（流程）**：真机卡住时我先按代码形状猜了一个「发布条件只挂在 ack 事件上」的原因，还照着猜的写了两条改动和一个用例 —— 结果用例构造不出来（`accept_snapshot` 在 manifest 到达时本来就会尝试发布），改动也就没有能力。**先取事实再改**：`adb logcat -d \| grep flutter` 一条命令就给出了确切错误码，之后二十分钟定案。临时猜测的那两处改动已全部撤回（`git diff` 里没有 `publish_if_complete`）。

---

## 验收指标（C4 完成后一次性复核）

| 指标 | 本次现场 | 目标 |
|---|---|---|
| 手机会话从邀请到「同步完成」 | 永不完成（停滞 > 115 min） | ≤ 5 min |
| 同一封 wire 的 `attempts` | 19（持续增长） | ≤ 3（除非对端真掉线） |
| 导出 `ready` 后无回执的最长停滞 | 无限 | ≤ 2 min（停滞推进兜底） |
| relay 上该导出的在途未确认片数峰值 | 174（无界） | ≤ `SNAPSHOT_MAIL_WINDOW` |
| `sync_relay_cursors.after_id=0` 僵死 | 有（F9） | 0 |
| 桌面端「174 项已完成」是否可复核 | 假象（acknowledged=0） | UI 与 `acknowledged` 一致 |
| 推送可观测性 | 成功零日志 | 每拍一行 `push-summary` |

---

## 进度记录

- 2026-10-26 建立。现场确证 F1~F9（见 §一），根因判断为「mail 传输缺三条不变式」（见 §二），不是数据问题。定硬时限当日收口，C5 为止损预案。
- 2026-10-27 09:20 **真机第 1 次复跑定案：快照被永久拒收，不是传输问题**。手机 logcat 只有一条交接日志 `tick failed: AnyhowException(sync_snapshot_frontier_not_covered)`；桌面 owner 库按同一口径自检才发现 **`sync_frontiers=241` 而库里只剩 64 条 ops**（GC 删掉 177 及以前）—— 也就是说**桌面导出的这份快照天生不可能被任何接收端接受**。链式后果：`snapshot-end:` 每次 accept 都 Err → 拒收计数到顶后 drop → manifest 从未落库 → 片拉完 ack 光后 `get_window` 恒 `from=None` → 手机 174/174 永久。修法是把 frontier 校验从「行数 == 序号」改成「行数 ≤ 序号 + 连续序号 + 上界等于 frontier」，两条回归用例均做变异验证。教训：**「邮箱里没有那封信」不等于「对端没收到」**——它可能被对端读走并 Err、再去拒收表里等到 drop；同理 **「relay 上没有分片」不等于「传输没完成」**，它可能已经全部 ack 干净。
- 2026-10-26 17:05 **C5-A 全部落地**（C5-A-1~7 全绿，C4-5 门禁同批复核）。核心结论：**协议形状选错时补丁补不齐** —— mail 分片路径缺的三条不变式（有界在途/进度反馈/停滞推进）在「几百封信 + 整份重发兜底」这个形状上无法补齐，而 blob 数据面（R4 已完工）天生具备。换成唯一数据面之后，两条并发收益：①快照在信箱里的封数从 **176 → 2**，每拍预算 16 → 3，分片回执整条代码路径消失；②原本被 mail 路径掩着的**四个环形前置条件**（导出 ready↔分片 ack、名单↔导出 ready、开场信令↔快照选取、窗口 deadline↔稳态流）全部现形并被修掉（见上方「附加发现」）。教训：**`cargo test --all-targets` 跑不到交接链路** —— 必须 `cd relay && npm test`；而 relay 集成用例里的「故障代理」本身就是协议的一部分（丢了一个响应头就等于改了协议）。剩余唯一待办：C4-4/C5-2 真机复跑（验收口径：写入 ≤1.05×、完成后残留 0）。
- 2026-10-27 11:20 **C4-4 / C5-2 真机验收通过，配对跑通**。第 3 次复跑（11:00–11:08）：桌面导出 `af56a582` 走到 `acknowledged`（`transport=blob`、`blob_index=173`），日志阶段链完整到 `complete`；手机从「等待成员授权与初始同步」变为「成员设备」（重启 app 后生效）；数据完整性以「手机是全新安装」为前提实测（知识库 8 条 + 两天的历史对话 + 3+ 条 AI 提供商配置）；relay 残留 0（手机信箱 mail/blob/chunk 全 0，桌面信箱仅 1 封非本会话的 827 B 旧帧）；上传事件仅 1 条 `bytes=9069001 chunks=173`、无重传、无 blob 错误 ⇒ 写入估算 1.00×。**遗留两条（已各自落档，均不阻塞）**：①relay 侧累计写入字节未采样 → 严格 ≤1.05× 判定待第 4 次复跑专项采样；②~~`provider:` 信令永不 ack~~（11:25 复核推翻：8 条 `provider:*` 全 `acknowledged`，「待确认 1」只是手机锁钥期暂态，见 C4-4 ⑥）。本轮新暴露的 D3（交接完成后手机 UI 不收敛，需重启 app）已转入 `2026-10-27-sync-lifecycle-boundaries-roadmap.md` C7。
