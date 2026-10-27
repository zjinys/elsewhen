# 事故档案：enrollment 快照传输三连环死锁（扫描爬行 / 50MB 配额 / 补发回执 subject 白名单）

**日期**：2026-10-24
**对话**：本周 sync-pairing 主线（phone=PEUM00 ↔ owner=CachyOS 桌面，invitation 382bdc19）
**涉及模块**：
- `src/sync/runtime/enrollment/flow.rs`（`advance_proof` / `pull_transfer` 的扫描页大小）
- `src/sync/engine/enrollment.rs`（owner 侧 `envelope-receipt` 的 subject 校验）
- NAS relay `elsewhen-relay` 容器（`MAILBOX_QUOTA_BYTES` 环境变量）

---

## 一、现象与三层根因

用户视角：joiner 卡在「等待成员授权与初始同步」，进度条长期停在 173/174。桌面日志止于
`sending_snapshot`。逐层剥开后是**三个独立缺陷栈在一起**，每一层单独看都像"随机卡住"：

1. **扫描爬行缺陷**：`pull_transfer` 每次 driver tick 只取 3 封邮件
   （`fetch_page(..., 3)`）。桌面邮箱 `ddb17544` 里积压了历轮配对留下的 ~500 封
   永远不被 accept 的死邮件（旧轮 sender 的业务帧，`accept()` 一律回 `Waiting`、
   不 ack 不删），扫描要以 3 封/爬的方式逐封推进 → roster ack、offer ack 各拖
   ~10 分钟。
2. **50MB 信箱配额**：relay 默认 `MAILBOX_QUOTA_BYTES=50MB`，而本轮快照
   174 chunks × ~700KB ≈ 122MB。桌面 3 chunk/tick 灌信、手机 3 mail/tick 取信，
   占用冲到 50MB 上限后所有 send 507 失败；更糟的是失败重试会把 done 的
   `relay_id` 洗回 NULL（holes 反而变大），形成配额震荡。
3. **回执 subject 白名单缺陷（真死锁）**：接收端 `publish_snapshot` 在最后一片
   chunk 落地时才发布，回执 subject 取的是**触发发布的那封邮件 id**——可能是
   `snapshot:{id}:{idx}`（最后一片先于 mail 乱序时），不一定是
   `snapshot-end:{id}`。而 owner 的 accept 白名单只认
   roster/offer/snapshot-end/completion（`engine/enrollment.rs`），chunk subject
   的回执被一律拒绝 → 导出永远无法 acknowledged → outbox 里 176 条 wire 永远
   pending → **桌面永远重发（3/tick 无穷重试）→ relay 洪流冲到 248MB/512MB**。
   接收端 `queue_sync_wire_receipt` 的 UPSERT（pending=1 re-arm）保证重复邮件
   会持续重队回执，所以只要 owner 侧修好，死锁自动解开，无需改接收端。

## 二、修复

| # | 位置 | 改动 | 为什么最小 |
|---|------|------|-----------|
| 1 | `flow.rs` | 新增 `const SCAN_PAGE_LIMIT: usize = 30`，两处 `fetch_page` 使用 | relay 允许 1..=100、单页 16MB 钳制，30 安全；一页 30 封，~500 积压 20 tick 内翻完 |
| 2 | NAS 容器 env | `MAILBOX_QUOTA_BYTES=536870912`（512MB，`configured()` 的最大值） | 快照类传输的邮箱本质是"发送排队 + 接收消费"队列，50MB 对 ≥100MB 快照天然不够 |
| 3 | `engine/enrollment.rs` | 接受 `snapshot:{id}:{idx}` subject 的回执，按 `acknowledge_snapshot_export` 处理（导出 + 全部 chunk wire 一起 ack） | 回执语义不变（同一 manifest 的 hash），只是 subject 解析放宽；`acknowledge_snapshot_export` 本来就会把导出与 `sync_snapshot_export_wires` 全量置 acknowledged |

