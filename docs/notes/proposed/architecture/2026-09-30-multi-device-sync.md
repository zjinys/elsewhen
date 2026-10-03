# Agent Note: 多设备同步的设计路径

Status: proposed

关联需求：无（新功能提案）

## Problem

Elsewhen 是 local-first 个人事件系统，所有数据存于单一 SQLite 文件（`~/.local/share/elsewhen/elsewhen.db`）。用户提出多设备同步需求：在多台设备（比如桌面 + 笔记本，或未来移动端）上使用 elsewhen，数据保持一致。

核心约束与现状盘点：

| 现状 | 对同步的影响 |
|---|---|
| 所有表主键是 `Uuid::new_v4()` | ✅ 天然避免 ID 冲突，不需要中心发号器 |
| `events.raw_text/recorded_at/source` 有不可变触发器 | ✅ 事件 append-only，同步语义简单（只插入，不更新） |
| 27 张表，大部分有 `created_at`，一半有 `updated_at` | ⚠️ 缺 `updated_at` 的表无法做增量同步（`messages`、`event_analyses`、`wiki_revisions` 等） |
| 无外键开启证据（grep 无 `PRAGMA foreign_keys`） | ⚠️ 若开启，同步时的插入顺序必须满足外键依赖 |
| `ON DELETE CASCADE` 用于 messages/wiki_revisions 等 | ⚠️ 删除会话会级联删除消息，同步删除语义复杂 |
| `api_key` 存在 `ai_provider_configs` 表 | ❌ 密钥不能同步 |
| 无设备概念（无 `device_id`、`node_id`） | 需要新增设备身份 |
| 单文件 SQLite，WAL 模式 | 文件级同步（Dropbox/网盘）会导致冲突 |

## 核心判断

**同步是 elsewhen 架构的下一个数量级复杂度跃升，比流式、工具调用、知识库都更根本。** 流式是"一个进程内的并发"，同步是"多个进程/设备间的并发"，引入了 elsewhen 从未面对过的问题：冲突、时钟漂移、网络分区、密钥分发。

因此第一个设计决策是：**先明确同步的边界与目标，再选技术方案。** 三个候选目标，成本与价值差异极大：

| 目标 | 用户价值 | 技术成本 | 风险 |
|---|---|---|---|
| **A. 备份/迁移** | 换电脑时数据不丢 | 低（导出/导入 SQLite 文件） | 低 |
| **B. 单向同步** | 桌面记录，手机只读查看 | 中（定期导出增量，导入端只读） | 中（增量导出的正确性） |
| **C. 双向同步** | 多设备同时读写，实时一致 | 高（CRDT 或中心服务） | 高（冲突解决、密钥同步、网络分区） |

本方案聚焦 **B（单向同步）作为第一阶段**，理由：

1. elsewhen 的核心场景是"记录 + 回顾"，记录端（桌面）与查看端（手机/平板）天然分离
2. 双向同步的冲突解决（两台设备同时编辑同一 wiki 页）与 elsewhen 的"防静默覆盖"原则冲突——自动合并必然引入用户不可见的变更
3. 单向同步的增量导出机制是双向同步的前置（双向 = 两个方向各自的增量 + 冲突解决）

## Proposal

### 0. 设备管理：同步的信任层与拓扑定义

数据怎么流动之前，先回答"数据在哪些设备之间流动"。设备管理要回答五个问题：我是谁、还有谁、各是什么角色、怎么加入/退出、信任边界在哪。其中信任边界最关键：**如果同步文件只是丢在网盘里，任何拿到文件的人都能导入你的全部个人数据**——设备管理本质上是同步的信任层，而不只是花名册。

#### 0.1 设备注册表：`devices` 表

```sql
CREATE TABLE devices (
  id TEXT PRIMARY KEY,              -- device_id (UUID v4)
  name TEXT NOT NULL,               -- 用户起的名字："我的台式机" / "MacBook"
  platform TEXT NOT NULL,           -- "linux" / "macos" / "windows" / "android"
  role TEXT NOT NULL CHECK(role IN ('writer','reader')),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','revoked')),
  paired_at TEXT NOT NULL,
  last_seen_at TEXT,                -- 最近一次成功同步的时间
  acked_writer_id TEXT,             -- 该设备确认的导出方 device_id（见 3.2）
  acked_seq INTEGER                 -- 该设备对该导出方已确认导入的最大 sync_log 序号
);
```

本机的 device_id 存 `app_meta`（键 `device_id`，首次启动时生成）；其他设备存 `devices` 表。

#### 0.2 角色是设备属性，硬限制 1 个 writer

- **writer**：能产生导出文件的设备（通常是桌面主力机）
- **reader**：只能导入的设备（手机、平板、备用机）

**writer 数量硬限制为 1**（数据库层唯一部分索引，不只是应用层提示）：

```sql
CREATE UNIQUE INDEX idx_devices_single_writer
  ON devices(role) WHERE role='writer' AND status='active';
```

