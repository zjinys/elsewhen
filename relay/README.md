# Elsewhen 盲中继（Blind Relay）使用手册

> 设计文档：`docs/notes/proposed/architecture/2026-09-30-multi-device-sync.md` §6.3
> 客户端实现：`src/sync/transport_relay.rs`（Rust `RelayClient`）

## 1. 这是什么

relay 是 Elsewhen 多设备同步的**自托管盲中继**：一个部署在你自己 Cloudflare 账户上的
Worker + Durable Objects 服务，为多设备同步提供两种能力：

1. **加密邮箱**（异步兜底）：发件设备把端到端加密的增量包暂存到收件设备的邮箱，
   收件设备上线后取走。电脑白天写的，手机晚上在 4G 下取——不要求同时在线。
2. **设备发现**（presence/peers）：设备周期性心跳上报"我在线"和**端到端加密的**
   端点信息，对端查询后优先尝试 P2P 直连，直连失败才退回邮箱取件。

### 威胁模型：中继是不可信传输介质（与网盘同级）

| 中继能做的 | 中继不能做的 |
|---|---|
| 用 `relay_cred` 验签（HMAC-SHA256），挡住伪造投递 | 解密任何载荷（密文 blob 端到端加密） |
| 看到 device_id 的哈希、流量大小与时间（元数据） | 看到明文 device_id、表名、seq、内容 |
| 丢包 / 阻断服务（可用性风险，协议层用 seq 单调性防重放） | 伪造内容（无凭证签名不通过） |

**密钥派生**（在扫码绑定流程中由双方各自完成，见 `sync/binding.rs`）：

```
relay_cred  = HMAC(shared_key, "relay-auth")        # 只用于验签，不是共享密钥本身
mailbox_key = hex(HMAC(relay_cred, "relay-mailbox")) # 中继只存这个哈希
```

**自托管，官方不运营**：代码开源，你部署到自己的 Cloudflare 账户（约 5 分钟）。
不配中继不影响局域网 P2P 与文件导出路径——中继是可选的加速路径，不是唯一路径。

## 2. 部署

### 前置条件

- Node.js 18+
- 一个 Cloudflare 账户（免费额度 10 万请求/天，个人 2-3 设备远用不完）

### 步骤

```bash
cd relay
npm install
# 认证二选一：
#  a) npx wrangler login                      # 浏览器 OAuth 授权一次
#  b) cp .env.example .env 后填入 token      # CI/无浏览器环境推荐
npm run deploy            # = npx wrangler deploy
```

### 认证：把 Cloudflare Token 放到 `.env`

wrangler 4 会自动加载 `wrangler.toml` 所在目录的 `.env`（dotenv），所以把 token
写进 `relay/.env` 即可，`npm run deploy` / `npm run dev` 都能读到：

```bash
cd relay
cp .env.example .env
# 编辑 .env，填入：
#   CLOUDFLARE_API_TOKEN=xxx      # dash → My Profile → API Tokens，用 "Edit Cloudflare Workers" 模板
#   CLOUDFLARE_ACCOUNT_ID=xxx     # dash 右侧栏的 Account ID
```

注意事项：

- **仓库根 `.gitignore` 已忽略 `.env` 和 `.env.*`**（任意目录），token 不会被提交；
  提交的是 `relay/.env.example` 这个模板。
- 不要把 token 写进 `wrangler.toml`——它会进 git。
- shell 里已 `export CLOUDFLARE_API_TOKEN` 会覆盖 `.env` 的值（环境变量优先于 dotenv）。
- 认证只需一种：已 `wrangler login`（OAuth）的话 `.env` 可以不要；token 方式适合
  无浏览器环境与 CI。
- Token 泄漏时到 dash 同一页面 Roll（重置）即可，比全局 API Key 风险面小。

首次部署会自动应用 `wrangler.toml` 里的 Durable Objects 迁移（`v1` 创建 `Mailbox`
类的 SQLite 存储）。部署成功后 wrangler 会输出你的 Worker URL，形如：