用户确认取值：`SCAN_PAGE_LIMIT` 由 100 改 30（"100 有点极端 30 吧"）。

## 三、验证

- `cargo test --release --lib`：918 passed / 0 failed。
- 观测闭环：relay 侧 mailbox `SUM(bytes)`、桌面 outbox holes（无 relay_id 的
  chunk 数）随每轮变化即可判断层 1/2/3 各在何处生效。

## 四、附带发现（未修，需另行立项）

- **OPPO Hans 冻结**：手机 app 切后台（比如去看抖音）会被 `OplusHansManager`
  冻结，取件与回执全停，进度"假死"在 173/174。切回前台即恢复。配对/快照传输
  期间手机必须保持前台，或考虑申请前台服务豁免。
- **失败重试洗掉 relay_id**：`record_sync_wire_attempt` 的 Err 分支把
  `relay_id` 覆写为 None，导致"已发送过"的证据丢失（本档案 holes 反常扩大
  的直接原因）。可改为 Err 时保留旧 relay_id。
- **relay 邮箱无 GC**：每轮配对产生新 mailbox 文件 + 死邮件只进不出，长期
  需要"owner 对非活跃 sender 的邮件批量 ack/drop"的整理命令。
- **重复发送风暴**：wire 在收到回执前每 tick 重发（3 条 700KB chunk/tick），
  ack 慢一拍就放大 10-100 倍流量。可考虑对已获 relay_id 的 wire 采用更长
  的补发退避。

## 五、教训

1. **"进度条不动" ≠ 传输链路坏**：三层缺陷叠加时，先用 DB + relay 两侧的
   数值（mails/bytes/holes/acked/cursor）做差分，把"通但慢"与"真死"分开，
   再读代码，否则会在错误一层停留。
2. **接收端回执 subject 不要依赖消息序**：发布触发邮件的 id 是环境相关的
   （乱序到达时是 chunk id），身份校验应按"它证明什么"而不是"它叫什么"。
3. **relay 配额与产品数据规模要联动**：信箱配额、TTL、单 envelope 上限应
   随快照大小设计检查，50MB 默认值在 100MB+ 数据集上是隐形地雷。

## 六、后续修复：初次交接仍是"一次 3 条"（2026-10-24 晚）

**症状**：配对界面每拍只前进 3 片（174 片要十几分钟），用户原话「一次还是同步 3 条记录」。

**根因**：发送预算是两条独立的 SQL。

- 日常增量：`src/sync/engine.rs:259` 已按「快照导出线在途 → 16 封/拍，否则 3 封/拍」放宽。
- 初次交接 / 重快照：走的是 **enrollment 路径** `src/sync/engine/enrollment.rs::push_due`
  → `storage/sync/wire.rs::due_enrollment_wires`，那里的 `LIMIT 3` 是硬编码，
  从未享受上面那条放宽规则。Dart 侧 tick 间隔 2s，于是发送端 1.5 封/秒，
  174 片光上传就要 ~2 分钟，叠加失败重发与回执往返，实测十几分钟。

**修复**：`due_enrollment_wires` 复用同一个判据 `has_pending_snapshot_export_wires(peer)`
（3 → 16 封/拍）；真正的上限仍是函数内原有的 8MB/拍字节预算（约 11 片 700KB chunk）。
只改上限，不动 scope 白名单与字节熔断，控制消息路径行为不变。

**验证**：`cargo test --release --lib` 918 passed / 0 failed（含 251 条 sync 用例）。

**注意**：该上限在**发送方**生效。本轮桌面端是 owner（发送 chunk），必须重启桌面进程
（`scripts/elsewhen.sh` 需要重新加载 `target/release/libelsewhen.so`）才会变快；
接收端（手机）不需要为此重装。

## 七、后续修复：两个 `--ignored` e2e 长期红 = 配对帧被当损坏信封 + completion 回执从未发出（2026-10-25）

这两项在 CI 里看不见（CI 只跑 `cargo test --all-targets`，不含 `--ignored`），
所以从 R0 起就一直红，直到 v3 dataplane roadmap 的 R1 才正面处理。

