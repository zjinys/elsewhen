# Elsewhen 个人认知主循环 Roadmap

## 目标

把现有的事件、对话、Wiki、待办和内容导入能力连接成一个每天可使用、可验证的闭环：

```text
随手记录 → AI 理解 → 人物 / 项目 / 认识沉淀 → 每日回顾 / 后续行动
保存内容 → 阅读理解 → 观点 / 灵感 / 创作素材 → 采用或归档
```

本路线图接管 [旧版 Personal Event System Roadmap](2026-09-11-personal-event-system-roadmap.md) 的产品实施顺序。旧路线图继续作为早期范围和架构约束记录；其中原始事件不可变、AI 失败不阻塞 Capture、派生结果可重算等门禁继续有效。

对应架构决策见 [以个人认知主循环重组 Elsewhen](../notes/proposed/architecture/2026-09-17-personal-cognition-main-loop.md)。

## 当前基线

已经具备：

- SQLite 不可变事件和可恢复分析队列；
- Flutter 对话、Capture、知识库、待办和设置界面；
- OpenAI-compatible Provider、对话工具调用与确认后写入；
- Wiki 页面、修订、人物关系、来源 URL、标签和页内 AI 对话；
- 任意网页 / X 内容 / 文本导入；
- 派生产物的 `area / based_on / content_type` 在当前工作区开发中。

主要缺口：

- 对话消息与个人事件没有统一进入每日记录流；
- Flutter `trigger_analysis` 尚未真正处理分析任务；
- 没有“今天”和每日总结视图；
- 项目、人物主要是静态知识页，缺少带时间的事实与状态变化；
- 外部素材缺少完整的阅读、加工、采用状态；
- Bridge 集成测试会接触实际数据目录，需要先隔离。

## 执行状态

更新规则：每完成一个可独立验证的步骤，立即在对应 Phase 勾选，并在下方完成记录中追加验证证据。

- **当前阶段：** Phase 2.5 — 事件账本质量治理收尾（已完成）
- **整体状态：** 进行中，当前执行 Phase 4C
- **下一步骤：** [AI 对话体验优化子路线](2026-09-30-ai-chat-experience-roadmap.md) C00–C09 工程、自动化和真实端点合成场景均完成；真人GUI点按签收已移交独立[人工验收 roadmap](2026-10-04-ai-chat-manual-acceptance-roadmap.md)（S1–S6，尚未执行）。既有建议反馈、素材采用和规模验证继续独立记录，不因对话优化而视为完成。

### 多设备同步子路线（2026-10-03）

用户确认采用“多端离线记录、加密中继优先、后台加工有归属、修改有版本”，并要求新设备加入后完整恢复 Provider 配置（含 API Key、当前激活项和运行参数），无需重新填写。规范见[同步设计 Note](../notes/proposed/architecture/2026-09-30-multi-device-sync.md)，实施与验收见[同步子路线 S00–S06](2026-10-03-multi-device-sync-roadmap.md)。这不替代既有 AI 对话人工签收，也不代表完整 Wiki 同步已经完成。

- [x] S00：修订同步方案、建立实施路线与验收清单；2026-10-06 补齐类型/回归与独立 Node/OpenSSL 固定信封向量。协议基线不代表 S01–S06 功能验收完成。
- [ ] S01：两/三份隔离数据库的身份、领域操作、outbox/inbox、版本/删除与原子复制。
- [ ] S02：安全配对、真实端到端加密、Provider 敏感载荷保护与中继权限加固。
- [ ] S03：首次一致快照、持久投递、应用回执、TTL/GC 和恢复。
- [ ] S04：App 接入、Provider 开箱即用、前台生成身份与指定后台加工。
- [ ] S05：Wiki/来源/修订/依赖及待办/规则的完整同步与冲突审阅。
- [ ] S06：设备撤销、明确移交、备份恢复与分范围真实设备发布验收。

**2026-10-03 — S00 完成证据：** 原位修订同步 Note，新增同步子路线并更新 `relay/README.md`。Provider 全部信息从首版通过认证加密同步，设备身份私钥不复制；普通设置 DTO 继续不返回明文 Key。当前仅有 Rust RelayClient/Worker 传输原型，尚未接入配对/加密/业务复制。中继权限与发现路由问题仅作源码风险记录，未在本轮运行复现或修复；四份文档的 40 个相对链接、Note 结构、状态与文档限定范围检查通过，`git diff --check` 通过。文档校验不计为 S01–S06 或人工验收完成，既有 C00–C09 状态不变。

