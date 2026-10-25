# relay 传输层重构（v3）：数据面与信令面分离

- 日期：2026-10-24
- 状态：proposed → **已实现**（2026-10-25，R0 ~ R6 全部落地 + 验收 6 项指标复核通过；进度与勾选唯一权威：[v3 数据面重构 roadmap](../../../roadmap/2026-10-24-relay-v3-dataplane-roadmap.md)）
- 性质：事故驱动的架构重构（协议 v3）
- 关联：[enrollment 快照传输死锁事故档案](../../incidents/2026-10-24-enrollment-snapshot-transfer-deadlock.md)

---

## 0. 一句话

**快照是一条有序字节流，却被塞进了"一封封彼此无身份关联的加密邮件"的管道。** 去重、续传、逐片确认、精确回收全都因为"邮件没有身份"而做不到，于是一切问题都靠"无限重试 + 永久保留"来补偿。根治办法不是给邮件加幂等键，而是**把数据面从信令面里拆出来**。

---

## 1. 范围收窄（已落地）

本轮同时把 relay 的功能面砍掉一半。**这些已经改完、测试全绿，不是提案**：

| 项 | 做了什么 |
|---|---|
| 应用自动升级 | 删除：`relay` 的 `/release/latest` / `/release/apk`、客户端 `check_app_update` / `download_update_apk`、`AppUpdateDto`、`ReleaseManifest`、设置页入口、`scripts/publish-release.sh` |
| Cloudflare 部署 | 删除：`relay/src/worker.ts`、`wrangler.toml` 的 R2 桶、`scripts/deploy-relay.sh`。**relay 只有 standalone 一个实现** |
| 双实现同构约束 | 随之取消：任何 relay 改动只写一遍 |

> 落地进度：relay 侧（§release 路由 / R2 桶 / Dockerfile / releases 挂载 / test helpers 换后端）已完成；客户端与 UI 侧的 `check_app_update` 等尚未删。**逐项勾选见 roadmap R0。**

连带代价（必须知道）：
- `relay/test/helpers.mjs` 原用 workerd 起服务，现已改为 spawn `relay/standalone/server.mjs`（`runtime.ready` / `dispose()` / `dispatchFetch` 形状保留）。`relay/test/crypto.test.mjs` 13/13 通过。
- 这次迁移**直接暴露了一个真 bug**：standalone 的 `base64Size` 比原 Worker 版少了三条校验（非字符串 / 空体 / 长度非 4 的倍数），后果是空 payload 被当成 0 字节邮件入库、非字符串在 `endsWith` 上抛 500、非法长度算出小数大小。已修（`server.mjs:78-86`）。**这是双实现漂移的真实代价，也是删掉一份实现的最好理由。**
- `relay/docker/Dockerfile` 目前只 `COPY standalone/server.mjs`；后续新增 `blob.mjs` / `schema.mjs` / `sweeper.mjs` 时必须补 COPY，否则容器启动即 `ERR_MODULE_NOT_FOUND`。

---

## 2. 根因链（每条都有本次实测数据）

### F1 数据面被塞进信令面（最根本）

| 事实 | 位置 |
|---|---|
| 快照 = 逐片 256KiB 的有序流 + 整流出 `digest` 的 manifest | `storage/sync/snapshot/exporter.rs:205`、`records.rs:220-234` |
| 每片被封成独立邮件，id = `snapshot:{id}:{idx}` | `sync/engine/snapshot.rs:128` → `stage_piece` → `stage_snapshot_export_payload`（`storage/sync/delivery.rs:160`） |
| 投递走通用信箱：一封一封 `/send`，一封一封 `/fetch` | `sync/engine.rs:247 push_due`、`engine.rs:371 pull_page` |
| 确认只能是"整份快照"级（manifest-subject 回执） | `engine.rs:553 acknowledge_snapshot_export`、`engine/enrollment.rs:113-140` |

⇒ 邮件**没有可用来去重的身份**（`seq_hint` 全是 0，见 P0-A），**没有"缺哪片"的信息**（只能整批重发），**没有逐片释放**（一份没 ack 就 174 片全留）。实测结局：174 片 → 801~812 封 → 533MB → 打满 512MiB 配额。

### F2 「未确认即永久保留 + 反复重扫」不是回收策略，是缺两个动词

