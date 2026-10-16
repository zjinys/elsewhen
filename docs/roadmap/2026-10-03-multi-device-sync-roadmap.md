# Elsewhen 多设备同步 Roadmap

## 目标与归属

实现“在任意授权设备离线记录，在其他设备继续查看/对话”，并让**新设备授权加入后获得完整 Provider 配置（含 API Key），无需重新填写**。后台加工有明确归属，可变内容有版本与冲突审阅；同步不自动批准或重放业务动作。

本路线是[个人认知主循环总路线](2026-09-17-personal-cognition-main-loop-roadmap.md)的同步子轨，不重置既有 AI 对话和 Wiki 进度。规范与取舍见[同步设计 Note](../notes/proposed/architecture/2026-09-30-multi-device-sync.md)，现有传输原型及已知边界见[中继说明](../../relay/README.md)。

**开发交接：** [2026-10-07 实现状态与接手顺序](2026-10-07-multi-device-sync-handoff.md)区分已有底座、App 缺口、历史测试与待补验项；后续最新状态仍以本路线的完成记录为准。

## 执行状态

- **⚠️ 本路线已于 2026-10-14 关闭（开发完成）。** S00–S05 全部交付（工程与自动化验收记录见完成记录与
  [Handoff §1–§31b](2026-10-07-multi-device-sync-handoff.md)）；S06 真人发布门禁移交
  [人工测试路线 M0–M7](2026-10-14-sync-manual-test-roadmap.md) 与
  [S06 门禁清单](2026-10-14-s06-human-gate-checklist.md) 执行——那是唯一的发版阻塞项。
  **下文各切片的细项勾选是开发过程中的工作态快照，不再逐一回填**；以各切片最新的 Handoff 章节为准
  （如 S01 的「接入事件统一提交核心」等未勾项实际已由 P3/P4 落地）。代码基线 `f7f6e74`，
  迁移 34，快照版本 3；最终门禁：Rust 711 passed、Flutter 372 passed、analyze 0 error/warning。
- **建立日期：** 2026-10-03。
- **已交付：** 方案修订，以及 S01 复制底座、S02 加密/中继加固、S04 Provider 加密应用和 owner 邀请开发预览；owner P0 补验已完成，全仓质量例外单列，整个同步功能尚未完成。
- **P0 已验证基线：** schema v16–v24；签名成员授权、隔离数据集目录册、App 本机准备/解锁、受保护修订、工作区租约、Provider 静态加密及编辑基底已接通。2026-10-07 基于 `6743a09` 的补验：Rust **588 passed / 11 ignored**（另配置环境 **5 passed**）、release 通过；Flutter owner 定向 **6 passed**／同步相关定向 **25 passed**／全量 **312 passed**；实际 Worker **14 passed**。owner 格式和定向 analyze 通过；全仓 Rust 格式仍有 3 个既有文件差异，Flutter 全量 analyze 有 78 项问题，不记为通过。证据见完成记录及[开发交接](2026-10-07-multi-device-sync-handoff.md)。
- **当前 P1 基线：** 2026-10-08 工程与自动化验收完成；Rust 600 passed / 11 ignored（另配置环境 5 passed）、Flutter 新增定向 11／同步相关定向 36／全量 323 passed、实际 Worker 14 passed，P1 格式/定向 analyze、release 通过。全仓格式 3 个文件差异和 Flutter analyze 78 项仍未通过；详见完成记录与 Handoff §9。
- **P2 已验收（2026-10-11）：** 安装证明/正式通道/名册/快照自动交接的工程与自动化验收完成。P2 Rust 定向 16 passed、全量 Rust 640 passed / 16 ignored（另配置环境 5 passed）、实际 Worker 15 passed（含完整生产入组故障恢复）、桥接重生成/release 通过（hash 233902080）；Flutter 控制器 11 passed、同步相关定向 **48 passed**、全量 **345 passed**、P2 定向 analyze 无问题；P2 Rust 23 文件与 Dart 10 文件格式通过。同日经用户明确要求，`cargo fmt --all` 修复全部 16 个文件（历史 3 个 + 并行改动）后全仓 `--check` 通过并复验全量/release；唯一保留例外为 Flutter analyze 71 项全 info，单列。详见完成记录与 [Handoff §12](2026-10-07-multi-device-sync-handoff.md)。
- **P3 已验收（2026-10-12）：** 数据集选择/修复 API（`list_sync_datasets`/`select_sync_dataset`/`select_default_sync_dataset`，仅下次启动生效、以用户所见预览+原生确认 vault 提交）与 App 自动同步调度（网络稳态 tick 复用持久 outbox/回执台账，接入同一后台 worker，状态页显示范围/待同步/最近成功/异常）的工程与自动化验收完成。全量 Rust **640 passed / 17 ignored**、实际 Worker **16 passed**（含新 P3 稳态增量用例）、Flutter 全量 **345 passed**、桥接重生成/release 通过；全仓 Rust 格式 `--check` 通过；Flutter analyze 72 项（71 info + 并行任务新增 1 warning，P3 文件 0 项）继续单列。详见 [Handoff §13](2026-10-07-multi-device-sync-handoff.md)。
- **首个可用范围：** S01–S04 与 S06 对应发布门禁通过后，开放记录、普通对话、分析结果、Provider 完整配置同步。Wiki/待办/规则的完整同步依赖 S05。
- **更新规则：** 每个切片完成立即更新本文件及总路线，列出代码、测试、失败与剩余边界。内部原型通过不等于真实设备发布验收通过。

| 切片 | 内容 | 依赖 | 终态（2026-10-14 关闭） |
|---|---|---|---|
| S00 | 方案、范围与验收清单 | 无 | 完成 |
| S01 | 双库领域复制核心 | S00 | 完成（P0–P3 交付，证据见完成记录） |
| S02 | 配对、真实加密与中继权限 | S00 | 完成（P0–P4e 交付；含 §30 扫码加入） |
| S03 | 快照、投递、回执与恢复 | S01、S02 | 完成（P3/P4 交付；含快照 v2 §29、v3 §31） |
| S04 | App 接入、Provider 开箱即用、加工归属 | S01–S03 | 完成（P4/P5a/P5b 交付） |
| S05 | Wiki 等可变对象与证据闭包 | S04 | 完成（Handoff §24–§28） |
| S06 | 设备生命周期与分范围发布验收 | S01–S05 | **移交人工测试路线**（M0–M7 + S06 门禁清单）；开发侧条目已随 P4d/P4e 完成 |

## 不变式

1. 本地捕获的事件、输入、消息关联与待加工资格原子保存；离线不阻塞，原始事件不改写。
2. 全部 Provider 信息属于首版加密同步范围；设备身份/配对私钥仍不复制，普通设置 DTO 和日志仍脱敏。
3. 相同操作幂等，同 ID 异文拒绝。来源水位、领域事务和回执一致，不能先确认再落库。
4. 同步内容不是执行授权。历史消息、运行状态、提议文本和业务回执不触发新模型调用或旧动作重放。
5. 后台加工由指定设备负责，非加工设备不能领取；前台聊天由发起设备显式启动。
6. 可变对象按基底版本处理，删除留 tombstone；人工修改与不确定副作用不静默覆盖/重做。
7. 中继只投递认证密文，不裁决业务冲突；本地 epoch/索引不是分布式互斥。
8. 新设备加入现有数据集，不自动合并两份独立数据库；已有本地数据和 Provider 不被覆盖。

## S00 — 文档与契约基线

- [x] 原位修订同步 Note，替换单 writer/软只读、通用 INSERT OR IGNORE 及 Provider 不同步的旧规范。
- [x] 定义记录/对话首版、Provider 完整同步、后台加工归属和后续 Wiki 范围。
- [x] 新增本路线，修正 relay README 实际状态，并关联总路线。
- [x] 将文档中的协议约束转成 S01/S02 的类型、固定向量和可执行测试；独立 Node/OpenSSL 生成的 v1 信封向量冻结 AAD/签名 framing、AEAD 密文与 Ed25519 签名，Rust 读取/验证与逐字节重建通过。功能出口仍由各切片验收。

**出口：** 文档内无相互冲突的当前规范，有代码接入位置和明确待实现范围。不以 S00 宣称任何同步功能已完成。

## S01 — 两份数据库的复制核心

**收益：** 先证明断网追加、重复收件和崩溃恢复不会丢记录，隔离传输问题。

