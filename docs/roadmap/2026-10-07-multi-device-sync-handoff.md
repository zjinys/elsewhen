# 多设备同步开发交接（2026-10-07）

> **结论：复制、安全和恢复底座已相当完整，最新切片做到 owner 邀请与明确审批的开发预览；完整双端 App 同步尚未闭环，不能用于生产数据。下一步先补验最新切片，再实现 joiner 独立暂存库的原生握手及后续自动交接。**

## 1. 交接范围与阅读顺序

本文是 [2026-10-03 多设备同步 roadmap](2026-10-03-multi-device-sync-roadmap.md) 的**开发接手快照**，不新建一套功能路线，也不替代其验收勾选。

- **整理日期：** 2026-10-07。
- **代码基线：** 会话已知分支 `feat/ai-chat-experience`，HEAD `c339def`。同步实现大量存在于未提交及未跟踪文件中；**只检出该 commit 不能复现本文状态**。接手先检查实际工作区，保留其他任务的改动。
- **本次核查：** 阅读路线图、设计 Note、backlog、relay 说明及关键 Rust/Flutter 源码、测试；未重新运行 Rust、Flutter、Worker、原生密钥库或真实设备验收。下文通过数均为路线图中的历史记录，不是本次新结果。
- **本次限制：** 工具服务返回 `temporarily unavailable`（server error / connection failed），补充 shell 检索及相对链接/空白自动校验未执行成功；已通过文件读取核对关键接入点，不把工具失败当成代码失败，也不宣称做过全仓审计。

建议按以下顺序阅读：

1. 本文 §2–§5：现在到哪里、接下来做什么。
2. [同步 roadmap](2026-10-03-multi-device-sync-roadmap.md)：S01–S06 原始出口与最新完成记录。
3. [同步设计 Note](../notes/proposed/architecture/2026-09-30-multi-device-sync.md)：协议、不变式及已否决方案；整体仍为 `Status: in-progress`。
4. [relay README](../../relay/README.md)：权限、协议与本地 Worker 测试入口。
5. [个人认知主循环总路线](2026-09-17-personal-cognition-main-loop-roadmap.md) 与 [proposal backlog](../proposal-backlog.md)：后续交付须同步更新的索引。

## 2. 当前实现状态：不要按勾选数计算完成率

| 切片 | 已有实现／证据 | 仍不能宣称完成的原因 |
|---|---|---|
| S00 契约基线 | 设计修订、对象范围、路线关联、独立 Node/OpenSSL 信封固定向量及 Rust 验证 | 已完成的是契约基线，不是同步产品验收 |
| S01 领域复制 | v16；事件统一写入核心与操作/outbox 原子提交；普通对话/消息及元信息版本；重复、乱序、缺依赖、冲突、回执的隔离库测试 | 路线图若干大项尚未勾选，不能据此重做已有底座；仍需逐项核对所有入口、失败状态与首版出口，不开放未经授权的内部原语 |
| S02 配对与安全 | Noise 配对、ChaCha20-Poly1305 + Ed25519、方向/路由绑定；本机 vault；固定 owner 签名名册；后审批加密检查点、临时投递恢复；relay v2 权限与实际 Worker 回归 | joiner App/原生握手、双端网络授权协调及原生平台签收未闭环；owner 预览仅是部分入口 |
| S03 快照与恢复 | v17/v18；一致分块快照、完整 Provider、超单信封增量分块、密文暂存、持久重试、应用回执、有界分页取件 | App 自动调度、成员保留窗口、TTL/配额后的补发或重快照、GC/tombstone 生命周期、受支持备份恢复未完成 |
| S04 App 与执行归属 | 本机准备/解锁/锁定；工作区租约；owner 邀请/预览/明确批准；Provider 静态保护和编辑 token；后台领取防线；分析终态和回复原输入关联；独立数据集目录册 | joiner、安装证明/名册/快照自动交接、选择/修复 UI、生命周期调度、完整冲突审阅和加工移交仍缺失 |
| S05 完整知识对象 | 未开始；现有版本/依赖设施可作为接入基础 | Wiki/来源/修订/证据/依赖闭包、待办/目标/规则冲突以及知识页专属会话均未交付 |
| S06 生命周期与发布 | 有撤销与本机执行防线等底层原语 | 完整设备管理、正常加工移交、升级/恢复和真人设备发布门禁未完成 |