- 接收方没有「我不会要这封信」的表达（无 `drop`/`reject` 永远重扫）：桌面端实测 `[pull]` 每页 30 封、**全部 `accepted=false`**、全是 507×`provider:` + 303×`pair-complete:` 死信，永远绕圈。
- 发送方没有「你收到第几片了」的能力 ⇒ 只能整批重发。
- **relay 的回收机制只有一个：删行**。`clean(now)`（`server.mjs:185`）只按 `expires_at` 删 chunks/mail/nonces；`server.mjs:446` 的 `setInterval` 每小时遍历一次——但只遍历**当时已打开的句柄**（`mailboxes` Map 里的），所以：设备退役后再无请求的信箱**永远不会被任何路径碰到**；删行也不缩文件（`d6157af7` 0 封邮件仍占 67MB）；WAL 从不 checkpoint。

### F3 没有任何容量不变式

系统从未定义"relay 上界应该是多少"。1.7GB 是被事故发现的，不是被设计排除的。实测单信箱可堆积到配额上限（`SUM(bytes)=536,870,692`，上限 `536,870,912`，差 220 字节）、邮件数可堆到 `9996 / MAX_MAILS=10_000`。

### F4 失败没有状态机
`507 / 413 / 409 / 超时` 全部塌缩成 `RelayHttpError(status)`（`synology`→`transport_relay/crypto.rs:28`），退避上限 60 秒（`storage/sync/wire.rs:278`）。任何新错误码都会变成新风暴——本次配额满后的狂发就是这么来的。

### F5 全量快照是唯一入网路径，且不可续（客户端自己也在放大）

- 发送方把**全部** 174 片密文先落进本地 `sync_wire_outbox` 再逐封推：实测 `pending 176 封/121MB` + `acknowledged 403 封/113MB` = 本地库也多背了 234MB。
- 中断后没有"从第 N 片继续"，只能整批重来。

---

## 3. 目标架构：两层

```
                 ┌──────────── 信令面（保留 v2 邮箱，语义收窄）────────────┐
  名册 · offer · manifest · approval · receipt · proof        ≤ 每封 100KB
                 └───────────────────────────────────────────┘
                 ┌──────────── 数据面（新增，v3）──────────────────────┐
  快照字节流：分片按 (blob_id, index) 幂等 · 接收方按需拉 · 逐片 ack 释放
                 └───────────────────────────────────────────┘
```

### 3.1 数据面端点契约（新增路由，鉴权复用现有机制）

| 端点 | 语义 | 关键性质 |
|---|---|---|
| `POST /blob/open` `{blob_id, chunks, bytes, digest}` | 开一次传输（owner 签名，`blob_id` 绑定 mailbox） | 幂等：重复 open 返回 `have` |
| `PUT /blob/chunk` `{blob_id, index, ...}` body=**原始密文**（非 base64） | 上传第 index 片 | **`PRIMARY KEY(blob_id,index)` ⇒ 结构性去重**，重投不占空间 |
| `GET /blob/state?blob_id` | 返回 `{chunks, highest, present, bytes, digest}` | 续传只需问一句就知道缺哪片 |
| `GET /blob/chunk?blob_id&from&limit` | 连续返回密文分片（**裸二进制流**，不 JSON 包装） | 省掉 base64 的 1.37× 膨胀 |
| `POST /blob/ack` `{blob_id, upto}` | 确认 `index ≤ upto` 已落盘 | **立刻释放**这些分片的空间 |
| `POST /blob/close` `{blob_id, status: done\|cancel}` | 结束，删除整个 blob | 精确回收，不靠 TTL |

存储：

```sql
relay_v3_blob(blob_id TEXT PRIMARY KEY, chunks INT, bytes INT, digest TEXT,
              created_at INT, expires_at INT)          -- 一个 blob 一行
relay_v3_blob_chunk(blob_id TEXT, index INT, ciphertext BLOB,
              PRIMARY KEY(blob_id, index))              -- 身份即 (blob_id, index)
```

**盲性不变**：relay 依旧只见 opaque 密文 + 长度 + 索引。
**内容格式不变**：快照仍是「长度前缀帧 → 256KiB 分片 → 每片 AEAD」，manifest 的 `digest` 仍是整流出校验。变的是**运输方式**。