- [x] 新增 v16 复制存储底座：安装身份/成员/来源流、操作、outbox/inbox 状态、连续水位、对象版本/冲突和删除标记；未修改历史迁移。
- [x] 定义有界版本化领域包：稳定 op/object/revision ID、依赖、来源序号、内容哈希和成员范围；不暴露任意表/SQL。
- [ ] 接入事件统一提交核心及 Capture/主对话/工具记录路径；统一输入的 event/message/input_record 是原子单元。导入保留原始 ID/时间，不通过重新捕获制造副本。
- [ ] 普通对话创建和消息追加保留因果/父对象；重命名、归档和删除具备最小版本契约，避免首版只复制新增却留下旧标题或复活已删会话。
- [x] 准备 Provider bundle 的版本化类型：完整 profile、明确 Key 语义、激活引用、备用顺序；合成 Key 的加密应用、轮换/删除、重放与冲突测试通过，普通 DTO 仍脱敏。
- [ ] 接收端在一事务内验证去重、应用完整对象或持久保存冲突、推进连续进度、生成应用回执；导入不重新创建本地操作形成同步回声。
- [ ] 缺序号/父对象/未知版本/同 ID 异文有等待、隔离与补齐状态，不能吞掉失败再 ack；其他独立来源不被全局阻塞。
- [ ] 未启用完整加工归属前只作为测试能力，不开放真实多设备运行。

**主要位置：** `src/storage/events.rs`、`src/storage/conversations.rs`、`src/storage/mod.rs`、`src/storage/new_migrations.rs`；拟新增 `src/storage/sync/` 与 `src/sync/protocol.rs`。

**验收：** 两/三份临时库，双向并发追加、时钟漂移、重复/乱序/缺口、部分事务失败、未到父对象、同身份异文、已有外键/不可变约束、对话删除后迟到消息、导入不自动生成。覆盖全部记录来源，不只测一个新入口。

## S02 — 配对、加密与中继加固

**收益：** 内容及 Provider Key 只能进入已授权设备，投递权限不能变成收件箱管理权限。

- [ ] 固定认证密钥交换协议与库、邀请兑换和确认状态机、设备公钥绑定；带外邀请短期/一次性，不自行设计裸 token 握手。
  - [x] 后审批恢复切片：批准后的响应/方向密钥、安装证明和完成回执只入本机加密 vault；保护修订确认前不发送，授权/密钥安装/来源 outbox/回执同事务。10 项隔离存储、3 项运行时测试通过；未审批 Noise 邀请重启失效。2026-10-07 补齐加密临时投递能力、恢复清单和撤销门禁，实际 Worker 14 项通过。App 邀请传输、joiner 原生解锁和 relay 授权协调仍未完成。
- [x] 明确数据集成员授权链与初始化 owner 身份；新成员由授权设备确认，不以新凭证自签证明入组资格。首版采用固定 owner → 成员的一跳签名名册，不做委派或 owner 轮换。
- [x] 修复 Worker owner/sender 能力边界：授权注册、续期/撤销、限定收件人的发送能力；fetch/ack/status/成员管理仅 owner。
- [x] 实现 ring ChaCha20-Poly1305 + Ed25519 的独立信封模块，认证绑定数据集/收发设备/Key 版本/消息身份，正文和合成 Provider Key 不出现在 wire 明文；11 项测试通过。这不等于配对或操作重放层完成。
- [ ] 实现真实 AEAD、唯一 nonce、用途/方向/设备/数据集/密钥版本隔离，以及来源操作/快照/控制记录验证。邮箱身份不得因共享密钥相同而相同。
- [x] 本机密钥持久化底座：Flutter 原生主密钥适配（失败不重置/不降级明文）与 Rust 加密 vault、版本 CAS/快照排除；原生平台验收与 App 解锁协调单列未完成。
- [ ] Provider 敏感 bundle 加密封装后才进入持久 outbox、快照暂存和传输；身份私钥留本机，普通日志/DTO/错误报告不出现 Key。
- [x] 验证签名规范使用请求原值，TTL 截断不要求客户端猜服务器时间；nonce 重放、授权版本、续期/撤销和 owner 轮换已有实际 Worker 回归。
- [ ] 修复取件的排他游标与 mailbox 前缀、有界收件/JSON/解压/页大小/配额；用实际运行时验证并发、崩溃恢复和计账，不假设 DO 所有异步路径天然事务化。
- [ ] 首版不依赖 presence/peers；后续启用前修复跨邮箱路由，不能以“空列表正常”证明设备发现可用。

**主要位置：** `src/sync/transport_relay.rs`、`relay/src/worker.ts`、`relay/test/`，拟新增配对/加密模块与本机密钥存储适配。

**验收：** 固定加密向量；未经授权注册、sender 读删、跨邮箱访问、篡改/重放、撤销/续期、错误目标、截断载荷、限额/分页；直接运行实际 Worker 的多邮箱路径。现有 `crypto.test.mjs` 的镜像逻辑测试不能代替生产路由测试，Rust 自发自收 smoke 不能代替两个设备。

## S03 — 一致快照与可恢复投递

**收益：** 新设备一次加入、旧设备长离线后仍可补齐，不依赖设备同时在线。

- [x] 受支持对象的一致快照绑定每来源水位；分块清单/哈希、暂存校验、原子发布，快照期间新增写入继续保留。
- [x] 初始快照携带完整加密 Provider bundle；Key 不落明文临时 SQLite/JSON 文件。
- [x] 超过 1 MiB 单信封的领域增量按 256 KiB 分块；总操作上限 16 MiB，未收齐不应用，整条操作应用回执才停止分块重发。
- [ ] 持久 outbox 调度，重试退避；中继投递确认、接收端应用回执和 GC 水位分别建模。
- [ ] 上传响应丢失、下载中断、落库后回执丢失时按原 op 身份重试；不重复保存用户输入。
- [ ] 每来源连续进度、依赖补齐和有界隔离队列；未知协议阻断对应范围，不静默降级导入。
- [x] 新成员加入的快照/增量保留窗口、落后成员、TTL 过期、配额满、补发/重快照以及 tombstone 生命周期。（2026-10-13，见 Handoff §17–§18；迁移 v29/v30 + 每成员应用水印 + catch-up 窗口 + needs_resnapshot + 单调 GC 游标，落后成员失败关闭不静默清历史；relay TTL 语义=仅丢传输副本；§18 补上自动重快照调度：durable 请求行 + 单调 nonce + 请求绑定导出 + 合成应用 ack，完成时用构建时 frontier 重播种水印解除 GC 冻结）
- [ ] 受支持备份恢复/重装需重新登记来源流或设备身份，旧流冲突明确拒绝；不复制运行租约。

**主要位置：** 拟新增 `src/storage/sync/{snapshot,delivery}.rs`、`src/sync/engine.rs`；`transport_relay.rs`。

**验收：** 大于单信封上限的分块快照、导出过程中持续写入、收件途中断电、回执丢失、三设备中的长离线者、新设备尚未完成快照、TTL/配额、旧协议、恢复旧数据库。源数据清理和接收进度逐一核对。

## S04 — App 接入与新设备开箱即用

**收益：** 在真实 App 完成配对、记录同步与 Provider 恢复，同时不引入重复后台执行。

### Provider 完整配置

- [x] Rust 内部使用完整配置读取路径，不能从读取时 `api_key=''` 的设置 DTO 导出。
- [x] profile ID、名称、类型、URL、模型、实际 Key、temperature、max_tokens、context_window 全量保持；显式空值不变。
- [x] 整体发布配置集合、激活项和故障切换顺序；新设备不是“谁先导入就先激活”，无需再次填写/选择。
- [x] 同步应用提供明确的 Key 替换/删除语义，不复用设置表单“空串保留旧 Key”；密钥/URL 并发变更不能自动混配。
- [x] 保存、删除、切换激活项、Key 轮换都生成版本化 bundle；冲突掩码展示，并绑定用户实际看到的版本。（2026-10-13，见 Handoff §15/§16；Rust 存储 + FRB API + Flutter 审阅对话框接线完成并测试）
- [x] 保护接收端落盘密钥，保留数据库作为配置真源、FRB 读取脱敏；受保护安装使用 AEAD 密文列与禁止明文写入的触发器，增量/快照、锁定/解锁、受控调用与隔离 bridge 已测。原生平台签收、历史明文/备份清理不是本项承诺。
- [x] 对环回/不可达地址明确提示，保持原配置，不静默改 URL、切换未经选择的 Provider 或自动探测外部模型。（2026-10-13，`src/api/provider_reachability.rs`：纯静态提示码，冲突审阅 DTO 逐条携带；设置保存路径接线仍属后续切片）

### 后台与前台执行

