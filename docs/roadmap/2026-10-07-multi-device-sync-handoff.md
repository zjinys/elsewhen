# 多设备同步开发交接（2026-10-07）

> **当前（2026-10-11）：P1 已验收；P2 工程与自动化验收已完成（原生平台与真人设备门禁单列）。新 session 先读 §12。** 2026-10-11 收尾复验：P2 Rust 定向 **16 passed**、Rust 全量 **640 passed / 16 ignored**（另配置环境 **5 passed**）、实际 Worker **15 passed**、桥接 hash 两侧 **233902080**、Flutter 同步相关定向 **48 passed**／全量 **345 passed**、P2 定向 analyze **No issues found**、P2 Rust 23 文件与 Dart 10 文件格式通过、`git diff --check` 通过。经用户明确要求，全仓 Rust 格式例外已于 10-11 修复（`cargo fmt --all`，含历史 3 个与并行文件，复验全量 640 passed 与 release）；仅剩 Flutter analyze 71 项全 info 单列（详见 §12）。§8/§9 的 600/323/14 是 P1 历史基线。不进入 P3。

## 1. 交接范围与阅读顺序

本文是 [2026-10-03 多设备同步 roadmap](2026-10-03-multi-device-sync-roadmap.md) 的**开发接手快照**，不新建一套功能路线，也不替代其验收勾选。

- **整理日期：** 2026-10-07；最后更新 2026-10-10。
- **代码基线：** 分支 `feat/ai-chat-experience`，工作区已由用户完整提交为 `6743a09`（`chore: 保存多设备同步工作区与交接基线`）；提交后检查工作区干净。P0/P1 历史验证见 §8/§9，P2 当前补验与接手断点见 §12，原批准方案见 §10。本会话未新增提交，实际 HEAD 和并行改动需接手时核对。
- **首次交接核查：** 阅读路线图、设计 Note、backlog、relay 说明及关键 Rust/Flutter 源码、测试；首次编写时未重跑测试。交接前历史结果保留在 roadmap 完成记录中，P0 实测结果见 §8，P1 验收见 §9，**P2 最新结果以 §12 为准**，不把历史通过数冒充当前回归。
- **工具限制：** 自动安全检查服务间歇返回 `temporarily unavailable`（server error / connection failed / rate-limited）；失败发生在命令执行前时，不计作代码失败或测试通过。首次交接的自动链接校验未执行成功，本轮补验逐项记录实际执行状态。

建议按以下顺序阅读：

1. **先读 §12：P2 最新实测、Flutter 修正与剩余静态门禁。** §11 保留 2026-10-09 接续与统一验证脚本，§10 保留已批准方案和更早断点；§2–§5 是 P2 之前的架构/任务背景，§8/§9 是 P0/P1 历史验证。
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

### 文档状态漂移的处理

首次交接时，roadmap 顶部还停留在 Rust 583 / Flutter 306，下一步也泛称 App 邀请/预览尚未接入。随后 P0 补验确认 Rust 588 / Flutter 312；2026-10-08 P1 验收进一步确认 Rust 600 / Flutter 323 和新的实际 Worker joiner 流程通过。当前下一代码步骤已同步为 **P2 安装证明、正式通道、名册与快照自动交接**；历史完成记录保留当时结果，不回写成新一轮通过。

仍需注意：S01、S02、S03 和 Provider 部分未勾选大项同时包含“已有底层实现”和“尚缺端到端验收”。先拆清子条件再更新证据，不因看到空框就从零实现，也不因底层存在就整项勾选。relay README 与设计 Note 保留了部分旧基线、“拟新增”位置和早期边界说明；以源码确认存在性、以最新完成记录确认验证、以原始出口判断是否交付。

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

[owner 配对运行时](../../src/sync/runtime/pairing.rs) 已将部分存储步骤接入租约和原生修订确认。**P1 已接通 joiner 的 FRB、控制器、原生修订确认和证明投递/恢复，见 §9。** owner 自动接收安装证明、正式通道/名册/快照协调仍属 P2，不能因底层函数存在而视为已接通。

两类重启语义必须保留：

- **未批准：** 邀请 PSK、Noise 中间态只在内存，重启后重新邀请/审阅；不要为“恢复体验”把它们明文持久化。
- **已批准：** 从加密 vault 恢复原响应/安装证明/临时投递能力，按原字节重发；不重新批准，不生成新 nonce/新方向密钥来冒充同一次恢复。输出前仍须确认当前原生保护修订，过期或已知撤销时拒绝。

### 3.3 现有测试不等于 App 自动协调

[实际 Worker 多库配对测试](../../src/sync/transport_relay_smoke_tests/membership.rs) 仍由测试代码显式安排邮箱创建、临时回程授权、双方 vault 修订、安装证明、正式方向 sender 授权与临时权限撤销。

它是后续状态机的时序参考，**不是可直接暴露给 Flutter 的生产流程**。特别是测试直接传入修订/密钥和调用内部 Store，不等于设备完成了原生存储保存、回读和用户审批。owner Flutter 的 `pendingPairingId` 仍是内存中的同次会话重发入口；owner 恢复列表及自动交接仍需后续接入。joiner 的目录册恢复入口和原生修订握手已在 P1 单独实现并验证，不再属于未接入项。

## 4. 推荐接手顺序与每步出口

以下是实现建议，全部属于原 S01–S06，不改变原发布门禁。

### P0 — 最新 owner 切片的补验（已完成，全仓质量例外单列）

- [x] 确认完整工作区基线提交 `6743a09`，保留其他任务变更。
- [x] 修正 owner 的 5 个 Rust 文件格式；owner Rust/Dart 格式与定向 analyze 通过，Rust 定向 5/3/10、Flutter owner 定向 6/同步相关定向 25 项通过。
- [x] release 重建后，Flutter 全量 312 passed；Rust 全量 588 passed / 11 ignored（另配置环境 5 passed），实际 Worker 14 passed。
- [x] 核对并记录全仓格式 3 个既有差异和全量 analyze 78 项问题；更新本交接、同步 roadmap、总路线及提案进度，不将非零退出写成通过。

**出口记录：** §8 保存完整结果、日志来源和未通过项。当前可进入 P1 的工程开发；S02/S04/S06 总项与真人设备发布门禁仍未完成。

### P1 — joiner 独立暂存库 + 原生握手（工程与自动化验收已完成）

2026-10-08：已核对 `/tmp/elsewhen-joiner-validation.6fu81z`，P1 格式/定向 analyze、Rust/Flutter 回归与实际 Worker 新流程通过，详见 §9；全仓静态检查例外与真人设备门禁单列。下一代码步骤为 P2。

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

### P3 — 数据集选择/修复和 App 自动同步（工程与自动化验收已完成，见 §13）

- 复用 `DatasetCatalog::prepare_selection/select_import/select_default`，以用户实际看到的目录册/快照/vault 版本提交选择；仅下次启动生效。
- 已选数据集丢失、损坏、身份不符时明确提示修复；不静默回默认库，不搬动打开的 SQLite/WAL。
- 接入应用启动、恢复前台、网络恢复的有界同步 worker，复用持久重试和收件/应用回执，而不是另造一个无状态轮询器。
- 网络 worker 必须检查授权根、本机活动成员、当前已确认的 vault 修订、可信方向路由及工作区租约。不得只凭 `sync_identity` 存在便调用内部 engine。
- 状态页显示范围、待同步、上次成功与异常；到达记录/有效结果后刷新相关视图。缺失 Wiki 引用明确不可用，旧提议不能变成执行按钮。

**出口：** 两端离线写入、重启补齐、视图刷新和锁定/恢复可重复验证；只有指定加工设备领取后台任务。现有 `RustBridgeRepository.localSync.onReady` 唤醒的是分析 worker，不是已经完成的网络同步 worker。

### P4 — 首版剩余语义与发布恢复门禁

- S01 所有记录来源、普通对话生命周期、缺口/未知版本及隔离队列逐项验收。**（P4a 已完成源隔离/idempotency 保真/未知版本容错/长离线追平，见 §14；普通对话生命周期与其余记录来源的逐项首版验收仍在 P4 继续。）**
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

## 6. 验证基线与复现命令

### 6.1 已核对的当前基线

P0 历史结果见 §8，最新 P1 完整结果、日志与例外见 §9。当前为 Rust **600 passed / 11 ignored**（另配置环境 **5 passed**）、Flutter joiner 定向 **11 passed**／同步相关定向 **36 passed**／全量 **323 passed**、实际 Worker **14 passed**（含新 joiner 流程）；release 以及 P1 格式/定向 analyze 通过。

全仓 Rust 格式检查与 Flutter analyze 分别因 3 个既有文件差异和 78 项问题返回 exit 1，仍未通过。原生密钥库、真实双端与产品发布门禁不计完成。交接前 306/302 等历史通过数留在 roadmap 的对应完成记录中，不作为当前基线。

### 6.2 后续变更的复现命令

P0 结果见 §8，P1 最终结果见 §9；下面保留供后续代码变更复现，不表示这些命令仍全部待执行。P0 只有格式调整；P1 新增公开 Rust API，已执行 `regen.sh` 并重建 release，不能省略后续 API 变更所需的桥接重生成。

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

> 阅读本交接与同步 roadmap 最新完成记录。owner P0、joiner P1 与 P2 自动交接的工程/自动化验收均已完成（结果与全仓质量例外见 §9/§12），下一步实现 P3 数据集选择/修复与 App 自动同步调度，不重做 P1 独立暂存库原生握手或 P2 证明/正式通道/名册/快照交接；复用现有 Noise、加密后审批检查点和目录册，不重做底座。保持原展示 token、原生修订确认先于输出、固定 AppConfig 路径、后台单加工设备及不自动执行历史动作。所有自动验证只用临时库、合成 Key、fake 原生存储与显式本地 Worker；不要动个人数据、线上服务或其他任务改动。每片完成立即更新 roadmap、总路线、backlog 和设计 Note，真实设备门禁继续单列。

## 8. 2026-10-07 owner 切片补验记录（P0 完成，全仓质量例外单列）

**结论：** owner 切片的格式、定向 analyze、定向测试、release/隔离 bridge、Rust/Flutter 全量及实际 Worker 回归已核对完成。全仓 Rust 格式和 Flutter analyze 仍有下面列明的问题，**不是全仓所有检查均通过，也不是 S02/S04/S06 整体或真实设备验收完成**。下一代码步骤是 P1 的 joiner 独立暂存库与原生握手。

**证据来源：** 用户执行本地验证脚本后提供 `/tmp/elsewhen-owner-validation.HPSWCT`；已直接读取 `summary.tsv`、各项测试结尾及两个失败检查的完整日志。18 个检查均已执行、没有 SKIPPED，其中 16 个 exit 0、2 个 exit 1；不把日志核对写成又重跑了一轮。该临时目录不是持久归档，关键结果在本节保留，复现命令见 §6.2。

**基线与环境：** `6743a09`，Flutter **3.47.6** / Dart **3.13.5**。本轮 owner 源码改动限于 5 个 Rust 文件的格式/模块顺序整理；无新公共 API、schema 或业务行为变更，因此未再次运行 `regen.sh`。日志中的工作区另含 `ui/lib/widgets/chat_run_status.dart` 变更，不属于本轮 owner 修改，未覆盖或归入 owner 交付。