**首版范围仍是：** 原始记录、普通对话及关联/元信息、分析结果、完整 Provider 配置。完整 Wiki/待办/规则另依赖 S05；首版通过也不能顺带勾选完整知识同步。

### 当前文档存在的状态漂移

这些是接手时需要核对的地方，不是本次已补验的结论：

- roadmap 顶部“当前实现基线”仍写 Rust **583**、Flutter **306**；同页最新“owner 邀请与明确审批入口”记录已经是 Rust **588**。以最新完成记录判断切片时间，**不能把旧 Flutter 全量当成最新切片回归**。
- 顶部“下一代码步骤”仍泛称 App 邀请/预览待接；owner 开发预览已存在，真正缺的是 **joiner 与审批后自动交接**。
- S01、S02、S03 和 Provider 部分未勾选项同时包含“已有底层实现”和“尚缺端到端验收”。先拆清子条件再更新证据，不因看到空框就从零实现，也不因底层存在就整项勾选。
- relay README 与设计 Note 中保留了旧基线、“拟新增”位置和早期边界描述。以源码确认存在性、以 roadmap 最新完成记录确认历史验证、以原始出口判断是否交付，三者不能互相替代。

## 3. 最近停在哪里

### 3.1 owner 侧已经接通的路径

```text
本机准备／解锁，确认系统保存的 vault 修订
  → 明确同意未来共享 Provider Key
  → 输入 relay 地址，创建 owner 邮箱并注册短期 sender
  → 注册成功后才展示可复制邀请码
  → 有界检查加入请求，显示设备及公钥指纹
  → 用原展示 token + 明确同意进行批准
  → 批准结果进入加密 vault；运行时暂停
  → 原生存储保存修订 → 回读 → Rust 确认
  → 发送已持久化审批响应
  → 等待新设备安装证明【App 自动流程尚未接通】
```

关键文件：

- [Rust FRB 接口](../../src/api/sync.rs)：`create_owner_sync_invitation`、`inspect_owner_sync_invitation`、`approve_owner_sync_invitation`、`send_owner_sync_approval`、`cancel_owner_sync_invitation`。
- [邀请运行时](../../src/sync/runtime/invitations.rs)：`issue/poll/approve/send_approval/cancel`；只有已解锁签名 owner 可以使用。网络调用不持全局 mutex，但持工作区文件租约，迟到回调不能复活旧会话。
- [Flutter 控制器](../../ui/lib/sync/sync_setup_controller.dart) 与 [owner 配对扩展](../../ui/lib/sync/sync_setup_pairing.dart)：明确审批 → 原生修订确认 → 发送，顺序不能交换。
- [设置入口](../../ui/lib/widgets/local_sync_settings.dart) 与 [owner 配对 UI](../../ui/lib/widgets/owner_sync_pairing_settings.dart)。`ELSEWHEN_SYNC_PREVIEW` 是 Dart 编译时开关（`--dart-define=ELSEWHEN_SYNC_PREVIEW=true`），不是设置 shell 环境变量就会生效，也不是生产发布开关。

**用户界面现在不应显示“同步成功”：** `local_ready` 仅表示本机密钥和运行时已准备；审批投递成功仅表示 relay 接收，不代表新成员安装、授权、初始快照或应用回执完成。

### 3.2 已有的审批后恢复，不要重新设计

[配对存储](../../src/storage/sync/pairing.rs) 已提供：