这与"单向优先"一致；允许多个 writer 实质上是分片的双向同步，需要冲突解决，本方案明确不做。换主力机时用户显式移交 writer 角色（旧设备降级为 reader，新设备升级），移交动作本身也会随导出传播。

#### 0.3 绑定流程：扫码带外授权（一次性 token）

elsewhen 的同步是**私人**的：配对不是"在网络上发现谁开着 elsewhen 再挑一个"，而是**带外绑定**——两台设备在物理上碰面（扫码），网络永远不知道这件事。这是与 mDNS 发现方案的本质区别：网络上即便有 10 个人同时用 elsewhen，彼此在协议层完全不可见（见 §6.1 的零广播原则）。

绑定流程（以 Android 手机绑定电脑为例）：

```
writer（电脑）：Settings → 同步 → [添加设备] → 显示二维码，60 秒有效
二维码内容：
{
  "device_id": "writer 的 uuid",
  "addr_hint": "192.168.1.5:7610",      ← 仅提示，可过期
  "pairing_token": "32 字节随机串",       ← 一次性，60 秒过期
  "expires_at": ...
}

新设备（Android）：Settings → 同步 → [扫码绑定] → 扫二维码
  → 按 addr_hint 直连 writer，报 pairing_token
  → writer 校验 token 有效且未过期 → 写入 devices 表（role=reader）
  → 双方各自持久化对方的 device_id + 共享密钥（用于后续会话 TLS-PSK）
  → 各自派生中继凭证 relay_cred = HMAC(shared_key, "relay-auth")（§6.3，仅当中继配置后使用）
  → 同一会话完成首次全量同步
```

安全属性（对比 mDNS+确认框方案的强项）：

- **零网络暴露面**：不扫码，writer 不接受任何配对请求；二维码不出现在网络上（手机对着电脑屏幕扫，是物理带外通道），网络上其他 elsewhen 用户连敲门的入口都没有
- **token 一次性 + 60 秒过期**：二维码被拍照泄露也只有极短窗口，且用一次即废
- **没有"从列表里挑设备"这一步**：不存在挑错、不存在被诱导确认陌生设备

远程绑定（两台设备不在一起）：writer 把二维码导出成图片经已有信道（微信等）发给对方，安全性依赖对该信道的信任——与 Signal 安全码验证同一模型，UI 文案如实说明。

绑定后两台设备互相"认识"：对方的 device_id + 共享密钥都在本地 `devices` 表 / `app_meta`（密钥不导出、不进同步流）。后续同步会话用 TLS-PSK 双向认证——冒充 device_id 的人没有密钥，握手直接失败。撤销设备即删密钥，硬失效。

文件导出兜底路径（漫游/备份，见 §6.2）没有实时确认通道，退回**同步密码**模型：用户设置密码，导出文件用密码派生密钥加密（Argon2id → XChaCha20-Poly1305）；导入端输入同一密码完成绑定。能解密即授权，密码永不写入任何文件。

#### 0.4 设备注册信息的传播

设备注册表本身随同步数据传播（`devices` 表不含密钥材料）：

- **P2P 路径**：会话握手时 writer 直接推送最新 `devices` 表；新设备的注册信息在配对时已写入 writer，无需反向通道。reader 的同步确认（ack）在会话内即时返回——reader 导入成功后发一条消息「我收到 (writer_id, seq=N)」，writer 当场更新 `acked_writer_id` / `acked_seq` / `last_seen_at`
- **文件兜底路径**：writer 导出时带上 `devices` 表；reader 的确认通过回执文件（receipt-*.db）异步传回，writer 下次导出前回收

ack 从异步回执变成会话内即时确认后，清理水位（见 3.1）的语义反而更强：writer 清掉的每一行都已被所有 active reader **明确确认过**，而不是"假设它拿到了"。离线 reader 的 `acked_seq` 卡住水位，`sync_log` 只是多存一阵，数据不丢。

#### 0.5 设备撤销

旧设备丢失/淘汰时：

- writer 上把该设备 `status` 改为 `revoked` 并删除其同步密钥，撤销信息随下次同步传播
- **P2P 路径（硬失效）**：被撤销设备发起连接时 TLS-PSK 握手直接失败（密钥已删），比文件方案强
- **文件兜底路径（软约束）**：被撤销设备导入新文件时发现自己在撤销列表 → 拒绝导入并提示"该设备已被移除"；但已被拷走的旧文件仍可读取，启用同步密码时可换密码实现硬失效

诚实的限制：文件介导的撤销不是即时的（要等被撤销设备下次尝试导入才生效），且无法阻止旧导出文件被读取。这个限制必须写进 UI 文案，不假装撤销是绝对安全的。

#### 0.6 设备管理 UI

挂在 Settings → 数据 → 同步：