```
https://elsewhen-relay.<你的子域>.workers.dev
```

**注意：URL 的 UI 配置入口尚未实现。** 多设备同步是进行中的路线图功能
（`docs/notes/proposed/architecture/2026-09-30-multi-device-sync.md`），当前交付状态：

| 组件 | 状态 |
|---|---|
| relay Worker（本目录） | ✅ 可部署，API 完整 |
| Rust 客户端 `src/sync/transport_relay.rs`（`RelayClient`） | ✅ 已实现，有冒烟测试 |
| 扫码绑定（`sync/binding.rs`）、局域网 P2P、文件导出 | ⬜ 未实现 |
| 设置页「数据 → 中继配置（relay_url）」UI 入口 | ⬜ 未实现（设计文档 §6.3 的规划，勿在设置页寻找） |

设计文档中的 UI 描述是**目标状态**。当前部署完 relay 后，它已可作为后端等待
客户端接入；URL 可先记着，待绑定流程落地后再配置。`curl <URL>/health`
可验证服务存活。

### 配置项（`wrangler.toml` 的 `[vars]`）

| 变量 | 默认值 | 含义 |
|---|---|---|
| `MAIL_TTL_MS` | `604800000`（7 天） | 邮箱条目存活时间，到期惰性清理 |
| `MAILBOX_QUOTA_BYTES` | `52428800`（50 MB） | 每个设备邮箱的最大字节数（防滥用） |
| `MAX_ENVELOPE_BYTES` | `8388608`（8 MB） | 单条信封最大字节数 |

改完重新 `npm run deploy` 生效。

## 3. 本地开发与测试

```bash
cd relay
npm run dev     # wrangler dev，本地起服务（默认 http://127.0.0.1:8787）
npm test        # node --test test/*.test.mjs，不依赖 wrangler
```

测试（`test/crypto.test.mjs`）锁定的是**协议不变量**，客户端与服务端必须共同遵守：

- 签名消息格式（域分隔串 + 参数拼接顺序）
- 验签拒绝：错误密钥 / 篡改参数 / 过期时间戳
- nonce 重放拒绝、配额与 TTL 边界
- 凭证注册不可被覆盖（防陌生人注入）

**改动 `worker.ts` 的签名规范时，必须同步改 Rust 客户端 `src/sync/transport_relay.rs`
和这份测试**，三方逐字节一致。

### 对已部署实例做端到端冒烟

`test/e2e-smoke.mjs` 模拟两台设备（A 发 / B 收）对线上 Worker 跑完整协议：
health → register（含重复注册 409）→ send（含 nonce 重放 409、错误凭证 403）→
status → fetch（校验密文原样返回）→ ack → presence → peers。只依赖 Node 18+，
零安装：

```bash
cd relay
node test/e2e-smoke.mjs https://elsewhen-relay.<你的子域>.workers.dev
# 或本地：npx wrangler dev 起服务后
node test/e2e-smoke.mjs http://127.0.0.1:8787
```

每次运行用随机 cred 派生新邮箱，互不干扰；测试邮件会 ack 删掉，nonce 10 分钟后
自动过期，不留垃圾。

## 4. HTTP API（v1，轮询式）

所有邮箱操作按 `mailbox_key`（64 位小写 hex）路由到对应的 Mailbox Durable Object
实例——每台设备一个邮箱实例。POST 请求必须 `Content-Type: application/json`。

签名公共规则：

- 算法：`HMAC-SHA256(relay_cred, 域分隔串|参数1|参数2|...)`，输出小写 hex；
  参数均为 hex/数字/固定串，不含 `|`。
- `ts`（毫秒时间戳）与服务器时间差超过 **±5 分钟**拒绝（`403 stale timestamp`）。
- `nonce`（32 位 hex）在 **10 分钟**窗口内重放拒绝（`409 nonce replay`）。
- 除 `/health` 与 `/register` 外，所有接口要求发件人/收件人事先已注册凭证。

### `GET /health`

