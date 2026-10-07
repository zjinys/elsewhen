# Elsewhen 盲中继

> **当前：relay v2 的权限、收发与真实 Worker 测试已实现；完整 App 同步仍未完成。**
> Noise 配对/本机密钥库、owner 签名成员名册、快照/大增量投递、分析结果及回复原输入关联已有内部测试；完整 App 配对/解锁/方向通道及生命周期仍未完成。不要把部署成功当成可用的多设备同步，也不要手工把个人数据或真实 Provider Key 传给底层 `send`。
>
> [同步设计](../docs/notes/proposed/architecture/2026-09-30-multi-device-sync.md) · [实施路线](../docs/roadmap/2026-10-03-multi-device-sync-roadmap.md)

## 1. 职责与交付边界

中继运行在用户自己的 Cloudflare 账户，使用 Worker + SQLite-backed Durable Objects 为每个邮箱保存短期信封。它只负责传输，不负责数据库合并、业务授权或加工设备仲裁。

| 组件 | 当前状态 |
|---|---|
| relay v2 邮箱 | owner 绑定创建、owner 授权 sender、续期/撤销、owner 轮换、收发/分页/配额已实现 |
| Rust `RelayClient` | 已与实际 Worker 互通；严格 URL/响应上限，禁用重定向 |
| 端到端信封 | `src/sync/crypto.rs` 已实现 ChaCha20-Poly1305 + Ed25519，1 MiB 明文分块上限；与 relay HMAC 分离 |
| 双库复制/Provider | 已有内部原子复制与加密 Provider 应用测试；尚无完整配对/联网/App 流程 |
| 快照/增量调度 | 内部一致分块快照、大记录增量、密文暂存/重试、应用回执与有界取件已通过实际 Worker 多库测试；TTL/GC 与 App 调度未完成 |
| 配对/密钥持久化 | Noise 原语、本机加密 vault 和 Flutter 原生主密钥适配有隔离测试；完整配对/解锁协调及原生平台签收未完成 |
| 数据集成员授权 | v22 固定 owner 的签名完整名册，Noise 审批绑定版本锚点；入组、撤销和公钥校验不依赖 relay 自签凭证，旧快照不改写权限 |
| presence/peers | v2 明确返回 410，不提供错误的“对端离线”结论 |
| P2P、同步文件、App 同步设置 | 未实现，后续按路线推进 |

Provider 完整配置（含 API Key、激活项和参数）属于首版目标；设备身份/配对私钥不复制。撤销同步资格不能回收该设备已经持有的 Provider Key，设备丢失时还需在 Provider 侧吊销或轮换。

### 两层安全

- relay HMAC 只验证传输访问。中继知道这些密钥，因此也能生成对应认证码；它不是端到端来源证明。
- 内容离开设备前由独立的认证加密模块封装，可信公钥和内容密钥来自配对层，不能从收到的信封自行建立信任。
- `send` 仍接收调用方提供的 base64 字符串；base64 不是加密，参数名 `ciphertext` 不保证内容安全。
- 中继可见连接 IP、邮箱关联、流量、时间和 `seq_hint`，也能丢包/阻断。自托管不代表匿名或没有基础设施提供者。

## 2. 本地测试

使用项目锁定的 Wrangler/Miniflare/esbuild 依赖。当前测试适配已安装的 Miniflare 5（通过官方 `convertV4MiniflareOptions` 适配器），禁用测试 telemetry。

```bash
cd relay
npm ci
npm test
```

`npm test` 启动隔离本地 workerd，执行真实 `worker.ts`，并由随机 loopback 端口驱动 Rust 互通测试。需要 Rust 工具链和当前项目依赖。不会访问个人数据库、线上中继或真实模型。

