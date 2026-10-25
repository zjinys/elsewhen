# R4 客户端窗口化快照 —— 实施范围申报（2026-10-25）

所属：[relay v3 数据面重构 roadmap](../../roadmap/2026-10-24-relay-v3-dataplane-roadmap.md) 的 R4-1 ~ R4-6。
上位方案：[relay 存储放大与配额失效修复](2026-10-24-relay-storage-amplification-and-quota.md) §3.2 / §5 P2。

状态：**已落地**（R4-1 ~ R4-6 全部完成，验收数据见 §5；roadmap 已回填证据）。

---

## 1. 要解决的三个事实

1. **上传侧目前把整份快照塞进 wire outbox**：`snapshot.rs::stage_snapshot_inner`
   逐片 `stage_piece` → `stage_sync_wire_envelope`，174 片 = 174 行 outbox，
   每行是**完整密文**（本地 121 MB 放大）。发送侧随后把这些行当普通信件一封封推。
2. **接收侧只能"整份等到齐"**：`publish_snapshot` 要求 `sync_snapshot_chunks` 已有
   `manifest.chunks` 行才允许提交，提交后才整批 ack；中途断掉只能整份重来。
3. **relay 上峰值 = 整份**：126 片 × 1 MB 全等在 relay 上，等接收方整份 ack 才删。

## 2. 设计（与 R3 已落地端点的一处契约修订）

### 2.1 修订：`/blob/open` 不再声明 `chunks / bytes / digest`

**为什么必须改**：上传是流式的 —— 片数、总字节、整流摘要只有 `exporter.finish()`
才算得出来（`exporter.rs::next_encrypted_chunk` 边读边累加 `manifest.chunks/bytes`）。
R3 落地的 `open` 要求**先声明**这三项，流式上传无法满足；若先跑一遍 exporter 只为
计数，等于把 122 MB 完整读两遍。

**改法**：

- `open` 只收 `blob_id`（幂等：已存在就续期并回报 `have[]`），不再有 `409 metadata
  mismatch` 分支 —— 没有声明就没有「不匹配」。
- 配额改成**按实际落盘字节**逐片判（`SUM(LENGTH(ciphertext))`）：`put` 时
  `stored - 该片旧长度 + 新长度 <= quota`，否则 507。这样也不再需要「声明即预扣」，
  一个 open 了但没传片的 blob 占 0 字节。
- `state` 不再返回 `chunks / bytes / digest / missing`（总量未知），只返回
  `present[] / highest / stored_bytes / expires_at`。接收方的总量来自 **manifest**（端到端
  加密 + 签名，比 relay 的账可信）。
- 整流 `digest` 由 manifest 携带并在 `publish_snapshot` 里校验（本来就校验），
  relay 侧重复保存一份只是多一个可见字段，删掉更符合盲性。

**同一处修订：`state` 允许 opener sender 调用**（owner 或开户 sender，两者都验，
sender 必须等于 `relay_v3_blob.sender_mb`）。理由：上传方需要看接收方 ack 到哪了才能
维持窗口；`state` 只暴露**它自己那个 blob** 的分片索引，不含内容。不这么做，发送方
唯一的窗口信号就是 507，等于把流控退化成撞墙重试。

### 2.2 上传：分片级缓冲 + 每拍有界

- 发送方每片：`exporter.next_encrypted_chunk()` 得到的就是**待上传的 wire 密文**
  （`crypto::seal(chunk_header(..))`，与旧路径 `stage_sync_wire_envelope` 产出的字节同构，
  仅 nonce 不同，接收端校验用明文摘要，因此可安全重投）。
- 逐片 `PUT /blob/chunk`，**不写 outbox**（R4-3）。只有 `snapshot-end` manifest
  进 outbox —— 它才是信令：告诉对端"去把 blob 拉下来"。
- 每拍最多上传一个窗口（原计划 32 片 = 8 MB，**落地改为 10 片，原因见 §3.1**）**且在途未 ack 片数不超过窗口**：
  在途量用 `GET /blob/state` 的 `present[0]`（最小未 ack 索引）反推。窗口满就本拍收手，
  下一拍继续 —— 不阻塞、不等满整份。
- 进度持久化在 `sync_snapshot_exports.blob_index`（迁移 60）：续跑时
  `SnapshotExporter::skip_chunks(n)` 直接跳过已传片（不重封、不重传），从断点继续。
  重投同一片是幂等的（`(blob_id, idx)` 主键覆盖），即使 nonce 变了。
- 全部片传完后才 `stage_piece(manifest)` 把 manifest 写进 outbox（与旧路径同一份代码）。