| 检查 | 核对结果 | 日志 |
|---|---|---|
| owner Rust 格式 | 5 个文件通过，exit 0 | `owner_rust_format.log` / `summary.tsv` |
| owner 邀请 Rust 定向 | **5 passed** | `owner_rust_tests.log` |
| 审批运行时定向 | **3 passed** | `pairing_runtime_tests.log` |
| 配对存储定向 | **10 passed** | `pairing_storage_tests.log` |
| Rust 全量 | **588 passed / 11 ignored**，另配置环境 **5 passed**；保留 3 条既有测试 warning | `all_rust_tests.log` |
| release 构建 | **通过**，50.24 秒；之后才运行包含真实 bridge 的 Flutter 测试 | `rust_release.log` |
| owner Dart 格式 | **8 files / 0 changed** | `owner_dart_format.log` |
| owner Flutter 定向 analyze | **No issues found**，5 个目标，4.2 秒 | `owner_flutter_analyze.log` |
| owner Flutter 定向 | **6 passed** | `owner_flutter_tests.log` |
| 同步相关 Flutter 定向（6 个测试文件） | **25 passed**，包括 owner、密钥库 fake、本机设置/控制器及隔离 bridge | `sync_flutter_tests.log` |
| Flutter 全量 | **312 passed**，22 秒；这是 release 重建后的实测，不是从 306 推算 | `all_flutter_tests.log` |
| 实际本地 Worker | **14 passed / 0 failed / 0 skipped**；包含明确启动隔离 Worker 的 Rust 互通 | `actual_worker.log` |
| diff 空白检查 | 验证脚本执行时 exit 0；本次回填文档后再次运行 `git diff --check`，通过 | `diff_whitespace.log` / `summary.tsv`，以及本轮文档校验 |

定向集合包含于全量集合，不能把这些数字相加当作独立用例总数。生成绑定此前已静态比对为 Rust/Dart **-253006005**；本轮 release 与隔离 bridge 测试补齐了实际构建/加载验证，不沿用旧动态库冒充通过。

### 两项仍未通过的全仓检查

1. **`cargo fmt --all -- --check`：exit 1。** 只剩 `src/storage/sync/providers.rs:239`、`src/storage_tests.rs:318`、`src/sync/datasets/tests.rs:184` 的既有格式差异；owner 的 5 个文件已通过。未为清空全仓报告而修改这些非本切片文件。
2. **`fvm flutter analyze`：exit 1，78 项（4 warning、74 info，无 error）。** 位于既有 bridge/settings/wiki/test 代码和 `third_party/appflowy_editor`，包括未使用声明/导入、括号、弃用 API、测试 print 和第三方类型标注。owner 本轮定向范围无问题；未关闭 lint、修改第三方代码或将全量 analyze 记为通过。

Rust 的 3 条 warning 位于 `src/ai/tool/tests.rs` 与 `src/storage/tests.rs`。Flutter 全量日志另有测试中的 FRB 未初始化和失败路径诊断，最终仍是 312 passed；测试通过不等于日志零告警，也不等于原生 OS 密钥库签收。

**P0 结束时的下一步：** 按 P1 实现 joiner；上述全仓静态检查欠账继续显式保留。原生平台、两个真实 App、加工移交、TTL/GC、完整知识对象和发布门禁仍未完成。本轮没有使用个人数据库/真实 Key、访问线上中继、调用外部模型、部署或新增 Git 提交。

## 9. 2026-10-08 P1 实施交接（工程与自动化验收已完成）

**状态：P1 已完成工程与自动化验收，全仓质量例外单列，P2 未开始。** 已直接核对用户提供的 `/tmp/elsewhen-joiner-validation.6fu81z/summary.tsv`、测试汇总和失败检查的日志；18 项检查全部执行、无 SKIPPED，其中 16 项 exit 0、2 项 exit 1。这里记录的是用户执行后的日志核对，不冒充再次运行。临时目录可能被清理，关键结果保留如下，不能只靠临时日志宣称可复现。

### 已实现的代码

- [joiner runtime](../../src/sync/runtime/joining.rs) / [网络协调](../../src/sync/runtime/joining/network.rs)：在同一工作区租约/注册表中区分 joiner，原 token、暂存 entry/安装/修订与迟到回调均受约束。专用 native 确认不要求新端已经成为活动成员，也不执行 owner 的队列恢复逻辑。
- 复用目录册和 [vault 存储](../../src/storage/sync/vault.rs)；新增空库校验与 vault 创建的原子存储方法。原库及 Provider 保留，`AppConfig::pin` 不变，未审批的重启流程须重新邀请。
- 临时响应方向授权与 Noise 原请求在会话内精确重试；已审批结果进入现有加密检查点。原生修订保存/回读确认前不能发安装证明。恢复从原 entry 和本机 master/修订读取同一证明，不替换密钥；证明投递不等于成员已激活，P1 不应用名册或快照。
- [FRB API](../../src/api/sync.rs)、[joiner 控制器](../../ui/lib/sync/sync_join_controller.dart) 和 [设置入口](../../ui/lib/widgets/joiner_sync_pairing_settings.dart)：不联网预览、明确 Key 共享同意、两端指纹、失败关闭、等待 native 写入后释放、原证明重发及最小暂存项恢复。生命周期由现有 `SyncSetupController.close/dispose` 收尾。
- 新增 [Rust 运行时测试](../../src/sync/runtime/joining/tests.rs)、[网络状态测试](../../src/sync/runtime/joining/network/tests.rs)、[Flutter 控制器测试](../../ui/test/sync_join_controller_test.dart)、[真实隔离 bridge 测试](../../ui/test/sync_join_bridge_test.dart)，以及接入本地 Worker 的 [joiner 流程](../../src/sync/transport_relay_smoke_tests/joiner_ui.rs)。

### 最终验证结果

环境：Flutter **3.47.6** / Dart **3.13.5**。以下日志均位于 `/tmp/elsewhen-joiner-validation.6fu81z`；定向集合包含于全量，不重复累加。

| 项目 | 核对结果 | 日志 |
|---|---|---|
| P1 Rust 格式 | 9 个目标文件，exit 0 | `p1_rust_format.log` / `summary.tsv` |
| joiner Rust 定向 | **12 passed** | `joiner_rust_tests.log` |
| Rust 全量 | **600 passed / 11 ignored**，另配置环境 **5 passed**；保留 3 条既有测试 warning | `all_rust_tests.log` |
| release 构建 | **通过**，49.56 秒；其后才运行 bridge/Flutter 测试 | `rust_release.log` |
| P1 Dart 格式 | **9 files / 0 changed** | `p1_dart_format.log` |
| P1 定向 analyze | **No issues found**，7 个目标，0.9 秒 | `p1_flutter_analyze.log` |
| Flutter 新增定向 | **11 passed**，含页面卸载回归、控制器、窄屏 widget 和真实隔离 bridge | `joiner_flutter_tests.log` |
| Flutter 同步相关定向 | **36 passed**，含 owner/joiner、fake native store、本机控制器和 bridge | `sync_flutter_tests.log` |
| Flutter 全量 | **323 passed**，22 秒 | `all_flutter_tests.log` |
| 实际本地 Worker | **14 passed / 0 failed / 0 skipped**；含新 joiner 生产运行时流程 | `actual_worker.log` |
| diff 空白检查 | 验证脚本执行时 exit 0；最终 Handoff/路线图回填后再次 `git diff --check`，通过 | `diff_whitespace.log` / `summary.tsv` 与本轮文档校验 |

`regen.sh` 已完成公开 API 的 Dart/Rust 桥接重生成，静态 hash 均为 **2049820871**；最新 release 与隔离 bridge 回归通过。实际 Worker 新流程验证了原请求重发、审批后原生确认、重开检查点恢复、同一证明重发、relay 撤销后拒绝投递，以及默认库/Provider 保留；它仍是临时库与模拟 native 确认，不是两个真实 App 或 OS 密钥库签收。

**两项全仓检查继续未通过：** Rust 全仓格式仍是 `src/storage/sync/providers.rs`、`src/storage_tests.rs`、`src/sync/datasets/tests.rs` 的 3 个既有差异；Flutter 全量 analyze 为 **78 项（4 warning、74 info，无 error）**，位于既有项目/测试/第三方文件，P1 定向范围已无问题。未关闭 lint 或修改这些无关文件，不能把脚本整体称为全绿。

首轮 Rust 编译因访问私有的空库校验方法失败，改为存储层原子 `create_sync_join_vault`；随后 12 项测试因 preview 的 quiescence 检查未开事务失败，已补齐事务并在 600 项全量通过中验证。没有删测试或绕过检查。

### 下一步：P2 自动交接，不重做 P1

1. owner 接收、验证安装证明，提交授权/密钥/名册后再次完成原生修订确认。
2. 可恢复地协调正式方向 sender 授权、临时权限撤销和响应丢失重试；仍绑定原 expected revision/凭证/到期，不盲目追随新版本。
3. joiner 验证签名名册、按既有事务启用 Provider 静态保护，再接通初始快照和应用回执；成员授权、快照发布和可切换状态分开显示。
4. 数据集选择/重启切换、生命周期自动同步、加工移交、GC/S05 及真人设备验收继续按后续切片推进。

P1 的复现可使用 §6.2 命令，或本机仍保留的临时脚本（构建目录被忽略，不提交）：

```bash
bash /home/pp/playground/ai/elsewhen/ui/build/joiner-validation/run.sh
```

脚本只格式化列明的 P1 文件，使用临时库、合成 Key、fake 原生存储和显式本地 Worker；先构建 release，再跑 bridge/Flutter 测试。每项保存日志和 `summary.tsv`，release 失败会跳过依赖新库的测试。此前自动启动曾被安全检查服务阻断，现已由用户执行并完成本节核对，不再列作待执行。

- [x] P1 代码与工程/自动化验证完成，结果和全仓质量例外已记录。
- [x] Handoff、同步 roadmap、总路线、backlog 与设计 Note 同步更新。
- [ ] P2 自动交接与完整同步出口。
- [ ] 原生系统密钥库与真人多设备签收。

本轮没有新增 Git 提交、推送、部署或修改其他任务的 `chat_run_status.dart`；整体 S02/S04/S06 仍不能勾选完成。

## 10. 新 session 接手断点（2026-10-08，P2 实施中）

**本节保留 2026-10-08 的批准方案与历史断点。当前代码和下一步以 §11 为准；不要按下文的“缺失文件”重新开始。P2 方案已经批准，但尚未验收。**

### 工作区与批准方案

- 项目目录：`/home/pp/playground/ai/elsewhen`；会话的 shell 曾进入 `ui/`，运行命令时明确目录，Flutter 始终使用 FVM。
- 最后已核对的提交是 `6743a09`。此后的 P0/P1/P2 改动大多仍在工作区，还有未跟踪的新文件；本会话没有新增提交。接手先核对实际 `git status`、`git diff` 和最新 HEAD，不能只读取提交版本。
- 批准的 **P2 完整方案**在本机 `/home/pp/.claude/plans/precious-scribbling-kitten.md`，已替换旧 P1 计划。新 session 可以按该方案继续实施，不必重新做 P1 调研。
- 存在其他并行改动，不只 `chat_run_status.dart`：最近状态还包括 AI tools、knowledge authoring/API、chat_queue、storage tests/migration 发现代码、Markdown/chat UI 等；backlog 新增的来源发现问题也来自其他工作。**不要 reset/clean、覆盖整份文件或把这些改动全部归因于 P2。**

### P2 已写入的部分（尚待验证）

| 位置 | 当前内容 |
|---|---|
| `src/sync/enrollment.rs` | `EnrollmentRecord`、原始 `GrantIntent`、`SnapshotChoice/Offer` 与控制消息 ID；正式授权 7 天、固定 expected revision、有限 pending/history、最多 3 次未发布快照尝试 |
| `src/sync/pairing.rs` / `pairing/journal.rs` | `ApprovedPair` / `PairingRecord` 增加可选可信 `bootstrap`，保存原邀请 `request_sender`，不能从邮件外层 sender 猜撤销对象；旧缺字段记录不伪造信息 |
| `src/sync/vault.rs` | 可选加密 enrollment 集合、读取/修改/seed、解码校验；仍受 vault 容量限制，不复制到快照 |
| `src/storage/sync/enrollment.rs` | 校验角色/身份/方向/epoch、grant/revoke 回执落盘、快照尝试预留、名册/保护状态原子应用、offer 选择与完成确认 |
| `src/storage/sync/pairing.rs` | 在 P1 安装或 owner 完成证明时，同事务 seed enrollment；完成 joiner 名册时记录对应 wire ID |
| `src/sync/engine/enrollment.rs` | 认证控制消息、限定的发送/回执集合、选择回执先于分块、快照 manifest hash 校验、完成通知；不导入普通 op/provider/op-part |
| `src/sync/engine.rs` | 提取公共认证 payload helper，保持通用接收路径；新 enrollment 子模块 |
| `src/sync/engine/snapshot.rs` / `storage/sync/snapshot.rs` / `delivery.rs` | `stage_enrollment_snapshot`、从同一 WAL 视图读取签名名册并关联到 export；避免名册和快照来自不同时刻 |
| `src/storage/sync/wire.rs` | `due_enrollment_wires` 限定控制消息及当前快照，避免把其他业务增量提前发送 |
| `src/sync/runtime/enrollment.rs` | owner 证明接收、native commit、正式授权、快照准备/传输、清单/进度/确认；`EnrollmentLoan` 持租约，operation/revision/role fencing、输出前复查、native 写入后暂停 |
| `src/sync/runtime.rs` / `runtime/pairing.rs` / `runtime/joining.rs` / `joining/network.rs` | enrollment busy/confirmation 状态、有限 proof 等待保护、P1/P2 并发互斥、joiner 新阶段/恢复与专用确认；不切换 `AppConfig::pin` |
| `src/api/sync.rs` | 新增 `SyncEnrollmentStateDto` 和 `list_sync_enrollments`、`advance_sync_enrollment`、`confirm_sync_enrollment`；**尚未为 P2 重生成 Dart/Rust bridge** |