- [x] Rust/SQLite 后台领取防线：非指定或非活动设备、初始快照暂存阶段不能领取；启用复制前拒绝切走在途加工，自动知识发布重验本机权限。尚不等于跨设备加工移交。
- [ ] 开启同步时明确后台加工设备，处理旧队列在途/待处理状态；非加工设备不领取任务，仅 UI 禁用不够。
- [ ] 稳定任务身份绑定输入版本和策略，结果与完成状态原子保存；已有分析结果不重复跑，派生结果仅同步支持的类型。
- [x] 终态回复绑定已保存的原输入并复制只读关联；跨设备并发/输入晚到/首次快照保持轮次，不复制 run、流式草稿或派发授权。
- [ ] 前台 run 绑定发起设备与原输入；接收远端历史/终态不调用生成，跨设备并发答复仍归属正确输入。
- [ ] 旧提议/授权/租约不跨设备自动执行，不确定动作不盲目重试；停止队列不会因同步恢复派发。

### UI 与生命周期

- [x] App 本机准备/解锁入口：原生密钥与受保护修订确认后才恢复后台；工作区文件租约防止跨进程抢占，旧请求取消不能关闭新会话。本机准备在开发预览开放，尚不启动配对/联网同步。
- [x] 配对确认说明会同步 Provider API Key；完成后自动恢复配置，显示明确的初始同步进度。
  - [x] owner 开发预览入口：输入中继地址后创建短期邀请，显示/复制邀请，核对设备指纹与原预览 token 后明确批准；系统修订保存/回读/确认先于发送，失败可重发同一审批而非重复批准。5 项 Rust、6 项新增 Flutter 测试及实际 Worker owner 流程通过。joiner UI、自动安装确认与初始同步仍未完成。
  - [x] owner P0 补验：基于 `6743a09`，owner 格式/定向 analyze、Rust 定向 5/3/10、Rust 全量 588 passed / 11 ignored（另配置环境 5 passed）、release、Flutter owner 6／同步相关定向 25／全量 312 passed、实际 Worker 14 passed。全仓格式 3 个文件差异及 Flutter analyze 78 项仍未通过，详见完成记录和 Handoff；不据此勾选整个配对或发布出口。
  - [x] P1 joiner 工程与自动化验收：独立暂存库、专用 native 确认、Noise 请求/审批安装、原证明投递/恢复及 UI 接入完成；Rust 定向 12／全量 600 passed（11 ignored，另配置环境 5 passed），Flutter 定向 11／同步相关 36／全量 323 passed，实际 Worker 14 passed，P1 格式/analyze 和 release 通过。全仓质量例外见完成记录；不勾选 P2、真实设备或整个配对出口。
  - [x] P2 自动交接工程与自动化验收（2026-10-11）：owner 自动接收/验证安装证明、正式方向授权与临时撤销的可恢复协调、joiner 签名名册/Provider 保护事务、快照 offer/选择回执/分块/应用回执/完成通知及 Flutter 单飞协调器/只读进度完成；P2 Rust 定向 16／全量 640 passed（16 ignored，另配置环境 5 passed），Flutter 同步相关定向 48／全量 345 passed，实际 Worker 15 passed，桥接 hash 233902080，P2 格式/定向 analyze 通过。全仓例外单列；不勾选原生签收、数据集选择或整个 S04 出口。
- [ ] 设置中提供设备、范围、上次成功、待同步和异常状态；无数据也可发现加入入口。
- [x] 数据集隔离底座：独立目录/本机目录册、已验证导入的选择令牌、启动选择原子提交；保留默认库与 Provider，运行中不切换 `AppConfig::pin`，重启后才生效。
- [ ] 已有独立数据库不覆盖：数据集隔离/选择明确，不以“开箱即用”为由自动合并或替换个人数据。
- [ ] 应用启动/恢复前台/网络恢复自动同步，后台调度有界；不新增知识后台手动触发/重试入口。
- [ ] 新消息与有效结果到达刷新相关视图；缺失 Wiki 引用明确不可用，不显示假链接或可执行旧提议。

**主要位置：** `src/storage/provider.rs`、`src/api/provider_config.rs`、`src/storage/events.rs`、`src/storage/digest.rs`、`src/api/chat_runs.rs`、`src/storage/approvals.rs`、`ui/lib/bridge/rust_bridge_repository.dart`、设置与对话 UI；拟新增 `src/api/sync.rs`。

**验收：** 两个 App/隔离 bridge：A 配置多个合成 Provider，B 授权后不填 Key 即对受控本地服务调用成功；激活项/备用顺序/窗口一致，改 Key/删配置/并发修改正确。两端都能离线记录，只有指定设备后台加工；任一端查看远端消息不生成、不执行。真实外部模型测试另行授权。

## S05 — 可变对象与完整知识依赖

**收益：** 把基础版本/冲突机制扩展到真正可用的知识库，而不是只搬运页面文本。

- [ ] Wiki 页面稳定 ID 与 slug 映射、来源快照、修订、引用、依赖及失效状态的完整闭包。
- [ ] 不同 ID 的同名 slug 显式处理；旧页引用不能被默默指向同名新页。
- [ ] 人工/人工、人工/AI、删除/编辑冲突保留候选，审阅产生新版本；不靠“时间更新”覆盖。
- [ ] 待办、目标、规则及其状态迁移遵守现有业务约束；并发规则不自动执行。
- [ ] Wiki 专属会话与引用可用性随依赖一起开放，缺失证据等待补齐，不推断历史依据。
- [ ] 审阅显示的来源/页面版本与确认 token 一致；导入数据不得绕开既有确认流程。

**主要位置：** `src/storage/wiki.rs`、`src/storage/knowledge.rs`、`src/knowledge/dependencies.rs`、`src/knowledge/review.rs`、`src/knowledge/maintenance.rs`、相关 FRB/UI。

**验收：** 两端编辑同一页、同名不同页、源版本先后抵达、依赖未到、删除后离线编辑、审阅期间再同步、规则/待办状态冲突、断档快照恢复。原事件与原始来源保持不变。

## S06 — 设备生命周期与发布门禁

- [ ] 设备重命名、撤销、凭证/内容密钥轮换，不允许失效成员自行恢复权限。
- [ ] 丢失设备的提示说明：撤销同步权限不回收已持有的 Provider Key；需在 Provider 侧吊销/轮换，剩余设备接收新版本。
- [x] 正常加工移交：旧端停止领取、收尾在途、完整状态交接、新端确认接管；旧端未确认时不自动提升新端。（P4d 已完成，见 Handoff §19）
- [ ] 验证迟到结果、本地 epoch 边界、备份恢复和旧安装重连；不声称盲中继或单机索引提供全局执行互斥。
- [ ] 同步版本升级、可用性降级、禁用/重新启用、长离线与错误诊断；诊断不泄露内容/Key。
- [ ] **首版发布门禁**：S01–S04 对应 Rust/Worker/隔离 bridge 全部通过，至少桌面与移动端实际加入、离线记录、重启补齐、Provider 恢复、撤销提示由真人签收。
- [ ] **完整知识同步门禁**：S05 与其真实设备审阅/证据/删除场景签收；不因首版通过而勾选。

自动脚本、mock/bridge 与人工设备验收分开记录。用户真实数据、真实 API Key、外部模型请求或部署需要另行授权，不能从文档认可推导授权。

## 验证与交付命令

每个切片先跑定向测试，再按涉及范围运行项目门禁：

```bash
# 仓库根目录：使用临时/内存数据库
cargo test --all-targets
cargo build --release

# 公开 Rust API 改动后先同步桥接
./scripts/regen.sh

# ui/ 目录：必须通过 FVM
fvm flutter analyze
fvm flutter test

# relay/ 目录：现有镜像测试只是基础；S02 增加实际 Worker 用例
npm test
```

真实 bridge 初始化使用 `ui/test/support/isolated_bridge.dart`。中继运行测试只访问显式启动的隔离本地 Worker；不得自动探测或写线上服务。秘密材料只使用合成 Key，记录通过/失败而非密钥内容。运行期实际 Worker/加密向量门禁在 S02 随实现加入，不能把尚不存在的命令写成已通过。

## 暂不进入首版

- P2P、局域网发现、NAT 打洞、通用文件路径同步（注：推文配图目录经快照 v3
  目录段定向同步已实现，§31b——此处「同步文件路径」仍保持关闭，指任意本地路径）；

- 任意两份已有独立数据库自动合并；
- 所有文本自动合并、通用 CRDT、依赖时钟的 LWW；
- 跨设备复制可执行审批令牌、执行租约和未核验流式草稿；
- 失联加工设备自动接管或“恰好一次”外部副作用承诺；
- 用中继短期邮箱替代备份，或承诺移动系统休眠期间实时同步。

## 完成记录

### 2026-10-16 — 配对死锁排查与手动解锁（S06 验收中的实证发现，修复未落地）