### 2.3 接收：拉取式导入 + 逐窗 ack

- 收到 `snapshot-end:{id}` 且 manifest 声明 `transport = "blob"`：**只暂存 manifest，
  不 publish**，返回 `Ok(false)`（邮件留在 relay 不 ack，下圈还会扫到）。
- 每拍在 `pull_page` 之后跑一次 `pull_pending_blob_snapshots`：
  `GET /blob/state` → 对 `present` 的缺口按一窗（10 片）`GET /blob/chunk`（裸帧流）→
  每片落 `sync_snapshot_chunks`（复用 `stage_snapshot_chunk`，字节同构）→
  每窗 `POST /blob/ack {upto}` 立刻释放 relay 空间。
- 片齐后 `publish_snapshot` → `POST /blob/close {status:'done'}` → 排队
  `snapshot-end` 回执（复用现有 `queue_sync_wire_receipt`）→ 该封邮件下圈被 ack。
- 因此 relay 峰值 = 一个窗口（8 MB）+ 单片，而不是整份 122 MB。

### 2.4 传输方式开关（R4-6）

- `SYNC_SNAPSHOT_TRANSPORT=mail|blob`（默认 `mail`，灰度期保守），在
  `begin_snapshot_export` 时**落库**到 `sync_snapshot_exports.transport`。
- 已经 `building/ready/acknowledged` 的导出**不重新读取环境变量**：回执、续传、关闭
  全按行里的值走。切回 mail 的方式是"下一次快照"（或重新配对），不做运行时嗅探。
- 接收端也不嗅探：manifest 里带 `transport` + `blob_id`（可选字段，`serde(default)`），
  两者都由发送端签名覆盖，是唯一可信来源。

#### 2.4 后注：能力协商的实测判定点（D5，2026-10-25 回填）

F2 落地后，上面「旧 relay 自动退回 mail」这句话有确切的判定点，不是靠 env 猜：

| 判定点 | 位置 | 行为 |
|---|---|---|
| 解析 `/status` 的 `capabilities` | `MailboxStatus`，`#[serde(default)] capabilities: Vec<String>` | 字段缺失 ⇒ 空 ⇒ **默认无数据面**（默认「有」才是危险方向：老 relay 上强行上传就是每拍两次 404 + 退避，面板表现为「快照同步永远不完成」） |
| 问谁 | `runtime.rs::relay_supports_blob`，探**本机信箱**的 `/status` | 同一台 relay 实例能力等价，而发送方凭据本来无权读对端 `/status` |
| 缓存 | 同上，`blob_capability(base_url)` / `set_blob_capability(base_url, bool)` | 结论按 `base_url` 缓存（一台 relay 一个结论）；**探测失败不缓存**，一次抖动不该让本进程永久退回信件面 |
| 事后降级 | `transport_relay/blob.rs` 的 `blob_open` 404 ⇒ `RelayBlobUnavailable`，同时 `set_blob_capability(url,false)` | 有数据面的 relay 上「不存在的 blob 会被建成新的」，所以这个 404 是**结论性**的：该 relay 没有 blob 路由 |

实测证据（2026-10-25）：
- `src/sync/runtime/capability_tests.rs` 3 例 —— `relay_without_blob_capability_falls_back_to_mail`（`/status` 无 `capabilities` ⇒ 返回 `Ok(None)` 走信件面，且假 relay 只被探**一次**）、`relay_with_blob_capability_uses_the_dataplane`（`['blob','drop']` ⇒ 走数据面）、`failed_probe_is_not_cached`（探测抛错 ⇒ 不写缓存，下一次仍会重新探）。
- `transport_relay/blob.rs` 2 例 —— `blob_open` 404 单列成 `RelayBlobUnavailable`（5xx 不变）且把该 `base_url` 降级成无数据面。
- 真 relay 阶段六：发送端在有 blob 的 relay 上走数据面（`[blob] upload …`），随后同 id 续传并发布 —— 反证「协商通过 ⇒ 真的用上了」。

## 3. 逐项改动范围