### 3.2 边传边释放 ⇒ 峰值从 122MB 降到窗口大小

发送方维持一个窗口（设计原写 32 片 = 8MB，[R4 落地](2026-10-25-relay-v3-r4-windowed-snapshot-plan.md)实测改为 **10 片 ≈ 6.7 MiB**：32 片的密文 ≈ 21 MiB 会撞穿 relay 的 16 MB 响应上限）：推窗口内分片 → 接收方落盘并 AEAD 校验通过 → `ack upto` → relay 立即删 → 窗口前移。

于是：

| 指标 | 事故实测 | v3 目标 |
|---|---|---|
| relay 峰值占用（单信箱） | **690MB**（配额上限） | **≈ 8MB**（窗口） |
| 同一片的存储份数 | 4.4 份（每 attempt 一行） | **恒为 1**（主键约束） |
| 中断恢复成本 | 整份 174 片重来 | 只补缺口（`/blob/state`） |
| 传输完成后的残留 | 直到 TTL（7 天） | `close` 立即为 0 |
| 客户端本地放大 | 121MB pending | 窗口内 ≤ 8MB |

⚠ 前提：**逐片 ack 在语义上是安全的**——每片本身就是一个带 AEAD tag 的 `EncryptedEnvelope`，落盘 + `crypto::open` 成功即证明该片完整；manifest 的整流出 `digest` 作为最终兜底，不做"逐片正确性依赖"。

### 3.3 附加的两个动词（治 F2）

1. **`drop`**：`POST /drop {ids}`（owner 签名）——接收方对永远无法接受的信（`provider:` / 已结束邀请的 `pair-complete:`）主动放弃，relay 立即删除，并**落一条本地审计**（哪个 subject、为什么）。
2. **"必表态"契约**：同一封邮件被同一次扫描访问超过 N 次（默认 3 次）仍未接受，客户端**必须** `drop` 或升级为 `blocked` 并上报 UI，禁止无限重扫。这条规则直接消灭 816 封死信绕圈与 9996 封堆积。

### 3.4 容量不变式（治 F3）

定义为可测试的不等式，并在 `/status` 暴露三项分账：

```
relay_bytes = signal_bytes + Σ blob_bytes
约束1  signal_bytes ≤ SIGNAL_QUOTA(默认 16MB)         # 信令永不被大文件挤死
约束2  blob_bytes   ≤ BLOB_WINDOW_MAX(默认 64MB)      # 在途窗口之和
约束3  任一无进展 blob 在 TTL(默认 30 分钟)后自动回收  # 不是 7 天
```

配额从"一个信箱一个总账"变成"信令/数据两本账"，彻底消除"快照把控制消息挤死"的死锁形态（本次 photo 的直接成因之一）。

### 3.5 失败状态机（治 F4）

`deliver: idle → uploading → awaiting_receipt → backoff(reason, until) → blocked(reason) → done`

| 状态码/情况 | 归类 | 行为 |
|---|---|---|
| 507 配额 | `blocked(quota)` | 停止推送，**上 UI**，要求人工处理，不自动重试 |
| 413 过大 | `blocked(protocol)` | 同上（不该发生，属实现 bug） |
| 409 nonce/revision | 内部重放 | 重取 stamp 后单次重试 |
| 5xx / 网络 | `backoff(transport)` | 指数退避上限 10 分钟，成功即复位 |
| `digest` 不匹配 | `backoff(integrity)` | 丢弃该片重取，超 3 次 → `blocked` |

---

## 4. 为什么这才叫"根本"

| 缺陷 | 只做战术修补（附录 A） | v3 结构性消除 |
|---|---|---|
| F1 无身份不可去重 | 借 `seq_hint` 凑唯一键，4.4× → 1× | 主键 `(blob_id,index)`，**重复投递在物理上不占空间** |
| F1 无断点续传 | 仍整批重发 | `/blob/state` 给出缺口，只补缺口 |
| F1 无逐片释放 | 仍整份保留到 ack | `ack upto` 边传边释放，峰值 = 窗口 |
| F1 配额与信令共享 | 调大配额（继续互相挤） | 两本账，信令 16MB 独立保障 |
| F2 死信永久重扫 | 无解 | `drop` + 必表态契约 |
| F3 无上界 | 无解（只能事后看盘） | 三条可测试不变式 + `/status` 分账 |
| F4 失败无状态 | 只给 507 加长退避 | 全码位映射的状态机，`blocked` 必须上 UI |
| F5 客户端本地放大 | 无解 | 窗口化，本地也 ≤ 8MB |