- **现象：** 手机（joiner）扫码后永远停在「等待成员授权与初始同步」`phase=receiving_snapshot`（每 2s advance 空转）；桌面 owner 永远 `sending_snapshot`/`awaiting_roster`。本机局域网 relay 健康（1ms 响应，`/health` 正常），排除网络。
- **根因（数据实证）：** owner 端 `sync_members`/`sync_member_keys` 中该手机设备 `active=0`，而 `storage/sync/wire.rs::due_enrollment_wires` 硬条件要求收件人 `active=1` → 164 条快照消息 `attempts=0` 从未被推送（同一条 membership 控制消息除外，重试 13 次）。历史五次配对尝试中同型死锁至少两起（设备 `430369be` 一条 membership 消息重试 **850 次**、`59559318` 全部 0 次从未出门）。**且 outbox 无放弃语义**：`state` CHECK 仅 `pending/acknowledged`，无ack 消息按退避无限重试。
- **手动解锁（用户同意，库已备份 `elsewhen.db.bak-20261016`）：** ① 置位手机设备 `sync_members.active=1` 与 `sync_member_keys.active=1`；② 删除三个死设备（`430369be`/`59559318`/`bcef5dfd`）共 492 条 pending 信封及孤儿 `sync_snapshot_export_wires`，死设备导出记录标 `aborted`；③ 设备 `47c8af9e` 为真实完成同步的成员（165 条全部 acknowledged），保留。解锁后待验证：手机应收到快照推进至 `awaiting_receipt`/`complete`。
- **待修工程项（两件，未开工）：** ① **死锁修复**：查「roster pin 时 `UPDATE sync_members SET active=0` 全员清零后由 `apply_signed_roster` 重置」路径的投影失步（新成员被丢在 0，`storage/sync/membership.rs:161`），以及 `due_enrollment_wires` 是否应豁免 enrollment 载荷的 `active=1` 前置（enrollment 是建立 active 的仪式，被前置卡死互为死锁）；② **outbox 生命周期**：信封过期（`GRANT_LIFETIME_MS`）后本地不可达标记/成员撤销与邀请过期时的批量清理，消除无限重试与悬空流量（与 S02「悬空邀请清理入口」缺口同属清理面）。
- **边界：** 本次仅数据侧手动处置与文档记录；`active=1` 为诊断性 workaround，vault 内签名名册的本源状态未验证（UI 成员页只读 vault，后续修复需核对投影与本源一致性）。没有代码变更、没有跑测试门禁、未提交。

### 2026-10-11 — P2 工程与自动化验收完成（原生/真人门禁单列）

- **收尾复验（本 session 逐条实际执行）：** P2 Rust 定向 **16 passed**、全量 Rust **640 passed / 16 ignored**（`--all-targets` 另含示例目标 0 tests）、配置环境定向 **5 passed**、实际 Worker **15 passed / 0 failed / 0 skipped**；桥接两侧 hash 均为 **233902080**（10-10 重生成/release 产物）。Flutter P2 定向 analyze（9 个目标）**No issues found**、同步相关定向（10 个测试文件）**48 passed**、全量 **345 passed / 0 failed**；`git diff --check` 通过。
- **P2 出口达成：** 批准方案列出的 owner 证明接收→原子授权/密钥→native 修订确认→双方正式授权→签名名册/Provider 保护→快照 offer/选择回执/分块/应用回执/完成通知全部经生产运行时验证（10-10 Worker 故障代理用例 + 本轮复验）。仍不切换 `AppConfig::pin`、不激活通用增量调度、不签收原生 OS 密钥库。
- **全仓例外：** 同日经用户明确要求，`cargo fmt --all` 修复全部 16 个 Rust 文件（历史 3 个 + 并行 AI/web-search/DoH/wiki/config 改动，纯格式化无语义变更），`--check` exit 0，复验全量 **640 passed / 16 ignored** 与 release 通过。Flutter 全量 analyze 为 **71 项全 info（0 error / 0 warning）**（10-10 首轮 75 项，P2 括号 lint 修复后下降；与 P1 基线 78 项不混用），继续单列未修，未关闭 lint。
- **边界：** P2 完成不等于 S02/S03/S04 总项勾选或 S06 发布门禁；P3 数据集选择/修复、通用生命周期同步、GC/长离线、加工移交、S05 与真人设备验收继续未完成。本轮没有提交、推送或部署，没有使用个人数据/真实密钥/外部模型/线上中继。证据与复现命令见 [Handoff §12](2026-10-07-multi-device-sync-handoff.md)。

### 2026-10-10 — P2 自动化补验，最终静态检查收尾中

- **实际通过：** `cargo check --tests`、P2 Rust 定向 **16 passed**、Rust 全量 **640 passed / 16 ignored**（另配置环境 **5 passed**）、实际 Worker **15 passed / 0 failed / 0 skipped**；P2 的 23 个 Rust 文件格式通过。公开桥接已重新生成并完成 release 构建，Rust/Dart hash 均为 **233902080**。
- **完整入组证据：** Worker 新用例实际运行生产 owner/joiner P1→P2，验证两个 grant、一次 revoke 和各类回执在服务端提交后丢响应时保持原参数/新 nonce 重试；native 未保存时重开、选定快照后恢复、并发捕获保留、Provider 静态保护/完整配置与受控环回调用通过。没有切换当前数据集或放开后台领取。
- **Flutter：** enrollment 控制器 **11 passed**；release 后全量首轮 **343 passed / 2 failed**。并行 Markdown 渲染使旧 finder 失效，完整工具请求超出模拟模型默认 32K 窗口；仅适配 RichText 断言和 64K 本地模拟窗口，保留所有连续性/审阅/原输入断言，不改生产预算或并行业务。复验全量 **345 passed**，P2 Dart 格式 **10 files / 0 changed**。
- **仍待收尾：** P2 joiner 的括号 lint 已改，但最终定向 analyze、同步定向集合仍在命令启动前遇到工具服务 unavailable/timeout。全仓 Rust 格式首轮为 **13 个文件**（含历史 3 个和并行改动），Flutter analyze 首轮 **75 项（2 warning、73 info）**，不沿用 P1 的 3/78 当作当前结果，也不修改无关文件清空报告。
- **边界：** P2 最终静态门禁待补，暂不勾选，不进入 P3；原生系统密钥库、真人双端、数据集选择、GC/加工移交和 S05 仍未完成。逐条执行日志与接续命令见 [Handoff §12](2026-10-07-multi-device-sync-handoff.md)；统一验证脚本未成功启动，不能宣称脚本整体通过。没有提交、推送或部署。

### 2026-10-09 — P2 接续实现（验证未完成）

- **Rust：** 补充 `runtime/enrollment/tests.rs` 与 `tests/boundaries.rs`；原生确认绑定 role/operation/invitation/token，owner 使用不恢复加工队列的专用确认。grant/revoke 原意图固定，401/403/409 与丢响应重试区分；坏证明不阻塞后续邮件，存储故障不冒充坏邮件吞掉。完成通知等待对端回执，失败导出的同名名册不阻塞新的 ready 快照。均待编译与回归，不当作安全验收结论。
- **Flutter：** enrollment API/协调器接入 owner 解锁/审批、joiner 安装/恢复与设置进度；生产仓库显式启用，旧 fake 默认关闭。单飞、有界退避、原生保存/回读后确认、关闭等待在途写入、旧 operation 不接管新会话均有新增测试定义。没有数据集切换、通用增量 worker 或知识手动触发。
- **实际 Worker 用例：** 新 `enrollment_ui.rs` 与 `relay/test/enrollment-runtime.test.mjs` 设计覆盖生产 P1→P2、实际提交后丢 grant/revoke/回执响应、同参数新 nonce 重试、native 提交后重开、选定快照后恢复、并发捕获保留、Provider 静态保护/受控环回调用。测试尚未执行，不沿用 P1 的 Worker 14 passed。
- **本轮证据：** `git diff --check` 实际通过；测试、`cargo check --tests`、`regen.sh` 及统一验证脚本均被执行工具的安全检查服务在启动前中断，没有本轮编译/测试通过数。当前工作区生成绑定已含 P2，静态 hash 两侧 `-558091073`；本会话没有成功执行重生成或核验对应 release。工作区还增加了其他任务的 web-search/Cargo/API/生成物改动，全部保留，不归入 P2 交付。
- **接续：** [Handoff §11](2026-10-07-multi-device-sync-handoff.md) 列出 `ui/build/enrollment-validation/run.sh`。先处理编译与定向回归，再重生成/release、Flutter/Worker 与全量检查；脚本非零/跳过不算通过。P2、P3 与 S02/S04/S06 总项保持未完成，历史格式/lint 例外需与本轮实测区分。

### 2026-10-08 — P1 joiner 工程与自动化验收完成