**2026-10-04 — 同步工程底座进展：** 工作区新增 v16、领域操作/outbox/inbox/应用回执、冲突与删除保护、持久加密信封、完整 Provider 配置加密应用及 relay v2 owner/sender 权限。最近全量 Rust lib 484 passed / 10 ignored / 1 live relay 明确排除；后续定向 sync 40 passed，实际 workerd/双库 Rust 互通 13 passed，覆盖事件与 Provider 配置经中继落库/回执。Noise 配对在用户显式授权新增依赖构建后已有 8 项原语测试通过；密钥持久化、完整快照、App 接入/加工归属、完整 Wiki 与真实设备发布验收仍未完成。详见[同步完成记录](2026-10-03-multi-device-sync-roadmap.md#完成记录)，S01–S06 总项继续保持未完成。

**2026-10-04 — S03 快照投递切片：** 新增 v17 和受支持范围的一致 WAL 快照（256 KiB 密文分块、单记录/总量有界）、全新暂存库的整批验证/原子发布、Provider 保真、操作哈希检查点、持久导出/最终应用回执及有界分页取件。清单先到、分块乱序、重启、源端持续记录、发布回执丢失和旧请求隔离已有回归；定向 sync 62 passed / 1 Worker 测试 ignored，全量 Rust 509 passed / 11 ignored；实际 Worker 13 passed，包含多库快照/Provider/毒邮件分页与最终回执。完整 Wiki、密钥持久化、App/加工归属、TTL/GC 和真实设备验收仍未完成，S03 总项不勾选；详细上限与证据见[同步完成记录](2026-10-03-multi-device-sync-roadmap.md#完成记录)。

**2026-10-04 — S03 大增量补齐：** 新增 v18、领域操作的加密分块/持久重组与整条操作回执；超过单信封的大记录不再只能依赖快照。每操作 16 MiB、每块 256 KiB；缺父对象等待、不完整/异文/未知版本拒绝提前确认，配额约束与其他来源隔离已有测试。定向 sync 66 passed / 1 ignored，实际 Worker 13 passed（含快照后的大记录增量往返）。首轮一项测试比较了捕获前的尾部空白，已改为比较两端实际不可变事件并重跑通过；详见子路线，S03 总项仍待 TTL/GC/恢复与 App 调度。

**2026-10-04 — S02 本机密钥库切片：** v19 以安装主密钥封装身份私钥/邮箱凭证/配对方向密钥，修订 CAS、撤销版本和快照身份隔离有 5 项 Rust 测试；Flutter 原生主密钥适配有 8 项 fake-store 测试，不在锁定/缺失/损坏时重置。Android 排除密钥备份/设备搬迁，Linux 补 libsecret 依赖。Flutter 全量 291 passed、Linux build 通过；原生密钥库、App 解锁/配对协调、受保护修订下限、Provider 静态加密和完整 S02/S04/S06 仍未签收，详见同步完成记录。

**2026-10-04 — S04 后台领取防线：** v20 的 Rust/SQLite 校验阻止非指定/非活动设备及初始暂存库领取后台任务，保留可离线追加和 pending 队列；启用时拒绝遗弃在途加工。6 项定向测试通过，最新全量 Rust 524 passed / 11 ignored、release build 通过（已有环回流式测试因进程代理 502，限定本次 NO_PROXY 后通过）。尚无加工移交/稳定结果复制/App 角色协调，不能视为完整 S04 或分布式互斥。

**2026-10-05 — S04 分析结果回传：** 绑定原事件哈希/策略的稳定任务身份、加工端资格与分析完成结果已接入原子领域复制；观察端仅更新终态，不启动模型。4 项双/三库测试加入后定向 sync 81 passed / 1 ignored，真实 Worker 13 passed，含新库恢复 Provider 后不重填 Key 即调用受控环回服务、分析结果与回执往返。桥接重生成/hash 校验通过。App 配对/解锁、前台输入身份闭环、完整派生知识及加工移交仍未完成，不当作 S04 整体或真实设备签收。

**2026-10-05 — S04 终态回复输入关联：** v21 只从实际完成 run 回填/保存回复原输入链接，并经领域操作与快照传递只读关联；三端并发/输入晚到时上下文回到原轮次，不复制执行队列、run 或草稿。6 项测试加入后 Rust 全量 534 passed / 11 ignored；桥接重生成/release、Flutter 全量 291 passed、定向 analyze、Linux build 和 Worker 13 项通过。完整 App 同步/成员授权链/加工移交/知识闭包仍在后续切片，真实设备门禁未完成。

**2026-10-05 — S02 签名成员授权：** v22 以固定 owner 的 Ed25519 签名名册授权成员，Noise 审批绑定信任根/版本锚点；配对不自动激活。撤销不回退，同版本异文、伪造/错公钥与无签名快照成员被拒绝，旧内容快照不能恢复已撤销权限或切换加工端。8 项隔离测试与实际 Worker 完整授权/收发/回执用例通过；Rust 全量 542 passed / 11 ignored、Flutter 291 passed、Worker 13 passed、release build 通过。成员管理暂限 owner；App 解锁/配对/方向通道、GC/恢复、加工移交与真实设备验收仍未完成。

**2026-10-05 — S04 数据集隔离底座：** 本机目录册与每次加入的独立 UUID 目录保留原库/Provider；只有已验证签名成员、解锁密钥及完成快照的目标才可提交启动选择。当前进程的 `AppConfig::pin` 不改，重启后生效；缺失/损坏目标不静默回默认。6 项隔离测试通过，Rust 全量 548 passed / 11 ignored、release build 通过。App 选择/恢复界面及完整配对生命周期仍在后续切片。

**2026-10-05 — S04 App 本机准备/解锁：** 原生主密钥、受保护修订、Rust 工作区租约与设置页接通；初始修订先于 DB vault 预留，确认后才恢复后台，旧回调不能释放新会话。在途后台保留租约，锁定不妨碍本地捕获；修复 `Processed(0)` 的 Flutter 热循环。Rust 全量 555 passed / 11 ignored、Flutter 302 passed，定向 19 项及新增文件 analyze、桥接重生成/release/Linux build 通过。本机创建仅开发预览，仍未开启不完整的配对/联网同步；完整 S04/S05/S06 继续推进。

**2026-10-06 — S00/S02/S04 密钥与路由基线：** v23/v24 Provider 静态密文列、SQL 明文禁写、运行时解锁及原编辑 token 的事务校验已验证；设置页和真实隔离 bridge 覆盖锁定/过期编辑失败，不拼接旧 URL 与新 Key。Noise prologue/双方签名绑定 relay 路由，方向路由进入加密 vault，新增 6 项测试；独立 Node/OpenSSL 固定信封向量新增 2 项，S00 协议基线完成。全量 Rust 570 passed / 11 ignored（另配置环境 5 passed）、Flutter 302 passed、实际 Worker 13 passed；定向 analyze、桥接/release/Linux build 通过。App 完整配对协调/自动调度、冲突与数据集恢复 UI、加工移交、GC/S05/真实设备门禁继续未完成，详见[同步完成记录](2026-10-03-multi-device-sync-roadmap.md#完成记录)。

**2026-10-06 — S02/S04 后审批恢复：** 已审批响应/方向密钥、安装证明与已消费邀请保存在加密 vault；未审批 Noise 状态重启失效。旧端密钥/名册/来源 outbox/回执原子提交，新端安装只接纳全新库，名册与 Provider 静态保护原子发布。运行时每次修订需原生确认才释放帧/名册；后台在途时拒绝更新，捕获不阻塞。新增存储 8 项、运行时 3 项测试通过；首轮全量 Rust 581 passed / 11 ignored（另配置环境 5 passed）、release 与实际 Worker 13 passed，含经邮箱投递的 Noise/安装证明和重开检查点。恢复清单/本机撤销门禁、relay register/revoke 精确后态重试的新增回归与 Flutter 重跑暂受工具服务错误影响，继续补门禁。App 邀请传输/relay 授权、joiner 原生握手、选择 UI/自动同步、GC/移交/S05 与真实设备仍未完成，S02/S04 总项不勾选；详见[同步完成记录](2026-10-03-multi-device-sync-roadmap.md#完成记录)。

**2026-10-07 — S02 恢复链补齐：** 临时方向 sender 能力与配对帧一同进入加密检查点，重启后无须保留邀请 PSK 或 UI 提交响应；完成/过期不再输出能力，恢复清单反映撤销，本机撤销不能继续发安装证明。relay register/revoke 的丢失响应只按原请求精确后态重试，不盲目追随新版本。配对存储 10 项、全量 Rust 583 passed / 11 ignored（另配置环境 5 passed）、release、实际 Worker 14 passed；Flutter 本轮 306 passed，最终 release 重建后再跑仍待工具恢复。App 邀请/确认、joiner 原生握手及自动调度等仍未完成，详见[同步完成记录](2026-10-03-multi-device-sync-roadmap.md#完成记录)。

**2026-10-07 — S04 owner 邀请预览：** 设置开发预览已接 owner 的实际中继注册/邀请、指纹核对与原 token 明确审批；原生修订确认后才投递，网络操作保留工作区租约，关闭等待原生写入。新增 Rust 5 项、Flutter 6 项（定向共15项）通过；Rust全量588 passed / 11 ignored（另5项配置）、实际Worker14 passed，桥接重生成/release通过。analyze两条测试风格提示已修，复验及Flutter全量受工具故障待补。joiner UI、安装确认/名册/快照自动交接仍未接通，界面明确不宣称配对完成；详见同步子路线。

### AI 对话体验优化子路线（2026-09-30）

以成熟聊天产品的生成生命周期、上下文管理和交互控制为参考，完善主对话与知识页对话，不整体迁移技术栈。产品方向与取舍见 [AI 对话体验方案](../notes/proposed/architecture/2026-09-30-ai-chat-experience.md)，任务、依赖与验收见 [子路线 C00–C09](2026-09-30-ai-chat-experience-roadmap.md)。本子路线支持 Phase 4C / Phase 5，不重置其既有进度。

- [x] D00：保存优化方案与实施路线图；该项仅为文档交付，不代表功能完成。
- [x] C00a：当前时间、时区与历史时间提醒；工程回归及合成材料真实端点时间感知验收通过（2026-10-02）。
- [x] C00：固定验收场景、可执行本机基线与共享生成契约已建立；后续能力按各阶段分别实现验收。
- [x] C01–C04：工程及真实端点合成审阅/取消场景完成；真人GUI点按签收已移交独立[人工验收 roadmap](2026-10-04-ai-chat-manual-acceptance-roadmap.md)。
  - [x] C01：具体提议/版本/目标授权、事务内复核、执行租约与回执、共享审阅界面。
  - [x] C02：统一生成 run、事件与恢复底座。
  - [x] C03：端到端流式与取消，真实Bridge本机门槛及实际端点流取消、原输入恢复通过。
  - [x] C04：真实工具进度、审阅与诚实收尾，两入口隔离Bridge及真实端点拒绝/新稿接受链路通过。
- [x] C05–C07：预算与选材协调、非破坏性摘要和任务连续性、长知识页对话。
  - [x] C05：分段预算、工具配对和整轮选材，最终请求预算闸门保留。
  - [x] C06：非破坏性摘要与连续性；真实摘要生成/复用及原文回读通过。
  - [x] C07：知识页任务范围、版本/选区保护；分块否定语义修复后，真实全文覆盖与语义核对通过。
- [x] C08–C09：生成中本地输入、持久待生成队列、轮边界补充要求和恢复；全量集成及双入口真实补充验收通过。

完成规则：每交付一个独立 C 步骤，立即在子路线勾选并追加代码、测试和验收证据，同时在本节追加对应完成记录；组合项只在全部子步骤验收后勾选。Rust/Bridge 测试不替代真实模型质量验收，事件不可变、输入不等待 AI、知识后台无手动触发等门禁继续有效。

**2026-09-30 — D00 完成证据：** 已保存上述方案与路线图，并为早期比较稿补充执行入口；未改业务代码，C00–C09 均未开始。

**2026-09-30 — C00a 工程实现：** 时间感知已接入主对话与知识页对话，保留消息时间、按需插入历史提醒，并随预算裁剪维护所属关系。新增 14 项定向测试；全量 Rust 341 passed / 2 ignored / 0 failed、release 构建与差异空白检查通过。真实模型验收待完成，C00a 整体及后续步骤尚未完成。实现取舍见[聊天时间上下文](../notes/implemented/architecture/2026-09-30-chat-time-context.md)。

**2026-09-30 — C00 基线与契约：** 新增[共享契约与验收基线](../testing/ai-chat-contract-and-baseline.md)和可执行本机基准；直答/读工具各 8 样本，显式测试通过。明确输入截止点、run/请求/动作身份与终态、取消、快照规则；这不代表后续事件协议和 UI 已实现。继续 C01 明确动作确认。

**2026-10-01 — C01 完成：** schema v10 为具体提议增加版本、目标依据、执行租约和回执；主对话/知识页及规则使用共享审阅界面，普通短回复不执行或删除积压动作。本地组合写入在实际事务内复核目标，联系人失效 slug 不静默换目标，重复/迟到执行受约束。全量 Rust 354 passed / 3 ignored、Flutter 233 passed、release 构建及隔离真实 Bridge 通过；analyze 无错误但仍有 4 条既有 warning，未记为全绿。决定与边界见[对话提议版本与回执](../notes/implemented/architecture/2026-10-01-chat-proposal-approval.md)。继续 C02，未完成整个第一里程碑。

**2026-10-01 — C02 完成：** schema v11 持久化 run/输入/历史基底/请求与单调事件，主对话和知识页共享观察、停止、重开与精确输入重试。终态和回复同事务，检查点有界，未知工具结果不自动重做，退出只停止本进程任务。切片全量 Rust 398 passed / 3 ignored、Flutter 251 passed、FRB 重生成与 release 构建通过；analyze 无错误但仍有 4 条既有 warning。C03 性能闭环、C04 与真实模型验收继续独立完成，未勾选第一里程碑。见[运行与恢复决定](../notes/implemented/architecture/2026-10-01-chat-generation-runtime.md)。

**2026-10-01 — C03/C04 工程完成：** 全量 Rust 403 passed / 3 ignored、Flutter 256 passed；FRB 重生成与 release 构建通过，重生成后 3 项真实 Bridge 回归通过。两入口均覆盖检索、草拟、显式拒绝/接受与回执；共享 widget 正文可见/取消实测 403ms/468ms，低于 2 秒本机门槛。全量 analyze 仍有 4 条既有 warning，真实模型验收未执行，组合阶段仍未完成。详见子路线完成记录及[基线](../testing/ai-chat-contract-and-baseline.md)。

**2026-10-02 — C05 工程完成：** 分段来源/版本/省略策略、provider-aware 前置工具预算、可选背景优先和整轮裁剪已接入，最终序列化 payload gate 保留。新增损坏旧交换与 SimpleMemory 边界回归，memory 25 passed，随后全量 Rust 409 passed / 3 ignored、FRB/release 通过。C06/C08 后续集成须另跑门禁，真实模型质量不由本条证明。见[预算边界决定](../notes/implemented/architecture/2026-10-01-chat-context-budget.md)。

**2026-10-02 — C00a 完成：** 用户授权下使用 `deepseek-flash`（窗口65536，输出4096）发送合成测试材料；当前日期、昨天记录的真实工具检索、明天截止全部通过，共4次请求，未发送个人原文或修改个人配置。全量 Rust 447 passed / 7 ignored，FRB/release 通过。集成首轮 Flutter 273 passed / 1 failed（32K预算），真实端点其他场景也有失败，未据此勾选阶段出口；详情见子路线的首轮记录。

**2026-10-03 — C06–C09 集成收尾：** 修复过期生成绑定，FRB/release 成功；全量 Rust 455 passed / 7 ignored、Flutter 283 passed，analyze 无 error、4 既有 warning（退出码1）。C06 真实夹具按工具预留后的选材预算触发，实际生成并注入1个摘要节点；首次3请求/7960ms、重试2请求/2313ms，重试无新增摘要请求，原始约束与回读正确。C07 六种范围、任务恢复、比较依赖、选区/版本保护有全量及Bridge证据。C08 输入/队列/保存未排队恢复与停止冻结通过；C09 双入口真实读工具后按补充范围收敛，各2请求，无业务写入。新授权17次中实际已用9次；完整真实工具审阅仍需复验，未勾选第一阶段组合项，详见子路线完成记录。

**2026-10-03 — 最终交付：** Rust 457 passed / 10 ignored、Flutter 283 passed、Python额度测试2 passed，FRB及最终release成功；analyze无error但4既有warning、退出码1。真实主对话与知识页均完成检索、草拟、拒绝/新稿接受及重复回执；真实取消/重开冻结/原输入重试通过。全文首次发现断句导致否定反转，修改 `ai/page_tasks.rs` 的句末边界与有界邻文，新增两项回归；最终4752字符全文3请求/20902ms，首中尾完整且保留“不表示试验已经完成”。本轮分次授权44次，实际39次，不再使用余额。四份决定迁为implemented，完整失败样例与数据见[子路线完成记录](2026-09-30-ai-chat-experience-roadmap.md#完成记录)及[验收基线](../testing/ai-chat-contract-and-baseline.md)。真人GUI连接真实端点的点按签收未执行，第一阶段严格人工出口保留未完成，不冒充人工验收。

### LLM Wiki 主线补全（2026-09-29）

本次完成 FR-PES-004 的知识闭环切片，不据此将整个 Phase 4C / Phase 5 标为完成。

- [x] 原料身份、不可变版本与旧页兼容回填；重复导入预览确认。
- [x] 方法/案例/规律提炼、出处和适用条件、接受/拒绝与规则强度确认。
- [x] 对话、搜索、事件/话题分析、洞察和日结共享有界选材；对话展示实际引用依据。
- [x] 人工编辑保护、来源变化检查、页内修订建议及全局知识审阅。
- [x] 临时库迁移/来源/任务恢复测试，Flutter widget 和真实 Bridge 流程验证。
- [x] 迁移版本断言对齐：`new_migrations.rs` 已纳入 `04-knowledge` 并验证最新版本为 4；Rust 全量 246 项通过。

完成证据与限制见 [FR-PES-004 §7](../requirements/product/FR-PES-004-LLM-Wiki知识库闭环.md#7-实施与验证记录2026-09-29) 和 [实施决定](../notes/implemented/architecture/2026-09-26-llm-wiki-completion-boundary.md)。当前检索使用关键词与适用条件；长期召回质量、跨平台发布和整个素材采用流水线仍按各阶段独立验收。

### LLM Wiki 来源修复与维护补充（2026-09-29）

- [x] R01–R02：选择有效依据修复知识；来源重新认可后自动恢复过期依赖更新。
- [x] R03：记录知识依赖，人工纠正后下游待复核，确认更新前暂停有效引用。
- [x] R04–R06：跨页冲突解决关联修订、分页审阅历史、完整只读整理队列。
- [x] Rust 全量 312 项、Flutter 全量 219 项与 Linux release 通过；现有数据副本 v7→v8 迁移保留事件和快照正文。

实现边界见 [缺口核对](../llm-wiki-implementation-gaps.md)。本次不将 Phase 4C 建议反馈或 Phase 5 阅读/采用流水线标为完成。

### 对话交付与任务连续性（2026-09-29）

- [x] 在四轮工具循环中预留只读收尾，持续预告/空回复按真实结果兜底，不再放行未核验的预告或保存声明。
- [x] `？` 等短追问从持久化会话恢复原要求，区分未完成、草稿待确认与已完成；不重复保存，不把进度追问当确认。
- [x] 原生工具正文回传、兼容重试用量补记、受保护页面在草拟阶段拒绝非法覆盖。
- [x] Rust 全量 260 项、release 构建及 Flutter 连续性/知识来源/基础 Bridge 三组测试通过；[现有数据测试指南 §5](../testing/llm-wiki-real-world-testing.md#5-复测页内空回复空头承诺与追问进度) 已补续聊与确认流程。

决策与限制见 [空回复调查与当前实现](../notes/implemented/bug-fix/2026-09-29-empty-reply-root-cause-investigation.md#本次落地交付结果与任务连续性)。尚未完成逐请求诊断、完整请求 token 预算或真实模型效果验收。

### 原料与知识产出工作区（2026-09-29）

- [x] 已保存原料按版本后台自动整理为参考知识；每次一份、去重、租约恢复和退避；不提供手动后台触发入口。
- [x] v5 为提案记录 manual/automatic 身份，后台不采纳人工待审建议，不覆盖人工正文、适用条件、强度或已确认规则。
- [x] 页面按“原文 / 产出 / 关联 / 来源与修订”组织，来源和对照页内展开；采纳不跳走，操作等高且均有图标。
- [x] 历史提炼页使用快照关系双向导航；现有 IP 方法与原料 v1 的关系已在一致性副本验证。
- [x] Rust 全量 277 项、Flutter 全量 212 项通过；Bridge 已重新生成，真实桥接覆盖自动参考知识和重复 tick 不重复生成。

机制与限制见 [原料与知识产出工作区](../notes/implemented/architecture/2026-09-29-source-knowledge-workspace.md)；[实际效果指南](../testing/llm-wiki-real-world-testing.md) 已区分默认自动整理与可选手动提炼。

### LLM Wiki 自动整理边界对齐（2026-09-29）

- [x] 按需提炼与后台整理统一保存新参考知识；历史人工待审/拒绝决定继续保留，人工定稿和规则修订需要确认。
- [x] 后台结合有界相关来源维护共享主题，至少两个独立来源、完整保留已有来源身份，并支持双向导航。
- [x] 自动生成带逐字出处的冲突、陈旧和重复提示，版本变更后失效；不自动裁定或合并。
- [x] 后台产出刷新已打开的知识视图，全局审阅只列待确认修改。

验证：Rust 全量 294 项、Flutter 全量 214 项通过，含隔离真实 Bridge。机制、输入边界与测试证据见 [自动整合决定](../notes/implemented/architecture/2026-09-29-wiki-automatic-integration.md)。没有执行个人数据库上的真实模型质量评分，不据此结束 Phase 4C / Phase 5。

### LLM Wiki 缺口补齐（2026-09-29）

- [x] 对话保存继承实际引用，规则修订统一强度与过期确认校验（G01–G02）。
- [x] 原料全文分块阅读、版本缓存、全文候选排序与命中片段（G03–G04）。
- [x] 来源变更逐依赖页维护、任务可解释状态与策略重算（G05、G08、G09）。
- [x] 可审阅的主题合并/拆分、段落选择、提示处理记录和历史正文恢复（G06–G07）。
- [x] Provider 窗口参数和完整请求 token 预算（G10）。

证据：Rust 305 项、Flutter 215 项回归通过，Linux release 构建通过；现有数据库副本 v5→v7 的 104 条事件和 32 份原料快照正文保留、完整性检查通过。原库未修改，未调用外部模型。范围与限制见 [交付核对](../llm-wiki-implementation-gaps.md) 和 [实施决定](../notes/implemented/architecture/2026-09-29-wiki-evidence-and-maintenance.md)。真实模型质量仍待按现有材料评分。

### 路线切换记录（2026-09-21）

本路线没有结束，当前执行 Phase 4C。已完成到 Phase 4B：事件账本质量治理、结构化用户模型地基、归档对话清理、实体确认与来源闭环均已完成；Phase 4C 已交付第一条上下文注入切片。

单流对话与主题沉淀路线已于 2026-09-21 完成，原因是对话信息架构是后续决策辅助、事件沉淀和上下文组装的入口约束。本路线现从 Phase 4C 继续，不重置已完成阶段。

## 整合执行路线（2026-09-20）

原有 Phase 编号描述产品能力层级；本节把 `todo.md` 中的方案按依赖重新排列，作为实际开发顺序。原则是先保证进入账本的数据可靠，再建立结构化用户模型，最后让模型参与决策辅助；Today UI 暂不提前上线。

### 已完成基线

- **Phase 0：安全与可观测性。** Bridge 测试隔离、分析队列状态、数据安全门禁完成。
- **Phase 1：统一输入。** Capture、主输入、对话和 URL 路由统一进入可追溯输入链路，幂等和确认边界完成。
- **Phase 2：分析闭环。** 非阻塞分析 worker、重试、重启恢复、统一 `event-analysis` 契约、详情 API 和 Bridge 验证完成。
- **Phase 3 数据层：每日概览契约。** `get_daily_overview`、`daily_reviews`、来源引用、显式生成和当天待办聚合完成；Today 页面暂缓。

### Step 1：Phase 2.5 — 事件账本质量治理

对应：`docs/notes/proposed/architecture/2026-09-19-event-recordability-filter.md`

- 对话层先判断是否值得记录；评价 AI、闲聊、元对话默认不写 `events`。
- **已完成第一步：** system prompt 与 `record_event` 工具描述已明确上述边界，用户显式要求记录时仍按意图执行。
- **已完成第二步：** 统一 `event-analysis` 契约保留 `recordable` 与 `kind`，旧 v1/v2 仅兼容读取；分析 prompt 区分 event / discussion / chitchat / meta，并明确 `projects` 只收录长期项目/产品/组织，不把动作或流程当项目。
- **已完成第三步：** 日流、每日概览和每日回顾生成输入均过滤最新分析中的 `recordable=false`；无分析或旧 v1 结果保持可记录，原始事件不删除。
- **已完成第四步：** migration v28 增加追加式人工记录性决策；消息时间线显示“记录 / 讨论 / 分析中”状态菜单，可纳入记录、作为讨论或重新分析。讨论决策优先于 AI 结果，从日流和每日回顾中撤出，并清理该事件产生的派生事实、关系和待确认动作；原始事件、消息和历史分析保留。隔离 Bridge 测试已覆盖真实往返。
- 明确记录时放宽判断，保留用户主动记录意图。
- 分析契约升级为兼容的 v2，增加 `recordable` 与 `kind`，为存量事件和 UI 过滤提供确定字段。
- 误记内容不物理删除：保留在 conversation，事件侧通过派生状态从时间线撤出。
- 复用现有分析队列做存量重新分析。

当前边界：默认写入路径仍先保存不可变原始事件、再异步分类，以保证 Capture 不等待 AI；因此“写入前 AI 判断”不是默认门禁。当前可验证闭环是分析后过滤、人工纠正和存量重新分析。若未来要求写入前门禁，必须新增显式严格模式并单独评估对即时记录体验的影响。

退出条件（当前第一版）：用户可在消息上下文中区分事件/讨论、人工纠正并立即影响日流和长期事实；原始事件保持不可变；存量事件可重新分析；Bridge/Flutter 可区分事件与讨论。决策见 [事件可记录性纠正闭环](../notes/implemented/feature/2026-09-21-recordability-correction-loop.md)。

### Step 2：Phase 4A — 结构化用户模型地基（已完成）

对应：`docs/notes/proposed/architecture/2026-09-19-decision-support-loop.md` 中“剩余四件硬活”的前两项。

- 以现有 `relations`、network Wiki 页面和事件分析结果为基础，增加统一实体事实表达：`person / project / topic`。
- **已完成第一步：** migration v21 新增 `entity_facts`，事实必须携带实体类型、slug、文本、发生时间、置信度和来源事件；重复的同实体/事实/来源幂等合并并提升置信度。
- **已完成第二步：** Rust `list_entity_facts` API 与 Flutter Bridge 已生成，后续实体详情可直接读取时间线。
- 每条事实包含来源事件、发生时间、置信度和当前状态；原始事件永不覆盖。
- 新事件只做增量维护，更新 `last_seen` / confidence，不重复制造关系。
- 建立名称规范化、slug 和别名表，为人物 / 项目消歧做准备。

已完成：统一 `event-analysis`（旧 v1/v2 兼容读取）、`people/projects/activities` 语义分离；普通分析候选确认卡片；`@/#` 确定性识别；确认后人物/项目事实写入与真实事件回溯；同名实体阻止静默合并并支持候选卡片选择明确页面 slug；隔离 Rust 测试、Bridge 重新生成和 Flutter analyze 通过。提交包括 `6b47654`、`07dbfe3`、`1ed0ae9`、`c1684c1`。

退出条件：同一实体可以查询最近事实；重复分析幂等；所有事实可回到来源事件；旧 Wiki 页面继续可读。

### Step 3：基础设施并行项 — 归档对话清理（已完成）

对应：`docs/notes/proposed/architecture/2026-09-19-archived-conversation-deletion.md`

- 只允许删除已归档、非知识页会话。
- UI 二次确认；AI 指令走 `WriteConfirm`。
- 删除与 messages、pending actions 级联，禁止留下悬挂引用。

已完成：归档列表中的普通对话提供永久删除入口，UI 二次确认；后端拒绝删除未归档对话和知识页专用会话。提交 `aabf15b`，Flutter analyze 与 Rust/Bridge 构建通过。

该项不属于 Phase 4 业务模型，但应在事件过滤后完成，保证讨论内容可收纳、可清理。

### Step 4：Phase 4B — 实体确认与来源闭环

当前进度：已完成。候选确认与忽略、别名匹配和同名消歧、冲突事实提示与人工纠正、安全合并/撤销拆分、来源回溯和关联待办均已形成应用内闭环。

- 所有 AI 实体候选均必须确认；`@/#` 只提供确定性候选线索，不自动写入长期记忆。
- 人工确认、忽略、合并、拆分都保存为显式操作，不修改历史事件。
- 人物 / 项目详情展示事实时间线、来源、最近动态和关联待办。
- 将事件分析、关系提议和 Wiki network 统一到同一实体模型。

退出条件：实体消歧可解释；人工纠正后引用一致；候选不会越权写入长期记忆。

完成证据：migration v24-v27 增加合并审计、逐行快照和旧草案兼容迁移；Rust/API/Bridge 提供合并状态、合并和安全撤销；知识页仅允许选择同类型真实目标，旧页可查看目标并撤销，事实、别名、关系和待办引用即时刷新。冲突事实保留各自来源并允许人工删除派生事实。隔离测试覆盖合并、去重、撤销、再次合并、后续修改原子拒绝、旧 schema 迁移和原始事件保留；`cargo test --all-targets`（lib 127 + bin 114）、Flutter 39 项测试、Bridge 往返及 `git diff --check` 通过，Flutter analyze 无 error/warning（保留 36 条 info）。决策见 [安全实体合并与拆分](../notes/implemented/feature/2026-09-21-safe-entity-merge-and-split.md)。

### Step 5：Phase 4C — 决策辅助第一版

对应：`decision-support-loop.md` 的规则触发、上下文引用和轻量闭环度量。

- [x] 新事件分析时注入已确认规则和知识页上下文（有界、只读，不越过写入确认门）。
- 先输出“关联事实 / 可复用经验 / 可能后续行动”，不直接执行高影响写入。
- 明确行动仍进入待办并保留来源；提醒按高置信即时、低置信回顾分级。
- [x] 记录用户接受、忽略或改写建议，为后续优化提供数据（2026-09-30：对话建议反馈、撤回与本会话上下文接入）。

当前进度：上下文注入与建议反馈切片已完成；自动提醒和完整行动度量仍需独立验收。

2026-09-30 对话入口回归修复：完整请求预算去除重复工具说明；跨会话背景排除当前流与知识页讨论，预算不足时整体省略；本地预算错误不触发协议重试，必要材料不截断。用户展示名“人物”改为“联系人”，内部数据标识不迁移。验证：Rust 326 passed / 2 ignored、Flutter 联系人文案与隔离关系测试 4 passed；详细边界见[输入预算调查的修复记录](../notes/proposed/refactor/2026-09-29-input-budget-token-accounting.md)。此修复不代表自动提醒或真实模型质量验收已完成。

### Step 6：Phase 3 UI — Today 工作台（后置）

对应：`docs/notes/proposed/product/2026-09-20-today-daily-review-workbench.md`。

Today 只有在日期切换、来源跳转、每日回顾版本、行动区和实体事实可用后才进入 UI。它不是 Records 或 Inbox，也不新增重复的待办 Tab。具体设计和上线条件以该设计文档为准。

### Step 7：Phase 5/6 — 素材加工与主动回顾

在实体模型和来源引用稳定后，再推进素材阅读 / 派生产物、跨域检索、周回顾和低打扰主动陪伴。情绪、健康、财务、地点、身份和创作主题等维度先作为独立可见的事实类型演进，不提前建设全知画像。

### TODO 映射

| todo.md 文档 | 新路线位置 |
| --- | --- |
| 事件可记录性过滤 | Step 1：Phase 2.5 |
| 归档对话可删除 | Step 3：基础设施并行项 |
| 决策辅助闭环 | Step 2、4、5，贯穿 Phase 4～6 |
| Today 每日回顾工作台 | Step 6：Phase 3 UI 后置 |

## 实施原则

## 交付与验证节奏

后续每个功能都按可验证的纵向切片交付，不先完成一批无法在应用中操作的数据层。

- 每个切片同时包含必要的存储/API、Bridge、Flutter 入口和测试。
- 每完成一个步骤，必须能从应用界面触发并观察到结果；纯后端准备工作不能单独宣称完成。
- 如果完整 UI 仍需后续迭代，先提供最小可用入口，但不得提供没有真实数据闭环的空壳页面。
- 每次交付都明确说明：界面新增或改变了什么、从哪里进入、如何操作、预期看到什么、哪些内容尚未实现。
- 用户验证后再进入下一个切片；验证反馈优先于继续扩展范围。
- 完成标准必须覆盖真实闭环：用户输入/触发、必要确认、持久化、界面可见和来源核验；单独的 schema、API、Bridge 或末端展示不计为功能完成。
- 如果上游尚不能产生真实数据，不继续打磨下游空状态或展示细节，优先补齐生成与确认链路。
- 内部存储类型不得自动升级为用户导航概念；`topic` 保持内部实现，不设置“议题库”或专门创建入口。

- 每个阶段保持增量兼容，不先建长期无人使用的抽象层；
- 新流程必须兼容现有数据，优先增量迁移，不重写用户原始数据；
- 保存路径永不依赖 AI 或网络；分类、分析、日结均异步执行；
- AI 结果是候选或派生数据，确定性 Core 负责校验、幂等和状态变更；
- 每个阶段同时交付 schema / Rust API / Bridge / Flutter UI / 测试与对应文档；
- 在主循环完成前，不扩展云同步、多用户、自动外部采集和复杂 Dashboard。

## Phase 0：安全基线与可观测性

**目的：** 先保证后续迭代不会污染用户数据，并能看见异步任务发生了什么。

### 范围

- [x] 所有真实 Flutter Bridge 集成测试强制使用独立临时数据目录；
- [x] 清理依赖或写入个人数据库的测试，并以完整测试前后数据库哈希与记录数不变作为隔离门禁；
- [x] 为分析任务暴露最小状态：pending / running / retry / succeeded / failed；
- [x] Flutter 展示全局或记录级后台任务状态，不要求第一阶段展示完整分析内容；
- [x] 更新过时的 README、CLAUDE 和 NEXT_STEPS，使其反映当前能力。

### 退出证据

- 全量测试前后，真实数据目录的数据库文件哈希和记录数不变；
- 可以在 Flutter 中看到待处理、失败和可重试任务数量；
- Rust、Flutter 测试全绿，Flutter analyze 无 error / warning。

### 完成记录

- **2026-09-17 — Bridge 测试数据隔离：完成。** `init_bridge(database_path)` 现在真正设置后续 API 使用的数据目录；`RustBridgeRepository` 支持初始化时传入隔离目录；真实 Bridge 测试统一通过 `test/support/isolated_bridge.dart` 创建并清理临时数据库，Wiki 与设置测试改为自种数据，不再假设个人库已有内容。验证：完整 `fvm flutter test` 36 项通过；真实数据库测试前后 SHA-256 均为 `23ae80c302b8c344cecbfaa79e1f664dd80637f5c12b4c986430b8540cf5e29c`，`events/wiki_pages/conversations` 计数均保持 `18/19/45`。
- **2026-09-17 — 分析队列状态可观测：完成。** Rust `analysis_job_stats` 稳定聚合 pending / running / retry / succeeded / failed 五种状态，Bridge 新增 `get_analysis_job_stats`，Flutter 设置页“数据”标签新增“事件分析队列”概况，显示待处理、处理中、等待重试、已完成和失败数量。验证：新增 Rust 五状态聚合测试与真实 Bridge/UI 测试；全量 Rust `103 + 90` 项、Flutter `37` 项全部通过；`flutter analyze` 无 error/warning，保留 32 条既有 info。
- **2026-09-17 — 当前文档同步：完成。** README 补充个人认知主循环、现有 Flutter 能力、分析队列入口和 Bridge 重生成/测试隔离约束；CLAUDE 更新测试、Bridge 与当前产品方向说明；NEXT_STEPS 删除已完成但仍被列为待办的早期会话/消息任务，改为指向唯一 Roadmap。验证：文档链接与 `git diff --check` 通过。
- **2026-09-17 — Phase 0 完成。** 数据隔离、分析任务状态、Flutter 最小可见反馈和开发文档均已交付；全量 Rust/Flutter 回归通过，静态检查无 error/warning。进入 Phase 1。

## Phase 1：统一记录入口与输入路由

**目的：** 用户只管表达，系统可靠保存后再判断如何处理。

### 范围

- [x] 引入输入记录或等价的关联机制，记录一次用户提交的来源、原文及其产生的 event / message / source / todo ID；
- [x] 主输入接受普通文本、网址和明确行动指令；
- [x] 普通个人记录先落为不可变事件；需要继续交流时，同时关联会话消息；
- [x] Capture 保持直接写事件，但进入同一“今日记录”查询；
- [x] URL 输入走现有导入预览流程，保存后关联原始输入；
- [x] 为重复提交、AI 重试和工具重放定义幂等键；
- [x] 明确自动记录与确认边界：事实陈述可草拟为事件，高影响业务写入仍需确认。

### 数据与 API

- 增量 migration，不改变已有 `events.raw_text`；
- 提供 `submit_input`、`list_daily_entries` 或语义等价的业务 API；
- 旧的 `record_event`、`send_message` 保持兼容，逐步改为调用统一编排层。

### 退出证据

- 从主输入提交一条经历后，可同时继续对话，并且该经历只产生一条权威事件；
- Capture、主输入和历史事件都能由同一日流查询返回；
- 同一输入重试不会重复创建事件、素材或待办；
- Provider 未配置或断网时，输入仍立即保存。

### 完成记录

- **2026-09-17 — 统一输入关联模型：完成。** migration v19 新增 `input_records`，保留用户原始提交、来源、路由状态、可选幂等键，以及 event / message / wiki page / todo 关联；该表只承担溯源关联，不复制各对象业务状态。存储层支持幂等创建和渐进补充路由结果，并校验状态词汇。验证：新增幂等、原文保留、事件关联和非法状态测试；全量 Rust `104 + 91` 项通过。
- **2026-09-17 — `submit_input` 普通文本业务 API：完成。** 存储层在单一 SQLite 事务内创建 input record、不可变 event 和 pending analysis job；相同幂等键返回原路由结果，不重复制造事件。Rust API 与 Flutter Bridge 已暴露 `submit_input`。验证：原子提交/幂等 Rust 测试通过；真实隔离 Bridge 测试确认首次统一提交增加一条事件、重复提交不增加事件。
- **2026-09-17 — 主对话输入接入：完成。** `ConversationRepository.sendMessage` 的生产路径改走 `submit_conversation_input`；单一事务同时创建用户消息、不可变 event、pending analysis job 和带双向 ID 的 input record，之后仍按原流程生成 AI 回复。知识页专用 AI 对话不经过此路径，避免把内容加工指令误记为个人事件。验证：Rust 对话统一提交测试确认一条消息对应一条事件且幂等；Flutter ConversationRepository/Enter/生成中/重试测试通过，并新增真实 Bridge 断言确认三条用户消息只产生三条事件和三个 pending job；静态检查无 error/warning。
- **2026-09-17 — Capture 与统一日流：完成。** Flutter Capture 通过 `recordUnifiedInput` 写入 input record + event，非 Rust 适配器保留 `recordEvent` 回退；新增 `list_daily_entries`，以 events 为权威全集并左连接 input records，使历史事件、Capture 和主对话输入在同一日历日查询中只出现一次。验证：Rust 日流测试覆盖旧事件、Capture、对话三种来源；真实 Bridge 测试确认三条事件全部可见，其中两条带 input 关联；Mock 适配器补齐统一输入接口后，完整 Flutter `38` 项回归通过；真实数据库测试前后 SHA-256 保持 `23ae80c302b8c344cecbfaa79e1f664dd80637f5c12b4c986430b8540cf5e29c`。
- **2026-09-17 — URL 输入路由与来源关联：完成。** 主对话输入识别独立 `http/https` 链接后，不创建对话消息或个人事件，而是先持久化 `url_import` input record，再复用既有推文/网页抓取与预览 tab；已存在素材直接关联，新增素材在用户点击保存后关联 `wiki_page_slug`，抓取失败保留原始输入并标记 failed。验证：真实隔离 Bridge 覆盖 `needs_confirmation → routed` 及无事件副作用；完整 Rust `107 + 94` 项、Flutter `38` 项通过；静态检查无 error/warning（保留 32 条 info）；真实数据库 SHA-256 保持 `23ae80c302b8c344cecbfaa79e1f664dd80637f5c12b4c986430b8540cf5e29c`。
- **2026-09-17 — 端到端提交幂等边界：完成。** Flutter 主输入为一次提交意图生成稳定 key，失败重试复用、成功后清除；普通对话与 URL 路由均把 key 传到事务写入层，避免“服务端已提交但客户端未收到响应”时重复创建 message、event、analysis job 或 input record。AI 回复重试只重跑 assistant 生成，不重写 user input；URL 保存继续按来源页和 input 关联收敛。决策与取舍记录在 [统一输入路由与幂等边界](../notes/implemented/architecture/2026-09-17-unified-input-routing-and-idempotency.md)。验证：真实 Bridge 使用同一 key 两次提交仅返回一条消息与一个分析任务；完整 Flutter `39` 项通过；`git diff --check` 通过，真实数据库 SHA-256 不变。
- **2026-09-17 — 自动记录与确认边界：完成。** Capture 与主对话个人陈述本地立即落为不可变 event；独立 URL 只创建 input record，必须在预览点击保存后才形成并关联素材；查询类工具可直接执行，新建待办、改名、归档、人物关系等业务写入继续沿用 pending action 确认门禁。统一入口只改变路由，不提升任何工具权限。验证：既有 Rust 测试覆盖 pending action 跨消息保留、确认后执行、拒绝后丢弃，以及 rename/archive/relation 工具的 confirm-gated 行为；Flutter 的发送成功/AI 失败测试确认 Provider 故障不回滚用户输入。
- **2026-09-17 — Phase 1 完成。** 普通文本、URL 和明确行动指令均可从主输入进入对应路径；Capture、对话与历史事件汇入同一日流；普通提交和 URL 提交具备端到端幂等与来源关联；保存路径不依赖 Provider，高影响写入仍需确认。进入 Phase 2。

## Phase 2：打通记录分析闭环

**目的：** 每条记录都能可靠经历“待理解 → 已理解 / 待重试”。

### 范围

- [x] 实现 Flutter `trigger_analysis`，复用现有 claim / complete / fail / retry 机制；
- [x] 保存后非阻塞唤醒后台 worker，应用重启后能继续处理积压；
- [x] 为分析结果定义稳定版本，至少提取：记录类型、简短摘要、人物、项目 / 主题、可能的后续事项；
- [x] 提供分析详情 API，暴露状态、结构化结果、来源与错误，供 Today 等来源视图按需展示；
- [x] 不设置独立 Records / Inbox 导航：分析失败由设置中的队列状态承载，明确行动进入待办，普通记录进入 Today；
- [x] 所有候选人物、项目、待办不由分析 worker 自动写入；后续只在具体上下文中按确定性合并和确认策略落库。

### 退出证据

- 新记录保存后可观察到状态从 pending 走到 succeeded；
- 模拟超时、非法 JSON、应用退出后重启，记录不丢失且任务能够重试；
- 分析版本升级可以重跑派生结果，不改变原始事件；
- 用户能从分析结果跳到对应人物、项目或待办。

### 完成记录

- **2026-09-18 — 分析队列处理器：完成。** `trigger_analysis` 读取当前激活 Provider，逐项 claim 持久化队列任务，并将合法 JSON 对象写入 `event_analyses`、标记事件为 `processed`；无 Provider 返回 `no_provider`，Provider 错误或非法 JSON 通过 `fail_analysis` 进入 retry / failed 状态，原始事件保持不变。补充了离线 StubProvider 测试，覆盖多任务成功、重复调用不重处理、非法 JSON 和 Provider 故障。验证：`cargo test`（109 个 lib 测试及全部目标通过）、`git diff --check` 通过；真实数据库 SHA-256 保持 `23ae80c302b8c344cecbfaa79e1f664dd80637f5c12b4c986430b8540cf5e29c`。本次 `./regen.sh` 因宿主 FVM 缓存只读而无法运行，Bridge 生成文件此前已与公开 API 同步，后续环境可写时需重跑确认。
- **2026-09-18 — 非阻塞分析 worker 与重启恢复：完成。** Flutter Bridge 初始化后立即唤醒串行 worker，事件、Capture 和对话统一输入保存成功后只合并唤醒信号，不等待 Provider；5 秒周期唤醒覆盖 retry 到期，pending 超过 Rust 单次 50 项上限时自动续批。Bridge 启动边界把上次进程遗留的 running 任务恢复为可立即 claim 的 retry，恢复不放在每次 `Store::open`，避免抢占当前进程仍在执行的任务。隔离 Bridge teardown 会停止周期 worker 后再删除临时库。验证：新增 running 恢复 Rust 测试；`./regen.sh` 成功并重建匹配动态库；`cargo test --all-targets`（lib 110 + bin 97）和 Flutter 39 项全部通过；`flutter analyze` 无 error/warning（32 条既有 info）；`git diff --check` 通过。
- **2026-09-18 — 稳定结构化分析 schema：完成。** `event-analysis-v1` 现在要求固定 schema_version、event_type、confidence、summary、clarifications、people、projects、follow_ups 八个字段，拒绝未知字段、错误类型、空摘要和越界 confidence；字符串数组 trim、去空并按首次出现去重。schema_version 同时写入 JSON 和 `prompt_version`，为后续按版本重算保留边界。验证：离线 StubProvider 覆盖合法结果、规范化、非法 JSON/非对象、confidence 越界、未知字段和 Provider 失败；`./regen.sh` 成功并同步动态库，`git diff --check` 通过。
- **2026-09-18 — 用户入口边界收敛：完成。** 撤销 Records / Inbox 一级导航：单独浏览原始记录价值不足，而把所有分析候选做成 Inbox 又与待办重叠并制造人工清理负担。明确行动继续进入待办；分析失败由设置中的队列状态承载；普通记录与来源详情进入 Phase 3 Today；人物/项目候选仅在具体上下文中确认。底层 `get_event_analysis_detail` 保留，原始事件和模型结果仍可追溯。验证：Flutter 导航/UI 回归测试 6 项通过；`flutter analyze` 无 error，仅保留既有 info/style 提示。
- **2026-09-18 — 一级导航与待办位置收敛：完成。** 一级导航只保留对话与知识库两个主要内容域；待办不再占用 Tab，移动到侧栏底部常用工具入口，以固定尺寸面板承载添加、勾选、编辑和删除。“今天”暂不以只有记录列表的半成品页面上线，等日期切换、日结、来源引用和行动关联形成完整场景后再进入导航。验证：Flutter 导航/UI 回归测试 6 项通过；`flutter analyze` 无 error/warning，保留 32 条既有 info；`git diff --check` 通过。
- **2026-09-18 — Phase 2 完成。** 持久化分析队列、非阻塞 worker、重启恢复、严格 `event-analysis-v1`、重试与错误可观测、来源详情 API 均已交付；保存路径不依赖 Provider，分析候选不会越权写入人物、项目或待办。最终验证：`cargo test --all-targets` 全部通过；完整 Flutter 39 项通过；`flutter analyze` 无 error/warning（32 条既有 info）；`git diff --check` 通过。进入 Phase 3，UI 信息架构暂冻结。

## Phase 3：“今天”与每日回顾

**目的：** 建立每天打开 Elsewhen 的默认场景。

### 范围

- 一级导航增加“今天”，作为默认主页；
- 时间流聚合当天事件、重要用户输入、分析状态和由它们产生的待办；
- 支持按日期切换，默认只加载必要窗口；
- 生成可重算的每日总结：做过的事、想法 / 决定、涉及的人和项目、未完成跟进；
- 日结必须引用来源记录，用户修正保存为显式修订，不覆盖来源；
- 对没有 Provider 或分析未完成的情况提供纯事实降级视图。

### 第一版明确不做

- 图表化生产力评分；
- 情绪打分和未经用户请求的心理判断；
- 自动推送大量提醒；
- 周报、月报等多周期报表。

### 退出证据

- 用户只使用 Capture 和主输入，也能在“今天”看到完整记录；
- 日结中的每条结论都能跳回至少一条来源；
- 删除并重算日结不会改变事件、人物、项目和待办；
- 无 AI 时仍能按时间可靠回顾当天事实。

### 完成记录

- **2026-09-18 — 每日回顾版本与来源契约：完成。** migration v20 新增追加式 `daily_reviews` 与 `daily_review_sources`，重算创建新版本且不覆盖事件或旧回顾；`daily-review-v1` 要求每条结论携带至少一个目标日期内的来源事件。`get_daily_overview` 在无回顾时返回当天事实与空 review，在有回顾时只暴露通过 schema 和来源校验的最新版本。Flutter Bridge 已生成 `DailyOverviewDto`、`DailyReviewDto` 和条目 DTO，但尚未新增页面。决策见 [每日回顾版本与来源契约](../notes/implemented/architecture/2026-09-18-daily-review-source-contract.md)。验证：存储版本/跨日来源测试与 schema 逐条引用测试通过；`./regen.sh` 成功并同步 release 动态库。
- **2026-09-19 — 每日概览行动关联：完成。** `DailyOverviewDto` 在事实列表和可选最新回顾之外，返回当天相关的未归档待办：事件关联待办或当天到期待办；无 AI 回顾时仍可直接使用事实与行动数据。验证：Bridge 重新生成并重建 release 动态库；每日回顾定向 Rust 测试通过；`flutter analyze` 无 error/warning，保留 32 条既有 info。
- **2026-09-19 — 每日概览 Flutter Bridge 验证：完成。** `RustBridgeRepository.getDailyOverview` 接入生成绑定；隔离真实 Bridge 验证日期、统一日流事实、无 AI 回顾降级和无关联待办结果，测试不接触个人数据库。验证：`flutter test test/bridge_integration_test.dart` 通过。
- **2026-09-19 — 每日回顾写入边界：完成。** 新增 `save_daily_review` API，仅接受 `daily-review-v1`、目标日期内的来源事件和每条结论的来源引用；回顾继续追加版本，不提供覆盖更新。生成绑定与 release 动态库已同步。验证：Rust schema/来源定向测试通过，`git diff --check` 通过。
- **2026-09-20 — 每日回顾显式生成：完成。** 新增 `generate_daily_review`，仅在用户或后续明确流程触发时读取当天事实和已完成分析，调用 Provider，严格校验 `daily-review-v1` 后追加版本；无 Provider 返回 `no_provider`，无事实返回 `no_entries`，Provider/解析失败不写半成品。Flutter 仓库层已接入方法，但尚未绑定页面或自动任务。验证：离线 StubProvider 生成测试通过；`./regen.sh` 成功并同步 release 动态库。

## Phase 4：人物、项目与长期记忆

**目的：** 从静态 Wiki 页面升级为有证据、有时间变化的个人记忆。

### 范围

- 明确实体：person / project / topic；明确带时间事实与实体的关联；
- 人物、项目详情增加事实时间线、最近动态、未完成行动和来源；
- 项目状态变化建模为带时间和证据的事实，不直接覆盖成一段 Markdown；
- 对别名、同名人物和可能重复项目提供合并 / 拆分流程；
- 多条事实稳定支持后，AI 才提出长期认识、习惯、约束或洞察候选；
- 现有 Wiki 页面继续作为可读摘要，内容可由结构化事实辅助更新。

### 退出证据

- 任一人物或项目可以回答“最近发生了什么”，且每项有来源和时间；
- 同一事实重复分析不会增加重复关系或重复状态；
- 人工纠正实体后，相关时间线和引用一致更新；
- 长期认识能说明由哪些事实支持，并能撤销或重算。

## Phase 5：素材阅读与加工流水线

**目的：** 让收藏的内容从“存下来了”走到“真正被使用”。

### 范围

- 素材区独立展示外部原文，不与个人记忆混在同一默认列表；
- [x] 状态至少包含：待读、已读、值得使用、已采用、归档；
- 派生产物类型至少包含：摘要、观点、灵感、脚本 / 文案；
- [x] 派生产物保留 `based_on`、本次用户生成指令、模型标识 / 流程版本、创建时间和采用状态（旧元信息缺失显示未知）；
- [x] 支持从一个原文生成多个版本、比较并选定采用版本；
- “与我有什么关系”必须检索个人记录 / 项目后再生成，并明确引用两侧来源；
- 只有用户认可的外部观点才能进入个人长期记忆。

### 退出证据

- 导入文章后可以完成“阅读 → 总结 → 提炼观点 → 生成文案 → 标记采用”；
- 原文在任何加工操作后保持不变；
- 每个派生产物都能返回原文，且多个版本不会互相覆盖；
- 素材列表可以按状态找到尚未阅读和已经采用的内容。

## Phase 6：检索、主动回顾与产品收敛

**目的：** 在主循环稳定后，让累积数据可被自然调用。

### 范围

- 跨事件、人物、项目、素材和派生产物的统一搜索；
- 基于来源的问答，答案必须展示引用；
- 周回顾 / 项目回顾由日流和事实动态生成；
- 只对明确规则或用户选择的事项主动提醒；
- 评估旧“对话 / 知识库”入口是否降级为上下文视图，删除重复导航和过时兼容层；
- 完成性能、备份恢复、迁移和跨平台验收。

### 退出证据

- 用户能从自然语言问题定位到原始记录和派生结论；
- 周回顾不建立第二套事实源，并可完整重算；
- 主导航不存在两个用途重叠的记录入口；
- 数据备份恢复后，来源关系、修订和状态保持一致。

## 阶段依赖

```text
Phase 0 安全基线
    ↓
Phase 1 统一输入
    ↓
Phase 2 分析闭环
    ↓
Phase 3 今天 / 日结
    ├──────────────┐
    ↓              ↓
Phase 4 个人记忆   Phase 5 素材加工
    └──────┬───────┘
           ↓
Phase 6 检索与收敛
```

Phase 4 与 Phase 5 在 Phase 3 完成后可以并行，但两者必须复用相同的来源引用、修订和幂等机制。

## 全程交付门禁

- **数据安全：** migration 前后数据量和关键引用可校验，禁止破坏性重建真实数据库；
- **离线可靠：** 保存路径不发网络请求，AI 故障有明确降级；
- **可追溯：** 派生结论、实体事实、日结和创作产物都能定位来源；
- **可重算：** Prompt 或模型变化可以生成新版本，不覆盖原始数据；
- **写入边界：** AI 提案必须经过 schema 校验，高影响操作需要确认；
- **测试：** Rust 单元 / 集成测试、Flutter widget / bridge 测试和 migration 测试随阶段同步增加；
- **文档：** 非平凡架构或行为决定同步更新 Agent Note、需求和开发文档。

## 成功指标

主循环完成后，不以页面数量或 AI 调用次数衡量成功，而观察以下行为：

- 用户能否每天以极低成本持续记录；
- 用户能否可靠回答“今天 / 最近在做什么”；
- 人物和项目是否随着记录自然积累，而非依赖手工维护 Wiki；
- 待跟进事项是否有来源且不会被遗漏或重复制造；
- 收藏内容是否产生被采用的观点、脚本或文案，而不只是停留在列表中；
- 任意 AI 结论出错时，用户是否能找到来源、修正并安全重算。


### 2026-09-30：Wiki 流程与规模切片

- [x] 主题整理历史与带并发保护的整批撤销。
- [x] 下游自动准备修订与每批最多 20 项人工审阅。
- [x] 阅读状态、产物版本比较与明确修订采用、对话建议反馈。
- [x] 浏览与产物按需加载、队列 SQL 汇总、依赖版本号筛查及 1 万/10 万篇合成测量。

完成证据与未完成性能路径见 [功能与规模评估](../llm-wiki-scale-and-refactoring.md)。这不是 Phase 5/6 全部完成，长期效果、全文索引、增量内容检查和后台吞吐仍有后续工作。