- `test/crypto.test.mjs`：13 项实际 Worker 用例，覆盖 mailbox 身份、owner/sender 越权、跨邮箱隔离、TTL、排他分页、撤销/轮换、并发 nonce/计账、载荷边界、持久化重启、授权/撤销响应丢失的精确重试和禁用发现。
- `test/rust-client.test.mjs`：启动本地 Worker 后运行明确指定的 Rust ignored 测试，验证双方签名及实际收发，包含密钥库落盘/Noise/签名成员激活、先到内容不越权、控制与领域回执，以及成员/relay 权限分别撤销。普通 `cargo test` 不再静默探测 `localhost:8787`。
- `src/sync/crypto.rs`：独立的认证加密测试，不等于配对或业务同步已验收。

2026-10-03 本地 `npm test`：**13 passed**。首轮因 Miniflare 5 构造参数不兼容而失败，已按安装包导出的适配器修复并重跑；不是用镜像邮箱逻辑替代失败的实际运行时测试。

显式开发服务：

```bash
npm run dev
# 另一个终端，仅对隔离测试实例使用
node test/e2e-smoke.mjs http://127.0.0.1:8787
```

该 smoke 只用合成占位载荷，不测试端到端内容加密或两个业务库。邮件清理后仍可能留下凭证记录，不声称完全无残留。对线上测试实例执行需要独立授权；不得自动传输个人数据或 Key。

## 3. 部署参考

部署动作与代码实现分开授权。本轮没有部署服务。

```bash
cd relay
# 浏览器认证，或使用本目录被 gitignore 的 .env
npx wrangler login
# 也可 cp .env.example .env，填写 CLOUDFLARE_API_TOKEN / CLOUDFLARE_ACCOUNT_ID
npm run deploy
```

不要把真实凭证写进 `wrangler.toml`、测试输出或提交记录。shell 环境变量优先于 `.env`。费用/额度以部署时账户和平台规则为准，不承诺固定免费成本。

`wrangler.toml` 的 `Mailbox` 仍是 SQLite-backed DO 类；v2 构造函数建立独立 `relay_v2_*` 表。旧 v1 KV 数据与凭证不被当成 v2 授权，也不会被此变更静默删除。v1 客户端需升级，不能沿用自签注册语义。

| 变量 | 当前默认值 | 服务端约束 |
|---|---|---|
| `MAIL_TTL_MS` | 7 天 | 配置上限 30 天 |
| `MAILBOX_QUOTA_BYTES` | 50 MiB | 配置上限 512 MiB |
| `MAX_ENVELOPE_BYTES` | 8 MiB | 硬上限 8 MiB |

请求读取前/读取中均限量；大邮件按 64 KiB 文本片存入 SQL，不放进超大的单个 KV 值。取件响应有 16 MiB 预算。邮件/nonce 清理在已授权事务中触发，没有请求时不承诺 TTL 时刻立即物理删除；邮箱不是备份。

## 4. relay v2 协议

请求显式带 `version: 2`；旧版本返回 426。POST 使用 JSON。邮箱标识是 64 位小写 hex，按名称映射到独立 DO。

认证公共字段：`ts`（毫秒，±5 分钟）、`nonce`（32 位 hex）、`sig`（64 位 hex）。签名为 HMAC-SHA256，消息按 `|` 拼接。`nonce` 在 10 分钟窗口内不可重用；与操作一同在 SQLite 事务里认领。计数和参数使用安全整数，不接受溢出、负数或非法游标。

### 邮箱 owner 与 sender 分离

1. 每台设备独立生成 owner credential，初始邮箱地址为 `HMAC(owner_cred, "relay-mailbox")`。
2. `/create` 验证此绑定和 owner 签名，不能用自己的密钥抢注另一个已知邮箱。
3. `/register` 必须由邮箱 owner 授权，sender credential 仅能向该邮箱投递；它不能收件、删除、查状态或授权他人。
4. sender 授权携带版本；续期、轮换、撤销都检查 `expected_revision`。撤销保留版本，不让旧注册请求恢复权限。
5. owner 轮换后邮箱地址不变，旧 credential 立即失去该邮箱管理权限；本机须保留地址而不是按新 Key 重算。