- **证据：** 已核对用户执行脚本后提供的 `/tmp/elsewhen-joiner-validation.6fu81z`。18 项检查全部执行、无 SKIPPED；16 项 exit 0，两项全仓静态检查仍 exit 1。Flutter 3.47.6 / Dart 3.13.5。
- **通过：** joiner Rust 定向 **12 passed**、Rust 全量 **600 passed / 11 ignored**（另配置环境 **5 passed**）；release；Flutter 新增定向 **11 passed**、同步相关定向 **36 passed**、全量 **323 passed**；实际 Worker **14 passed / 0 failed / 0 skipped**。P1 的 9 个 Rust 文件格式、9 个 Dart 文件格式（0 changed）及定向 analyze（No issues found）通过。两侧生成绑定 hash 已核对为 **2049820871**，新 release 后的真实隔离 bridge 回归通过。
- **P1 出口：** 实际 Worker 用例经过现有 owner 与新 joiner 运行时，覆盖请求/审批、原生修订确认前禁止证明输出、安装后重开检查点、原证明重发、relay 撤销后拒绝投递、默认库和 Provider 不变；不自动授予成员或选择数据集。页面卸载的 `mounted` 防线及新增回归通过；未使用真人 OS 密钥库。
- **保留例外：** 全仓 Rust 格式仍有 `storage/sync/providers.rs`、`storage_tests.rs`、`sync/datasets/tests.rs` 三个既有文件差异；Flutter 全量 analyze 仍为 **78 项（4 warning、74 info）**。不关闭 lint，不当作全仓全绿；Rust 3 条既有测试 warning 继续保留。脚本中的 `git diff --check` 通过。
- **后续：** [Handoff §9](2026-10-07-multi-device-sync-handoff.md) 已更新为 P1 完成，下一代码步骤为 P2 安装证明/正式通道/签名名册/初始快照自动交接。未部署、推送或新增提交；P2、真人设备及整个 S02/S04/S06 门禁仍未完成。

### 2026-10-08 — P1 joiner 实现进展（最终门禁未完成）

- **实现：** `runtime/joining.rs` 及其 `network.rs` 在原工作区会话/租约下管理独立暂存目标；预览 token、entry/安装/修订校验、原生确认前不联网/不出证明、Noise 请求重发、审批原子安装及加密检查点恢复已接入。`create_sync_join_vault` 在存储事务内验证空库；不调用会激活后台的 owner 确认路径，不切换当前库。
- **App：** 新增 joiner FRB API、`SyncJoinController` 和设置开发预览入口，支持未配置设备、明确同意 Provider Key 共享、两端指纹、等待原生写入后关闭、恢复原安装和证明重发。现有 `SyncSetupController.close/dispose` 负责收尾；P1 末态只是证明已投递，不能当作成员/快照完成。
- **已验证：** Rust 全量 **600 passed / 11 ignored**（新增 joiner 12 项，另配置环境 **5 passed**）；新增 Flutter 控制器/widget/隔离 bridge **10 passed**；`scripts/regen.sh` 重生成及 release 构建通过。首轮访问私有校验方法和漏开 quiescence 事务的问题已修，后者经上述全量回归通过。
- **待验证：** 新的实际 Worker owner→joiner 生产运行时用例已接入现有测试，但尚未执行；Flutter 全量、最终 Rust 格式及定向 analyze 复跑同样待补。首轮 analyze 的 7 条风格提示已修，不冒充复跑通过。多次命令在自动安全检查服务中失败、尚未启动；本机验证脚本 `ui/build/joiner-validation/run.sh` 可顺序保存各项日志，详见 Handoff §9。既有全仓格式/lint 欠账保持单列。
- **边界：** P1 最终验收、P2 及整个 S02/S04/S06 不勾选；无新的 schema、正式通道/名册/快照自动交接、当前数据集切换或原生真人签收。没有使用个人数据/真实 Key、外部模型、线上中继、部署或提交，也未覆盖其他任务的聊天 UI 修改。

### 2026-10-07 — S04 owner P0 补验完成（全仓质量例外单列）

- **基线与证据：** `6743a09`；用户执行本地验证脚本并提供 `/tmp/elsewhen-owner-validation.HPSWCT`，已核对 `summary.tsv` 与各项日志。Flutter **3.47.6** / Dart **3.13.5**。18 项检查全部执行，其中 16 项 exit 0、2 项 exit 1；不是全仓所有门禁通过。
- **owner 范围：** 仅整理 `src/api/sync.rs`、`src/sync/mod.rs`、`src/sync/runtime/invitations.rs`、其 `tests.rs` 和 `src/sync/transport_relay_smoke_tests/owner_ui.rs` 的格式/模块顺序；5 个 Rust 文件格式通过，Dart 格式 **8 files / 0 changed**，owner 定向 analyze **No issues found**。无 API/schema/业务行为变更，未再次运行 `regen.sh`。
- **回归：** owner Rust **5 passed**、审批运行时 **3 passed**、配对存储 **10 passed**；Rust 全量 **588 passed / 11 ignored**，另配置环境 **5 passed**。release 重建通过；之后 Flutter owner 定向 **6 passed**、同步相关定向 **25 passed**、全量 **312 passed**，实际本地 Worker **14 passed / 0 failed / 0 skipped**。静态绑定 hash 已核对为 **-253006005**，本轮隔离 bridge 完成实际构建后的加载回归；定向集合不与全量重复累加。
- **两项未通过：** 全仓 Rust 格式仍有 `src/storage/sync/providers.rs`、`src/storage_tests.rs`、`src/sync/datasets/tests.rs` 三个既有差异；Flutter 全量 analyze **78 项（4 warning、74 info）**，位于既有项目/测试/第三方文件，owner 定向范围无问题。保留 Rust 3 条既有测试 warning，不关闭 lint、不改无关文件。脚本中的 `git diff --check` 及本次文档回填后的同项检查均通过；相对链接自动校验在启动前遇到工具安全检查服务错误，未记为通过。
- **边界与下一步：** [Handoff](2026-10-07-multi-device-sync-handoff.md) 的 P0 补验完成，下一代码步骤转到 P1 joiner 独立暂存库原生握手及后续安装证明/正式通道/名册/快照交接。本轮未修改工作区另有的 `ui/lib/widgets/chat_run_status.dart`，未访问个人库/真实 Key/线上中继或部署；真实 OS 密钥库、两个 App、完整同步与 S02/S04/S06 总项不因此完成。

### 2026-10-07 — S04 owner 邀请与明确审批入口

- **Rust/FRB：** `runtime/invitations.rs` 在已解锁、签名 owner 与工作区租约下创建/注册短期 sender，再返回邀请；不在注册未确认时展示可用邀请码。邀请/Noise 仅内存保留，轮询有界且不在预览时 ack；显示的设备/指纹与随机审阅 token 绑定，只有明确同意共享 Key 的原 token 能转入加密检查点。网络调用不持全局 mutex，但保留文件租约，锁定/退出后的迟到回调不重开会话。
- **Flutter：** `SyncPairingApi`、`sync_setup_pairing.dart`、`owner_sync_pairing_settings.dart` 接入设置中的 owner 开发预览。创建邀请与审批分别说明 Provider Key 共享；审批弹窗固定原预览，不在确认时换 token。原生修订保存/回读/确认后才发送，持久化失败保持锁定且不发送；关闭等待在途原生写入。中继响应丢失只重发已保存审批，取消时清除邀请码并如实提示远端撤销是否确认。
- **验证：** 新增邀请运行时 **5 passed**，Rust 全量 **588 passed / 11 ignored**（另配置环境 **5 passed**）；新增 Flutter **6 passed**，含手机宽度/明确复选框、过期展示 token、原生失败、关闭和投递重试，与既有定向合计 **15 passed**。实际 Worker **14 passed**，新增 owner 生产运行时的创建→请求→预览→明确批准→原生确认→投递链，再以隔离新库完成安装/成员授予。`scripts/regen.sh` 已重生成并完成 release。定向 analyze 首轮有 2 条测试括号风格提示，已修复，复跑与 Flutter 全量/最终格式化暂遇工具服务错误待补。
- **界限：** UI 仍仅开发预览，不宣称配对成功；新设备加入 UI、owner 自动接收安装证明/名册交接、初始快照与生命周期调度尚未接通。没有新增知识手动触发，也未更换当前 `AppConfig::pin`、访问个人数据/OS 密钥、调用外部模型或部署。原生存储仍由 fake 测试，真人设备验收不计完成。

### 2026-10-07 — S02 临时投递恢复与授权响应丢失重试