```
同步设备
├─ 本机：我的台式机 (linux) · 写入端 · 本机
├─ MacBook Air (macos) · 只读端 · 3 小时前同步
├─ Pixel 8 (android) · 只读端 · 昨天同步
└─ [+ 添加设备（扫码）]

文件导出/导入（漫游/备份）：
[导出到文件]  [从文件导入]  [设置同步密码（仅文件导出用）]
```

每台设备显示：名字、平台、角色、最后同步时间、操作（重命名/撤销）。`devices` 表为空时入口仍可见（与目标入口"空态可发现"的纪律一致）。

### 1. 设备身份：本机 `device_id`

新增 `app_meta` 键值：

```rust
// 首次启动时生成，持久化
let device_id = Uuid::new_v4().to_string();
store.set_meta("device_id", &device_id)?;
```

`device_id` 用于：
- 在 `devices` 表中标识本机行（`role='writer'` 或 `'reader'`）
- 标记每条数据的来源设备（`sync_log.origin_device_id`，见下）
- 导出文件的命名（`elsewhen-export-{device_id}-{timestamp}.db`）
- 未来双向同步时的向量时钟（本方案不实现，但 `device_id` 是前置）

### 2. 数据分类：什么同步、什么不同步

| 表 | 同步？ | 理由 |
|---|---|---|
| `events` | ✅ | 核心数据，append-only |
| `event_analyses` | ✅ | AI 分析结果，只插入不更新 |
| `conversations` / `messages` | ✅ | 对话历史 |
| `wiki_pages` / `wiki_revisions` / `wiki_log` | ✅ | 知识库 |
| `todos` / `rules` / `goals` | ✅ | 用户显式创建的对象 |
| `daily_reviews` / `insights` | ✅ | 生成的回顾 |
| `relations` / `entity_facts` 等 | ✅ | 实体关系 |
| `ai_provider_configs` | ❌ | 含 `api_key`，密钥不离开设备 |
| `analysis_jobs` / `knowledge_digest_jobs` | ❌ | 队列状态是设备本地的（另一台设备会自己重新排队） |
| `token_usage` | ❌ | 统计是设备本地的（或可选同步，但无必要） |
| `pending_actions` | ❌ | 待确认操作绑定设备上下文 |
| `devices` | ✅ | 设备注册表本身是同步拓扑的载体（见 0.4）；不含任何密钥材料 |
| `schema_migrations` / `app_meta` | ❌ | 元数据 |

**原则：用户创建的内容同步，系统运行状态不同步，密钥绝不同步。**

### 3. 增量导出：`sync_log` 表

问题：如何知道"上次导出后新增了哪些数据"？

方案 A（时间戳）：用 `created_at > last_export_at` 过滤。问题：时钟漂移（设备时间不准）、`updated_at` 缺失的表无法追踪更新。

方案 B（sync_log 触发器）：新增一张 `sync_log` 表，用触发器记录每次插入/更新：

```sql
CREATE TABLE sync_log (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  table_name TEXT NOT NULL,
  row_id TEXT NOT NULL,
  operation TEXT NOT NULL CHECK(operation IN ('insert','update','delete')),
  changed_at TEXT NOT NULL,
  origin_device_id TEXT NOT NULL
);
```

对每张需同步的表建触发器：

```sql
CREATE TRIGGER sync_events_insert AFTER INSERT ON events
BEGIN
  INSERT INTO sync_log (table_name, row_id, operation, changed_at, origin_device_id)
  VALUES ('events', NEW.id, 'insert', datetime('now'), (SELECT value FROM app_meta WHERE key='device_id'));
END;
-- 同理 UPDATE / DELETE
```

导出时：

```sql
SELECT * FROM sync_log WHERE seq > ?last_exported_seq ORDER BY seq;
```

**方案 B 的优势**：
- 不依赖时间戳（时钟漂移无害）
- `seq` 是单调递增的本地序号，天然有序
- 缺 `updated_at` 的表也能追踪（触发器记录的是操作，不是时间戳）
- `origin_device_id` 为未来双向同步的向量时钟留了口子

**方案 B 的代价**：
- 每次写操作多一次 `sync_log` 插入（SQLite 触发器开销极小，实测 <1% 写入放大）
- 需要定期清理已导出的 `sync_log` 行

#### 3.1 `sync_log` 清理：靠 reader 回执，不靠保留天数

清理水位 = 所有 active reader 对本 writer 的 `acked_seq` 最小值（P2P 会话内即时 ack，或文件兑底路径的回执文件，见 0.4）：

```sql
DELETE FROM sync_log WHERE seq <= (
  SELECT COALESCE(MIN(acked_seq), 0) FROM devices
  WHERE role='reader' AND status='active' AND acked_writer_id = ?my_device_id
);
```

没有 reader 时不清理（保留全部历史，导出永远全量——单设备场景 sync_log 增长与事件量同阶，可接受）。新配对的 reader 首次导入即全量，之后其 `acked_seq` 参与水位计算。被撤销的 reader 不再参与（`status='active'` 过滤），不会因一台死设备永远卡住清理。