- `approve_sync_pairing`：owner 持久保存已经明确批准的响应及方向密钥。
- `install_sync_pairing_join`：在**无独立业务数据的全新暂存库**安装批准结果、固定 owner 授权根；此时 joiner 仍不活动。
- `confirm_sync_pairing_installation`：owner 验证安装证明，密钥、签名名册、来源/outbox 与完成回执同事务提交。
- `finish_sync_pairing_join`：joiner 验证名册、启用 Provider 静态保护并完成本地配对记录；显式要求本机 master 已绑定到 Store。
- `sync_pairing_dispatch`、`sync_pairing_statuses`：恢复限定方向/到期的临时投递能力，以及不含秘密的恢复清单。

[owner 配对运行时](../../src/sync/runtime/pairing.rs) 已将部分存储步骤接入租约和原生修订确认。**这些内部函数存在，不代表对应 FRB、joiner 控制器和网络调度已经存在。** 当前公开同步 API 仍主要是本机准备及 owner 邀请。

两类重启语义必须保留：

- **未批准：** 邀请 PSK、Noise 中间态只在内存，重启后重新邀请/审阅；不要为“恢复体验”把它们明文持久化。
- **已批准：** 从加密 vault 恢复原响应/安装证明/临时投递能力，按原字节重发；不重新批准，不生成新 nonce/新方向密钥来冒充同一次恢复。输出前仍须确认当前原生保护修订，过期或已知撤销时拒绝。

### 3.3 现有测试不等于 App 自动协调

[实际 Worker 多库配对测试](../../src/sync/transport_relay_smoke_tests/membership.rs) 仍由测试代码显式安排邮箱创建、临时回程授权、双方 vault 修订、安装证明、正式方向 sender 授权与临时权限撤销。

它是后续状态机的时序参考，**不是可直接暴露给 Flutter 的生产流程**。特别是测试直接传入修订/密钥和调用内部 Store，不等于设备完成了原生存储保存、回读和用户审批。当前 Flutter `pendingPairingId` 也只是内存中的同次会话重发入口；跨 App 重启的恢复列表及交互尚需接通。

## 4. 推荐接手顺序与每步出口

以下是实现建议，全部属于原 S01–S06，不改变原发布门禁。

### P0 — 先收尾最新 owner 切片的验证

1. 检查工作区、未跟踪文件和最新完成记录，确认没有丢掉最近新增的邀请运行时/UI/测试。
2. 补跑格式检查、定向 analyze、owner 与本机准备相关测试，再跑最终 release 对应的 Flutter 全量；API 或生成物有变化时按项目规则重生成桥接。
3. 记录实际结果并同步 roadmap 顶部摘要。全量 lint 的既有问题与新增问题分开写，不能只给定向通过结论。

**出口：** 2026-10-07 owner 切片有可复现的最终验证基线，待验证和工具失败清楚标注；不因此勾选完整 S02/S04。

### P1 — joiner 独立暂存库 + 原生握手（下一个代码切片）

先做一个可独立验收的最小闭环：

1. 为“加入现有数据集”提供明确入口，校验邀请及短期/一次性约束，说明会获得完整 Provider 和 API Key。
2. 复用 `DatasetCatalog::create_staging/path_for`，为加入预留独立目录与安装身份；当前默认库及其 Provider 保持不变。
3. 在原生存储创建 joiner 自己的 master，并预留初始修订；以明确的暂存目标/session 管理后续操作。**不要复用创建新 owner 的流程给 joiner 建一个独立数据集，更不能临时改 `AppConfig::pin` 指向暂存库。**
4. 复用 `PendingJoin::begin_for_relay` 等 Noise/路由原语，协调 joiner 邮箱及临时响应方向 sender 授权，再发送加入请求；未审批重启仍按重新邀请处理。
5. 收到批准后调用 `install_sync_pairing_join`；DB 提交 → 原生修订保存/回读 → 确认完成后，才允许输出安装证明。
6. 对取消、锁定、关闭、网络响应丢失、旧回调、原生写入失败设计明确状态。新端在名册/快照未完成前不可当作普通活动数据集使用，也不能领取后台任务。