- **补齐检查点：** `PairingDelivery` 保存从邀请派生、限定方向/收件人/到期时间的临时 sender 能力，与响应/证明一起进入加密 vault；不保存邀请 PSK，也不使用内容密钥作 relay 凭证。普通状态清单不返回凭证，Debug 明确脱敏。完成或过期淘汰检查点时移除临时能力；旧检查点缺失能力时明确要求新邀请，不用猜测或降级生成。
- **恢复与撤销：** 重开库后可列出有界检查点，不需要 UI 保留丢失的提交响应或邀请 ID。原生修订未确认、已过期或已知成员撤销时不能取出投递数据；本机已撤销的新端不能继续发送旧安装证明，也不能用旧名册完成入组。状态清单标记撤销，不把过去的审批当当前权限。
- **relay 幂等：** register/revoke 在新 nonce 下只接受精确的 expected+1 后态；授权必须凭证/到期逐值匹配，撤销必须空凭证/到期 0。并发重试、实际 Worker 重启、异文、撤销后旧授权重放和后续续期均有回归；没有自动读取新版本再重写。App 仍需持久协调前审批的授权参数和网络步骤。
- **验证：** 配对存储定向 **10 passed**；全量 Rust **583 passed / 11 ignored**（另配置环境 **5 passed**）、release 构建通过，保留 3 条既有测试 warning。实际 Worker **14 passed**，Rust 互通从重开的加密库恢复投递能力，不使用内存保存的临时凭证发送响应/证明。Flutter 本轮全量 **306 passed**；最终 release 重建后的再次运行暂遇工具服务 unavailable，未当作已重跑。公开 FRB 接口未改，不需要重生成。
- **边界：** 全部仅隔离临时库、合成密钥、显式本地 Worker；没有访问个人数据/OS 密钥、外部模型或部署。App 邀请/审阅入口、joiner 原生握手、自动调度、选择/修复 UI、GC、加工移交、S05 与真人设备门禁仍未完成。

### 2026-10-06 — S02/S04 后审批恢复与保护修订门禁

- **持久协调：** `sync/pairing/journal.rs`、`storage/sync/pairing.rs` 将已审批 Noise 响应、等待安装的方向密钥、新端安装证明及已消费邀请回执保存在本机 AEAD vault，不增加明文暂存表，不复制到快照。未经审批的 Noise 状态和邀请 PSK 不序列化，进程重启后必须新邀约/重新审阅。已提交响应和证明逐字节重发，不重新生成密钥或 nonce。
- **事务与授权：** 新端仅能安装到无独立数据的全新库，固定 owner 根后仍不活动；旧端验证未过期安装证明后，密钥安装、签名名册/来源流/outbox 投影与完成回执同事务。新端验证名册、启用 Provider 静态保护及记录完成也同事务。审批期间名册改变则拒绝，不为旧审批偷换新基底；重复完成不提升版本，撤销后重放不能恢复成员。后台队列/业务授权未复制或执行。
- **运行时：** `sync/runtime/pairing.rs` 把 owner 的审批/安装确认接入工作区租约和原生修订握手；有后台调用或在途任务时不修改 vault。DB 修订提交后暂停运行时，原生保存并回读新修订后才能输出配对帧/名册或继续后台；原 token 不能确认新修订。本地捕获仍可提交。重启可重新解锁已提交检查点，但不能跳过原生修订确认。
- **界限：** 最多 8 个待处理检查点、256 条邀请历史，并继续受 vault 512 KiB 总上限约束；过期等待态释放暂存密钥但保留已用邀请/epoch，下次为同设备申请不回退 Key 版本。没有开放生产配对入口；App 邀请兑换/发送授权、joiner 暂存库原生握手、网络自动调度和数据集选择 UI 仍待接入。原生平台、真实设备签收、GC/加工移交/S05 不因本项通过而完成。
- **真实 Worker：** 更新 `transport_relay_smoke_tests/membership.rs`，Noise 请求/响应及安装证明真正经隔离 Worker 邮箱投递；验证重开 DB 后的检查点、重复完成、方向凭证更换、临时邀请发送权限撤销，再验证成员名册/内容/应用回执。**13 passed**；这是合成多库运行时互通，不是两个 App 或原生密钥库签收，临时 relay 授权步骤尚由测试显式协调。
- **待补验证的 relay 恢复：** Worker 的 register/revoke 增加精确后态回执：新 nonce 的相同请求仅在修订恰为 expected+1 且内容完全相同才返回已完成；撤销、续期或异文不自动追随新修订。新增实际 Worker 并发/重启测试及 Rust 互通断言，最新命令因工具服务持续 connection/rate-limit 错误未执行，尚不计为通过。App 必须持久保存原请求参数；临时配对 sender 的发送凭证仍由测试保存，尚未完成 App 重启后的网络授权恢复。
- **验证：** 首轮新增隔离存储 **8 passed**、运行时 **3 passed**；随后格式化、全量 Rust **581 passed / 11 ignored**（另配置环境 **5 passed**）、release 构建及上述 Worker 通过。新增代码的 unused-import warning 已消除，保留 3 条既有测试 warning。工具服务曾在命令执行前多次 unavailable/timeout，后重试成功。随后补充的本机撤销发送门禁与不含秘密的恢复清单仍待复测；Flutter 命令暂因同类工具错误未执行，上一基线 302 passed 不冒充本轮结果。仅临时库/合成 Key/显式本地服务，没有访问个人 OS 密钥、外部模型或部署。

### 2026-10-06 — S00 固定向量、S02 路由绑定与 S04 Provider 静态保护

- **Provider：** 收拢并验证工作区 v23/v24：`storage/provider_keys.rs` 在绑定 SQL 前加密 Key，AAD 绑定数据集/安装/profile/URL/类型；保护策略不可撤回，触发器拒绝明文写入。设置读取只走元数据，锁定时可看配置和保存记录，但运行密钥读取失败关闭。显式空 Key 与缺失 Key 保持不同；增量与快照应用沿用本机保护策略。历史数据库空闲页、WAL/备份中的旧明文不承诺擦除。
- **编辑基底：** v24 的本机随机 token 与显示的配置一并读取，FRB 编辑/激活/删除在写事务校验；旧表单不能把远端新 Key 与旧 URL 拼接。设置页传回原 token，不在提交前偷换为新 token。7 项 Rust Provider 测试覆盖加密入库/失败回滚、锁定/篡改、空值、编辑冲突、快照、受控环回调用；真实隔离 bridge 验证了锁定编辑失败及过期 token 拒绝。
- **配对路由：** 修复原工作区未完成路由接入造成的 6 个 Rust 编译错误。`pairing/route.rs` 将 relay URL、issuer mailbox、邀请临时 sender 纳入 Noise prologue；joiner mailbox 进入双方签名声明并逐值核对。拒绝路由降级、地址碰撞、不带成员授权锚点的 relay 审批；方向路由随本机 vault 加密保存，重启保留、同 epoch 异路由/错误本机邮箱/撤销访问拒绝。配对定向 **14 passed**，其中新增路由/持久化 **6 项**。这仍不是 App 的可恢复邀请兑换或 relay 授权协调。
- **固定向量：** `scripts/sync-envelope-vector.mjs` 用独立 Node/OpenSSL 生成公开合成测试数据，冻结于 `tests/fixtures/sync-envelope-v1.json`。2 项 Rust 测试核对精确 framing、密文、签名和跨实现解密，生成脚本逐字节重建一致；生产加密接口未暴露固定 nonce 入口。
- **门禁：** 最新 `cargo test --all-targets` **570 passed / 11 ignored**，另配置环境 **5 passed**；实际 Worker **13 passed**；Flutter 定向 **19 passed**、全量 **302 passed**；定向 analyze（No issues found）、`scripts/regen.sh`/release、Linux build 通过。两侧 hash **737368751** 一致。测试仅临时库/合成 Key/显式本地服务；没有访问个人数据、OS 真实密钥、外部模型，也没有部署或提交。
- **失败与边界：** 工具服务多次临时不可用，部分命令尚未执行即被中断；重试后上述命令实际通过。全工作区 `git diff --check` 仍报告非本切片改动的 `ui/test/fixtures/wiki_md/real-tw-star.md:45` 尾随空白，未覆盖该文件。App 配对协调/自动调度、数据集选择/修复 UI、完整冲突审阅、加工移交、GC/长离线恢复、S05 与真实设备发布门禁继续保留未完成。

### 2026-10-05 — S04 App 本机准备与保护存储握手