#### 3.2 writer 移交时的 seq 重置：确认序号必须带 writer 身份

writer 角色的设备各自维护独立的 `sync_log` 序号空间。移交 writer（旧 writer 降级 reader、新 writer 升级）后，新 writer 的 `sync_log` 从 seq 1 重新开始——如果 reader 的确认序号只是单值 `acked_seq`，移交后它会把新 writer 的 seq 5 误认为旧体系的 seq 5，清理水位直接错乱（可能清掉未同步的数据，或永远卡住）。

因此确认序号是 **(writer_id, seq) 二元组**：

- 回执文件里写 `acked_writer_id` + `acked_seq`，reader 对每个 writer 分别记忆
- writer 清理水位时只统计 `acked_writer_id = 自己` 的 reader 回执（见 3.1 的 SQL）
- 移交流程：旧 writer 导出最后一次（含 devices 表的角色变更）→ 新 writer 导入后发现自己被指定为 writer → 从全量状态重建自己的 `sync_log`（首批导出即全量），reader 端靠 `INSERT OR IGNORE` 幂等衔接，旧 writer 的 `sync_log` 历史不需要迁移

#### 3.3 多导出文件的顺序导入：reader 侧的进度游标

reader 一周没开机时，同步目录里会累积多个 export 文件。reader 需要一个本地进度游标，否则每次启动都重扫全部文件（虽幂等但浪费）：

- 每个 export 文件头写入单调递增的 **export_id**（writer 侧计数器，存 `app_meta` 键 `export_counter`）
- reader 本地 `app_meta` 存 `last_imported_export_id`，启动时只处理 export_id 更大的文件
- 按 export_id 顺序逐个导入（不是按文件名/时间戳——writer 时钟漂移不影响序号单调性）
- 发现 export_id 跳号（比如上次导到 10，目录里最小的是 15）说明中间文件被删或 writer 清理了：reader 报警并请求一次全量导出（writer 侧提供"生成全量导出"按钮），不静默跳过——否则 reader 会永久缺失 seq 11–14 的数据
- 已处理的 export 文件由 **writer 负责删除**（所有 reader 确认后），reader 不删目录里的文件——避免多 reader 时互相抢删

### 4. 导出格式：SQLite 文件而非 JSON

候选格式：

| 格式 | 优势 | 劣势 |
|---|---|---|
| JSON/JSONL | 可读、可 diff | 大（base64 编码二进制）、无索引、导入慢 |
| SQLite 文件 | 紧凑、可直接 attach、事务导入 | 二进制不可读 |

**选 SQLite**：导出时创建一个新的 SQLite 文件，只包含增量行：

```rust
fn export_increment(store: &Store, since_seq: i64, output_path: &Path) -> Result<ExportManifest> {
    let export_db = Connection::open(output_path)?;
    export_db.execute_batch("CREATE TABLE events (...); CREATE TABLE sync_metadata (...);")?;

    let tx = export_db.transaction()?;
    let mut stmt = store.connection.prepare(
        "SELECT DISTINCT table_name, row_id FROM sync_log WHERE seq > ?1 ORDER BY seq"
    )?;
    // 按 table_name 分批查出完整行，插入 export_db
    // ...

    tx.commit()?;
    Ok(ExportManifest {
        device_id: store.device_id()?,
        from_seq: since_seq,
        to_seq: store.max_sync_seq()?,
        exported_at: Utc::now(),
        table_counts: ...,
    })
}
```

导入端 `ATTACH DATABASE` 导出文件，事务内逐表插入：

```rust
fn import_increment(store: &Store, import_path: &Path) -> Result<ImportReport> {
    let tx = store.connection.transaction()?;
    tx.execute("ATTACH DATABASE ?1 AS import", [import_path.to_str()])?;

    // 按依赖顺序插入（events → event_analyses → conversations → messages → ...）
    tx.execute("INSERT OR IGNORE INTO events SELECT * FROM import.events", [])?;
    // ...

    tx.execute("DETACH DATABASE import", [])?;
    tx.commit()?;
}
```

**`INSERT OR IGNORE`**：如果导入端已有该行（比如重复导入同一文件），静默跳过。这是幂等导入的关键。

#### 4.1 导出文件的加密（仅文件兜底路径）

P2P 路径的加密在传输层（TLS-PSK，同步密钥见 0.3），数据不落盘，无需文件加密。

文件兜底路径启用同步密码时，导出文件整体加密：密码派生密钥（Argon2id），文件体用 XChaCha20-Poly1305 加密，文件头保留少量明文元数据（magic、版本、writer device_id、salt、nonce）以便导入端识别"这是 elsewhen 导出文件且需要密码"。密码不写入任何文件，只存在于各设备本地（`app_meta` 存派生校验值，不存原文）。

未启用密码时导出文件是明文 SQLite，信任边界是传输通道本身——这个取舍必须在导出按钮的 UI 文案里明示。