**需先解决的结构问题：** 现有 runtime session、公开 FRB 和解锁路径主要围绕当前 pinned 数据库及 owner。加入流程需要业务级暂存目标身份和受租约保护的 session，不能给 UI 任意 SQLite 路径/SQL，也不能仅靠“数据库修订是多少”伪造系统保护下限。

**出口：** 隔离库 + fake native store + 本地 Worker 验证 joiner 请求→批准接收→原生持久确认→安装证明；失败不覆盖现有库，不提前激活，不发送未保护的结果。此出口尚不声称初始同步成功。

### P2 — 安装证明、正式通道、名册与快照自动交接

1. owner 自动接收并验证安装证明，复用 `confirm_pairing_installation`；新 vault 修订再次经原生保存/回读/确认，才能输出名册及后续材料。
2. 持久协调双方正式方向 sender 的注册/更新、临时 sender 撤销及响应丢失恢复。记录原 `expected_revision`、凭证、到期和目标，使用加密持久层；不要把内容密钥原值当中继凭证。
3. Worker register/revoke 已有精确 `expected + 1` 后态幂等。客户端要用原参数、新请求 nonce 重试；**不得读取新 revision 后盲目重写权限**。
4. joiner 验证签名名册，复用 `finish_sync_pairing_join` 启用本机 Provider 保护，再经原生修订确认推进；处理重复、延迟和已知撤销。
5. 接通已实现的一致快照与应用回执，初始数据完整发布前不启动普通增量应用。进度区分邀请、已批准、安装、授权、快照接收、快照应用、可选择。
6. 将不含秘密的检查点清单接入 App 恢复流程，不依赖控件内存保存上次提交返回值。

**出口：** 两个隔离端走业务级入口完成入组、快照及 Provider 恢复；新端不填 Key 即可调用显式受控环回服务；每个持久边界可故障注入并恢复。原生 OS 与真人设备仍另验收。

### P3 — 数据集选择/修复和 App 自动同步

- 复用 `DatasetCatalog::prepare_selection/select_import/select_default`，以用户实际看到的目录册/快照/vault 版本提交选择；仅下次启动生效。
- 已选数据集丢失、损坏、身份不符时明确提示修复；不静默回默认库，不搬动打开的 SQLite/WAL。
- 接入应用启动、恢复前台、网络恢复的有界同步 worker，复用持久重试和收件/应用回执，而不是另造一个无状态轮询器。
- 网络 worker 必须检查授权根、本机活动成员、当前已确认的 vault 修订、可信方向路由及工作区租约。不得只凭 `sync_identity` 存在便调用内部 engine。
- 状态页显示范围、待同步、上次成功与异常；到达记录/有效结果后刷新相关视图。缺失 Wiki 引用明确不可用，旧提议不能变成执行按钮。

**出口：** 两端离线写入、重启补齐、视图刷新和锁定/恢复可重复验证；只有指定加工设备领取后台任务。现有 `RustBridgeRepository.localSync.onReady` 唤醒的是分析 worker，不是已经完成的网络同步 worker。

### P4 — 首版剩余语义与发布恢复门禁

- S01 所有记录来源、普通对话生命周期、缺口/未知版本及隔离队列逐项验收。
- Provider dirty/bundle 和加密投递已存在；补完整冲突掩码审阅、原展示版本确认、轮换/删除/激活端到端及不可达地址提示，不静默改 URL 或外部探测。
- 补 S03 的 active 成员保留窗口、TTL/满配额、长离线重快照、GC、tombstone 与备份恢复。三个确认分开：**relay 收到 ≠ 对端已应用 ≠ 允许 GC**。
- 补 S04/S06 的正常加工移交：旧端停领与收尾、交接完整状态、新端确认接管。没有旧端确认时不自动提升另一设备；本地 epoch/锁不是分布式互斥。
- 补撤销、诊断、升级与真实桌面+移动端验收。撤销同步权限不能收回已复制的 Provider Key，必须提示用户在 Provider 侧吊销/轮换。