| # | 文件 | 函数 / 行段 | 是否动协议 | 是否动表结构 | 测试 |
|---|---|---|---|---|---|
| R3 修订 | `relay/standalone/blob.mjs` | `open`（去声明）、`put`（逐片配额）、`state`（双签名者 + 精简字段）、`senderOperation` 不变 | 是（`/blob/open` 请求体、`/blob/state` 响应体） | 否 | 改 `blob.test.mjs` 相关断言 |
| R3 修订 | `relay/standalone/schema.mjs` | `relay_v3_blob` 去掉 `chunks/bytes/digest`，留 `blob_id/sender_mb/created_at/expires_at` | 否 | 是（新表，未上线可直接改） | `sweeper.test.mjs` 不涉及 |
| R3 修订 | `relay/standalone/server.mjs` | `statusPayload` 的 `blob_bytes` 已是实际字节，不变 | 否 | 否 | `blob.test.mjs` |
| R4-1 | `src/sync/transport_relay/blob.rs`（新增） | `BlobTransport`：`open/put/state/ack/close/get_window` | 否（客户端侧） | 否 | 单测 + 真 relay 契约测（`transport_relay_smoke_tests`） |
| R4-1 | `src/sync/transport_relay.rs` | `mod blob;` + `pub use blob::BlobTransport;` | 否 | 否 | — |
| R4-2 | `src/sync/engine/snapshot.rs` | `stage_snapshot_inner` 分叉出 `upload_blob_snapshot`；新增 `StageOutcome { Ready, InFlight }` | 否 | 否 | `src/sync/engine/tests.rs` + roundtrip |
| R4-3 | `src/storage/sync/delivery.rs` | `begin_snapshot_export` 落 `transport`；新增 `mark_snapshot_blob_progress` / `snapshot_blob_progress` | 否 | 是 | 迁移测试 |
| R4-4 | `src/storage/sync/snapshot/import.rs` | 新增 `stage_snapshot_manifest`（只暂存 manifest，不 publish）与 `blob_snapshot_pending` | 否 | 否 | import 测试 |
| R4-4 | `src/sync/engine.rs` | `accept_relay_payload` 的 `snapshot-end` 分支返回 `Ok(false)` 并暂存；`pull_page` 后调用 `pull_pending_blob_snapshots` | 否 | 否 | roundtrip |
| R4-5 | `src/sync/engine/enrollment.rs` | 回执从"整份 ack"改为 manifest 到达即回执（blob 路径） | 否 | 否 | roundtrip |
| R4-6 | `migrations/60-snapshot-transport/up.sql`（新增） | `sync_snapshot_exports` 加 `transport TEXT NOT NULL DEFAULT 'mail'`、`blob_index INTEGER NOT NULL DEFAULT 0` | 否 | 是 | `storage::new_migrations` 清单 + `assert_eq!(version, 60)` |
| R4-6 | `src/storage/sync/snapshot/records.rs` | `sync_snapshot_exports` 属 per-device-sync-state（已在排除表内，核对） | 否 | 否 | `frame_completeness` |

**协议版本不变**：仍是 v2，能力靠 `/status.capabilities` 的 `blob` 协商；
`SYNC_SNAPSHOT_TRANSPORT` 只决定发送端用哪条路，旧 relay 没有 `blob` 能力时
发送端自动退回 mail（并在 roadmap 证据里写明判定点）。

**不做**（沿用上位方案 §10）：不改 AEAD、不改帧格式、不改 pairing/roster/approval，
不引入对象存储，不做 P5 接收端增量应用。

## 3.1 落地时修订的三处（原计划按 32 片窗口写的，实测必须改）

1. **窗口按字节上限定，不按片数凭感觉**：单片 256 KiB 明文经 `next_encrypted_chunk`
   先 base64 进 JSON 再 `seal` 成信封后实测 **≈ 683 KiB**（两层 base64 膨胀 2.67×）。
   32 片 = **21 MiB** 直接撞穿 relay 的 `RESPONSE_BYTES = 16 MB`（`server.mjs:34`）：
   请求合法、响应超限，接收端永远拿不到完整一窗 → 窗口流控死锁（表现为
   `relay blob response too large`）。
   现在两侧都按 `WINDOW_CHUNKS * CHUNK_BYTES_MAX ≤ RESPONSE_BYTES` 定：
   relay `GET_CHUNKS_MAX = 12`（`blob.mjs:37`，12 × 1 MiB = 12 MiB），
   客户端 `WINDOW_CHUNKS = 10`（`transport_relay/blob.rs:18`，最坏 10 × 1 MiB = 10 MiB；
   真实片大 ≈ 6.8 MiB/窗）。**两侧必须同步**：客户端永远不许请求超过 relay 的上限。
2. **`window_wait_ms` 是调用方属性，不是进程配置**：等 ack 的预算属于「这一拍的调用方」，
   所以放 `BlobUpload.window_wait_ms` 字段（生产 `WINDOW_WAIT_MS = 20_000`，集成测试 300 ms）。
   曾经读过 `ELSEWHEN_BLOB_WINDOW_WAIT_MS` 环境变量 —— 那是把一拍的预算错当成进程配置，
   单线程集成测试每窗等满 20 s 会把一条用例拖成分钟级。