实施中补充的两个重要约束：

1. `SnapshotExporter` 要求 Provider dirty bundle 已为所有当前 peer 加密持久化；P2 先复用 `stage_provider_updates` **暂存而不发送**这些增量，再建立快照。
2. P2 快照同时暂存其同一读视图中的签名名册；只在名册应用回执、offer 目标选择回执完成后发分块，防止成员版本不一致或邮箱先被分块塞满。

### 明确未完成／下一 session 的第一批任务

1. **补 Rust 测试文件。** `src/sync/runtime/enrollment.rs` 已有 `#[cfg(test)] mod tests;`，但 `src/sync/runtime/enrollment/tests.rs` **尚不存在**。`cargo check --lib` 不编译这部分；直接跑测试可能先报缺模块文件。应新增真正的 P2 单元/事务/故障恢复测试，不要删声明或跳过用例来制造通过。
2. **Flutter 自动协调器尚未写完。** 本节更新时确认 `ui/lib/sync/sync_enrollment_api.dart`、`sync_enrollment_controller.dart` 未存在；对子 agent 的委派因 API 连接错误终止，没有可依赖的交付，不必等待旧 agent。
3. 实现已批准的单飞、有界、退避 tick：仅恢复已经批准的 enrollment；native 修订保存/回读后，用原 role/operation/invitation/token 确认；关闭等待 native 写入，不释放其他会话。接入 owner 审批/解锁、joiner 安装确认/恢复，以及设置进度 UI。
4. 保留 P1 fake 测试的隔离：不要因增加自动协调而让旧 fake controller 隐式调用真实 FRB。建议为自动 enrollment 提供显式开关/注入点，由生产 `RustBridgeRepository` 接通，并在新测试中使用 fake API/timer/native store。
5. 扩展实际 Worker 测试，走生产 owner + joiner 的完整 P1→P2，而不是用手工授权/导入代替新增阶段。验证丢响应、重复回执、跨重启、超过原邀请期限但已授权的恢复、offer 前禁止分块、既有库/Provider 保留。
6. 对照批准方案复查当前 Rust 代码，特别是扫描游标与坏邮件的错误分类、native 变更后停止后续输出、旧 session 回调、snapshot ID 重试、完成通知/应用回执的结束条件。**这些是待验证点，不是已通过的安全结论。**
7. 核对并完成公开 API 后运行 `scripts/regen.sh`，再跑定向 Rust/Flutter、最新 release/真实 bridge、全量 Rust/Flutter 和实际 Worker。本会话未为 P2 重生成或验证 bridge；并行会话可能改动其他公开 API 或生成物，须核对当前工作区，不能仅因生成文件存在就认定 P2 已接通。

新 DTO 的预定 Flutter 字段：`role`、`operationId`、`invitationId`、`installationId`、`revision`、`confirmationToken`、`phase`、`snapshotId`、`receivedChunks`、`totalChunks`、`localApplied`、`remoteApplied`、`notificationPending`。以当前 Rust API 和重新生成结果为准，不手写生成文件。

### 验证实况与权限边界

- P2 已实际执行过 `cargo check --lib`，最近一次 exit 0（1.72 秒）。日志：`/tmp/claude-1000/-home-pp-playground-ai-elsewhen/bed07539-db3c-4251-a678-bcfc6555e1f3/tasks/bwbtvttb5.output`。之后只又调整了 enrollment 恢复清单优先级并整理交接；且有并行改动，接手需重新检查。
- **P2 尚未执行单元/集成测试、P2 bridge 重生成、Flutter 协调器测试或实际 Worker 新完整流程。** P1 的 600/323/14 是上一阶段记录，不可挪用。
- 根目录自动安全检查服务间歇 unavailable；子 agent 三次因 `ECONNRESET` 结束。文件读写通常可用，命令/agent 失败可能发生在启动前。不要循环盲重试或伪造通过；需要时做一次范围明确的本地验证脚本，请用户用 `! bash …` 执行后提供日志目录。
- 所有验证只用临时库、合成 Key、fake 原生存储和显式本地 Worker。不得使用个人 DB、真实 Key/OS 密钥库、外部模型或部署；不增加知识后台手动触发入口。
- 不把 P2 扩展到 P3 的数据集切换、通用后台同步、GC/移交或完整 Wiki。完成并验证后再更新本 Handoff、同步 roadmap、总路线、backlog 和设计 Note；未验证之前保持 P2 未完成。

## 11. 2026-10-09 接续实现与待执行验证（历史断点）

**状态：继续完成了代码/测试补充，但 P2 仍未验收。** §10 的“tests.rs、Flutter 协调器缺失”是当时断点，当前这些文件已经存在。不要删除或重建已有实现，也不要只检出旧提交。

### 本轮接入与修正

- `src/sync/runtime/enrollment/tests.rs` 已有初始用例；本轮补充原确认 token 幂等与最终回执断言，并新增 `tests/boundaries.rs`：坏证明不阻塞扫描、存储错误不推进游标、native 确认不恢复在途任务、原邀请期限与正式授权期限分离、旧 bootstrap 缺失、原 revoke/epoch 约束、两个应用回执先于分块、失败导出的共享名册可用于新 ready 尝试，以及 relay 权限/修订错误分类。
- owner P2 原生确认走 `runtime::confirm_enrollment_key`，不能用旧 owner 确认接口恢复后台。confirmation 保留原 role/operation/invitation/token 绑定以支持丢响应重试；新修改替换绑定。正式 grant/revoke 保存意图 ID 与原目标/修订，不从邮件外层 sender 猜撤销对象；401/403/409 与可重试网络失败区分。
- `runtime/enrollment.rs` 在正式接收授权改变 phase 后仍能重发未过期的原安装证明，不重生成密钥，也不因临时 sender 已被 owner 撤销而阻塞接收正式名册。坏证明先认证再进入存储事务；SQL/IO 失败不当作坏邮件吞掉。
- owner `notification_pending` 等待完成通知的对端回执，不把 relay 收到当作最终确认。`due_enrollment_wires` 允许失败旧导出与新 ready 导出共享同一签名名册，同时仍阻止未 ready 快照发送。完成记录与快照应用状态不一致时要求修复。
- Flutter 的 `sync_enrollment_api.dart`、`sync_enrollment_controller.dart` 接入 `SyncSetupController`、`SyncJoinController` 与 `RustBridgeRepository`。只有生产仓库显式启用自动协调；旧 fake 默认不隐式调用真实 FRB。单飞、退避、原 operation 库存绑定、native 保存/回读/原 token 确认、关闭/锁定等待写入、迟到响应与会话释放防线均已写入。
- `sync_enrollment_progress.dart` 为只读进度，区分本机快照应用、owner 收到回执及完成通知投递；不提供 P3 切库、持续增量同步或知识手动触发按钮。新增 `ui/test/sync_enrollment_controller_test.dart` 和 `sync_enrollment_bridge_test.dart`，后者经 `isolated_bridge.dart` 初始化。
- 新 `src/sync/transport_relay_smoke_tests/enrollment_ui.rs` 与 `relay/test/enrollment-runtime.test.mjs` 从生产 owner/joiner 入口走完整 P1→P2。代理在实际 Worker 提交后丢弃 grant/revoke/各类回执的第一次成功响应，检查原参数/新 nonce 重试；Rust 用例设计覆盖 native 未保存时重开、选定快照后恢复、快照期间捕获、原库/Provider 保留、多 Provider/静态保护与显式受控环回调用。**这些是待执行测试定义，不是通过证据。**

### 当前实际验证状态

- 本轮成功运行过 `git status --short`、针对性 diff、`git diff --check`，保留所有已有与并行修改。
- `cargo test --lib sync::runtime::enrollment::tests`、`cargo check --tests`、`./scripts/regen.sh` 和下面的统一验证脚本均在启动前遭遇执行工具的安全检查服务 `temporarily unavailable` / `timed out`。没有测试/编译结果，没有可引用的本轮测试通过数量，脚本也尚未生成结果日志目录。
- 工作区后来出现的生成绑定已包含 `SyncEnrollmentStateDto` 和三个 P2 API；已用 `git grep` 核对两侧静态 hash 为 **-558091073**。这是对当前文件的静态核对，**不是本会话成功运行 regen、release 或真实 bridge 的证据**。
- 并行工作后来还修改了 Cargo、web-search/DoH、`migrations/25-web-search/`、API/生成绑定、设置及 backlog；没有覆盖这些修改，也不归入 P2。当前 schema/测试基线需要按实际工作区重验，不能继续假定只到 v24 或沿用 P1 数量。未新增提交、推送或部署。

### 下一步：先执行验证，再按真实失败收尾

本机已准备脚本（位于忽略的构建目录，不提交）：

```bash
bash /home/pp/playground/ai/elsewhen/ui/build/enrollment-validation/run.sh
```

若 Claude 的执行工具仍不可用，用户可在本会话输入 `! bash /home/pp/playground/ai/elsewhen/ui/build/enrollment-validation/run.sh`；输出会直接回到会话。脚本仅对列明 P2 Rust/Dart 文件整理格式，不清理工作区或改其他任务文件；使用隔离数据根、合成 Key/fake native store/显式本地 Worker。它依次执行测试目标编译、P2 与相关定向、Rust 全量、regen/release/hash、Flutter 定向/全量、实际 Worker、全仓静态与 diff 检查，并输出临时日志目录及 `summary.tsv`。编译/重生成失败时依赖步骤标为 SKIPPED，不能当作通过。

1. 先看 `rust_check_tests.log` 和 `p2_rust_tests.log`，修正实际编译/断言失败；当前新代码尚无成功编译证据。
2. 确保新的 API/生成物与最新 release 一致，再跑真实 bridge 与 Flutter；不能只靠静态 hash 或已有库文件。
3. 核对新增实际 Worker 用例真正执行了故障代理和两端生产状态机；不以 P1 的旧 smoke 或手工 Store 调用代替。
4. 补齐最新全量/格式/analyze，单列既有 Rust 三个格式差异与 Flutter 78 项历史例外及其他并行任务的新差异，不盲目豁免、不修改无关代码清空报告。
5. 只有 P2 出口与上述门禁实际通过才勾选 P2，并再次同步本交接、同步路线、总路线、backlog、设计 Note；P3/S05/GC/加工移交与真人设备验收仍单列未完成。

## 12. 2026-10-10 补验与 Flutter 收尾（当前断点）

**状态：P2 工程与自动化验收已完成（2026-10-11 收尾复验）；原生系统密钥库、真人双端、P3/S05/GC/加工移交单列未完成。** 2026-10-10 完成了 Rust/Worker/桥接与 Flutter 全量补验，2026-10-11 补齐最终定向 analyze 与同步定向并复验全量。保留 `6743a09` 后的全部未提交及并行变更（其后代码已由用户提交/合并，实际 HEAD 接手时核对），不进入 P3。