**出口：** S01–S04 与 S06 的首版门禁逐项有证据；不以一次 smoke 或测试数量替代真人签收。

### P5 — S05 完整知识同步

在首版记录/对话链路稳定后再扩展 Wiki/来源/修订/引用/依赖闭包、待办/目标/规则和知识页专属对话。保留精确证据身份、删除/编辑冲突与原展示版本审阅；不能先复制正文、再把缺失证据假装补齐。完成后另走 S06“完整知识同步门禁”。

## 5. 代码导航与不能突破的边界

### 5.1 优先阅读的实现

| 任务 | 代码位置 |
|---|---|
| 本机 session、文件租约、原生修订确认 | [runtime.rs](../../src/sync/runtime.rs)、[runtime/pairing.rs](../../src/sync/runtime/pairing.rs) |
| owner 邀请与原 token 审批 | [runtime/invitations.rs](../../src/sync/runtime/invitations.rs)、[api/sync.rs](../../src/api/sync.rs) |
| Noise、路由、后审批检查点 | [pairing.rs](../../src/sync/pairing.rs)、[pairing/journal.rs](../../src/sync/pairing/journal.rs)、[storage/sync/pairing.rs](../../src/storage/sync/pairing.rs) |
| 主密钥适配与 Flutter 生命周期 | [sync_key_store.dart](../../ui/lib/sync/sync_key_store.dart)、[sync_setup_controller.dart](../../ui/lib/sync/sync_setup_controller.dart)、[sync_setup_pairing.dart](../../ui/lib/sync/sync_setup_pairing.dart)、[sync_pairing_api.dart](../../ui/lib/sync/sync_pairing_api.dart) |
| 独立数据集与启动选择 | [datasets.rs](../../src/sync/datasets.rs)、[config.rs](../../src/config.rs) |
| 领域事务、消息/输入复制 | [storage/sync/mod.rs](../../src/storage/sync/mod.rs)、[events.rs](../../src/storage/events.rs)、[conversations.rs](../../src/storage/conversations.rs)、[protocol.rs](../../src/sync/protocol.rs) |
| 加密投递与快照/大记录 | [engine.rs](../../src/sync/engine.rs)、[engine/snapshot.rs](../../src/sync/engine/snapshot.rs)、[storage/sync/snapshot.rs](../../src/storage/sync/snapshot.rs)、[storage/sync/delivery.rs](../../src/storage/sync/delivery.rs)、[fragments.rs](../../src/sync/fragments.rs) |
| 成员授权、后台归属 | [membership.rs](../../src/sync/membership.rs)、[storage/sync/membership.rs](../../src/storage/sync/membership.rs)、[storage/sync/processing.rs](../../src/storage/sync/processing.rs) |
| Provider bundle、静态加密、编辑基底 | [storage/sync/providers.rs](../../src/storage/sync/providers.rs)、[provider_keys.rs](../../src/storage/provider_keys.rs)、[provider.rs](../../src/storage/provider.rs)、[api/provider_config.rs](../../src/api/provider_config.rs) |
| 实际 relay 与跨实现测试 | [transport_relay.rs](../../src/sync/transport_relay.rs)、[worker.ts](../../relay/src/worker.ts)、[rust-client.test.mjs](../../relay/test/rust-client.test.mjs)、[membership.rs 互通用例](../../src/sync/transport_relay_smoke_tests/membership.rs) |

当前业务库迁移截至 **v24**，由 [new_migrations.rs](../../src/storage/new_migrations.rs) 从目录发现：v16 复制核心、v17 快照投递、v18 增量分块、v19 本机 vault、v20 加工防线、v21 回复输入关联、v22 成员授权、v23 Provider 静态保护、v24 Provider 编辑基底。后续按实际最新序号追加，不修改历史迁移；本机 `sync-workspace.db` 目录册是独立 schema，不参与内容同步。