存活探测，无鉴权。返回 `{"ok": true, "service": "elsewhen-relay", "version": 1}`。

### `POST /register` — 注册验签凭证

```json
{
  "mailbox_key": "<收件人邮箱，64-hex>",
  "sender_mb":   "<发件人 mailbox_key，64-hex>",
  "cred_hex":    "<relay_cred，64-hex>",
  "expires_at":  <凭证过期时间，ms epoch，须为未来>,
  "sig":         "HMAC(cred_hex, \"relay-register\"|sender_mb|mailbox_key|expires_at)"
}
```

- 自证签名：用申报的 `cred_hex` 自己签，证明你确实持有它。
- **仅当该 (sender → 本邮箱) 尚无有效凭证时接受**；已有则 `409`——陌生人无法
  覆盖受害者凭证。凭证的首次注入走带外通道（扫码二维码），轮换也必须带外进行。
- 取件/查状态用的是"自己 → 自己"注册的凭证（recipient = self）。

### `POST /send` — 投递加密信封

```json
{
  "mailbox_key": "<收件人邮箱>",
  "sender_mb":   "<发件人 mailbox_key>",
  "seq_hint":    1,
  "ciphertext":  "<base64 端到端密文>",
  "expires_at":  <期望过期时间，ms epoch；服务端会 clamp 到 now+MAIL_TTL_MS>,
  "ts": <ms>, "nonce": "<32-hex>",
  "sig": "HMAC(cred, \"relay-send\"|sender_mb|mailbox_key|seq_hint|expires_at|ts|nonce|SHA256(ciphertext))"
}
```

注意：签名里的 `expires_at` 是 **clamp 之后**的值（即 `min(请求值, now+TTL)`），
客户端要按服务端实际生效的过期时间签名。

成功返回 `{"ok": true, "id": "<16 位零填充序号>"}`。错误：

| 状态码 | 含义 |
|---|---|
| 403 | 无凭证 / 签名错误 / 时间戳超窗 |
| 409 | nonce 重放 |
| 413 | 单条超过 `MAX_ENVELOPE_BYTES` |
| 507 | 邮箱配额满（`MAILBOX_QUOTA_BYTES`） |

### `GET /fetch` — 游标取件

```
GET /fetch?mailbox_key=<自己>&recipient_mb=<自己>&after=<游标>&limit=<n>&ts=<ms>&nonce=<32-hex>&sig=...
sig = HMAC(cred, "relay-fetch"|mailbox_key|after|limit|ts|nonce)
```

- `after`：上次取到的最大信封 `id`，首次传 `0`；`limit` 上限 100。
- 返回 `{"ok": true, "envelopes": [{id, sender_mb, seq_hint, ciphertext, created_at, expires_at}, ...]}`。
- `seq_hint` 是发件方本地序号提示，用于乱序/丢包检测；真正的 seq 单调性校验
  在解密密文后由 sync 协议层做（拒绝 ≤ acked_seq 的包）。

### `POST /ack` — 确认删除

```json
{
  "mailbox_key": "<自己>", "recipient_mb": "<自己>",
  "ids": ["0000000000000001", "..."],
  "ts": <ms>, "nonce": "<32-hex>",
  "sig": "HMAC(cred, \"relay-ack\"|mailbox_key|ids_csv|ts|nonce)"
}
```

`ids` 一次 1–100 条，`ids_csv` 是逗号拼接。返回 `{"ok": true, "deleted": n}`。
**只有密文解密并成功落库后才 ack**——ack 即删除，删了中继上就没有了。

### `GET /status` — 邮箱状态

```
GET /status?mailbox_key=<自己>&recipient_mb=<自己>&ts=<ms>&nonce=<32-hex>&sig=...
sig = HMAC(cred, "relay-status"|mailbox_key|ts|nonce)
```

返回 `{"ok": true, bytes, count, quota, earliest_expiry}`，用于 UI 展示用量与
最早过期时间。

### `POST /presence` — 在线心跳