3. **`state` 的最高片号修 `COALESCE(LENGTH(...))` 恒 NULL 的 bug**：空 blob 上
   `SUM` 为 NULL，`?? 0` 兜底，否则 `stored_bytes` 与配额判断一起失真。

## 3.2 落地时修订：峰值采样点与「分片不进 outbox」的断言方式

- **峰值只能在 `push_due` 之后立刻采样**：接收端是「收一窗 ack 一窗」，relay 上的
  分片在 import 之后已经被删掉，读完 import 再读 relay 的账恒为 0（断言会假绿）。
- **「分片不进 outbox」按消息 id 前缀断言**（`!message_id.starts_with("snapshot:")`），
  而不是按条数：blob 路径下 outbox 里合法地还有 `snapshot-start:{id}` 与
  `snapshot-end:{id}` 两条信令，「条数 ≤ 1」既漏又错。
- **`snapshot-start:{id}` 幂等**：整份导出跨几十拍反复重开 exporter，outbox 里那条
  「分片在哪个 blob 上」永远只有一行（重复新增等于每拍刷一次信令）。

## 4. 验收方法

1. **真实规模回归**：174 片 / 122 MB 快照端到端（`--ignored` 的 roundtrip 用例 +
   本地 relay 实例），断言 relay 侧 `blob_bytes` 峰值 ≤ 窗口 + 单片，`close` 后为 0。
2. **客户端内存**：上传路径不再构造整份 `Vec<u8>`（代码层面只有单片 + 窗口），
   用 `sync_snapshot_exports.blob_index` 的推进与 outbox 行数（应为 1）证明。
3. **回退**：`SYNC_SNAPSHOT_TRANSPORT=mail` 时行为与 R3 之前完全一致（outbox 174 行、
   整份 ack），`roundtrip.rs` 既有断言不改仍绿。

## 5. 落地结果与证据（2026-10-25）

**R4-5 回执语义的最终口径**（原计划写「改为 blob closed」，落地时纠正）：
回执的对象仍然是 **`snapshot-end:{id}` 那封 manifest 信封**，不是 blob close 的回声 ——
分片走数据面，压根没有「逐片回执」这种东西。变的只是**时机**：
`publish_and_receipt` 只在「manifest 已到 + 最后一片已落盘 + `publish_snapshot` 成功」
之后才排队回执，紧接着 `blob_close`。这条顺序由 `receipt_subjects` 里硬编码的
`snapshot-end:{id}`（`enrollment.rs:127`）与 `accept_relay_payload` 的
`envelope-receipt-` 分支（`engine.rs:581` → `acknowledge_snapshot_export`）接住，
blob 与 mail 两条路共用同一段回执代码。

**证据**（`cargo test --lib ... end_to_end_send_fetch_ack_against_real_worker -- --ignored`，
本地 standalone relay，五阶段全绿）：

| 项 | 实测 |
|---|---|
| 快照规模（R4 验收口径） | **174 片**，密文合计 ≈ **121.7 MB**（174 × 699 KB），18 拍 |
| relay `blob_bytes` 峰值 | **6,995,630 字节 ≈ 6.67 MiB**（= 一窗 10 片），基线 0 |
| close 后残留 | `blob_bytes` = 0、`blobs` = 0 |
| 客户端 outbox | `snapshot-start:{id}` **1 条**（40 拍反复重开仍只有 1 条）、`snapshot-end:{id}` 1 条，分片密文 0 条 |
| 回执 | 接收端排队回执 **1 条**，subject = `snapshot-end:blob-checkpoint`，`acknowledging_device_id = target` |
| 导出状态 | 回执到达前 `ready`（不是 `acknowledged`），证明只有回执能收尾 |
| 缺口 | relay 上只有 20..29 片时 `import_blob_snapshots` 返回 `Ok(false)`、落盘 0 片、`pending` 保持、不 ack 任何片 |
| 续传/漂移 | `BeginExport::Resume{blob_index:3}` 读行；`SYNC_SNAPSHOT_TRANSPORT=mail` 切换与 `blob_clock` 漂移都拒（`sync_snapshot_export_requires_new_request`） |

规模回归的跑法（默认 9.5 MB / 73 片，日常不要放大）：
`ELSEWHEN_BLOB_SNAPSHOT_EVENTS=3 ELSEWHEN_BLOB_SNAPSHOT_UNITS=400000`；
单条记录上限 `MAX_RECORD_BYTES = 20 MB`，所以只能加条数不能把一条撑爆。