### 5.2 实现红线

1. 所有捕获来源仍走统一事件事务核心；离线、锁定和模型失败不能丢弃原输入，不通过重新捕获复制远端事件。
2. 同步历史/终态/提议文本不是授权；不复制可执行 run、审批 token、运行租约或流式草稿，不自动触发模型或业务动作。
3. 普通 Provider DTO 继续脱敏，导出用受限完整配置路径。显式空 Key、缺失 Key、表单空串保留旧值是不同语义，不能混用。
4. 密钥/URL 绑定和用户已见编辑 token 不可偷换。旧表单不得拼接远端新 Key 与本地旧 URL；不可达端点不能自动改地址并携带 Key 请求。
5. 原生密钥缺失、损坏、回读失败不重置、不降级明文；DB 修订不能自行充当原生保护下限。当前静态保护不承诺擦除旧 WAL、空闲页和备份中的明文。
6. 固定 dataset owner 签名授权与 relay 邮箱 owner 管理是两回事。relay 支持 owner 凭证轮换，不表示已实现数据集 owner 轮换/委派。
7. `AppConfig::pin` 进程内不变；不覆盖默认库、不自动合并独立库、不在后台切库。
8. 不把内部无授权测试兼容路径开放给生产；不把本机锁/epoch 当跨设备执行锁。
9. 不新增知识后台手动触发/重试按钮；配对明确审批不等于新增知识加工入口。
10. 不使用个人数据库、真实 Key、真实 OS 密钥库、外部模型或线上中继做自动验证；需要时另行取得授权。此交接不授权部署、提交、推送或发布。

## 6. 验证基线与待补门禁

### 6.1 最近记录的结果（不是本次重跑）

来源为 roadmap **2026-10-07「S04 owner 邀请与明确审批入口」**及同日上一条完成记录。

| 范围 | 历史结果 | 接手注意 |
|---|---|---|
| 最新 Rust 全量 | **588 passed / 11 ignored**，另配置环境 **5 passed** | 11 个 ignored 不是通过；不要将前一切片 583 当作最新数 |
| 新增 owner Rust 用例 | **5 passed** | 位于 `src/sync/runtime/invitations/tests.rs` |
| 实际 Worker 门禁 | **14 passed** | 包含隔离实际 Worker + 指定 Rust 互通；不是 14 台设备或真人 GUI 验收 |
| 最新 Flutter 定向 | 新增 owner **6 passed**，与既有定向合计 **15 passed** | fake native store，不等于原生密钥库签收 |
| Flutter 全量 | 前一切片 **306 passed** | owner 最新切片后的全量仍待补，不推算成 312 或其他数字 |
| 桥接与 release | 最新 owner 记录中 `scripts/regen.sh` 重生成并完成 release | 不沿用前几个切片记录的旧 hash；按当前生成文件/加载库核对 |
| 格式与定向 analyze | 首轮两条测试括号风格提示已修 | 修复后的 analyze 复跑、最终格式检查未在最新记录中完成 |
| 原生平台、真实双端 | **未完成** | Linux 曾构建通过不代表 Android/iOS/桌面密钥库或真实加入验收 |

### 6.2 下一轮建议执行的命令

以下是**待执行清单**。仅文档交接不要求现在运行完整测试；后续开发按影响范围执行并记录实际输出。

```bash
# 仓库根目录：先看状态，不能清理/覆盖其他任务的变更
git status --short
git diff --stat

# 只检查格式；若需修正，限定本切片文件，不顺手重排整个工作区
cargo fmt --all -- --check

# 最新 owner 与审批后恢复的定向 Rust 门禁
cargo test --lib sync::runtime::invitations::tests
cargo test --lib sync::runtime::pairing::tests
cargo test --lib storage::sync::pairing::tests

# Rust 全量（环回请求不经环境代理，仅对本次测试进程设置）
env NO_PROXY=localhost,127.0.0.1,::1 no_proxy=localhost,127.0.0.1,::1 cargo test --all-targets
cargo build --release

# 公开 Rust API 有改动时，从根目录执行并等待 release 重建完成
./scripts/regen.sh

# 实际本地 Worker；不要直接运行所有 ignored 测试或探测任意现有中继
npm --prefix relay test
```