### 7.1 症状

```
→ P2 runtime failed: invalid sync envelope encoding
```

两个用例（`production_enrollment_recovers_grants_native_writes_and_receipts`、
`tick_moves_deltas_between_joined_devices_without_new_pairing`）在任何后端都
在第一次拉件时秒败（0.8s）。

### 7.2 第一层根因：配对帧与数据面信封共用信箱（生产 bug）

用 `ELSEWHEN_DEBUG` 门控的临时插桩 + 真实字节 dump 取证（**不靠读代码猜**）：

```
[dbg] envelope parse fail: expected value at line 1 column 1 head=87081461c6d097bc…
[dbg] refuse role=Joiner from=572c0946 peer=572c0946 local=dae52b5a len=2336 err=invalid sync envelope encoding
[pull] mail id=0000000000000001 from=572c0946 len=2336 subject=None accepted=false
```

`/tmp/envfail.bin`（1754 B）的 hexdump 全随机字节。结论：信箱里混着**两类载荷**：

| 载荷 | 生产者 | 形态 | 首字节 |
|---|---|---|---|
| 数据面信封 | `crypto::seal().to_bytes()` | 紧凑 JSON `{"header":…}` | `{` |
| 配对帧 | 配对阶段的 `dispatch.frame`（原始 AEAD 密文） | 随机字节 | 随机 |

配对阶段对端会往本机信箱回过 response/proof-ack；冷启动后这些残留帧还在信箱里，
`flow.rs::pull_transfer` 把它们当"损坏信封"冒泡（`first_error` → `return Err`），
joiner 每拍被打断，于是永远推进不了。

**修复**（`src/sync/runtime/enrollment/flow.rs`）：新增
`looks_like_dataplane_envelope()`（base64 解码后看首字节是否 `{`），在
`transfer::accept` 返回 `Err` 时先分类：

```rust
Err(_) if !looks_like_dataplane_envelope(&mail.ciphertext) => {
    // 外来帧（别的协议/别的轮次）：不是我的东西，不 accept、不 ack、不报错
    false
}
```

只有「解出来确实是 `{` 开头但解析/验签失败」才继续响亮报错（防篡改）。
不 ack 外来帧——ack 语义是"已应用"。分类放在 `Err` 分支里，happy path 零额外开销。

### 7.3 第二层根因：`stage_receipts` 把 completion 回执吞了（生产 bug）

修完 7.2 后两个用例不再秒败（跑满 7s），双方都推进到 `phase=complete`，但
测试循环的 `state.phase == "complete" && !state.notification_pending` 永不成立。
插桩取证（`view.rs` 两处派生）：

```
[np] owner   msg=enrollment-complete:80b2db14… state=Some(("pending", true))   ×1243
[np] joiner  msg=envelope-receipt-d0edded3… state=None                        ×1244
```

- **owner**：`enrollment-complete:` 行存在且已投 relay（`relay_id` 非空），但永不被 ack。
- **joiner**：`envelope-receipt-…` 行 **根本不存在** ⇒ 回执从未 stage。

链路本应是：owner 发 `enrollment-complete:` → joiner 收到后 `owner_confirmed=true`
→ joiner 下一拍 `stage_receipts` 发回执 → owner 收到回执 → `acknowledge_sync_wire`
→ owner 的 completion 行转 `acknowledged`。断在第二步：

```rust
if record.owner_confirmed {
    // 交接已完成：回执已经生效过，收尾就好。否则 rearm 会把它无限重发。
    for message in &subjects { store.mark_sync_wire_receipt_sent(…)?; }
    return Ok(());
}
```

早退的意图是"已完成的交接别再 rearm"。但 **completion 回执恰恰只能在
`owner_confirmed` 之后才存在**（它的 subject 就是 `enrollment-complete:` 这封信），
于是这一条永远走不过去。

**修复**（`src/sync/engine/enrollment.rs::stage_receipts`）：删掉整块早退，改成
逐条看送达状态——