**判据**：战术修补让"这次事故"不再复发；v3 让"这一**类**事故"（任何大对象走信令通道）在新代码里不可表达。

---

## 5. 实施阶段

### P0 止血（半天，先做，与 v3 无关，v3 上线后仍保留）

五项互不依赖的改动，目标是把"下一次"从 690MB 压到 130MB，即使 v3 还没做。
**P0-0 已完成**（见 §1）。

- **P0-A `seq_hint` 作投递幂等键**：relay `send()` 先删同 `(sender_mb, seq_hint)` 旧行再插新行（返回新 id，保持"重发在游标后可见"）；`sync_wire_outbox` 加 `seq_hint`（schema v59）并在 `ON CONFLICT` 时**保留**原值。
  - 现在 6 个生产 `send` 点全传 `0`（`engine.rs:274`、`engine/enrollment.rs:251`、`enrollment/flow.rs:496`、`invitations.rs:400`、`joining/network.rs:199,297`），`seq_hint` 接收端从未被消费（`transport_relay.rs:48`）⇒ 改语义零风险。
  - 最小版本只改 `engine.rs:270` 这一条（快照全走它）。
- **P0-B `507` 语义化**：`transport_relay/crypto.rs:28` 新增 `RelayQuotaExceeded`，`engine.rs::push_due` 命中即 `break` + 10 分钟退避 + `PushReport.quota_exhausted`，并经 `enrollment.error` 上配对面板。
- **P0-C `relay_id` 不再被覆盖成 NULL**：`storage/sync/wire.rs:282` 改为 `relay_id=COALESCE(?4, relay_id)`。**这条不修，relay 上已存下的邮件会因为客户端忘掉 id 而永久孤儿**——是 801 封堆量的直接来源之一。
- **P0-D relay 卫生**：`openMailbox` 句柄 LRU（`server.mjs:132`）+ `worker_threads` 里跑周期 sweep（`node:sqlite` 同步 API 不能阻塞事件循环）+ `freelist > 20%` 时 `VACUUM` + `wal_checkpoint(TRUNCATE)`。现有 `server.mjs:446` 那个 `setInterval` 不够：它只遍历**当时已打开**的句柄，退役设备再也不会有请求打开它，且删行不缩文件、WAL 不 checkpoint ⇒ 治"0 封邮件还占 67MB"。

### P1 数据面协议（relay 侧）

> relay **只保留 standalone（Node）单实现**，Cloudflare Worker 版已移除。因此本阶段每个改动只需写一次——原方案的"两版同构"约束取消。

- 新增 `relay/standalone/blob.mjs`：§3.1 六个端点、两张表、窗口/配额/lease 回收。**已落地（2026-10-25）**，见下方「P1 落地实测」。
- 新增 `relay/standalone/schema.mjs`：把 `openMailbox` 的 DDL 抽出供 server 与 sweeper 共用。**已落地（2026-10-25）**，同时给新库加 `auto_vacuum=INCREMENTAL`（空库才生效；老库上是"下次 VACUUM 后生效"，安全）。

**落地后的实测（2026-10-25，P0 全部完成）**

- 放大倍数是**按整串密文分组**量出来的，不是估算：`37306d0c` 801 封 / 534.8 MB / **175 个不同载荷 = 4.41×**；`c007ebf1` 9996 封 / 43.5 MB / **11 个不同载荷 = 909×**。客户端的"每拍重发同一条 outbox"就是这样把磁盘吃掉的。
- 同一形态重放（`relay/tools/replay-amplification.mjs`，同一批载荷规模跑两遍）：`seq=0` → 801 封 / 510.0 MB；固定 `seq_hint` → **175 封 / 115.5 MB**，**4.41× → 1.00×**。
- 碎片不是测试工件：归档里 `d6157af7` **0 封邮件占 66.6 MB**（freelist 17042 / page_count 17054），sweeper 后 **0.1 MB**；111 个信箱合计可回收 **109 MB**。
- 两条由此固定的实现约束：
  1. **幂等替换必须保住原 `id`**。`DELETE`+重插会把行挪到 id 末尾，而接收方按 `id>after` 翻页 ⇒ 每拍重发的机器会永远卡在最新一批的前几封（实测 40 拍只拉到 3 封、`snapshot-end` 永远到不了）。改成原地 `UPDATE` 后 `roundtrip.rs:341` 从 23.3s 稳定红变成 12.6s 稳定绿。
  2. **回收必须扫目录，不能扫句柄**。原 `setInterval` 只遍历 `mailboxes` 里已打开的句柄，退役设备再没人请求 ⇒ 结构上永远收不到；且 `node:sqlite` 是同步 API，`VACUUM` 一个 690 MB 的库必须放 `worker_threads`，否则所有请求被卡住。