### 本轮实际执行结果

统一 `ui/build/enrollment-validation/run.sh` 仍在启动前遇到安全检查服务故障，因此改为逐条执行其验证步骤；**下表不是统一脚本运行成功，也没有脚本生成的 `summary.tsv`**。

| 检查 | 实测结果 | 本会话任务日志文件名 |
|---|---|---|
| `cargo check --tests` | exit 0；3 条既有测试 warning | `bary6mez0.output` |
| P2 Rust 定向 | **16 passed / 0 failed** | `bygfnua9q.output` |
| `cargo test --all-targets` | **640 passed / 16 ignored**，另配置环境 **5 passed**；示例目标 0 tests | `b0oozkrhc.output` |
| `scripts/regen.sh` | codegen 与 release 均 exit 0，release 构建 56.93 秒 | `bvoylmju9.output` |
| 桥接静态 hash | Rust/Dart 均 **233902080**；不是旧记录的 `-558091073` | 本轮 `rg` 核对 |
| P2 Rust 格式 | 列明的 23 个 P2 文件 `rustfmt --check --config skip_children=true`，exit 0 | `bdykcbrpw.output` |
| 实际本地 Worker | **15 passed / 0 failed / 0 skipped**，含新 P2 生产运行时故障代理流程 | `bciq1z3th.output` |
| Flutter enrollment 控制器 | **11 passed**（修正前，fake native store） | `bzewmdurn.output` |
| Flutter 全量首轮 | **343 passed / 2 failed**，exit 1；在上述 release 重建后执行 | `b13nwzcvr.output` |
| Flutter 全量复验 | **345 passed / 0 failed**，exit 0；包含两处测试适配及 P2 括号修正 | `b34rtne5v.output` |
| P2 Dart 格式 | **10 files / 0 changed**，exit 0 | 本轮 `fvm dart format` 前台输出 |
| Rust 全仓格式 | 首轮 exit 1，**13 个文件**（历史 3 个 + 并行 AI/web-search/DoH 改动） | `bdmie0192.output` |
| Flutter 全量 analyze | 首轮 exit 1，**75 项（2 warning / 73 info）**；其中 1 项属于 P2，已修 | `bg9038o0s.output` |

### 2026-10-11 收尾复验（本轮实际执行）

本节命令由本 session 逐条实际执行并观察输出，未依赖旧日志或统一脚本；执行时代码已为用户提交后的工作区（干净），仍不改写 10-10 的原始记录。

| 检查 | 实测结果 |
|---|---|
| P2 Rust 定向 | **16 passed / 0 failed** |
| Rust 全量 `cargo test --all-targets` | **640 passed / 16 ignored**，exit 0 |
| 配置环境定向 `cargo test --test config_environment` | **5 passed** |
| 实际本地 Worker `npm --prefix relay test` | **15 passed / 0 failed / 0 skipped**，含 P2 生产运行时故障代理流程 |
| P2 定向 analyze（9 个目标） | **No issues found** |
| Flutter 同步相关定向（10 个测试文件：owner/joiner/enrollment/key store/setup/bridge） | **48 passed** |
| Flutter 全量 | **345 passed / 0 failed** |
| 桥接静态 hash | Rust `src/frb_generated.rs` 与 Dart 均为 **233902080**；`target/release/libelsewhen.so` 存在（10-10 重生成/release 产物） |
| `git diff --check` | 通过，exit 0 |
| Rust 全仓格式 | 首轮 exit 1，**48 处差异 / 16 个文件**（历史 3 个 + 并行 AI/web-search/DoH/wiki/config 改动，P2 文件无差异）；**同日经用户明确要求 `cargo fmt --all` 修复后 `--check` 为 exit 0**，复验全量 Rust 640 passed 与 release（56.51 秒）通过 |
| Flutter 全量 analyze | exit 1，**71 项全部 info（0 error / 0 warning）**；与 10-10 的 75 项差异来自 P2 括号 lint 修复及并行改动；P2 文件无问题，为当前唯一保留的全仓例外 |

历史基线与当前基线不混用：13 文件/75 项是 10-10 记录，48 处/16 文件与 71 项 info 是 10-11 修复前实测。Flutter analyze 的 71 项 info 继续单列，未关闭 lint。

日志目录为 `/tmp/claude-1000/-home-pp-playground-ai-elsewhen/be258774-e28d-45fe-b44e-80096d7dbe05/tasks/`。临时目录可能被清理，关键结果保留在上表；定向集合包含于全量，不能累加。Rust 全量使用专用临时 `ELSEWHEN_DATA_DIR` 和环回 `NO_PROXY`；P2 fixture/bridge 使用隔离库，Worker 只访问测试创建的环回地址。没有使用个人数据、真实密钥库、外部模型或线上中继。

实际 Worker 新用例已经执行，而非只检查定义：生产 owner/joiner 完成 P1→P2，代理在 Worker 提交后丢弃两个正式 grant、一次 revoke 和各类回执的首次成功响应；原参数/新 HTTP nonce 重试通过。覆盖 SQLite 提交而 native 未保存时重开、选定快照后恢复、快照期间新捕获不被提前确认、两端完成通知、多个 Provider/密钥/顺序保真和受控环回调用。暂存数据集未被选择，后台仍不可领取；这不是两个真实 App/OS 密钥库签收。

### 三处收尾修改与取舍

1. `ui/lib/sync/sync_join_controller.dart`：为迟到状态检查加括号，处理本轮 P2 的 `curly_braces_in_flow_control_structures`。不改变 operation/修订防线。
2. `ui/test/chat_run_history_test.dart`：并行实现已把部分回答从 `SelectableText` 改为 `SelectionArea + MarkdownView`。将原正文断言改为 `find.text(..., findRichText: true)`，仍要求准确显示同一回答，并保留原输入重试/不重复保存断言；不把生产 UI 改回旧控件。
3. `ui/test/conversation_continuity_bridge_test.dart`：首次全量因模拟模型默认窗口 32768、输入预算 27034，而实际估算请求 27237 token 失败。改用完整 Provider DTO 明确指定 **65536** 的模拟窗口；仍只调用本地 fixture，仍验证承诺收尾、追问连续性和显式审阅。该用例不是预算边界测试，不能靠删除工具、截断必要上下文、跳过断言或放宽生产预算检查来过测；生产预算逻辑与并行 AI 实现均未修改。

三处修改之后，全量 Flutter 已实际复验 **345 passed**；随后 P2 Dart 格式 **10 files / 0 changed**。先前复验尝试在命令启动前遇到 `temporarily unavailable` / timeout，不计作代码失败；成功复验以 `b34rtne5v.output` 为准。最终定向 analyze 与同步定向仍未启动成功，不冒充已执行。未为清空报告而修改其他格式/lint 文件。

### 接续顺序

P2 与 P3 的工程与自动化验收已完成（P3 见 §13），下一步为 P4（首版剩余语义与发布恢复门禁，见 §4）；不重建 P1/P2/P3 已有实现。复现命令见 §6.2 与 10-11 收尾表；统一脚本 `ui/build/enrollment-validation/run.sh`（忽略目录，不提交）仍可用于完整复验。全仓 Rust 格式已于 10-11 修复为零差异；Flutter analyze 71 项全 info 例外持续单列；真实 OS 密钥库与真人设备签收是独立门禁。

- [x] P2 Rust 编译/16 项边界测试、全量 Rust、实际 Worker 完整流程、桥接重生成/release 已补验。
- [x] 三处 Flutter 收尾修改经全量 **345 passed** 验证；P2 Dart 格式 **10 files / 0 changed**。
- [x] 本轮文档回填后的 `git diff --check` 通过。
- [x] 最终定向 analyze（No issues found）与同步相关定向 **48 passed** 补齐；P2 工程与自动化验收完成（2026-10-11）。
- [x] P3 工程与自动化验收完成（2026-10-12，见 §13）：数据集选择/修复 API + 网络稳态 tick + App 调度/状态页，环回真实 Worker 16/16、Rust 640/640、Flutter 345/345。
- [x] P4a 源隔离/idempotency 保真/未知版本容错/长离线追平完成（2026-10-12，见 §14）：迁移 v27 + 7 个同步测试 + v27 迁移测试，Rust 648 全量通过。
- [x] P4b Provider 冲突掩码审阅/显式裁决/可达性提示完成（2026-10-13，见 §15）：迁移 v28 + 冲突审阅/裁决存储与 FRB API + 6 个端到端测试 + v28 迁移测试，Rust 660 全量通过；审阅对话框 UI 接线完成（§16），Flutter 349 全量通过。
- [x] P4c S03 生命周期 GC/水印/补快照完成（2026-10-13，见 §17）：迁移 v29 + 每成员应用水印 + catch-up 窗口 + needs_resnapshot + GC 游标，7 个新测试，Rust 667 全量通过；加工移交与撤销 UX 属后续切片。
- [x] P4d S04/S06 正常加工移交完成（2026-10-14，见 §19）：迁移 v31 + 移交凭据/状态机 + frontier 门禁确认接管 + 延迟分析放行，Rust 675 全量通过，Flutter 349 全量通过。
- [ ] P4e（撤销 UX、真人设备签收）、P5（S05）、原生 OS 密钥库发布门禁（未开展）。

## 13. 2026-10-12 P3 实施（数据集选择/修复与 App 自动同步调度）

### 本轮实际交付

1. **网络稳态 tick（`src/sync/runtime/network.rs`）**：`tick(&AppConfig)` 复用 `engine::stage_operations/stage_provider_updates/stage_receipts/push_due/pull_page` 与持久 outbox/receipt 台账；有界（每 peer ≤3 封、≤4 页、≤8 peer）。防线按 §4/P3 要求：确认会话（master 在场、非 enrollment 中）→ 快照导入待决跳过 → `has_sync_authority` → 本机活跃成员（`require_local_sync_membership`）→ 当前确认 vault 修订/设备身份与 runtime 一致 → vault 路由的 `mailbox_owner` 与本机身份一致 → 受信方向路由（`RelayRoute::validate`）。发送凭证只在 tick 时由 `send_key.to_bytes()` 派生；缺 peer 密钥/签名钥是显式错误而非静默跳过。`stage_receipts` 一并暂存应用回执，否则对端已应用操作永不确认。返回 `Ok(None)` 表示合法跳过（锁定/未初始化/他 worker 占用），错误上抛由状态页展示。
2. **数据集选择/修复 API（`src/api/sync.rs` + `runtime::session_master` + `DatasetCatalog::selected_id`）**：`list_sync_datasets` 返回目录册与修订；`select_sync_dataset(entry_id)` 只用「用户实际看到的」`prepare_selection` 预览 + 本会话 `session_master`（原生确认过的 vault 修订）提交 `select_import`，仅下次启动生效；`select_default_sync_dataset(expected_catalog_revision)` 防并发选择串改，冲突报 `sync_dataset_catalog_changed`。`session_master` 接受「已完成恢复/选择的 joining 会话」（scope 仍指向默认库），普通会话必须与钉住库一致——不放宽修订/确认/master 检查。
3. **App 自动同步调度（`ui/lib/bridge/rust_bridge_repository.dart`）**：网络 tick 与分析/消化复用同一条 5 秒后台 worker 通道、串行执行；到达/上行时唤醒分析队列并递增知识修订号刷新视图；错误只记入 `lastSyncTickError` 状态页，绝不影响本地记录。锁定=暂停同步不暂停本地捕获。
4. **状态页（`ui/lib/widgets/local_sync_settings.dart`）**：待确认操作数、指定处理设备提示（非处理设备不领后台任务）、最近 tick 范围（上行/到达/等待依赖/回执失败）与最近错误。
5. **测试（环回）**：`src/sync/runtime/network/tests.rs` + `relay/test/network-runtime.test.mjs`。复用真实生产 P1→P2 enrollment（`enrollment_ui::run_with_fault_proxy(false)` 重构暴露 `CompletedEnrollment`）；随后 owner 重启式解锁、joiner 以「恢复后的在线选择」提交暂存目录册、锁定期间本地写入不同步、重解锁后 joiner 推→owner 拉→回执回流→outbox 排空→owner 新写入到达 joiner、默认库全程隔离。`enrollment_ui` 暴露的修订从两侧真实库读取（owner=默认库，joiner=暂存库），不是中间楼层。

