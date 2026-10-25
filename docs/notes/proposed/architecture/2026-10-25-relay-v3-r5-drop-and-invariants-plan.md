# R5 `drop` 动词 + 必表态 + 不变式进 CI —— 实施范围申报（2026-10-25）

所属：[relay v3 数据面重构 roadmap](../../roadmap/2026-10-24-relay-v3-dataplane-roadmap.md) 的 R5-1 ~ R5-3。
上位方案：[relay 存储放大与配额失效修复](2026-10-24-relay-storage-amplification-and-quota.md) §3.3 / §3.4。
前置：R0 ~ R4 已完成（信箱 + blob 两本账、窗口化快照、`/blob/*` 六端点）。

状态：实施中（本文先落盘，代码按 §3 顺序推进，每完成一项回填 roadmap 证据列）。

---

## 1. 要解决的两个事实

1. **死信永久重扫**：事故里 9996 封邮件堆到 `MAX_MAILS`，其中大部分是**永远无法接受**的
   （`provider:` 快照包已被取代、`pair-complete:` 对应邀请早已结束、身份对不上的信封）。
   接收端每圈都扫到、每圈都失败、每圈都不 ack —— 谁也不删，直到 7 天 TTL。
   relay 侧 `ack` 是唯一的删除入口，而 `ack` 的语义是「**已消费**」；
   客户端不肯把没接受的信说成已消费，于是没有删除动词可用。
2. **无终态的压力**：没有任何机制逼迫客户端对一封反复失败的信**表态**。
   「先放着，下一版再处理」是无限期重扫的温床。

## 2. 设计

### 2.1 `drop`（R5-1）：与 `ack` 分开的**放弃**动词

- relay：`POST /drop {ids: [...]}`，owner 签名，域 `drop [mailbox_key, ids.join(',')]`，
  校验与 `ack` 完全同形（1..100 个、正整数、不许重复），删除 `relay_v2_chunks` + `relay_v2_mail`，
  返回 `{ok, deleted}`。
- **为什么不是复用 `ack`**：`ack` 是「这封信我消费了」，`drop` 是「这封信我永远不会消费」。
  两者在 relay 上动作相同（删除），但**语义不同、审计不同**：本地要留「哪个 subject、为什么放弃」，
  否则下一次排查又变成猜测。合并成一个动词等于把「已处理」和「已丢弃」写成同一个词。
- blob 的终态**已经有了**：`/blob/close` 的 `status` 已是 `done | cancel`
  （`cancel` = 放弃这份快照传输），relay 两种都整块回收。不再新增 `dropped` 这个词 ——
  同一个意思写两遍（`cancel`/`dropped`）只会让日志出现两种写法，对排查毫无帮助。
- 客户端：`RelayClient::drop(mailbox, owner, ids) -> Result<usize>`。
- 本地审计：`sync_wire_rejections`（迁移 61，见 §2.2）里那一行就是审计 —— subject、
  尝试次数、最后错误码、放弃时间。不新造日志表。

**协议版本不变（仍是 2）**：`/drop` 走同一套 `stamp()` / `validateEnvelope` / nonce 检查，
老客户端不知道它；新客户端在 `/status.capabilities` 里能看到 `drop`（追加一项），
不知道 `drop` 能力的旧 relay 会 404，客户端退回「不 drop，只上报 blocked」。

### 2.2 必表态（R5-2）：N 次拒绝后必须表态

契约：**同一封邮件被同一次扫描拒绝超过 3 次，客户端必须 drop 或升级为 blocked 并上报 UI**，
禁止无限重扫。

- 计数落表（迁移 61）：`sync_wire_rejections(mail_id TEXT PRIMARY KEY, mailbox_id, peer_device_id,
  message_id, attempts INTEGER, last_error TEXT, dropped_at INTEGER)`。
  `mail_id` 是 relay 的邮件 id（`fetch` 返回的 `id`，游标语义），`message_id` 是信封头里的业务 id。
- 判定分两类，**只有「不可恢复」才计数**：
  - `Ok(false)`（依赖未到：roster 未应用、`snapshot-start` 早到）= `waiting`，**不计数**，
    它本来就会随时间自愈；
  - `Err`（身份不符 / 未知 subject / 摘要不符 / 版本不支持）= `rejected`，计数 +1。
    版本不支持也不立刻 drop（升级错位是可恢复的部署问题），但同样计数 —— 部署修好后
    重新接受即清零。