**P1 落地实测（2026-10-25，relay 侧全部完成）**

- 六个端点 + 两张表已上线到 `relay/standalone/{blob,schema}.mjs`，`/blob/*` 在 `server.mjs` 里**第一个分叉**（`PUT /blob/chunk` 的 body 是裸二进制，不能过 JSON 解析）。7 个契约用例 + 32 个既有用例 ⇒ `node --test relay/standalone/*.test.mjs relay/test/*.test.mjs` **39/39 pass**。
- **签名域实际是 6 个，不是 4 个**：`blob-open` / `blob-put`（sender 签）、`blob-state` / `blob-get` / `blob-ack`（owner 签）、`blob-close`（owner 或 sender）。原方案漏了读侧两个域 —— 每个端点必须签自己的域，否则 `state` 的签名可以拿去重放 `get`。
- **上传侧用 sender 凭据，不是 owner**：原方案写"open 由 owner 签名"，但那等于要求上传方持有接收方主密钥（能读能删整个信箱），与盲中继的信任模型冲突。上传方在 v2 里本来就是接收方的 sender 身份，只有读/确认侧才该是 owner。
- **配额分账不是把信令压到 16 MB**：blob 独立成 `BLOB_QUOTA_BYTES`（默认 64 MB），配额按 `open` 声明的 `bytes` 预扣、`/status.blob_bytes` 报**已落盘**字节。这样既能让快照在结构上挤不掉信令，又不用在滚动升级中途把老客户端的 50 MB 信箱砍到 16 MB（那会让还在 mail 路径上的设备突然 507）。§3.4 的 8 MB 信令上限作为 P3 不变式随客户端切完一起收紧。
- `GET /blob/chunk` 返回 `[4 字节大端长度][密文]` 连续帧裸流（省 base64 的 1.37×），且只给「从第一片起连续」的那段；缺口由 `state.missing` 报区间，客户端补完再拉。
- blob 行记 `sender_mb`（开户人）：`put`/`close` 只认它，否则同信箱里另一个合法 sender 就能覆盖或撤回别人的传输（契约测试专门守这条）。
- `close` 幂等（TTL 已收掉的 blob 再 close 仍 200），`ack upto` 立即删片（契约测试断言 `blob_bytes` 从 10240 → 2048 → 0）。
- cutover 开关：old clients 不认识 blob ⇒ 走原 mail 路径；`/status` 增加 `capabilities:["blob"]`，老字段 `bytes`/`count`/`quota`/`earliest_expiry` 语义不变。
- **部署注意**：`relay/docker/Dockerfile` 必须显式 COPY `blob.mjs` / `schema.mjs` / `sweeper.mjs`（已补全），漏一个容器启动即 `ERR_MODULE_NOT_FOUND`。

### P2 快照切换（客户端）

- 发送端 `sync/engine/snapshot.rs:128` 的 `stage_piece` 循环改为**窗口化流式上传**：从 `SnapshotExporter` 直接取片 → `PUT /blob/chunk` → 等 `ack` → 前移；`sync_wire_outbox` 不再承载快照密文（只留 `snapshot-end` 的 manifest 信令）。
- 接收端 `storage/sync/snapshot/import.rs` 改为**主动拉取**：`/blob/state` → `GET /blob/chunk?from=` → `stage_snapshot_chunk` → `ack upto`；到齐后仍走原 `publish`（原子发布不变）。
- 旧 relay（无 blob 能力）自动回退 mail 路径 ⇒ 可以灰度：先发客户端，再升 relay，或反之。

### P3 `drop` + 必表态 + 容量不变式