### 验证结果（本轮实际执行）

| 检查 | 结果 |
| --- | --- |
| `cargo test --all-targets` | **640 passed / 0 failed / 17 ignored**（含新 ignored 环回用例） |
| `npm --prefix relay test`（真实 Worker，环回） | **16 passed / 0 failed**，含新 **P3 steady-state network ticks move deltas after enrollment completes**（约 14.3 s）与既有 P2/互通用例 |
| `cargo fmt --all -- --check` | exit 0 |
| FRB 桥接 | `scripts/regen.sh` 完成，hash 与 release 库一致（56 s 重编） |
| `cd ui && fvm flutter analyze` | exit 1，**72 项**（上轮 71 项 + 并行任务新增 1 项 warning；P3 改动文件 0 项），继续单列不关闭 |
| `cd ui && fvm flutter test`（全量） | **345 passed**（含 `local_sync_settings`/worker pause 等同步定向 48 项） |

全程只用隔离 `ELSEWHEN_DATA_DIR`、环回 `NO_PROXY`、合成密钥与本地 Worker；没有使用个人数据、真实密钥库、外部模型或线上中继；没有提交、推送或部署。

### 边界与未完成

- P3 的「选择仅下次启动生效」「锁定暂停同步不暂停捕获」「只有指定处理设备领任务」已按设计落地；**真人双端重启/断网恢复/视图刷新仍属 S06 验收**，不以本轮环回测试替代。
- 数据集「修复」首版＝明确列出目录册/待决暂存 + 手动选择/回退（不静默回默认库、不搬动打开的 SQLite/WAL）；已选库丢失/损坏的自动引导修复 UI 仍是后续工作。
- P4（GC/TTL/重快照/加工移交）、P5（S05 Wiki 同步）、原生 OS 密钥库与真人设备签收继续未完成；不进入 P3 之外范围。

## 14. 2026-10-12 P4a 实施（S01 源隔离 / idempotency 保真 / 未知版本容错 / 长离线追平）

### 调查结论（先核对，不从零重做）

P4a 开工前先逐项核对 S01 已有底座，确认以下机制**已带测试存在，不在本增量重做**：去重/同内容异序拒绝、乱序缺口等待与排空、跨源缺基等待不造假冲突、持久应用回执与丢失后重建、被撤销成员不阻塞他设备、捕获原子提交且重放无任务/无回显、非法远程时钟不影响本地捕获、回执失败回滚、协议校验（版本/ID 长度/序号/时钟/`op_id==stream:seq`/`deny_unknown_fields`/消息角色与父环）、`sync_operations` 域校验失败置 `rejected`。据此只补 **4 个真实缺口**。

### 本轮实际交付（单独提交 `3cbe569`）

1. **迁移 v27（`migrations/27-sync-source-isolation/up.sql`）**：`sync_operations.state` CHECK 增加 `'isolated'`。SQLite 不能 ALTER CHECK，故重建表：保留全部行与 `UNIQUE(origin_stream_id,origin_sequence)`，重建 `sync_operations_pending` 索引；先 `PRAGMA legacy_alter_table=ON` 再 DROP/RENAME，避免 `validate_sync_receipt` 触发器引用旧表名导致重建断裂。
2. **按源隔离替代整批回滚（`src/storage/sync/mod.rs`）**：三类原先会 `ensure!` 中止整个 receive 的情况，改为**按源持久隔离**——先持久隔离行（`state='isolated'` + 持久 `error_code`），commit 后再排水，使排水的 NOT-EXISTS 防护能看到该序号不可判定，从而：
   - 同 ID/同（流，序号）不同字节（`sync_operation_content_conflict`）：隔离，不回滚本批、不应用分歧负载、不阻塞其他流、本地捕获不受影响。
   - 逻辑钟远超本地（`sync_clock_requires_snapshot`，长离线/需快照）：隔离该 op，不再硬失败。
   - 未知协议版本（`sync_protocol_unsupported`）：用新增 `identity_header()` 容错提取身份；若数据集匹配且发送方是活跃已认证成员（且非本机流）则隔离，否则维持硬拒绝。
   - 排水永不返回 `isolated` 行（其状态非 `waiting`），frontier 只经真正应用的 op 推进，真身链完整时后续同流 op 继续应用。
3. **idempotency_key 保真（`src/sync/protocol.rs` + 应用侧）**：`InputRecord` 增加可选 `idempotency_key`（`None` 时 `skip_serializing_if`，向后兼容旧格式）；捕获应用的 INSERT 与读取均携带，使跨设备同 key 用户输入可去重。
4. **测试（`src/storage/sync/tests.rs` 7 个 + `src/storage/new_migrations.rs` 1 个）**：同 ID 异文隔离不回滚、不可判定分歧不应用不阻塞他流、未知版本隔离不阻塞、idempotency 跨设备保真、带 key 重放无重复无任务、三设备长离线追平（每端只转发自己的 op，重放幂等）、不可信设备/数据集/版本永不应用；v27 迁移保留旧行 + 接受 `'isolated'` + 拒绝集外值 + 保留 UNIQUE。

### 验证结果（本轮实际执行）

| 检查 | 结果 |
| --- | --- |
| `cargo test --lib` | **648 passed / 0 failed / 17 ignored** |
| `cargo test --lib storage::sync::tests` | **25 passed**（含 7 个新/重写用例） |
| `cargo test --lib storage::new_migrations::tests::v27` | **1 passed** |
| `cargo fmt --all` | 干净 |

全程只用临时目录隔离库与合成身份；未用个人数据、真实密钥库、外部模型或线上中继；已提交 `3cbe569`，未 push。

### 边界与未完成

- **协议固有边界**：接收方**从未见过某序号真身**时，伪造该序号无法识别（无真身可比），会作为合法新 op 应用；需后续签名/序列缺口检测解决，不在本增量。
- 线格式演进：`InputRecord` 新增可选字段 + `state` 增加 `'isolated'` 属首版前允许的演进；旧设备 `deny_unknown_fields` 会拒新格式，跨版本协商留待后续 P4（见 §4）。
- S01 的普通对话生命周期与其余记录来源的**逐项首版验收**、P4b–P4e、P5、原生 OS 密钥库与真人设备签收继续未完成。

## 15. 2026-10-13 P4b 实施（Provider 冲突掩码审阅 / 显式裁决 / 可达性提示）

对应同步 roadmap S04「Provider 完整配置」两项 checkbox（版本化 bundle + 冲突掩码绑定实际所见版本；环回/不可达提示不静默改 URL），见 §4。

### 本轮实际交付

1. **迁移 v28（`migrations/28-sync-provider-conflicts/up.sql`）**：`sync_provider_versions.state` CHECK 增加 `'dismissed'`。沿 v27 先例：`PRAGMA legacy_alter_table=ON` → `CREATE ..._new` → `INSERT ... SELECT` → DROP/RENAME。保留全部行与密文信封；驳回的 revision 保持可识别（重投为幂等 duplicate 而非复活为新冲突），其密文绝不重新解释为 applied。
2. **冲突审阅与裁决存储层（`src/storage/sync/provider_conflicts.rs`）**：
   - `list_sync_provider_conflicts`：查询 `state='conflict'` 行，用 vault 包裹密钥 + 名册发送方签名密钥解密信封（与 `apply_encrypted_provider_bundle` 相同的成员双侧 active 判定），本地/远端两侧配置全部掩码返回；锁定状态失败关闭（`provider_key_locked`）。
   - `resolve_sync_provider_conflict`：**只**支持显式 `KeepLocal`/`AcceptRemote` 整包裁决，绝不按字段拼接 Key/URL。`AcceptRemote` 解密并 `replace_provider_bundle_on` 整包应用、置 `applied`、推进 `sync_provider_state`、清脏标记；`KeepLocal` 置 `dismissed` 并 `sync_mark_provider_dirty` 重新广播当前本地状态。
   - **双版本绑定**：`expected_local_revision` 与 `expected_remote_revision` 必须原样回传用户审阅时所见版本，任一不匹配失败关闭（`provider_conflict_local_changed` / `provider_conflict_revision_mismatch`），避免审阅期间的并发本地编辑或新冲突被静默覆盖。
   - 信封重开要求与暂存时记录的 header 完全一致（`crypto::open` 的 `sync envelope context mismatch`），篡改密文不能被重新解释为同一 revision。
3. **掩码工具（`src/storage/provider_keys.rs::mask_api_key`）**：UTF-8 char 边界安全；`<=8` 字符整体遮蔽，更长保留前 3/后 4（如 `sk-****7890`）；空值返回空。仅用于冲突审阅展示，绝不进入 DTO 之外的存储/日志/诊断字符串。
4. **可达性提示（`src/api/provider_reachability.rs`）**：纯静态、零 I/O、零新依赖。环回（`localhost`/`127.0.0.0/8`/`::1`/`0.0.0.0`）、私有网段（RFC1918/链路本地/`.local`/`.internal`/`.lan`）、缺 scheme、非常规 scheme、缺 host 五类提示码；**只提示不阻止**，原样回显地址，绝不返回修正后的 URL、不探测外部。
5. **FRB API（`src/api/sync.rs`）**：`list_sync_provider_conflicts()` 返回 `ProviderSyncConflictDto`（含发送方设备名、双侧掩码 profile 列表 + 逐条可达性提示、远端激活项与故障切换顺序、本地激活项）；`resolve_sync_provider_conflict(revision_id, expected_local_revision, expected_remote_revision, accept_remote)`。DTO 绝不携带明文 Key。

### 验证结果（本轮实际执行）

| 检查 | 结果 |
| --- | --- |
| `cargo test --lib` | **660 passed / 0 failed / 17 ignored** |
| `cargo test --all-targets`（`NO_PROXY=localhost,127.0.0.1,::1`） | **660 passed / 17 ignored** |
| `cargo test --lib -- sync::provider_conflicts` | **6 passed**（掩码/双版本绑定/接受/保留/轮换+删除+激活/UTF-8 边界） |
| `cargo test --lib -- new_migrations`（v28 迁移测试） | 通过（保留旧行 + 接受 `'dismissed'` + 拒绝集外值） |
| `cargo fmt --all -- --check` | 干净 |
| `cargo build --release` | 通过（经 `./scripts/regen.sh`） |
| `./scripts/regen.sh` + `./scripts/test_bridge.sh` | 桥接 Dart/Rust hash 一致，桥接校验通过 |
| `npm --prefix relay test` | **16 passed / 0 failed** |
| Flutter `fvm flutter test` | **345 passed** |
| Flutter analyze（`lib/bridge/generated.dart`） | 无问题；全量 analyze 仍为既有例外（1 warning + 70 info，非本切片） |

### 边界与未完成

- **审阅对话框 UI 已接线**（§16）：设置 → 数据 → 设备同步在 `local_ready` 时展示冲突列表与「保留本地/接受远端」按钮；设置保存路径的可达性提示此前已由 `ProviderEndpointNotice` 覆盖，本次未改动。
- 快照只导出 `state='applied'` 的 provider checkpoint（`snapshot.rs:157`），冲突/驳回行不进快照——属既有设计，未改动。
- 用户明确不接受的较早冲突候选保持 `conflict` 终态，绝不随较新候选的接受而自动应用或消失，仍需逐条显式裁决。
- P4c–P4e（S03 GC/TTL/重快照/加工移交、撤销 UX、真人设备签收）、P5、原生 OS 密钥库发布门禁继续未完成。
- 全程只用临时目录隔离库与合成 Key/合成身份；未用个人数据、真实密钥库、外部模型或线上中继。

## 16. 2026-10-13 P4b 收尾（冲突审阅对话框 UI 接线）

### 本轮实际交付