- 达到 `MAX_WIRE_REJECTIONS = 3` 时，`pull_page` 把该 `mail_id` 放进本圈的 `dropped` 列表，
  调 `client.drop`，并保留那一行审计（`dropped_at` 非空）。
- 接受成功（`Ok(true)`）或本圈不再出现时清零：`ack` 与 `drop` 都删邮箱里的信，
  行留着只为审计，`dropped_at IS NULL` 的行视为「仍在计数中」。
- `PullReport` 增 `dropped` / `blocked` 两个计数，供 UI 与结构化日志（R6）消费。

### 2.3 不变式进 CI（R5-3）

三条不等式（上位方案 §3.4）在 `relay/standalone/*.test.mjs` 里变成可执行断言：

1. `signal_bytes ≤ MAILBOX_QUOTA_BYTES`：信令账满即 507，且**不被 blob 占用挤死**
   （两本账独立：塞满 blob 后写信仍成功）。
2. `blob_bytes ≤ BLOB_QUOTA_BYTES`：按实际落盘字节逐片判，超限 507，
   且**已落盘的片不因超限而丢**（只拒新的那片）。
3. 无进展 blob 在 `BLOB_TTL_MS` 后被 `clean` 整块回收，`blob_bytes` 归零。
4. `/status` 三分账自洽：`bytes(信令) + blob_bytes(数据) = 该信箱实际占盘口径`，
   并且 `bytes`/`count`/`quota` 老字段语义不变。

## 3. 逐项改动范围

| # | 文件 | 函数 / 行段 | 协议 | 表结构 | 测试 |
|---|---|---|---|---|---|
| R5-1 | `relay/standalone/server.mjs` | `POST_ROUTES` 加 `/drop`；`Mailbox.drop(body)`；路由 `case '/drop'` | 是（**新增端点**，v2 不变） | 否 | `server.test.mjs` 新增 drop 契约用例 |
| R5-1 | `relay/standalone/blob.mjs` | 无改动（`close` 已有 `done\|cancel` 两个终态，`cancel` 就是「放弃」） | 否 | 否 | `blob.test.mjs` 已有覆盖 |
| R5-1 | `src/sync/transport_relay.rs` | 新增 `pub(crate) fn drop(&self, mailbox, credential, ids) -> Result<usize>` | 否（客户端） | 否 | 真 relay e2e |
| R5-1 | `src/sync/transport_relay_smoke/mod.rs` | 阶段一追加 drop 断言 | 否 | 否 | 同上 |
| R5-2 | `migrations/61-wire-rejections/up.sql`（新增） | `sync_wire_rejections` | 否 | **是** | `storage::new_migrations` 清单断言 61 |
| R5-2 | `src/storage/sync/wire.rs` | `record_wire_rejection` / `clear_wire_rejections` / `wire_rejections` | 否 | 否 | `storage::sync::tests` |
| R5-2 | `src/sync/engine.rs` | `pull_page`：`Err` 分支计数 + 达阈 drop；`PullReport` 加 `dropped`/`blocked` | 否 | 否 | 单测 + 真 relay e2e |
| R5-3 | `relay/standalone/server.test.mjs` / `blob.test.mjs` | 三条不变式断言 | 否 | 否 | 自身 |

**不做**：不改 AEAD、不改帧格式、不改 `/send` `/fetch` `/ack` 语义、不动 pairing/roster；
`drop` 不是「远程撤回发给别人的信」（那是发送方的事，本轮不做）。

## 4. 验收方法

1. `drop` 契约：owner 签名有效才删；非 owner 403；重复 id 400；越界 400；删后 `/status.count` 减少。
2. 必表态：同一封坏信被扫 4 次后 relay 上消失、本地留审计行、`PullReport.dropped` 计数正确；
   而 `waiting`（依赖未到）永远不计入、永远不会被 drop。
3. 三条不变式在 relay 测试里断言（见 §2.3），`node --test` 全绿。
4. 回归：`cargo test --all-targets`（R4 基线 924/0/19）与真 relay e2e 五阶段仍绿。