- 客户端扫描器（`enrollment/flow.rs` 与 `engine::pull`）加"访问计数"与"必表态"。
- relay 加 `POST /drop`；`/status` 分账 `signal_bytes` / `blob_bytes` / `blobs`。
- 三条不变式写成自动化断言（见 §7）。

### P4 可观测与状态机

- 客户端 `deliver` 状态机（§3.5），`blocked` 状态必须出现在配对面板与设置页。
- relay 结构化日志：每次 blob 生命周期一行（open/ack/close + 字节数），便于下次复盘不用再翻 700MB 库。

### P5（可选，最彻底）接收端增量应用

分片到了就逐帧应用（帧是长度前缀的记录，天然可续），`frontier` 记录进度 ⇒ 接收端也不再需要在本地缓存整份快照（122MB → 窗口级）。代价是要把当前的"原子发布"改成"可续应用 + 完成时提交符号位"，风险最高，建议单独评估。

---

## 6. 改动范围一览

**P0**

| 文件 | 改动 |
|---|---|
| `relay/standalone/server.mjs` | `send()` 去重（`:289-325`）、句柄 LRU（`:132`）、startup+周期 sweep（替代现有只遍历已打开句柄的 `setInterval`）、`VACUUM`、`/status` 分账 |
| `relay/standalone/schema.mjs` | 新增：DDL 抽取（含 `relay_v2_mail_idem` 唯一索引） |
| `relay/standalone/sweeper.mjs` | 新增：`worker_threads` 回收器 |
| `src/storage/sync/migrations`（v59） | `sync_wire_outbox.seq_hint` + 回填 |
| `src/storage/sync/wire.rs` | `stage_sync_wire_envelope`（`:157`）分配并保留 `seq_hint`；`record_sync_wire_attempt`（`:268`）+ `relay_id` COALESCE（`:282`） |
| `src/storage/sync/delivery.rs` | `due_sync_wire_envelopes` 返回 `seq_hint` |
| `src/sync/transport_relay/crypto.rs` | `RelayQuotaExceeded`（`:28`） |
| `src/sync/engine.rs` | `push_due`（`:247`）507 中断/长退避/`quota_exhausted` |

**P1**

| 文件 | 改动 |
|---|---|
| `relay/standalone/blob.mjs` | 新增：六个端点 + `relay_v3_blob` / `relay_v3_blob_chunk` |
| `relay/standalone/server.mjs` | 路由注册、`capabilities`、两本账配额 |
| `relay/standalone/server.test.mjs`、`relay/test/*.mjs` | 契约测试 |

**P2**

| 文件 | 改动 |
|---|---|
| `src/sync/transport_relay/blob.rs` | 新增：上传/拉取/state/ack（reqwest 流式；可参照 `transfer.rs` 已有的 `/send` `/fetch` 写法） |
| `src/sync/engine/snapshot.rs` | `:128` 循环改窗口化流式上传 |
| `src/storage/sync/delivery.rs` | 快照密文不再进 `sync_wire_outbox`；`begin/stage/acknowledge_snapshot_export` 改为窗口水位 |
| `src/storage/sync/snapshot/import.rs` | 拉取式导入 + `ack upto` |
| `src/sync/engine/enrollment.rs` | 回执语义从"整份 ack"改为"blob closed" |

**P3/P4**

| 文件 | 改动 |
|---|---|
| `src/sync/runtime/enrollment/flow.rs`、`src/sync/engine.rs` | 访问计数 + 必表态 + `drop` |
| `src/sync/transport_relay/drop.rs` | 新增 |
| `src/sync/*/state.rs` | `deliver` 状态机 |
| `ui/lib/sync/*` | `blocked` 原因上 UI |

---

## 7. 测试与验收

### 7.1 契约测试（relay）

- `PUT` 同一 `(blob_id,index)` 两次 → 1 行、字节不翻倍、`bytes` 统计正确。
- `ack upto=10` 后 → `index ≤ 10` 全部消失、`blob_bytes` 立即下降。
- `close` 后 → 两张表零残留。
- 越权：非本信箱 sender 不能 `PUT`；非 owner 不能 `GET/ack/close`。
- `blob_id` 非法/超长/含路径分隔符 → 400（防目录穿越）。
- 分片超过 `MAX_ENVELOPE` → 413。
- 信令配额与 blob 配额互不侵占（开一个大 blob 后 `/send` 仍成功）。