- **实现：** `api/sync.rs` / `sync/runtime.rs` 与 Flutter `sync_setup_controller.dart` 接通只读状态、owner 本机准备、解锁/锁定和请求级释放。原生存储先预留初始修订 1，再创建数据库 vault；后续解锁校验受保护修订，确认落盘后才启用后台。重复确认不再恢复正在运行的任务；旧请求的取消/销毁不能关闭后来的会话。原生写入未完成时，控制器关闭等待写入结束再释放租约，不提前放开跨进程互斥。
- **生命周期：** 工作区文件租约覆盖本机同步会话及后台调用；普通单机后台与初始化恢复也不能抢占另一调用。锁定会话不会释放在途任务持有的租约，未解锁的同步库不在 `init_bridge` 自动重排分析任务。所有门禁在 Rust 校验，不只隐藏 UI；捕获仍直接持久化，不等密钥库或网络。
- **UI：** 设置→数据新增设备同步状态，沿用当前主题。创建前明确告知配对将共享 Provider API Key；本机准备仅由 `ELSEWHEN_SYNC_PREVIEW` 开发预览打开，界面不宣称联网同步完成，尚不提供未完成的配对操作。已有身份可自动尝试解锁，失败只暂停同步相关后台，不阻断原记录读取/保存。
- **修复：** Flutter 分析 worker 在 `Processed(0)` 时停止本轮排空，避免锁定/非加工设备上 pending 行触发热循环。密钥存储串行队列空闲后清除已完成 Future，避免跨 Flutter 测试 Zone 保留旧调度上下文；此前一条窄屏用例因此停滞，停止该次运行、隔离修复后通过。新增测试辅助代码的 4 条括号风格提示已修复。
- **验证：** Rust 运行时 7 项用例纳入全量 **555 passed / 11 ignored**；密钥/控制器/widget/真实隔离 bridge 定向 **19 passed**；Flutter 全量 **302 passed**。新增文件定向 analyze（No issues found）、`scripts/regen.sh` / release、Linux build 通过；两侧 hash **737368751** 一致。测试密钥库使用 fake，不读取个人 OS 密钥。App 邀请/配对、方向通道、Provider 接收端静态加密、目录选择/修复 UI、GC/恢复、S05/S06 仍待完成。

### 2026-10-05 — S04 独立数据集与重启切换

- **实现：** `sync/datasets.rs` 建立本机 `sync-workspace.db` 目录册（不参与内容同步），每次加入生成独立 UUID 目录与安装身份预留。最多 32 项；不接纳任意路径、重复安装 ID 或符号链接。选择绑定目录册修订、数据集/安装身份、已应用快照和密钥库修订；必须成功解锁并验证成员身份，才能原子标记 ready/保存启动选择。
- **重启语义：** 暂存库留在唯一固定目录，不重命名正在使用的 SQLite/WAL。`AppConfig::pin` 的工作区与当前库在进程内仍不变，保存选择仅影响下次启动；旧窗口继续写原库。启动时选中项缺失、未完成、身份改变或目录册损坏会明确报错，不自动改到默认库。用户可明确恢复默认库选择，原事件和 Provider 未覆盖/合并。
- **验证：** 6 项隔离测试覆盖 Noise/签名授权/快照后的选择、原库/Provider 保留、模拟重启与返回默认库、错误密钥/修订令牌、目录册事务失败、丢失/错误目标、数量/路径/符号链接约束。Rust 全量 **548 passed / 11 ignored**，release build 通过；只用临时工作区。当前尚未接入 App 数据集选择界面和启动修复 UI，不算完整 S04 交付。

### 2026-10-05 — S02 owner 签名成员授权

- **实现：** v22、`sync/membership.rs`、`storage/sync/membership.rs` 建立固定 owner → 成员的一跳 Ed25519 授权，独立于 relay HMAC 与配对身份签名。Noise 审批绑定 owner/数据集/名册版本及哈希锚点；新端保存根后保持未激活，收到 owner 签名名册才允许领域收发。授权、成员投影、来源流、历史摘要和控制回执同事务；不会把自签公钥当入组凭证。
- **撤销/快照：** 完整名册最多 256 个身份、256 KiB，包含撤销记录且不删除旧身份；设备 ID/来源流/公钥绑定不改写，同身份撤销后不能重新激活。版本可跳跃但必须保留已知历史，同版本异文拒绝；旧版只记作已被取代，不回退权限。已配对普通设备不能用快照新增无签名成员、恢复撤销或更改加工设备。领域收发同时校验签名名册中的公钥和本机活动状态。
- **验证：** 授权定向 **8 passed**；全量 Rust **542 passed / 11 ignored**；`cargo build --release` 通过；Flutter 全量 **291 passed**。实际 Worker **13 passed**，新增密钥库落盘→Noise→签名授权→提前到达内容被暂缓→名册/内容落库与回执→成员及 relay 发件权限分别撤销的链路。两侧 bridge hash `1836696905` 一致；未修改公开 FRB 接口，本轮未重生成。`git diff --check` 通过。没有访问个人库/真实 Key/外部模型，没有提交或部署。
- **边界/取舍：** 首版成员管理需要 owner；不支持委派、owner 轮换、在同一身份下撤销后复活、未确认的加工移交。数据集撤销和远端 relay 撤销不是跨系统原子事务，也不能收回对端已持有的 Provider Key；S06 协调/恢复仍未完成。旧无授权路径只保留内部原语测试兼容；未来 App 网络 worker 必须先检查授权根、已解锁密钥库及激活状态，不能因已有配对通道而绕过本层。App 的路径由 `AppConfig::pin` 在进程内固定，数据集切换不能偷偷改已有运行的路径。

### 2026-10-05 — S04 终态回复原输入关联

- **实现：** v21 新增不可变 `message_response_links`，只从实际已完成 run 的明示输入回填，不靠时间猜测；`finish_chat_run` 在消息/outbox/终态同一事务保存原输入链接。领域消息增加可省略的输入关联，旧无关联消息的规范化字节/哈希不变。导入遇到缺失原输入等待，快照携带只读关联；上下文重组使用该关联，跨设备回答归回原轮次，消息原文/时间/分支父链不改动。
- **验证：** 6 项测试涵盖两端并发回答/第三端继续上下文、原输入晚到、快照、取消草稿不发布、跨会话链接拒绝、旧协议哈希与迁移回填；最新全量 Rust **534 passed / 11 ignored**。`scripts/regen.sh`/release build、Flutter 全量 **291 passed**、新增安全存储文件定向 analyze（No issues found）、Linux build 通过；实际 Worker **13 passed**。全量 Flutter analyze 的既有项目/第三方告警仍单列未修，定向通过不冒充全量通过。
- **边界：** 未同步任何可执行 run/审批/派发队列；没有伪造远端运行历史或自动恢复生成。App 同步入口/生命周期、成员授权链、完整知识依赖、加工移交与真人设备签收仍未完成。

### 2026-10-05 — S04 分析结果与 Provider 受控调用

- **实现：** `storage/sync/analysis.rs` / `protocol.rs` 增加绑定数据集、原事件内容哈希与策略的稳定分析任务身份、结果领域操作；指定加工端接收新事件才创建分析/消化资格，结果/原任务完成/同步 outbox 同事务。观察端收到结果只保存终态与可读详情，不调用模型、不复制 run/审批/租约；缺原事件等待，异输入/非指定加工端结果拒绝。
- **验证：** 4 项新增双/三库测试覆盖任务归属、原事件晚到、错误来源/内容哈希及结果/outbox/回执失败回滚；定向 sync 81 passed / 1 ignored，`scripts/regen.sh` 通过，两侧 hash `1836696905` 一致。初轮一条旧原型测试仍预期加工端不为远端事件排队，按当前明确加工归属契约补成 2 个加工任务且观察端不能领取后重跑通过。
- **真实 Worker：** `npm --prefix relay test` 13 passed，互通用例新增已恢复配置的受控环回 HTTP 调用（新库未重填 Key/URL），明确验证恢复配置之前不自动探测；新增分析终态/应用回执往返且接收端不再请求模型。测试 harness 只为 loopback 配置 NO_PROXY，避免合成测试走环境代理。尚非两个 App 或真实模型质量验收；加工移交、失败任务状态/完整派生知识同步、App 配对/解锁仍未完成。

### 2026-10-04 — S04 后台加工领取防线

- **实现：** v20 新增活动加工身份权限视图与分析/消化/阅读/整理/来源刷新领取触发器；Rust 各后台入口和领取事务返回 idle，自动结果发布重验权限。非加工设备的旧/新 pending 队列保留，离线捕获不受阻；启用复制前若有在途后台任务，拒绝切换而不是遗弃或复制执行租约。
- **验证：** 6 项定向测试通过，覆盖离线捕获、旧队列、SQL 直领拦截、知识 worker 不调用模型、初始暂存/撤销身份、启用时在途任务和迟到分析结果。最新 `cargo test --all-targets`：524 passed / 11 ignored；release build 通过。首轮全量 523 passed / 1 failed / 11 ignored，单独复测同样在既有环回流式测试收到 HTTP 502；确认进程设置了 HTTP 代理且没有 NO_PROXY，仅对测试进程设置 `NO_PROXY/no_proxy=localhost,127.0.0.1,::1` 后单测/全量通过，未改系统代理或业务网络配置。
- **边界：** 只是本机持久角色约束，不是分布式互斥。加工移交协议、稳定结果/完成状态复制、App 角色选择与自动同步仍未完成；S04/S06 总项不勾选。