```rust
if store.enrollment_wire_state(&peer, &wire_id)?.is_some_and(|(_, delivered)| delivered) {
    store.mark_sync_wire_receipt_sent(&peer, &message)?;   // 已送达：收尾
    continue;
}
// 未送达：继续 stage（stage_sync_wire_envelope 对同 hash 幂等，不会重封）
```

「已送达」用 `enrollment_wire_state` 的**第二列**（`relay_id IS NOT NULL`），
不是 `state == 'acknowledged'`：**joiner 的回执是链路终点，没有任何一方会再回执
它**，所以它的 outbox 行投递成功后永远停在 `pending`。同理 `view.rs` 的
`notification_pending` 与 `scope()` 的待办判据也按角色分开——owner 用
`acknowledged`（会被回执），joiner 用 `delivered`（送达即完成）。

### 7.4 验证

| 命令 | 结果 |
|---|---|
| `cargo test --lib …tick_moves_deltas_between_joined_devices_without_new_pairing -- --ignored --exact` | **ok**（7.42s） |
| `cd relay && node --test test/enrollment-runtime.test.mjs`（含丢响应 fault proxy） | **ok**（12.6s，`counts.grant=2 / revoke=1 / receipt≥4`） |
| `cd relay && node --test standalone/*.test.mjs test/*.test.mjs` | **27/27 pass**（修前 22 pass / 4 fail） |
| `cargo test --all-targets` | **917 passed / 0 failed / 19 ignored** |
| `fvm flutter analyze` | **0 error**（45 warning 全在本次未触碰的既有文件里） |

第二个用例必须走 `relay/test/enrollment-runtime.test.mjs` 这个入口才会绿：它
在 relay 前面架了一个 MITM，对每个 `grant`/`revoke`/`receipt` 身份**丢掉第一次
成功响应**（`counts.grant==2`、`revoke==1`、`receipt>=4`），Rust 侧的
`grant_failures >= 3` 就是在验证这个丢响应的恢复路径。直接对裸 relay 跑，响应
不丢，`grant_failures` 恒为 0，那个断言必然失败——**不是代码缺陷**。

### 7.5 教训

1. **信箱是"协议无关"的**：同一个 mailbox 上跑着两套协议（配对 Noise 帧 / 数据面
   信封）时，接收端必须能按内容辨认"这不是我的"，而不是把外来载荷一律当损坏。
   否则一个对端的残留帧就能永久打断本方每拍推进。
2. **`pending` / `acknowledged` 不是通用的"送达"**：确认链路的终点站没有下家，
   它的完成判据只能是"relay 收下了"（`relay_id`），不能是对端回执。
3. **`--ignored` 的用例等于不存在**：CI 只跑 `--all-targets`，红了一年也没人知道。
   这类用例要么进可跑入口，要么写清手动跑法与前置。

## 八、第三轮：三条不变式补齐与"看不见的推送"（2026-10-26）

前两轮修的是**具体那条边**（扫描爬行、回执白名单、配对帧误判）。第三轮承认
mail 快照路径缺的是 blob 路径（R4）已有的**三条不变式**：有界在途、进度反馈、
停滞推进。缺任何一条，任意一次回执丢失都没有收敛路径。逐条落地见
[配对快照交接收敛性 roadmap](../../roadmap/2026-10-26-enrollment-snapshot-convergence-roadmap.md) 的 C1/C2。

### 8.1 现场事实（可直接复核）

| # | 事实 | 证据 |
|---|---|---|
| F3 | 桌面每拍推 11 封、3~5 s 一拍，**成功推送零日志**（`Ok(id) =>` 分支只记 outbox，不记日志） | 当时 `src/sync/engine.rs` 的 `push_due` 只在 `failed > 0` 时打汇总行 |
| F8 | 导出只有收到最终应用回执才批量退休 176 条 wire，无逐片回执 | `acknowledge_snapshot_export` |
| F9 | 信箱残留一封 10-24 wipe 前的旧配对信，被重扫 **1239 次**，`sync_relay_cursors.after_id` 恒 0 | `agent-debug.log` 的 `pull-mail … accepted false` 1239 条 |
| F12 | 174 片一直躺在信箱里（不是被秒删）：接收端一片都没落盘，发送端每拍重推 → 新 id | relay 只读副本 `relay_v2_mail` |