| 接口 | 权限 | 业务签名字段（公共尾部 `ts, nonce` 之前） |
|---|---|---|
| `GET /health` | 无 | 无，返回版本 2 |
| `POST /create` | 新 owner 身份证明 | `relay-v2-create, mailbox_key` |
| `POST /register` | owner | `relay-v2-register, mailbox_key, sender_mb, cred_hex, expires_at, expected_revision` |
| `POST /revoke` | owner | `relay-v2-revoke, mailbox_key, sender_mb, expected_revision` |
| `POST /rotate` | owner | `relay-v2-rotate, mailbox_key, new_cred_hex` |
| `GET /sender` | owner | `relay-v2-sender, mailbox_key, sender_mb` |
| `POST /send` | 有效 sender | `relay-v2-send, sender_mb, mailbox_key, seq_hint, expires_at, ts, nonce, SHA256(ciphertext)`（hash 在最后） |
| `GET /fetch` | owner | `relay-v2-fetch, mailbox_key, after, limit` |
| `POST /ack` | owner | `relay-v2-ack, mailbox_key, ids_csv` |
| `GET /status` | owner | `relay-v2-status, mailbox_key` |

- `/register` 授权到期在未来一年内，返回递增 `revision`；`/sender` 返回版本/有效性，不返回凭证。
- 授权/撤销响应丢失时，客户端保持原 `expected_revision`、凭证与到期时间，用新 nonce 重签重试。新增逻辑仅在当前状态恰为 `expected_revision + 1` 且凭证/到期逐值相同（撤销则为空凭证、到期为 0）时返回已有修订；不能通过重读新版本盲目重写，旧授权不能恢复撤销成员。2026-10-07 实际 Worker 并发/重启/异文回归与 Rust 互通通过，最新 `npm test` **14 passed**。
- `/send` 签的是**原请求 expires_at**，验签后服务端再截断 TTL。新 ID 是 16 位零填充的邮箱序号，不是业务操作序号。
- `/fetch` 的 `after` 为 `0` 或规范化邮件 ID，严格排除起点；`limit` 为 1–100。响应带 `envelopes`、`next_cursor`、`has_more`，受页数和字节双限。
- `/ack` 接收 1–100 个不重复 ID，删除邮件及其片段。**应用数据/回执未持久化前不得 ack。** 投递成功也不能代替收件设备的应用回执。
- `/status` 返回 `bytes/count/quota/earliest_expiry`，不能据此宣称业务已同步。
- `/presence`、`/peers` 返回 410；离线邮箱取件不依赖在线发现。

Rust API：`create_mailbox`、`register_sender`、`sender_status`、`revoke_sender`、`rotate_owner`、`send`、`fetch_page`、`fetch`、`ack`、`status`。旧 `register_credential` 明确报不支持，不自动提升权限。HTTP 客户端拒绝 URL 前缀伪装、凭证/query/fragment 和重定向，并限制响应体；服务端错误正文不透传以免泄露秘密。

## 5. 尚未完成的接入与边界

- 用户设备的配对、可信公钥分发、本机私钥持久化与成员撤销传播；不能用 `/create` 替代加入用户数据集的授权。
- 完整快照、分块重组、持久传输调度、应用回执/GC、Provider 首次恢复和后台加工归属的 App 闭环。
- 生产部署还需账户级限流/创建额度、脱敏观测与运维策略；单邮箱配额不限制攻击者创建大量独立邮箱。
- 未承诺分布式自动加工接管或恰好一次外部副作用，盲中继不会提供全局执行锁。
- Provider 的 `localhost` 地址不能自动指向旧设备；应保持配置并明确提示可达性，而不是静默改地址发送 Key。

完整功能和真实设备验收仍以路线图为准，不因底层 13 项 Worker 测试通过而勾选整个 S02/S04/S06。