### 5. 冲突策略：导入端永远落后

单向同步的定义决定冲突不存在：
- 导出端是唯一写者
- 导入端只读（或导入端的本地修改不会被导回）

如果用户在导入端本地也记录了事件（违背了"单向"的约定），`INSERT OR IGNORE` 会保留导入端的本地行，导出端的同 ID 行被跳过。由于 ID 是 UUID v4，两台设备独立生成的 ID 不会冲突，所以实际是**两份数据共存**，不是冲突。

如果未来要支持"导入端也能写，然后双向同步"，才需要冲突解决（见「未来扩展」）。

### 6. 传输：局域网 P2P 为主，中继漫游，文件导出兜底

同一套增量协议（`sync_log`、`(writer_id, seq)`、确认水位）挂三个传输，协议层不感知传输差异。

#### 6.1 P2P（主）：reader 拉取 + writer 零广播

**发现：不用 mDNS 广播，而是 reader 定向寻找已绑定的设备。** 设备绑定（§0.3）之后双方互知 device_id 与共享密钥，重连只需解决"IP 变了"的问题：

1. **记住地址直连（主）**：绑定时和每次成功同步后，reader 记住 writer 最近的成功地址。启动 app 时先试直连——局域网 IP 相对稳定，大部分时候零广播直接成功
2. **mDNS 定向询问（兜底）**：直连失败时，reader 在局域网询问「device_id=X 在吗」，**只有 X 本机应答**。writer 平时不广播任何服务——网络上其他 elsewhen 用户（以及任何扫描者）看不到它的存在；询问包只泄露"我在找 X"，不泄露 X 是否在网

**writer 零广播原则**：未绑定设备在网络上看不到 writer 的任何存在（无服务广播、无开放端口扫描面——同步端口只响应已绑定设备的认证握手）。网络上即便有 10 个人同时用 elsewhen，彼此完全不可见。

**会话流程（reader 拉取）：**

```
1. reader 启动 app → 直连记住的地址（或定向询问）→ 找到 writer
2. 双向出示 device_id，各自查 devices 表：对方是 active 设备？
   → 不在表里 / 已撤销 → 立即断开（陌生人连握手都进不了）
3. TLS-PSK 握手（绑定时建立的共享密钥）→ 密钥不对 → 握手失败
4. reader 报 acked_seq；writer 流式发送 sync_log seq > acked_seq 的增量行
5. reader 事务内写入，成功后回 ack(N)
6. writer 更新该 reader 的 acked_seq / last_seen_at
```

**reader 拉取而不是 writer 推送**：手机在后台无法可靠监听网络（Android Doze、iOS 后台限制），同步时机只能是"用户打开 app 的那一刻"——打开 app → 连接已绑定的 writer → 拉取 → 完成。

P2P 消除了文件方案最丑的部分：

| 文件方案的问题 | P2P 下 |
|---|---|
| 导出文件堆积、谁负责删除 | 传输是临时的，不产生文件 |
| 回执文件异步周转 | ack 会话内即时返回 |
| export_id 跳号事后检测 | 握手时 reader 报 seq，断档立刻暴露（writer 回退全量流式发送） |

诚实的代价（与文件方案对比的取舍）：

| 问题 | 文件方案 | P2P 方案 |
|---|---|---|
| 同时在线 | 不需要（网盘是天然存储转发） | **需要**。手机和电脑同时在同一局域网才同步 |
| 跨网络 | 网盘天然解决 | 出门在外不同步；漫游靠文件兜底或 Tailscale 虚拟组网（进阶选项，不内置） |

对个人 2-3 台设备，"手机和电脑每晚都在家里 wifi 上"基本成立，这个取舍可接受。**NAT 打洞需要协调服务器且成功率约 80%（对称 NAT 失败），明确不做——漫游需求由盲中继解决（§6.3）。**

#### 6.3 盲中继（漫游路径）：自托管的加密邮箱

漫游场景（出差一周，手机在 4G 下想同步电脑上的记录）要求 P2P 也能跨网络。中继节点（比如在 Cloudflare 上跑一个 Worker）换回网盘的"存储转发"能力，但**不把数据交给第三方**。

**核心原则：中继是无知的路由器与目录，不是同步参与者。** 它不解密、不知道内容、不存明文地址，只持有鉴权凭证与端到端加密的盲索引。威胁模型与加密导出文件丢网盘完全一致——中继和网盘都是"不可信传输介质"。

中继的职责有两个：

1. **设备目录（发现）**：设备心跳上报"我在线"+ 端到端加密的端点信息（addr/port/device_id），中继存盲索引；对端来问时返回加密端点。中继不知道设备的真实地址，只转发密文。
2. **加密邮箱（异步传输）**：当直连不可达时，writer 把端到端加密的增量包暂存中继，reader 上线取走。