### 8.2 教训：**"成功无日志"会把排障引向反方向**

本次排障前两小时全部花在错误假设上：「桌面没在推」「relay 秒删了分片」。
两条都来自日志的**空白**，而空白被当成了「没发生」。

- **只记失败等于把成功当成默认值**。`push_due` 里 `if report.failed > 0` 才打
  汇总行，看起来是"少打无用日志"，实际是把「在推但对方不要」与「根本没推」
  变成同一种日志形态 —— 而这两件事的修法完全相反（前者改接收端，后者改发送端）。
  现在两条推送路径（`engine::push_due` 与交接期的 `engine::enrollment::push_due`）
  都按 `push_summary_line_opt` 口径「成败都打、零尝试不打」，一行四字段
  （`event/subject/bytes/detail`），`bytes` 是**密文字节**不是封数。
- **副作用指标会被当成健康指标**。同一个"11 封/拍"在 F3 里是毒信循环的表象，
  但它其实是 8 MiB 字节预算切断的结果 —— 一个**设计常量**，与"推得好不好"
  无关。把「outbox 恰 2 条」「每拍 11 封」这类副作用当成健康读数，
  曾经让我们在 R4 之后误判过一次（见 relay-v3 roadmap 的 R7-P5 留档）。
- **静默跳过与静默丢弃都不算表态**。扫描循环里的 `else { false }`（本机信箱里
  既不路由、也不是数据面信封的邮件）让同一封信被重扫 1239 次，谁也不计数、
  谁也不删。现在三态显式（`Applied` / `Waiting` / `Rejected`）：拒收计数落库、
  够 3 次在 relay 上**显式 `drop`**（不是 `ack` —— ack 的语义是"已应用"），
  且游标必须**先推进再冒泡错误**。

## 九、第四轮：不是补不变式，是把那条路删掉（2026-10-26，C5-A）

第三轮承认了「mail 快照缺三条不变式」，第四轮的结论更直白：**那三条在这个形状上
补不齐**。于是按 `docs/roadmap/2026-10-26-enrollment-snapshot-convergence-roadmap.md`
的 C5-A 执行 —— mail 只留信令面，分片一律走 `/blob/*` 数据面。

### 9.1 为什么是删而不是补

| 不变式 | mail 分片路径 | blob 数据面（R4 已完工） |
|---|---|---|
| 有界在途 | 174 封全塞 `sync_wire_outbox` 等对端 ack，每拍 11 封只是 8 MiB 字节预算的副作用 | 一窗 ≤10 片（10 MiB ≤ relay 16 MB 响应上限），窗口外的东西根本不存在 |
| 进度反馈 | 无逐片回执，只能等最终应用回执一次性退休 176 条 wire | 逐窗 `x-blob-next` 游标 + `ack` 即删 |
| 停滞推进 | 无（导出 `ready` 之后没有任何推进路径） | 窗口空闲预算超时 → 下一拍从 `blob_index` 续传 |
| 实测吞吐 | 45 MB / 117 s ≈ **0.38 MB/s**，上线 **2.67×** | 121.7 MB / 29.5 s ≈ **4.1 MB/s**，上线 **1.33×** |

**教训**：不变式不是三条可以逐个补的 bug，而是协议形状的属性。「几百封信 + 整份
重发兜底」这个形状下，补了①就撞②、补了②还是没③。判定「该补还是该删」的标准是
**同一份需求在仓库里是否已有一条形状正确的路** —— 这里答案是 R4 的 blob，于是 mail
快照路径的正确处置是删除，而不是把 blob 的不变式一条条搬进信箱。