1. **`ui/lib/sync/sync_provider_conflicts_controller.dart`**：`SyncProviderConflictApi` 接口（默认 `FrbSyncProviderConflictApi`）+ `SyncProviderConflictsController`。首次展示时拉取；裁决成功后重拉确认该冲突确实消失（不乐观移除）；列表/裁决失败保留旧视图并原样展示错误，绝不静默清空。锁定/双版本绑定/已裁决三类失败关闭错误映射为本地化文案。
2. **`ui/lib/widgets/sync_provider_conflicts_settings.dart`**：冲突卡片展示发送方设备名、本机/远端两侧掩码 profile（名称/模型/激活项/base_url 原样回显/掩码 Key/逐条可达性提示本地化）、「保留本地」/「接受远端」按钮。密钥绝不以明文出现；提示仅为静态分析，不代表实际可达。
3. **接线（`ui/lib/widgets/local_sync_settings.dart`）**：`local_ready` 时在「设备同步」区展示冲突面板；控制器可注入测试替身，未注入时自建并随 widget 销毁。设置保存路径的可达性提示此前已由 `ProviderEndpointNotice`（Dart 原生静态分析）覆盖，本次未改动。
4. **`src/api/provider_reachability.rs` 桥接修正**：`ReachabilityHint.code` 由 `&'static str` 改为 `String` 并加 `#[flutter_rust_bridge::frb(non_opaque)]`——`&'static str` 会被 FRB 当作 opaque `str` 生成无法编译的 `RustAutoOpaqueInner<str>`；改后生成普通 Dart DTO（`final String code/address`）。`ui/lib/bridge/api.dart` 补 `provider_reachability.dart` 导出。语义不变。

### 验证结果（本轮实际执行）

| 检查 | 结果 |
| --- | --- |
| Flutter `fvm flutter test` | **349 passed**（4 个新 widget 测试：掩码+提示+无明文、保留本地/接受远端原样回传版本、锁定失败关闭不清空列表、审阅期间本地已变双版本绑定上抛） |
| Flutter analyze（本切片 4 个文件） | 无问题；全量仍为既有 1 warning + 70 info 例外单列 |
| `fvm dart format --set-exit-if-changed`（本切片文件） | 通过 |
| `cargo test --all-targets`（`NO_PROXY=localhost,127.0.0.1,::1`） | **660 passed / 17 ignored**（无 Rust 语义变更） |
| `cargo fmt --all -- --check` / `cargo build --release` | 通过 |
| `./scripts/regen.sh` + `./scripts/test_bridge.sh` | 桥接 hash 一致，桥接校验通过（**必须用 regen.sh 而非裸 `flutter_rust_bridge_codegen generate`**：裸跑缺 `CPATH` clang 头导致 ffigen 把 `bool` 解析成 NativeFunction，生成无法编译的 `.io.dart`） |
| `npm --prefix relay test` | **16 passed / 0 failed** |

### 边界与未完成

- 本切片只接通**审阅对话框 UI**；Rust 存储/API 语义未改动。
- P4c–P4e（S03 GC/TTL/重快照/加工移交、撤销 UX、真人设备签收）、P5、原生 OS 密钥库发布门禁继续未完成。
- 真实双设备冲突的端到端演示仍需真人设备门禁（P4e），widget 测试不替代之。

## 17. 2026-10-13 P4c（S03 生命周期：GC/水印/补快照）

### 本轮实际交付

1. **迁移 v29**（`migrations/29-sync-gc/up.sql`）：
   - `sync_member_state(device_id, confirmed_sequence, needs_resnapshot)` — 每成员对本流的应用层水印与补快照标志。
   - `sync_gc_state(origin_stream_id, gc_floor)` — 每流单调 GC 游标。
   - `ALTER TABLE sync_snapshot_exports ADD COLUMN local_watermark` — 快照构建时钉住的本地流 frontier。
2. **`src/storage/sync/gc.rs`**：`advance_sync_member_watermark`（水印只升不降）、`run_sync_gc`（失败关闭的 GC）、`mark_member_snapshotted`、`sync_resnapshot_pending`、`sync_local_gc_floor`。常量 `SYNC_GC_CATCH_UP_WINDOW=256`、`SYNC_GC_MIN_RETAINED_OPERATIONS=64`。
3. **接线**：`acknowledge_sync_operation` 现在同时推进该成员水印（`gc.rs`）；`begin_snapshot_export` 记录构建时本地 frontier；`acknowledge_snapshot_export` 用该 frontier 经 `mark_member_snapshotted` 播种成员水印。

### 语义不变量（全部 SQL 强制，失败关闭）

- **三个确认分开**：relay 收下 ≠ 对端已应用 ≠ 允许 GC。GC **只**用应用层水印（`acknowledge_sync_operation` → `advance_sync_member_watermark`），绝不用 relay ack（`sync_wire_outbox.state`）。
- **所有 active 成员纳入水印计算**：`sync_members` 左连 `sync_member_state`，缺失行 `COALESCE(...,0)` 按 0 计——落后设备永远拉住 GC，NULL/缺失不被过滤。
- **落后成员失败关闭**：`confirmed < floor - catch_up_window` 翻 `needs_resnapshot=1`；任何成员被标记时 GC 完全停摆（`operations_pruned=0`、floor 不动），绝不静默清历史。
- **保留窗口保底**：除水印外，GC 永不修剪最新 `SYNC_GC_MIN_RETAINED_OPERATIONS` 条本地已应用 op，最近的本地捕获始终可重放。
- **只修剪本流已确认历史**：GC 只删本设备 `local=1 AND state='applied'` 且每个 active 成员都已 ack 的 op；先删 `sync_outbox` 行再删 `sync_operations`（外键顺序）。绝不动域数据（events/conversations/messages）或他设备历史（receiver 不是他设备历史的权威）。
- **快照 ack 用建快照时 frontier 播种**：新成员从快照进入正常增量集合，水印从构建时 frontier 起算——不是 0（会拉住 GC）也不是实时 frontier（会过度声明快照之后的 op）。
- **中继 TTL = 仅丢传输副本**：TTL 到期只丢 relay 侧信封，源操作未确认就重发或安排快照；7 天邮箱不是备份（代码注释固化，语义由「GC 只认应用水印」保证）。

### 验证结果（本轮实际执行）

| 检查 | 结果 |
| --- | --- |
| `cargo test --all-targets`（`NO_PROXY=localhost,127.0.0.1,::1`） | **667 passed / 0 failed / 17 ignored**（7 个新 GC 测试 + v29 迁移版本断言） |
| `cargo fmt --all -- --check` | 通过 |
| `cargo build --release` | 通过 |
| `./scripts/test_bridge.sh` | 桥接校验通过 |
| `npm --prefix relay test` | **16 passed / 0 failed** |
| Flutter `fvm flutter test` | **349 passed**（无 Flutter 变更） |

7 个新测试覆盖：水印只升不降、全确认后修剪且不动域数据、未确认成员完全拉住 GC、缺失水位行按 0 计不被过滤、落后成员翻 needs_resnapshot 且 GC 停摆、快照 ack 用构建时 frontier 播种、GC 不用 relay ack 且不修剪他设备历史。

### 边界与未完成

- 本切片实现 **GC/水印/补快照状态**；`needs_resnapshot` 只置标志，**自动重发快照的调度**（检测标志后自动为新成员重建快照）属后续切片。
- **tombstone 生命周期**：对话删除的 tombstone 防复活已在 `sync_object_heads.deleted` + `sync_known_revisions`；其**回收**需考虑离线设备与快照世代（§5），本切片未实现 tombstone 回收（保守保留，永不自动清）。
- P4d 加工移交已完成（§19）；P4e 撤销 UX/诊断/升级、原生密钥库签收、真人设备门禁继续未完成。

### 事故记录：外来 stash 污染

本轮 `cargo fmt --all` 后 `git stash`/`stash pop` 操作意外把 stash@{0} 中**另一工作流的 storage-adapter 重构**混入工作区（`src/storage/adapter.rs`、`src/storage/mod.rs`、多个 UI settings 文件、2 个新架构 note、`proposal-backlog.md` 的两条非本切片提案）。已逐一甄别并 `git checkout HEAD --` 还原全部外来文件，仅保留 P4c 自有改动；stash@{0} 保留未动（属另一工作流）。**教训**：多工作流共用仓库时，`git stash` 不带 message 极易张冠李戴；本仓库禁止无 message 的 stash。

## 18. 2026-10-13 P4c 收尾（自动重快照调度闭环）

**目标**：闭合 §17 遗留的「`needs_resnapshot` 只置标志」——自动检测标志、幂等重建/重发快照、并在对端确认后解除 GC 冻结。本节全部失败关闭。

### 设计：durable 请求行 + 单调 nonce + 请求绑定导出 + 合成应用 ack

- **迁移 v30**（`migrations/30-sync-resnapshot/up.sql`）：
  - `sync_snapshot_exports` 增加 `roster_revision`（导出嵌入的权威名册修订，接收端完成时交叉校验）。
  - `sync_resnapshot_requests(device_id, snapshot_id, request_nonce, open)` — 每成员一条 durable 请求；`request_nonce` **单调不减**（完成只把 `open` 置 0 而不删行），因此新一代必得新 id，旧导出永远无法满足新请求。快照 id 在 `begin_snapshot_export` 时绑定（请求先于导出行存在，故无外键）。
- **`src/storage/sync/resnapshot.rs`**：
  - `claim_sync_resnapshot` — 仅对「已标记 + active + 非自己 + 无在途导出」的成员产出 claim；已 claim 未建 / 已 abort 的请求**复用同一 id**（重建导出在同一 id 下字节一致），其余情况 nonce+1。幂等跨 tick/重启。
  - `finish_sync_resnapshot_requests` — 仅当**当前请求 id** 的导出被 ack 才完成：经 `mark_member_snapshotted` 用**构建时 frontier** 重播种水印并清标志（解除 GC 冻结）。
  - `sync_resnapshot_states` — 只读状态（供 tick 报告）。
- **完成回路的合成应用 ack**：接收端无法把快照导入非空活库（restore 流程不在本切片），因此当某导出的全部 wire 都已 ack 时，接收端发一条合成应用 ack（`op_id = "resnapshot-ack:{export_id}"`）。发送端 `accept_sync_receipt` 识别该前缀并路由到 `acknowledge_snapshot_export`（不经过普通 op 退休路径）。
- **tick 顺序**（`runtime/network.rs`）：peers push/pull → `finish_sync_resnapshot_requests` → `run_sync_gc` → claim+stage 重快照（`stage_enrollment_snapshot` 嵌入权威名册）→ 接收端 ack 检测 → 报告。`NetworkTickReport`/`SyncTickResultDto` 增加 `resnapshot_pending`。

### 失败关闭不变量

- 未标记 / 已撤销 / 自己 / 已有在途导出的成员拿不到新导出（claim 返回 `None`）。
- 重新标记（re-flag）后 nonce 轮换：旧导出即使已 ack 也**不**满足新请求（`finish` 只认当前请求 id）。
- `begin_snapshot_export` 强制 id 绑定：请求行存在时 id 不匹配即拒绝（`sync_snapshot_resnapshot_request_mismatch`）。
- 全程只用应用层水印；relay ack（`sync_wire_outbox.state`）不作为 GC 依据。

### 顺手修复的两处潜在 `QueryReturnedNoRows`

- `current_signed_roster`：`sync_authority` 行存在但 `signed_roster` 为 NULL（旧配对流程）时，原裸 `query_row` 抛 `Invalid column type Null`；改为 `.optional()?.flatten()`。这修复了配对/membership 一组测试（v30 前因 `begin_snapshot_export` 尚不读它而潜伏）。
- `sync_local_gc_floor` 与 `begin_snapshot_export` 的 frontier 查询：无行时改 `.optional()`，不再依赖「恰好有行」。

### 验证结果（本轮实际执行）

| 检查 | 结果 |
| --- | --- |
| `cargo test --all-targets`（`NO_PROXY=localhost,127.0.0.1,::1`） | **673 passed / 0 failed / 17 ignored**（新增 6 个重快照存储测试；含上述两处修复后回归全绿） |
| `cargo fmt --all -- --check` | 通过 |
| `cargo build --release` | 通过 |
| `./scripts/regen.sh` | 桥接重生成（`SyncTickResultDto.resnapshot_pending`），hash 一致 |
| Flutter `fvm flutter test` | **349 passed** |
| Flutter `fvm flutter analyze` | 72 issues（70 info + 1 warning，均为 third_party/测试文件，非本切片，维持既有基线） |