```
手机(reader)                Cloudflare Worker(中继)              电脑(writer)
     │                            │                              │
     │── 注册(device_id 哈希) ──→│←── 注册 ─────────────────────│
     │                            │                              │
     │←──── 端到端加密隧道(中继转发密文) ────────────────────────→│
     │      (TLS-PSK,密钥来自扫码绑定,中继从未接触)              │
     │                            │                              │
     │←──── 同步协议照旧(报 seq → 流式增量 → ack) ──────────────→│
```

**发现优先于传输：能直连则直连，否则走邮箱。** reader 启动时先问中继"我的对端在线吗"，拿到加密端点后尝试直连；直连失败（NAT 阻隔、对端不在局域网）才退回邮箱取件。这样中继的存储与带宽成本最小化——大部分同步走 P2P 直连，邮箱只是兜底。

**中继是无状态转发，还是短期邮箱？**

| 方案 | 行为 | 漫游体验 |
|---|---|---|
| A. 纯转发 | 双方同时连上中继才传 | 仍需同时在线（只是不要求同一局域网了） |
| B. **加密邮箱** | writer 把增量包（已端到端加密）暂存中继，reader 上线取走 | **真异步**：电脑白天写的，手机晚上在 4G 下取 |

**选 B——真异步才是漫游的价值。** 邮箱条目是端到端加密的 blob（中继无法读），带 TTL（默认 7 天）+ 每设备配额（防滥用）。

**信令鉴权：谁能往我的邮箱放东西？**

中继需最低限度鉴权，否则任何人的邮箱都会被垃圾填满：

- 绑定（§0.3）时双方各自派生**中继凭证**：`relay_cred = HMAC(shared_key, "relay-auth")`——不是共享密钥本身
- writer 往 reader 邮箱放包时附带签名；中继持 relay_cred 验签 → 伪造的包进不了邮箱
- 中继不知道 shared_key：**只能验签，不能解密内容——鉴权与保密分离**

**Cloudflare 形态**：Worker + Durable Objects（Worker 接 WebSocket/HTTPS，Durable Object 做每设备邮箱，天然单实例带存储）。免费额度 10 万请求/天，个人 2-3 设备远远用不完。

**自托管，官方不运营**：Worker 代码开源，用户部署到自己的 Cloudflare 账户（约 5 分钟）。这一条很重要——**无中心原则的本质不是"没有服务器"，而是"没有别人的服务器"**：中继是用户自己的基础设施，elsewhen 不变成服务提供商。代价是漫游功能有部署门槛，文档与 UI 需如实说明（可选功能，不配不影响局域网与文件路径）。

**中继引入的新风险（诚实声明）：**

1. **元数据泄露**：中继运营者（Cloudflare，或部署者自己）可见"哪些 device 哈希在何时通信、流量多大"。看不到内容
2. **可用性依赖**：中继挂了 → 仅漫游路径失效，局域网与文件路径不受影响
3. **主动攻击面**：恶意中继可丢包/重放旧密文/阻断服务——协议层用 seq 单调性防御重放（reader 拒绝 ≤ acked_seq 的包）；**无法伪造内容**（签名）也无法读取内容（端到端加密）

**路径优先级与降级（reader 启动时）：**

```
1. 问中继：我的对端在线吗？（取加密端点）
2. 在线 → 尝试直连（局域网或经中继转发的端到端隧道）
   ├─ 直连成功 → P2P 同步（中继零存储成本）
   └─ 直连失败 → 走中继邮箱取件（异步）
3. 对端不在线 / 中继不可达 → 文件导入（手动）
```

中继是加速路径与目录服务，不是唯一路径——这是"弱中心"与"强中心"的分界线。局域网 P2P 与中继漫游共用同一套发现机制，只是端点的可达性不同。

#### 6.2 文件导出（兜底）：保留，两个用途

1. **终极兜底**：中继不可用或不想部署时的异步同步（网盘/U盘手动拷贝）
2. **备份**：导出文件本来就是现成的全量/增量备份

文件路径的协议与 §3-§5 完全一致（export_id 游标、回执文件、清理水位），只是传输从会话流换成文件。reader 侧导入逻辑统一：局域网 P2P、中继、文件三条路径收到的增量走同一个 `apply_increment()` 入口。

UI：Settings → 数据 → 同步设备列表（在线状态/上次同步时间）+ 中继配置（relay_url，可选）+ 导出/导入按钮（文件兜底）。

### 7. `updated_at` 缺失表的增量追踪

`messages`、`event_analyses`、`wiki_revisions` 等表缺 `updated_at`，但它们是 append-only（只插入，不更新），所以 `sync_log` 的 INSERT 触发器足够。

如果未来某张 append-only 表需要支持 UPDATE（比如 `messages.content` 允许编辑），需要：
1. 迁移加 `updated_at` 列
2. 加 UPDATE 触发器到 `sync_log`

本方案不预判这个需求，触发器机制已为 UPDATE 留了口子。

## 为什么这样设计