### 9.2 换成唯一数据面后才现形的四个环形前置条件

这四条都没有在旧路径上暴露过（旧路径靠「一次塞满 + 整份重发」掩盖了所有等待），
一删掉退路就全部现形。它们的共同形状是 **A 等 B、B 等 A**：局部每步都对，整体永不前进。

1. **导出 `ready` ↔ 分片 ack**：`owner_offer` 要求 `enrollment_export_state ∈
   {ready, acknowledged}`，而 blob 上传跨多拍、期间是 `building` → 每拍报
   `sync_enrollment_snapshot_not_ready`。修法：`ready|acknowledged` 才 `stage_offer`，
   否则本拍照常消化已 ack 的分片、下一拍再取。
2. **开场信令 ↔ 快照选取**：`accept()` 原逻辑「`record.snapshot is None` 一律
   `Waiting`」，于是「offer 要 exports ready ← ready 要全片 ack ← ack 要接收端先拉 ←
   拉要先收到 `snapshot-start`」成环。修法：`None if is_start => {}`（真正的发布仍
   由 offer 里的 manifest 摘要把守，未选中的快照永远进不了本机库）。
3. **名单 ↔ 导出 ready**：`due_enrollment_wires` 把整条快照线都压到 `ready` 才发，
   于是接收端拿不到 roster → 选不了快照 → 不 ack → 导出永不 ready。修法：只有
   manifest（`snapshot-end:`）等 `ready`，`membership:`/`snapshot-start:` 立即发。
4. **窗口 deadline ↔ 稳态流**：`WINDOW_WAIT_MS=20_000` 原来是「一窗一个 deadline」，
   把一条稳态流拆成一堆拍（P2 用例跑 220 s）。修法：`acked` 前进就续期（空闲预算），
   并把交接期的一拍上限降成 `ENROLLMENT_WINDOW_WAIT_MS=2_000` —— `advance` 是一次
   HTTP 调用，不能把请求扣在窗口上。

### 9.3 第三类教训：测试代理也是协议的一部分

P2/P3 跑通过程中最大的坑是 **fault proxy 丢掉了 `x-blob-*` 响应头**：blob 的窗口游标
为了省 base64 膨胀不走 JSON 而放在响应头上，代理只转发 body，于是接收端拿到「10 片
数据 + 没有游标」，判成「relay 上什么都没有」，每拍空转 —— **与真机事故现场一模一样的
外观**（`fetch blob … from=0 -> from=None` ×94）。

两处一起修：代理复制全部响应头（`enrollment-runtime.test.mjs` 与 `join-e2e.test.mjs`
两份都要改，否则只有一份用例绿）；客户端加一条**协议违约硬错** ——
`blob_get_window` 在「有片无游标」时必须 `ensure!(…)` 报错，绝不允许静默空窗。
「静默降级成一个能自圆其说的状态」正是 F9（同一封信重扫 1239 次）的根因形态：
**宁可响亮地失败，也不要安静地转圈**。

### 9.4 这一轮的量化结论

- 快照在信箱里：**176 封 → 2 条信令**（`snapshot-start:` / `snapshot-end:`）；每拍
  预算 16 → 3；逐片回执整条代码路径消失（`confirm_snapshot_chunk`、chunk 分支、
  `snapshot:{id}:` 白名单）。
- 迁移 64 把存量 `transport='mail'` 的在途导出/导入与其 wire 一次清掉，并用四个触发器
  把「忘了写 `transport`」变成写入即报错（SQLite 改不了列 DEFAULT，而这两张表被外键
  与八个 `sync_*_claim_*` 触发器引用，不能重建表）。
- 门禁：`cargo test --all-targets` **962 passed / 0 failed / 20 ignored**；`cd relay &&
  npm test` **49/49**；`cd ui && fvm flutter analyze` **0 error**；`scripts/regen.sh`
  桥 hash 一致。P2 fault-proxy 用例从 **220 s 跑不完 → 16 s 绿**。