6 个新测试覆盖：claim 仅对标记成员且幂等、在途导出阻塞新 claim、id 绑定拒绝错配请求、finish 只认当前已 ack 请求并用构建时 frontier 重播种、re-flag 后 nonce 轮换使旧 ack 失效、只读状态报告。

### 边界与未完成

- **接收端消费重快照**仍是 restore 流程的活：活设备收到快照分片会优雅拒绝（在中继过期为传输副本），本切片只闭合「发送端调度 + ack + 解冻结」。
- 中继 TTL 仅丢传输副本；未确认的源操作重发或安排快照，不以 relay 为准。
- P4e 撤销 UX/诊断/升级、原生密钥库签收、真人设备门禁继续未完成。
- 全程只用临时目录隔离库与合成身份；无个人数据/真实密钥/外部模型/线上中继（relay 测试 `ELSEWHEN_RELAY_TEST_URL` 未设，ignored）。

## 19. 2026-10-14 P4d（S04/S06 正常加工移交）

**目标**：实现跨设备后台加工归属的安全移交——旧端停止领取并收尾在途任务，通过密码学凭据完成状态交接，新端在 frontier 追平后显式确认接管。

### 实现内容

- **迁移 v31**（`migrations/31-sync-processor-handoff/up.sql`）：
  - `sync_processor_handoffs`：记录所有已批准的 handoff 凭据（主键 `roster_revision`）。
  - `sync_processor_state`：单例状态机（`idle` | `relinquished` | `pending_takeover` | `active`）。
  - `sync_background_permission` 视图：排除 `relinquished` 和 `pending_takeover` 状态。

- **`src/sync/membership.rs`**：
  - `ProcessingYieldProof`：旧端 Ed25519 签名的移交凭据（绑定 dataset_id、from/to device、roster_revision、final_sequence）。
  - `MembershipRoster.handoff`：向前兼容的 Option 字段，携带移交证明。
  - `validate_successor`：无加工端变更时拒绝附带 handoff；有变更时强制要求旧加工端签名凭据且 revision 匹配。

- **`src/storage/sync/membership.rs`**：
  - `yield_processor_role`：校验当前为 active processor → `require_background_quiescent_for_sync` → 计算 `final_sequence = next_sequence - 1` → 生成并签名 proof → 置 `sync_processor_state` 为 `relinquished`。
  - `apply_signed_roster`：检测到 `processor_device_id` 变更且带合法 handoff 时 → 记录 `sync_processor_handoffs` → 目标端置 `pending_takeover`（不更新 `processor_device_id`）→ 观察端直接更新 `processor_device_id`。
  - `confirm_processor_takeover`：校验状态为 `pending_takeover` + `applied_sequence >= final_sequence` → 更新 `sync_replica.processor_device_id` → 置状态为 `active`。
  - `get_processor_handoff_state`：查询当前移交状态（独立 DTO，不影响 `LocalSyncStateDto`）。

- **`src/storage/sync/analysis.rs`**：
  - `apply_sync_analysis`：若来源非当前 processor，查询 `sync_processor_handoffs` 中该设备的 `MAX(final_sequence)`；`op.origin_sequence <= final_sequence` 时允许应用，否则拒绝（`sync_analysis_wrong_processor`）。

- **API 层**（`src/api/sync.rs`）：
  - `yield_sync_processor(to_device_id)`：旧端让出加工权（从已解锁会话 vault 加载签名者），返回 `ProcessingYieldProofDto`；修订号由运行时取当前名册 +1。
  - `approve_processor_handoff(proof)`：owner 审批 old 端证明并签发后继名册。
  - `confirm_processor_takeover`：新端确认接管。
  - `get_processor_handoff_state`：查询移交状态，返回 `ProcessorHandoffStateDto`。

### 运行时闭环（同批补齐）

初版 `yield_sync_processor` 要求从 Dart 传入 `DeviceIdentity`（含私钥，前端无法构造），已改为 `src/sync/runtime/handoff.rs` 从已确认会话 vault 加载签名者（含锁定/未确认/加入中全拒）。新增 owner 审批链路：`approve_processor_handoff(proof)` → 存储层 `approve_processor_handoff`，防线全部 fail-closed：

- owner 签名密钥 ≠ 名册 owner 拒绝（`sync_membership_owner_required`）；
- proof 修订 ≠ 当前名册 + 1 拒绝（`sync_handoff_revision_mismatch`）；
- proof 来源 ≠ 当前加工端或旧端不活跃拒绝（`sync_handoff_source_mismatch` / `sync_handoff_old_processor_inactive`）；
- proof 必须用旧加工端注册密钥验签（`sync_handoff_signature_invalid`）；
- 目标非活跃成员拒绝（`sync_handoff_target_missing`）。

测试 `owner_approves_handoff_and_enforces_proof_boundaries` 覆盖 owner == 旧加工端与 owner != 旧加工端（移交回 owner）两个完整环路、全部审批防线，及目标端在新名册下保持 `pending_takeover`（processor 指针不提前切换）。

### 设置页接线

移交流程已接入设置页（`ui/lib/widgets/sync_processor_handoff_settings.dart` + `ui/lib/sync/sync_processor_handoff_controller.dart`，注入 `LocalSyncSettings`）：

- 只读展示状态机位置（idle / relinquished / pending_takeover 且区分 canConfirm）；
- 加工端 idle 态展示「让出加工权」：需填写目标设备 ID + 二次确认对话框（明确告知不可单方面恢复）；
- owner 同机时让出后自动 approve 并刷新；非 owner 本机让出后仅提示等待 owner 审批（对应下方跨设备传输边界）；
- 目标端仅 `canConfirm`（frontier 已追平）时展示「确认接管」按钮；
- 同步错误码全部映射为可读文案（未识别的原样透传），失败保留旧视图，绝不静默清空；
- API 层可注入测试替身（`ProcessorHandoffApi`），5 个 widget 测试覆盖让出/审批/接管/拦截文案。

### 验证结果

| 检查 | 结果 |
| --- | --- |
| `cargo test` | **680 passed / 0 failed / 17 ignored**（含 5 个 P4d 防线/审批集成测试） |
| `cargo fmt --all -- --check` | 通过 |
| `cargo build --release` | 通过 |
| `./scripts/regen.sh` | 桥接重生成，hash 一致（API 签名修正，Dart 桥接同步更新） |
| Flutter analyze（触碰文件） | No issues found |
| Flutter `fvm flutter test` | **354 passed**（含 5 个移交设置页 widget 测试） |

### 防线集成测试（`storage::sync::membership::tests`）

- `yield_requires_quiescence_and_disables_background_claims`：running 后台任务拦截 yield（`sync_background_processing_must_finish_before_enable`）且不留状态行；非加工端拒绝 yield；`relinquished` 后视图硬阻断 claim。
- `handoff_roster_moves_old_processor_and_holds_target_at_pending_takeover`：无 proof 的加工端变更拒绝；旧端立即采纳后继；目标端停在 `pending_takeover`（不切换本机 processor，无后台权利，状态跨重启保持）。
- `takeover_requires_frontier_alignment_and_restores_processing`：frontier 未追平拒绝接管（`sync_frontier_lagging`）；追平后确认切换并恢复后台权利；确认是单次状态机，不可隐式重放。
- `delayed_analysis_from_former_processor_is_bounded_by_handoff`：边界内 `<= final_sequence` 延迟分析放行（含 pending\_takeover 与接管后两个阶段）；超出边界严格拒绝且不落库。

### 实现修正（随本测试补齐时发现）

初版实现与 v31 迁移列名不一致（`from_sequence` vs `final_sequence`、`proof_json`/`dataset_id` 缺失）且 `confirm_processor_takeover` 误用本机流查 frontier——原有全量绿仅证明无回归，未覆盖这些路径。已修正为：handoffs 写入完整列（含 `dataset_id`/`proof_json`）；frontier 门禁改从 `from_device_id` 的成员流取水印；`apply_sync_analysis` 查 `MAX(from_sequence)`。

### 边界与未完成

- 旧加工端 != owner 时，yield 产生的 proof 尚无自动跨设备传输通道（v1 需 owner 设备与旧加工端同机，或用户手动携带 proof 审批）；后续作为 P4e/后续切片补充。
- 移交目标设备选择仍为手动输入设备 ID（设置页未提供活跃设备列表下拉）。
- P4e 撤销 UX/诊断/升级、原生密钥库签收、真人设备门禁继续未完成。
- 全程只用临时目录隔离库与合成身份；无个人数据/真实密钥/外部模型/线上中继。

## 20. 2026-10-14 P4e 切片 1（撤销 UX + 设备列表下拉）

### 目标与范围

上一节遗留两项 UI/UX 债务，本切片合并解决：

1. **撤销 UX**：存储层 `revoke_signed_sync_member`（含 `sync_membership_handoff_required` /
   `sync_membership_revision_changed` 等防线）此前无任何运行时/UI 通道。新增
   `revoke_member(runtime)` → `revoke_sync_member(device_id, expected_revision)` API。
2. **设备列表下拉**：移交目标设备从手动输入 ID 改为从名册 API 拉取的下拉。
   两项共享成员列表，合并实施。

### 实现

- `src/sync/runtime/handoff.rs`：`revoke_member(config, device_id, expected_revision)`，
  走 `with_unlocked_vault` + `require_sync_signer`（签名者不离开闭包作用域）。
- `src/api/sync.rs`：
  - `SyncRosterMemberDto`（`device_id/name/platform/active/is_owner/is_processor/is_local`）
    与 `SyncRosterDto`（含 `local_device_id`，供前端判断本机身份，不在前端重复守门逻辑）；
  - `get_sync_roster()` 只读（含已撤销成员，供 UI 审计展示，但撤销按 `active` 隐藏）；
  - `revoke_sync_member(device_id, expected_revision)`：校验 revision > 0 后转发 runtime。
- UI：
  - `SyncRosterController`（可注入 `SyncRosterApi`）：refresh / beginRevoke /
    confirmRevoke；**撤销绑定审阅时所见修订号**，并发更新由 Rust 侧
    `sync_membership_revision_changed` 失败关闭；`rosterErrorText()` 错误码映射。
  - `SyncMembersSettings`：owner 签名名册只读列表；撤销按钮仅 owner 本机可见，且仅对
    "活跃、非 owner、非加工、非本机"成员展示；加工设备行内提示「需先移交加工权」；
    确认对话框披露**丢失设备边界**（同步撤销不回收该设备已持有的 Provider API Key，
    提示到 Provider 控制台吊销/轮换）；失败保留旧视图 + 可读文案。
  - `SyncProcessorHandoffSettings`：目标设备下拉（活跃、非本机成员；owner 也可接收，
    用于移交回 owner），空候选禁用让出；yield/confirm 成功后回调刷新名册，
    两个区块视图保持一致。
- 设置页接线：`LocalSyncSettings` 可注入 `rosterController`（自建 + 随销毁）；
  成员区块位于锁定动作区之后。

### 验证结果

| 检查 | 结果 |
| --- | --- |
| `cargo test` | **680 passed / 0 failed / 17 ignored** |
| `cargo fmt --all -- --check` | 通过 |
| `cargo build --release` | 通过 |
| `./scripts/regen.sh` | 桥接一致 |
| Flutter analyze（触碰文件） | No issues found |
| Flutter `fvm flutter test` | **358 passed**（sync_members_test 4 例新增） |

UI 测试：owner 撤销普通成员成功（绑定 revision）；非 owner 本机无撤销入口；
`sync_membership_handoff_required` / `sync_membership_revision_changed` 映射为可读文案
且失败不清空视图；确认对话框披露 Provider Key 边界。

### 边界与未完成

