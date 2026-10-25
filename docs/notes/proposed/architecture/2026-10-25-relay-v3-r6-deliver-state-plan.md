# R6 `deliver` 状态机 + 结构化日志 —— 实施范围申报（2026-10-25）

所属：[relay v3 roadmap](../../roadmap/2026-10-24-relay-v3-dataplane-roadmap.md) R6-1 ~ R6-3。
上位方案 [§3.5 失败状态机](2026-10-24-relay-storage-amplification-and-quota.md)（F4：失败无状态）。
前置：R5-2 已把「该放弃的死信」收口成 `dropped` / `blocked` 两个计数。

状态：已落地（2026-10-25）。门禁：`cargo test --all-targets` **932 passed / 0 failed / 19 ignored**；relay `node --test` **41/41**；`fvm flutter analyze` **0 error**；`ui/test/sync_deliver_status_test.dart` 4 例绿。

落地时的三处修订（都写进了代码注释）：

1. **退避判定要排除「已送出的最小重发间隔」**。成功路径原本与失败共用同一个公式，拆开后才发现：只看 `available_at_ms > now` 会把刚发出的信封显示成「正在重试」。因此 backoff 的判定加 `last_error <> ''` —— 假话正是状态机要消灭的东西。
2. **配额不停止重试**。设计稿写「blocked 不自动重试」，落地保留 R2-4 的 15 s 再试：配额由对端取件腾出，通常几秒内回落，停下等于永久停摆。`blocked` 只负责让人看见，不负责刹车。
3. **成功路径的间隔改名 `resend_spacing_ms`**。它与退避是两件事（一个是「别每拍重发同一封」，一个是「失败了等多久再试」），共用一个函数会让日志把重发记成重试。

---

## 1. 事实：今天的失败是「一串计数器」，不是一个状态

现在一次投递失败只能看到零散痕迹：`sync_wire_outbox.attempts` 涨了、
`available_at_ms` 往后挪了、`blocked_reason='relay_quota'` 有了、`relay_id` 空着。
**没有任何地方能回答「这份投递现在到底处于哪个阶段」** —— 于是 UI 只能显示
「同步中」这种既不真也不假的话，排查时要手工对着四列拼图。

## 2. 设计

### 2.1 状态是**推导**出来的，不是再存一份（R6-1）

```
idle → uploading → awaiting_receipt → done
                        ↓
              backoff(reason, until) → blocked(reason)
```

- `idle`：该对端没有 pending 行（无事可做）。
- `uploading`：有 pending 行且 `relay_id IS NULL`（还没上过 relay）。
- `awaiting_receipt`：pending 行已上 relay（`relay_id` 非空），等对端回执。
- `backoff(reason, until)`：`available_at_ms > now`（等重试时间到）。
- `blocked(reason)`：`blocked_reason` 非空（`relay_quota` / `protocol` / `integrity`）。
- `done`：回执落库后 pending 行被删（`acknowledge_sync_wire`），回到 `idle`。

**关键决定：不新增 state 列。** outbox 的 `attempts` / `available_at_ms` /
`relay_id` / `blocked_reason` 已经是真相，再存一个 `state` 列等于同一件事写两遍，
并且必然出现「列说是 backoff、时间说早该重试」的自相矛盾。状态机只是一个
读侧视图函数 `Store::deliver_states()`。

唯一需要新增的持久列是 **`last_error`**（迁移 62）：`backoff(reason)` 里的
reason 必须能回放，而现在的表里没有它。

### 2.2 错误码 → 状态映射表（R6-2）