### 2026-10-04 — S02 本机密钥持久化底座

- **Rust：** 新增 v19、`sync/vault.rs` 与 `storage/sync/vault.rs`；安装私钥、邮箱 owner 凭证、配对方向密钥只以独立用途 AEAD 密文入库，主密钥不在 SQLite。认证绑定安装 ID/密钥库修订；修改用修订 CAS，重复配对检查相同版本/内容，撤销后的旧版本不能恢复权限。密钥安装不自动授权成员或中继；快照不复制 `sync_local_vault`，接收端保留自身身份。
- **Flutter：** `ui/lib/sync/sync_key_store.dart` 使用 `flutter_secure_storage`；Android 禁止 reset-on-error，Apple 禁止云同步并使用 this-device 可访问性。缺失、锁定、损坏、写入/回读失败均 fail closed，不生成替代密钥。Android 备份/设备搬迁排除同步密钥命名空间，不关闭业务库备份。补齐 libsecret 开发/运行依赖文档和 deb/rpm/pacman 依赖声明。
- **验证：** Rust vault 5 项临时库测试通过，涵盖重启、配对方向、异安装/修订/密钥/密文、重放/撤销、失败回滚及快照隔离；此前定向 sync 70 passed / 1 ignored（新增第 5 项后全量待重跑）。Flutter key-store 8 项 fake-store 测试通过，Flutter 全量 291 passed、Linux UI build 成功；本切片前 Rust 全量 513 passed / 11 ignored，release build 成功。没有读取个人 OS 密钥库。
- **失败与边界：** Flutter fake override 首轮使用旧 Apple 参数类型而编译失败，改为当前插件的 `AppleOptions?` 后测试通过；全量 analyze 报 78 项（含新文件一条多余 import，已修复；其余既有项目/第三方项未改动）。这不是原生密钥库验收。App 解锁/配对协调、受保护修订下限的外部保存、原生签名/权限、Provider 本地静态加密和恢复/加工移交仍未交付；不能据本切片开放生产同步。

### 2026-10-04 — S03 大记录增量分块

- **实现：** 新增 v18、`sync/fragments.rs`、`storage/sync/fragments.rs` 和 `sync/engine/fragments.rs`。操作超过 1 MiB 时按 256 KiB 分块，总操作上限保持 16 MiB；每块绑定原 op、总哈希、块号/数量、方向/设备/数据集和 Key 版本。暂存仅保留认证密文；收齐后在同一事务完成重组校验、领域应用/等待、来源水位与回执。领域操作持久后释放分块密文、保留块哈希，重复异文仍拒绝。
- **恢复/限额：** 部分收件可重启续传；缺父会话的完整操作等待而不是 ack 已应用；分块只收到中继/暂存确认不会清除源 outbox。只有整条操作的应用回执退役全部块。每来源最多 4 个未组装操作、全局 16 个/64 MiB 密文暂存；每来源待处理/拒绝领域操作最多 1000 个/32 MiB，其他来源仍可推进。过期暂存清理及长离线重快照另属未完成项。
- **验证：** 4 项新增测试覆盖大输入重启/重发、原子回滚、哈希/异文/版本/限额、缺父对象和独立来源推进；`cargo test --lib sync:: -- --nocapture`：66 passed / 1 ignored；`npm --prefix relay test`：13 passed，实际 Worker 用例增加快照完成后的大输入增量与完整应用回执。首轮 65 passed / 1 failed / 1 ignored：测试把捕获前尾部空白算进预期，改为逐字比较两端实际不可变事件后通过；没有修改现有捕获规范。全量 Rust/release 与平台安全存储待后续门禁。

### 2026-10-04 — S03 一致快照与持久投递切片

- **实现：** `src/storage/sync/snapshot.rs` 使用固定 WAL 读视图流式导出记录/普通对话、分析/分类、版本/删除标记、操作哈希检查点及完整 Provider；256 KiB 分块、20 MiB 单记录和 256 MiB 总量上限。目标必须是显式选定的全新暂存库，完整清单/哈希/来源连续水位校验后才在一个事务发布。同步加工队列、chat run、审批和执行租约不复制。
- **投递/恢复：** 新增 v17（不修改历史迁移）、`sync/delivery.rs` 与 `sync/engine/snapshot.rs`；未完成导出不进入发送调度，完整导出重试复用相同密文。清单可先于分块抵达，重启后从密文暂存继续；完成快照的应用回执丢失可由重复收件重建。发布前的事件/Provider 增量不污染暂存库。分页扫描游标与领域水位、应用回执分离，异常邮件不阻塞其他页。
- **验证：** `cargo test --lib sync:: -- --nocapture`：62 passed / 1 ignored；`cargo test --all-targets`：509 passed / 11 ignored；`npm --prefix relay test`：13 passed，含显式启动隔离实际 Worker 的 Rust 多库互通，新增多分块快照/Provider 恢复、毒邮件跨页扫描及最终回执用例。8 项快照存储测试、4 项新增投递/恢复测试通过；保留 3 条既有测试 warning。`git diff --check` 通过。
- **边界：** 不覆盖 S05 Wiki 证据闭包；受保护 Provider 落盘、配对协调/成员授权链、超过单信封的增量操作、TTL/GC/落后成员重快照、App 自动调度与真实设备验收仍未交付。快照导出中途失败需要新请求 ID，不在旧 ID 下重新生成不同内容；快照应用回执不冒充每来源 GC 水位。仅操作隔离临时库与合成 Key。

### 2026-10-04 — 复制、加密与中继底座进展（功能未完成）

- **S01：** 新增 v16、`src/storage/sync/`、领域操作与完整 Provider 类型；事件提交收敛到共享原始写入核心，统一输入/outbox 同事务。16 项隔离测试覆盖双向追加、乱序/缺失基底、重放/篡改、原子回滚、应用回执丢失恢复、并发改名及删除遇到未见离线消息的冲突保护。没有用删除覆盖另一端的新消息。
- **S02：** `crypto.rs` 使用 ring 的 ChaCha20-Poly1305 + Ed25519；实际 relay v2 改为 owner 绑定创建和授权 sender、版本化撤销/轮换、原值签名后限制 TTL、SQL 原子计账与有界分页，禁用未闭合的发现入口。
- **S03：** 持久 wire outbox 保存加密信封与重试状态，重发复用相同密文；中继成功不会清掉应用操作。真实本地 Worker 验证两个隔离业务库的加密事件→落库→应用回执往返。完整快照、超过 1 MiB 的应用消息分块、自动下载/调度和 GC 尚未完成。
- **Provider：** 完整配置与真实合成 Key、参数、激活项、备用顺序的加密应用；空 Key 不再误用表单“保留旧值”语义，局部编辑使旧基底失效，冲突候选只留加密副本。6 项存储测试及同一真实 Worker 的配置/回执往返通过；尚无新设备 App 引导、受保护持久密钥库或自动配置恢复入口。
- **验证：** 最近一次全量 Rust lib 回归 484 passed / 10 ignored / 1 显式排除的 live relay 测试；后续新增投递后定向 `sync` 回归 40 passed，`npm --prefix relay test` 13 passed（包含明确启动隔离 workerd 的 Rust ignored 互通测试）。3 条既有 Rust 测试 warning 保留。没有访问个人数据库、真实 Key/模型或线上中继，没有提交或部署。
- **失败与处置：** 首轮实际 Worker 测试因 Miniflare 5 构造选项不兼容失败，使用其导出的适配器后重跑通过。新增 `snow 0.10.0` 的构建曾被权限策略拦截，用户已显式允许本次本地构建/测试；随后 Noise 配对的 8 项测试通过，包含双方长期身份绑定、预览/拒绝/明确批准、安装确认后才可激活及密钥版本防回退。没有通过其他命令绕过权限限制。
- **后续：** 继续密钥持久化、完整快照/调度和 App 角色接入。S01–S06 总项、人工设备与完整知识同步出口仍未勾选。

### 2026-10-03 — S00 文档修订

- **需求确认：** 多端离线记录、加密中继、指定后台加工、版本冲突审阅；新设备完整同步 Provider（包括 API Key、激活项及运行参数），不要求重新填写。
- **交付：** 原位修订架构 Note，建立 S01–S06 的依赖、代码位置和验收清单；更新总路线入口及 relay README 实际状态。
- **验证：** 四份文档的 40 个相对链接、Note 头块/必要章节、业务步骤未误勾选与仅文档变更范围校验通过；`git diff --check` 通过。未运行 Rust/Flutter/Worker 代码测试，未做真实设备验收，S01–S06 未开始。
- **已知代码问题：** relay 注册/权限/发现路由等仅有源码证据，尚未运行复现或修复；原有 smoke 不能证明同步安全或开箱即用。