```json
{
  "mailbox_key":   "<自己>",
  "endpoint_blob": "<base64 端到端加密的端点信息（addr/port/device_id 等）>",
  "ttl_ms":        120000,
  "ts": <ms>, "nonce": "<32-hex>",
  "sig": "HMAC(cred, \"relay-presence\"|mailbox_key|SHA256(endpoint_blob)|ttl_ms|ts|nonce)"
}
```

`ttl_ms` 硬上限 5 分钟（建议 2 分钟心跳一次）。端点信息用共享密钥加密，
中继只作盲索引存着。

### `GET /peers` — 发现在线对端

```
GET /peers?mailbox_key=<自己>&ts=<ms>&nonce=<32-hex>&sig=...&want=<可选，逗号分隔的对端 mailbox_key>
sig = HMAC(cred, "relay-peers"|mailbox_key|ts|nonce)
```

可见性规则：只返回**已在本邮箱注册过有效凭证**（即绑定过）且 presence 未过期的
对端。返回 `{"ok": true, "peers": [{mailbox_key, endpoint_blob, updated_at}]}`，
`endpoint_blob` 原样返回密文，由客户端用共享密钥解密。

### 清理行为

无需运维：每次请求顺手做惰性过期清理（每分钟最多一次），清理过期邮件、
nonce 和 presence；不依赖常驻 alarm。

## 5. 客户端集成（Rust）

App 侧不直接拼 HTTP——用 `src/sync/transport_relay.rs` 的 `RelayClient`：

```rust
let client = RelayClient::new("https://elsewhen-relay.<sub>.workers.dev")?;
client.health()?;

// 派生（实际由 binding.rs 在扫码绑定时完成并存好，这里只演示）
let relay_cred: [u8; 32] = RelayClient::derive_relay_cred(&shared_key);
let mailbox_key = RelayClient::mailbox_key(&relay_cred); // 自己的邮箱

// 发件：绑定后先向收件人邮箱注册发件凭证（一次性），再投递信封
client.register_credential(&recipient_mb, &sender_mb, &relay_cred, cred_expires_at)?;
let id = client.send(&recipient_mb, &sender_mb, &relay_cred,
                     seq_hint, &ciphertext_b64, expires_at)?;

// 收件：fetch → 解密落库 → ack（取件者用 self→self 凭证自证）
let envelopes = client.fetch(&mailbox_key, &relay_cred, "0", 50)?;
// ...解密、apply_increment()...
client.ack(&mailbox_key, &relay_cred, &ids)?;

// 发现：心跳 + 查对端，能直连则直连，否则退回邮箱
client.presence(&mailbox_key, &relay_cred, &endpoint_blob_b64, 120_000)?;
let peers = client.peers(&mailbox_key, &relay_cred, &[])?; // want 为空 = 全部已绑定对端

// 邮箱状态（UI 展示用量）
let status = client.status(&mailbox_key, &relay_cred)?;
```

要点：

- `RelayClient::new` 强制 HTTPS（仅 `127.0.0.1`/`localhost` 允许 HTTP，供本地开发）。
- 客户端只做"信封的密封与投递"，不知道载荷内容；载荷是增量行/回执/快照由
  sync 协议层决定，三条传输路径（P2P / 中继 / 文件）共用同一个 `apply_increment()`。

## 6. 风险与边界（诚实声明）

1. **元数据泄露**：中继运营者（Cloudflare 或部署者本人）可见"哪些 device 哈希在
   何时通信、流量多大"。看不到内容。
2. **可用性依赖**：中继挂了只影响漫游路径；局域网 P2P 与文件导出不受影响。
3. **恶意中继**可丢包、重放旧密文、阻断服务——协议层用 seq 单调性防重放，
   签名防伪造，端到端加密防读取；但**可用性攻击无法防**，这是自托管换来的边界。
4. **凭证轮换必须带外**：`/register` 拒绝覆盖已有凭证，换 cred 需要重新扫码绑定。
5. WebSocket 实时推送留作后续；当前是轮询式取件。