| 失败 | 归类 | 行为 |
|---|---|---|
| 507 配额（`RelayQuotaExceeded`） | `blocked(relay_quota)` | **保留 R2-4 的决定**：15 s 后再试（`QUOTA_BACKOFF_MS`）。设计稿写的「不自动重试」会让配额一下降就永远停摆，R2 已实测 15 s 恢复是最优折中。同时上 UI。 |
| 413 过大 | `blocked(protocol)` | 不该发生（实现 bug 或两端上限不同步），上 UI，不重试。 |
| 409 nonce/revision | 内部重放 | 重取 stamp 单次重试，不进入状态机。 |
| 5xx / 网络 / 超时 | `backoff(transport)` | 指数退避，**上限 10 分钟**（现在是 60 s 封顶，抖动大时信令太吵）。 |
| 摘要 / AEAD 不符 | `backoff(integrity)` | 丢弃该片重取；累计 3 次 → `blocked(integrity)`。 |

分类函数只有一个入口 `classify_delivery_failure(&anyhow::Error)`，靠错误链
文本匹配（本仓库错误是 stringly typed，没有 downcast 层），返回值是枚举
`DeliveryFailure::{Quota, Protocol, Replay, Transport, Integrity}`。

### 2.3 结构化日志（R6-3）

一条线：`ELSEWHEN_DEBUG` 门控的 `wire_log(event, subject, bytes, state)`：
输出 `[wire] event=push subject=snapshot-end:9f2c bytes=1188 state=awaiting_receipt`。
**只输出 subject 与长度，永不输出载荷**（密文长度也是敏感度较低的元数据，
但明文/密钥/内容一概不出现）。替换现在那句没有 subject 的
`eprintln!("[sync-wire] push failed: ...")`。

### 2.4 UI：blocked 必须被看见

- 新 API `sync_delivery_status() -> Vec<SyncDeliveryStatusDto{ peer, state, reason,
  until_ms, pending, awaiting_receipt }>`（每对端一行）。
- 配对面板（owner / joiner 两块）与设置页显示：`blocked` 用警示色 + 具体原因
  （配额 / 协议 / 完整性），其余状态用中性文案。
- 需要 `scripts/regen.sh` 重生成桥接；Dart 侧只读新 DTO，不动既有字段。

## 3. 逐项改动范围

| # | 文件 | 函数 / 行段 | 协议 | 表结构 | 测试 |
|---|---|---|---|---|---|
| R6-1 | `migrations/62-wire-last-error/up.sql`（新增） | `sync_wire_outbox.last_error` | 否 | **是** | 迁移清单 |
| R6-1 | `src/storage/sync/wire.rs` | `deliver_states(peer)` / `record_delivery_failure(...last_error...)` | 否 | 否 | `storage::sync::tests::deliver_state`（新增） |
| R6-1 | `src/sync/engine/deliver.rs`（新增） | `DeliverState` / `DeliveryFailure` / `classify_delivery_failure` | 否 | 否 | 同上 |
| R6-2 | `src/sync/engine.rs::push_due`、`engine/enrollment.rs` | 用分类替换裸 `record_sync_wire_attempt`；退避上限 10 分钟 | 否 | 否 | 既有 e2e |
| R6-3 | `src/sync/engine.rs`、`engine/snapshot.rs` | `wire_log` 结构化行 | 否 | 否 | 手工（日志非断言对象） |
| R6-2 | `src/api/sync/join.rs`、`src/frb_generated.rs`、`ui/lib/bridge/**`、`ui/lib/widgets/*pairing*`、`ui/lib/screens/settings_*` | 新 DTO + 面板显示 | 否 | 否 | `fvm flutter analyze` + widget 测试 |

**不做**：不新增 UI 操作按钮（不准手动「强制重传」——重传靠时间与回执自愈）；
不改 outbox 既有列语义；不改协议版本。

## 4. 验收

1. 状态推导有单测：一个对端从 空 → pending → 已上 relay → backoff → blocked → 回执落库 全链路的状态序列逐条断言。
2. `classify_delivery_failure` 五类错误各一条断言，5xx 退避序列单调且有 10 分钟上限。
3. 结构化日志在 `ELSEWHEN_DEBUG` 关闭时无输出、开启时含 subject 与长度且**不含**载荷。
4. `cargo test --all-targets` 与 relay `npm test` 全绿；`fvm flutter analyze` 零 error。