| 决策 | 理由 |
|---|---|
| 单向同步优先 | elsewhen 的记录/查看场景天然分离；双向的冲突解决与"防静默覆盖"原则冲突 |
| SQLite 文件而非 JSON | 紧凑、可 attach、事务导入；JSON 导出 27 张表的序列化/反序列化成本高 |
| `sync_log` 触发器而非时间戳 | 不依赖时钟；`seq` 单调递增天然有序；缺 `updated_at` 的表也能追踪 |
| 密钥不同步 | `api_key` 绝不离开设备；每台设备独立配置 provider |
| 队列状态不同步 | `analysis_jobs` 等是设备本地运行状态，另一台设备会自己重新排队 |
| 三传输：局域网 P2P / 盲中继 / 文件 | 日常同步不出局域网；漫游走自托管盲中继（端到端加密，中继只见密文）；文件是终极兜底与备份。同一套增量协议三个传输 |
| `INSERT OR IGNORE` 幂等导入 | 重复导入同一文件无害 |
| `origin_device_id` 预留 | 为未来双向同步的向量时钟留口子，但本方案不实现 |

## 未来扩展：双向同步的路径

如果单向同步验证后用户要求双向（两台设备都能写），需要：

1. **冲突检测**：`sync_log.origin_device_id` + 向量时钟（每台设备记录"我见过 device X 的 seq Y"）
2. **冲突解决策略**：
   - `events` append-only：无冲突（UUID 不冲突）
   - `wiki_pages` 有 `human_edited_at`：人工编辑优先，AI 生成让步
   - `conversations.messages` append-only：无冲突
   - `todos.status` 状态机：最后写入胜出（LWW）或显式提示用户
3. **密钥同步**：用户显式导出加密的 provider 配置（密码保护），或每台设备独立配置
4. **删除传播**：`sync_log` 的 DELETE 操作需要传播（当前方案导入端忽略 DELETE）

双向同步的复杂度是单向的 5 倍以上（冲突解决、删除传播、密钥、网络分区），本方案的 `sync_log` + `origin_device_id` 是它的前置，但**不建议在单向验证稳定前启动**。

## 实现量估算

| 文件 | 改动 |
|---|---|
| `migrations/04-sync/up.sql` | 建 `devices`、`sync_log` 表 + 所有需同步表的触发器 + 单 writer 唯一索引（~120 行 SQL） |
| `src/storage/sync.rs`（新） | 增量协议：`build_increment()`（seq 之后的增量行）、`apply_increment()`（事务写入，三传输共用入口）、`device_id()`、清理水位（~250 行） |
| `src/sync/transport_p2p.rs`（新） | 地址缓存直连 + mDNS 定向询问（仅应答已知 device_id）+ TLS-PSK 会话 + 流式收发（~300 行） |
| `src/sync/transport_relay.rs`（新） | 中继客户端：WebSocket 连接、邮箱取送、relay_cred 验签（~200 行） |
| `relay/worker.ts`（新，独立部署） | Cloudflare Worker + Durable Objects：邮箱存储（TTL/配额）、信封验签、路由（~250 行 TS） |
| `src/sync/transport_file.rs`（新） | 导出/导入文件、回执文件、export_id 游标（~200 行） |
| `src/sync/binding.rs`（新） | 扫码绑定（二维码生成/解析、一次性 token、共享密钥 + relay_cred 派生/存储/撤销）（~140 行） |
| `src/api/sync.rs`（新） | 暴露到 bridge（~80 行） |
| `ui/lib/screens/settings_screen.dart` | 设备列表 + 在线状态 + 扫码绑定 + 中继配置 + 导出/导入按钮（~250 行） |
| `src/storage/mod.rs` | `device_id` 初始化与读取（~20 行） |

总计 ~1710 行（含中继 Worker ~250 行 TS），不改现有表结构（只加新表与触发器），不改现有写入路径（触发器透明）。交付顺序建议：先文件兜底路径（~650 行，最小可用），再局域网 P2P（+~720 行），最后中继（+~450 行）——每一步都独立可用，后一步是前一步的加速路径。

## 验收场景