### 7.2 三不变式的自动化断言（P3 起进 CI）

```text
1. signal_bytes ≤ SIGNAL_QUOTA
2. blob_bytes   ≤ BLOB_WINDOW_MAX + 单个在途分片
3. 无进展 blob 在 TTL 后 0 残留（用 10 秒 TTL 的测试实例验证）
```

### 7.3 事故回归（用本次真实规模）

复现：174 片 / ~122MB 快照，传输中途杀接收方，恢复。

| 指标 | 事故实测 | P0 后 | v3 后 |
|---|---|---|---|
| relay 峰值 | 690MB | ~130MB | **≈ 8MB** |
| 每片存储份数 | 4.4 | 1 | 1（主键保证） |
| 中断恢复 | 整份重来 | 整份重来 | 只补缺口 |
| 完成残留 | 7 天 | TTL | **0** |
| 客户端本地 | 121MB | 121MB | ≤ 8MB |
| 失败可解释性 | 面板转圈 | 面板显示配额原因 | 状态机 + `blocked` 原因 |
| 死信 | 无限重扫 | 无限重扫 | `drop` |

### 7.4 现有测试不得回归

`cargo test --all-targets`、`relay/standalone/*.test.mjs`、`relay/test/*.mjs`（真 Worker e2e）、`relay/test/rust-client.test.mjs`。

---

## 8. 迁移与兼容

- **协议版本仍写 2**，v3 能力通过 `/status` 的 `capabilities` 协商（不升级 `version` 字段，避免 426 全军覆没）。
- **双向兼容**：新 relay 保留全部 v2 路由；旧客户端不认 blob 就走 mail 路径；新客户端发现 `capabilities` 无 `blob` 自动回退。
- **数据迁移**：v3 表全新，不动 v2 表；v2 邮箱在 v3 稳定后可只留信令。
- **灰度顺序**：P0 上线（relay 重启）→ P1 上线（relay 重启，空表）→ P2 客户端发版 → 观察一次真实配对 → P3/P4。
- **回滚**：P0 全是加法与语义收敛；P2 有 env 开关 `SYNC_SNAPSHOT_TRANSPORT=mail|blob`，可一键回退。

---

## 9. 工作量与风险

| 阶段 | 量级 | 风险 |
|---|---|---|
| P0 | ~400 行 + 测试，半天~1 天 | 低（`seq_hint` 是死字段；其余纯加法） |
| P1 | ~500 行 + 测试，1~2 天 | 中（新端点 = 新攻击面，必须完整复用签名/nonce/越权检查） |
| P2 | ~600 行 + 测试，2~3 天 | 中高（快照是唯一入网路径，必须有 `mail` 回退开关） |
| P3 | ~200 行 | 中（"必表态"若判太急会丢正常迟到邮件，阈值要保守） |
| P4 | ~200 行 + UI | 低 |

**最大的风险不是写不出来，而是 P2 期间"两条路径都存在"的语义分裂。** 处置：
1. `SYNC_SNAPSHOT_TRANSPORT` 开关只允许在"开始一次新配对前"改变，配对于中途不可切换；
2. 记录 `sync_snapshot_exports.transport`（`mail|blob`）字段，回执/确认路径按记录里的值选，不做运行时嗅探。

---

## 10. 明确不做

- 不改快照内容格式（帧 + manifest + digest）、不改 AEAD、不改 pairing/roster/approval 流程。
- 不改 `fetch` 分页语义（v2 那条路径在 v3 之后只跑信令，本来就够用）。
- 不引入对象存储/外部依赖：relay 仍是单进程 + 每信箱一个 SQLite。
- P5（接收端增量应用）不进本轮。

---

## 附录 A：如果只做战术修补

即 §5 的 P0 四项（`seq_hint` 幂等 / 507 语义化 / `relay_id` COALESCE / relay 卫生）。它能：
把 690MB 压到 ~130MB、把"配额满后无限狂发"变成"10 分钟退避 + 面板提示"、把 0 封的 67MB 空壳缩回去。

它**不能**：消除整批重发、消除本地 121MB 放大、消除死信重扫、给出容量上界。
—— 所以它只是止血，不是答案。