在 `ui/` 目录运行：

```bash
# FVM 是必需的；该命令仅检查，不写入文件
fvm dart format --output=none --set-exit-if-changed \
  lib/sync lib/widgets/local_sync_settings.dart \
  lib/widgets/owner_sync_pairing_settings.dart \
  test/sync_owner_pairing_test.dart test/support/sync_setup.dart

fvm flutter analyze lib/sync \
  lib/widgets/local_sync_settings.dart \
  lib/widgets/owner_sync_pairing_settings.dart \
  test/sync_owner_pairing_test.dart test/support/sync_setup.dart

fvm flutter test test/sync_owner_pairing_test.dart \
  test/sync_key_store_test.dart test/sync_setup_controller_test.dart \
  test/local_sync_settings_test.dart test/sync_setup_bridge_test.dart \
  test/sync_worker_pause_bridge_test.dart

# 必须使用与当前生成绑定匹配的最终 Rust 动态库
fvm flutter test
fvm flutter analyze
```

验证纪律：

- 真实 bridge 统一使用 [isolated_bridge.dart](../../ui/test/support/isolated_bridge.dart)，关闭非必要后台 worker，原生存储用 fake。多端先用 Rust 多临时库；真实 App/bridge 端到端按独立进程与 pinned 工作区设计，不能在同一运行会话里偷换全局库路径。
- [relay/test/rust-client.test.mjs](../../relay/test/rust-client.test.mjs) 会启动隔离本地 Worker，设置专用测试 URL 后运行明确指定的 ignored 用例；普通 `cargo test` 不等于已经跑过它。
- 历史全量 Flutter analyze 有既有项目/第三方告警；历史 `git diff --check` 曾报 `ui/test/fixtures/wiki_md/real-tw-star.md:45` 尾随空白。它们是待核对的历史边界，不是本次重现，也不授权修改其他任务文件。
- 不因工具连接错误、测试尚未启动或子进程未返回便写“通过”。记录命令、日期、范围、通过/失败/ignored 与未执行原因。

## 7. 后续交付时怎样维护文档

每完成一个代码切片，在同一变更中：

1. 更新 [同步 roadmap](2026-10-03-multi-device-sync-roadmap.md) 对应子项、顶部基线/下一步与完成记录；有代码不等于出口通过。
2. 更新 [总路线](2026-09-17-personal-cognition-main-loop-roadmap.md) 和 [proposal backlog](../proposal-backlog.md) 的日期、证据与剩余范围。
3. 同步 [设计 Note](../notes/proposed/architecture/2026-09-30-multi-device-sync.md) 的相关决策/边界及整体状态；未完成前保持 in-progress，不为这份交接另立同步提案。
4. 将本文作为接手快照保留；后续最新状态仍以 roadmap 为准。若新建交接，互链并明确替代日期，避免多个“当前状态”互相冲突。

### 可直接给下一位开发者的任务摘要

> 阅读本交接与同步 roadmap 最新完成记录。先补验 owner 邀请切片的格式、analyze、Flutter 全量及桥接一致性，再设计并实现 S02/S04 的 joiner 独立暂存库原生握手；复用现有 Noise、加密后审批检查点和目录册，不重做底座。随后接 owner 安装证明、正式 relay 授权、签名名册与初始快照的可恢复协调。保持原展示 token、原生修订确认先于输出、固定 AppConfig 路径、后台单加工设备及不自动执行历史动作。所有自动验证只用临时库、合成 Key、fake 原生存储与显式本地 Worker；不要动个人数据、线上服务或其他任务改动。每片完成立即更新 roadmap、总路线、backlog 和设计 Note，真实设备门禁继续单列。