| 场景 | 输入/条件 | 预期 |
|---|---|---|
| 首次导出 | 设备 A 有 100 条事件 | 导出文件包含全部 100 条，`from_seq=0, to_seq=100` |
| 增量导出 | 上次导出后新增 10 条 | 导出文件只含 10 条，`from_seq=100, to_seq=110` |
| 首次导入 | 设备 B 空库 | 100 条事件全部导入，ID 不变 |
| 重复导入 | 设备 B 已导入过同一文件 | `INSERT OR IGNORE` 跳过，无重复 |
| 导入端本地也有数据 | 设备 B 本地记录了 5 条事件 | 导入后设备 B 有 105 条（5 本地 + 100 导入），ID 不冲突 |
| 密钥隔离 | 设备 A 配置了 provider | 导出文件不含 `ai_provider_configs`，设备 B 需独立配置 |
| 时钟漂移 | 设备 A 时间比设备 B 快 1 小时 | 同步不受影响（`sync_log.seq` 与 `export_id` 均不依赖时间戳） |
| 导入后导入端写入 | 设备 B 导入后新记录事件 | 新事件只在设备 B，不会传回设备 A（单向） |
| writer 移交 | 设备 A 移交 writer 给设备 B | B 重建 `sync_log`（首批全量导出）；reader 按 `(writer_id, seq)` 二元组分别记忆确认进度，清理水位不错乱 |
| reader 长期离线 | reader 一周未导入，目录累积 7 个 export 文件 | 按 `export_id` 顺序逐个导入；writer 清理水位被该 reader 卡住，数据不丢 |
| export_id 跳号 | reader 上次导到 10，目录里最小的是 15 | 报警并请求全量导出，不静默跳过（否则永久缺失 seq 11–14） |
| 死设备不卡清理 | reader 被撤销后再无回执 | `status='active'` 过滤后不再参与水位计算，sync_log 正常清理 |
| 扫码绑定 | 手机扫 writer 屏幕上的二维码 | token 校验通过后写入 devices 表 + 建立共享密钥，同一会话完成全量同步；二维码 60 秒过期、一次性 |
| 网络上有其他 elsewhen 用户 | 同一局域网有 10 台未绑定的 elsewhen 设备 | writer 不广播、不响应未绑定设备；陌生人发起任何连接在 devices 表检查处即断开，无确认框可被骗 |
| IP 变更后重连 | writer 换了 IP | reader 先试记住的地址，失败后退到 mDNS 定向询问（只有目标 device_id 应答） |
| P2P 握手断档 | reader 报的 acked_seq 大于 writer 当前 seq（writer 重装过） | writer 识别断档，回退为全量流式发送，不静默跳号 |
| P2P 会话中断 | 传输中途 wifi 断开 | reader 事务回滚，acked_seq 不前进；下次会话从中断处重传（幂等） |
| 撤销后 P2P 连接 | 被撤销设备发起同步连接 | TLS-PSK 握手失败（密钥已删），硬失效 |
| 漫游场景（已配中继） | 出差一周，手机在 4G 下打开 app | 经中继邮箱取走端到端加密的增量包，走同一 apply_increment()；中继只见密文与元数据 |
| 漫游场景（未配中继） | 出差一周，手机与电脑不在同一网络 | 漫游路径不可用；用文件导出兜底（网盘/U盘），回导后走同一 apply_increment() |
| 中继重放攻击 | 恶意中继重放上周的密文包 | reader 拒绝 ≤ acked_seq 的包（seq 单调性），重放无效 |
| 中继挂掉 | 中继不可达 | 仅漫游路径失效；局域网直连与文件路径不受影响 |

## 待决问题

| 编号 | 问题 | 候选 |
|---|---|---|
| Q1 | 导出文件的命名与组织 | A. 单文件覆盖（`elsewhen-export.db`）；B. 时间戳多文件（`export-20260930-143000.db`）。倾向 B，保留历史 |
| Q2 | 导入端是否只读 | A. 硬约束（UI 禁用编辑）；B. 软约束（允许编辑但不同步回）。倾向 B，不限制用户但明确语义 |
| Q3 | ~~传输方式~~ | **已定（2026-09-30）**：三传输——局域网 P2P 为主（扫码绑定 + writer 零广播 + TLS-PSK）、自托管盲中继（§6.3，漫游）、文件导出兜底；不内置 WebDAV/网盘客户端，不做 NAT 打洞（成功率 ~80% 且需协调服务器，由中继替代） |
| Q4 | ~~`sync_log` 清理策略~~ | **已定（2026-09-30）**：C，导入端确认后清理（3.1 节，`acked_seq` 最小值为水位） |
| Q5 | writer 是否硬限制 1 个 | A. 数据库层限制（唯一部分索引）；B. 应用层提示。倾向 A（见 0.2） |
| Q6 | ~~同步密码必选还是可选~~ | **已定（2026-09-30）**：P2P 路径用配对会话下发的同步密钥（TLS-PSK），无需密码；密码仅用于文件兜底路径，可选，UI 明示无密码的风险（见 0.3/4.1） |
| Q7 | ~~reader 回执频率~~ | **已定（2026-09-30）**：P2P 路径 ack 会话内即时返回，无回执文件；仅文件兜底路径用回执文件，每次导入后生成（见 0.4） |
| Q8 | 中继由谁运营 | A. 用户自托管 Cloudflare Worker（本方案倾向）；B. elsewhen 官方运营默认中继。倾向 A：避免 elsewhen 变成服务提供商；代价是漫游有部署门槛，UI 需如实说明为可选功能 |
| Q9 | 中继邮箱 TTL 与配额 | 默认 TTL 7 天、每设备配额待定（建议 50MB，个人增量同步远低于此）；超限行为：writer 侧提示"reader 超过 TTL 未取件，需重连后走全量" |