- 撤销 UX 测试替身与移交一致：**消耗型状态队列从首次 getRoster 即消费**
  （widget postFrameCallback 即触发首刷，预留的"下一状态"会被首刷吃掉——
  需在 pump 之后再入队）。
- 原生 OS 密钥链签名、真人设备门禁（S06 发版门禁）仍未动工，见 §21。
- 长离线成员回连时的名册收敛/升级容差为 P4e 后续切片。

## 21. S06 发版门禁（待办清单，含人工项）

### 核实结论（2026-10-14，修正本节初稿的过度承诺）

原生密钥链**大部分已在位**：`ui/lib/sync/sync_key_store.dart` 经 `flutter_secure_storage`
以平台安全属性托管 master（Android 独立 `storageNamespace`、iOS/macOS
`first_unlock_this_device` + `synchronizable:false`、桌面 libsecret/DPAPI）；
设计上**刻意无降级链**（无 file/preferences fallback，失败关闭），错误码
`unavailable/missing/corrupt/revisionRollback` 完备且吞平台诊断防泄漏。
设置页有健康探针（`SyncKeyStore.probe` → `SyncKeyHealthSettings` 状态行）。

仍为真实的边界/待办：

1. **Ed25519 签名不出安全边界不可达**：同步签名用 ring 的 Ed25519，而 Android
   Keystore 与 iOS Secure Enclave 均不支持（Keystore 仅 RSA/EC-P 系，Enclave 仅
   P-256）。签名私钥在 vault 内、由 master 解密进应用内存是当前算法栈下的唯一
   可行姿态。发版文案承诺应降级为："master 由 OS 安全存储托管、本机限定、
   进程内零化；签名私钥不出应用进程"。
2. **硬件背书不可强制**：flutter_secure_storage 该版本未暴露 Android StrongBox /
   Secure Enclave 强制开关；如需硬件背书得平台通道自行实现（后续独立切片）。
3. **工程不变量（必须遵守）**：vend/加入后自动注册降级为"需真人确认"；
   禁止静默生成替代密钥；`_serialized` 锁不可重入——由此产生的 probe→load
   自死锁已修（探针内联读取，勿再从持锁操作内调用 load/readRevision）。

人工项（真人双设备门禁，发布前必须逐条执行，AI 不可代做）：
清单见 `docs/roadmap/2026-10-14-s06-human-gate-checklist.md`。

**2026-10-14 决定：开发预览默认关闭改为默认开启。** P4d 全链路完成后，
`ELSEWHEN_SYNC_PREVIEW` 从"默认关、显式开"翻转为"默认开、可显式关"
（`--dart-define=ELSEWHEN_SYNC_PREVIEW=false` 仍是发布关断手段）。
对应调整：设置页「同步」独立 tab；关断态文案改为明确写出编译开关名；
历史章节中"仅由开发预览打开"的表述不再反映当前默认值。

## 22. P5a：Wiki 页面复制底座（2026-10-14）

**范围（S05 第一片，只做 Rust 存储层，不接 FRB/UI）：**
`SyncMutation::WikiPageChanged { page, base_revision, deleted }`（新
`WikiPageRecord`，serde deny_unknown_fields）+ `src/storage/sync/wiki.rs`
（`sync_wiki_page_record` / `sync_record_wiki_page` / `apply_sync_wiki_page`）。
捕获点：`save_wiki_page_content`、`update_wiki_tags`、`set_wiki_opinion`
（opinion 级联更新素材组多页，逐页捕获），全部包在领域事务内（追加
`begin_write_transaction`；此前 `set_wiki_opinion` 的裸 `new_unchecked`
事务不满足 `sync_requires_domain_transaction` 检查，钩子已移入其内提交）。

**S05 语义落实：**
- 修订 CAS 与会话同款：base_revision → Waiting（修订未见过）/ Conflict
  `base_revision_changed`；**冲突候选不覆盖、不按时间取胜**（双端并发编辑
  各自保留本地版，`sync_conflicts` 各记一条）。
- **slug 是身份锚**：同 slug 不同 id → Conflict `slug_owned_by_other_id`
  （同名新页不得静默重挂旧引用）；同 id 换 slug → 直接拒绝
  `sync_wiki_slug_changed`（重命名通道后续切片再设计）。
- **证据闭包**：`source_event_ids` 引用的事件未抵达 → Waiting；重新投递后
  自动应用（依赖 outbox 未 ack 重发）。接收端证据/tags/last_seen_at 只并集
  不回退（系统合并列），内容列随记录整行替换。
- 删除：capture(deleted=true) 只推进 head，不改本地行（业务删除 API 归后续
  切片）；已删 head 优先报 `wiki_page_deleted`（先于 base CAS 的 generic
  原因）。

**测试（`src/storage/sync/tests.rs` +4）：** 复制+双端并发编辑冲突保留 /
同名 slug 显式冲突不重挂 / 证据缺失等待后补齐 / 删除后离线编辑保留候选。
门禁：**Rust 684 passed / 0 failed**，fmt --check、release、`scripts/regen.sh`
hash 一致（无 FRB 改动）。坑：Rust `\<newline>` 继续符会吞下一行缩进，
SQL 关键词（`FROM`/`UNION`/`VALUES`）被粘连——续接符前必须留空格。

**仍未完成（S05 余项）：** wiki_revisions 历史/来源快照闭包、的人工/AI 冲突
审阅 UI 与确认 token、待办/目标/规则对象、Wiki 专属会话引用、删除业务 API，
S05 门禁整体不因本片通过而勾选。

## 23. P4e 收官：长离线名册收敛 + 版本不匹配可诊断（2026-10-14）

**缺口（勘察结论）：** `stage_membership_roster` 此前只在配对/加入时手动调用；
`revoke_signed_sync_member`、加工移交审定等名册推进**只落本地**，无向其余成员
的再分发通道。长离线成员回连后永远停在旧名册：撤销对它不生效（仍按 active
成员收发）、移交边的另一端永远不知道 processor 已换手——名册不收敛是正确性
缺口，不只是体验问题。

**实现（owner 单向推送，成员无拉取义务）：**
- `Store::membership_envelope_needed(recipient, message_id)`（wire.rs）：outbox
  已有该 `membership:{revision}:{digest}` 记录（pending 在途或 acknowledged
  已确认）⇒ 不再补发；依赖既有重试与回执 ack 语义。非 autocommit 下调用
  （`sync_requires_domain_transaction`）。
- `Store::sync_member_active(device)`：名册成员活跃查询。
- `engine::stage_membership_roster_if_stale(...)`：仅 owner 生效（身份对齐
  `roster.owner`）；计算当前修订的 membership message_id，_needed 时在领域
  事务内 `stage_membership_roster`。成员/已撤销成员/无名册一律静默返回 false。
- `runtime/network.rs` tick 每成员循环接线（stage_receipts 之后）：任何名册
  推进后下一个 owner tick 自动扩散；回连成员被下轮 tick 补齐。
- NetworkTickReport 增 `roster_converged`（本轮为该成员补发过当前修订），
  仅做信号，不新增 API 面。

**版本不匹配可诊断（升级容差最小片）：**
- `PullReport` 增 `rejected_version_mismatch`：pull 拒绝分支用错误链文本
  匹配 `sync_protocol_unsupported`（不用 downcast，错误可能被 context 包装）。
- receive 层保持既有语义：无法 decode 的 op 按来源隔离（`isolated` +
  `error_code='sync_protocol_unsupported'`），不阻塞流、不应用；本片只是让
  该信号在 pull 报表可见，不做版本窗口兼容（真多版本共存需升级协商，留
  后续）。

**测试（membership/tests.rs +2）：**
- `owner_converges_stale_member_to_current_roster_revision`：b 加收 c 与撤销
  c 两次修订期间离线 → owner stage-if-stale 补发 head 修订 → b 应用收敛；
  在途幂等（不重复补发，仅留既有重试条目）；成员侧无分发义务；已撤销成员
  不再补发。
- `protocol_version_mismatch_rejects_as_version_mismatch`：伪造
  protocol_version=999 的 op → receive 按 isolated + sync_protocol_unsupported
  落库，applied=0（验证隔离语义而非 Err）。

**门禁：Rust 686 passed / 0 failed（+2）；fmt --check、release、regen hash
一致（无 FRB/Dart 改动）。**

**仍未完成：** P4e 的真人设备签收属 S06 人工门禁；S06 第 169 行整项（含
可用性降级、禁用/重启用、诊断面板展示该新信号）保持未勾选；多版本共存
协商未做（当前仍是严格 `== PROTOCOL_VERSION` + 隔离失败关闭）。

## 24. P5b：Wiki 业务删除（复制）+ 业务重命名双端（2026-10-14）

**切片 1——业务删除 API（commit `1a6d45f`）：**
- `Store::delete_wiki_page(slug)`：同领域事务内先 `sync_record_wiki_page(id,
  deleted=true)` 捕获（趁行在），再物理删除本行；修订与素材链接随外键级联。
  不存在的页返回 `Ok(false)`（幂等）；非同步库退化普通删除。
- 删除语义 = 全端一致物理删除（收端 P5a 起 deleted op 即物理删除），不是本
  端标记；对端离线迟到编辑落 `wiki_page_deleted` 冲突候选，不静默覆盖。
- slug 随物理删除释放（新页可重用）；老 id 的 head 保留用于冲突报告。
- UI：详情页正文栏新增「删除页面」（仅人工页可删，素材页留给导入维护），
  破坏性确认披露不可恢复 + 冲突候选语义，删除后关页 tab、失效列表。
- 测试 `wiki_page_business_delete_removes_row_and_replicates`。

**切片 2——业务重命名双端（本片）：**
- 缺口：`rename_wiki_page` 本端完整（换 slug、迁移 relations/conversations、
  撞名避让），但无捕获——slug 变更从不复制，多端必漂移。
- 协议新增 `SyncMutation::WikiPageRenamed { old_slug, page, base_revision }`：
  真正换 slug 的 rename 捕获为该变体；标题-only（无前缀页保留 slug）仍走
  `WikiPageChanged` 整行捕获。validate：old_slug 走 slug 规则，且必须
  `old_slug != page.slug`（`sync_wiki_rename_noop`）。
- 捕获点在 `rename_wiki_page` 领域事务内：**base 锚点在本地变更前读取**
  （`sync_wiki_base_revision`），变更后 `sync_record_wiki_rename`。
- 应用端 `apply_sync_wiki_rename` 判序（关键：**先按 slug 判序，再做 CAS**）：
  1. head 已删 → `wiki_page_deleted` 候选；
  2. 本端无该 id 行 → Waiting（页本体 op 在途）；
  3. local_slug == 新 slug → 幂等重放只推进 head（含两端殊途同归改到同
     slug 的情况）；
  4. local_slug != old_slug → **`wiki_page_rename_diverged`**（两端各自改名，
     显式冲突不选边）；
  5. 过了 slug 判序才做 base CAS（未知 → Waiting；不匹配 →
     base_revision_changed）——两端改名时 head 必然双双推进，CAS 先行会把
     真分歧掩盖成 Waiting/泛化冲突（第一版实测踩过）；
  6. 新 slug 被别的 id 持有 → `slug_owned_by_other_id`；
  7. 通过：换 slug/title/summary + 本端同语义迁移 relations/conversations。
- 派生 slug 的撞名避让在起源端定名，收端原样采用；收端撞名即显式冲突，
  人工审（与 slug 身份锚立场一致，不静默改名）。
- 测试 +3（sync/tests.rs）：重命名复制+收端引用迁移+幂等重放；双端离线
  各自改名 → `wiki_page_rename_diverged` 且各自保留本端；删除后迟到重命名
  落 `wiki_page_deleted` 候选。

**门禁：Rust 690 passed / 0 failed（+4 累计）；fmt --check、release、regen
一致（无 FRB/Dart 改动，WikiPageRenamed 未暴露 Dart——无 UI 重命名入口，
rename 目前只有 AI/审批工具链调用）。**

**P5b 仍未完成：** 修订/来源快照闭包；Wiki 专属会话引用（S05）；冲突候选
人工审阅 UI（S05 大项，含 rename_diverged / slug_owned_by_other_id 的展示）。
