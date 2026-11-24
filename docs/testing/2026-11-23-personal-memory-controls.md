# 个人记忆生命周期与受控跨会话工程验证

日期：2026-11-23。关联 [FR-PES-002 v0.7](../requirements/product/FR-PES-002-记忆系统.md)、[同一记忆提案](../notes/proposed/architecture/2026-11-21-agent-memory-anthropic-research.md)。全部使用临时数据库、固定合成输入、内存 Provider、本机 HTTP 模拟或 Flutter widget 渲染；未读取个人数据库、调用真实模型或做设备实机验收。本记录不作为整体 FR 质量签收。

## 实现范围

- 迁移 92：本地主张元数据、来源版本/短引文、人工决定、授权、设置与访问回执。正文继续以 Wiki 为权威。新增表逐项声明为本地，不加入业务同步；恢复另一设备不会自动授予读取许可，跨设备记忆协议仍待设计。
- 初始核心阶段新增 3 个工具，复用既有工具注册、预算与版本绑定审阅卡；后续管理页新增专用 Rust/Dart 桥接接口，见下文。正文确认、证据 support、有效性、独立性和执行许可分别表达。短直接偏好可按保守的原文完整匹配记录为本会话显式输入；任意表达不由字符串规则保证理解。
- 默认检索当前已确认 active 条目，历史需显式开启，deleted 永不返回；来源最多 8 项。同一主张合并真实依据，来源重复不自动升级 support。人工否定/删除在原页、相同正文或同主体关系重提时保留；任意同义改写的等价性不是程序证明。
- 跨会话许可为有向、可撤回/过期、绑定源消息上界和哈希的快照。未来消息和个人主张元数据不随聊天许可开放；源归档后仍可撤回。自动背景、缓存工具输出、内部 Provider 重试均复核许可和来源。冻结输入同时约束关联事件及知识引用，后续排队输入不混入当前请求。
- 记忆自审每 tick 一项、单并发、每天 4 次，冲突/候选优先，正常 7 天重访；失败 6 小时起指数退避至最多 7 天，可暂停且让路前台。复用知识运行台账；只读 2400 字符正文和每来源最多 480 字符，部分覆盖不得标为完全 supported。审阅采纳只更新模型支持维度，不自动确认个人事实。
- 小索引默认关闭，按需重建，最多 8 项且包括指令/元数据的渲染总量最多 800 字符；实际可容纳项数由语言和内容决定，不保证装满。最终预算可以整体省略段。访问台账区分工具打开、检索、最终引用与实际预算后注入，失败/前次重试为 unknown，只有成功末次标 sent；这是传输成功证据，不证明模型阅读、采纳或回答正确，不反哺排名/可信度。

## 管理页之前的自动化结果

M5 增量覆盖迁移 93、精确版本固定/排除及恢复、显著性/活跃目标排序、无共同关键词的关联召回、目标暂停及清空的缓存围栏、跨会话隔离、普通自审保留名额、无注入反馈、有界维护报告及回执清理。通过现有聊天工具创建并真正批准固定审阅卡，验证批准前不生效；维护报告另验证 25 条可见条目不被 210 条较新的外部会话条目挤出，并按工具预算整体省略维护线索。目标删除只级联清理关联，保留个人记忆。明确查看被排除项只在当前输入有效，下一输入清理工具结果和查看轮次历史/关联事件，清理回执不解除限制，恢复自动使用后解除排除抑制。

调试阶段修正两处新增测试夹具问题：空候选台账行计数、`chat_input_effects` 字段名；不改既有记账语义。迁移目录预期清单补齐 92/93。扩大迁移与快照门禁时另发现既有迁移 91 的 `wiki_tags` 未登记到快照携带/排除清单，属于并行工作区改动，本轮未修改其同步语义；本轮新增两张表均已显式登记为本地数据。

**最终版本定向扩展回归：214 passed、0 failed、1 ignored**，包含 43 项本地记忆控制测试、从 92 升级到 93、目标工作区清空、记忆组装/工具/审批/知识与预算回归。忽略项仍为默认不运行的个人数据库诊断；不包含上述既有快照分类失败和两项需监听端口的 HTTP 测试。最终记忆专项另为 **89 passed、0 failed**，与扩展集重叠，不相加。两个工具目录 JSON、205 处文档链接目标、Agent Note 结构/格式及限定范围格式/空白检查通过；没有全工作区或真实质量通过声明。

以下保留 M5 之前的验证记录：

1. 第一轮记忆/注册回归：73 passed，0 failed。
2. 扩展回归覆盖 `storage::personal_memory::tests`、`ai::memory::`、`ai::tool::tests`、`ai::approval`、`knowledge::` 与 `ai::budget`。沙箱首次：188 passed、2 failed、1 ignored；两项失败均因禁止监听本机端口。
3. 两项 `knowledge::event_reading::tests::final_tests::http_tests` 在获准的沙箱外串行重跑：2 passed，0 failed，无外部服务请求。
4. M5 之前的扩展集加迁移检查、排除上述本机 HTTP 两项：**198 passed、0 failed、1 ignored**。其中 30 项本地记忆控制测试包含真实审阅 token 的接受/拒绝、正文预览、部分阅读、迟到删除/前台竞态、暂停、日配额/退避、过期租约恢复、合并来源、删除跨版本/改名重提、授权不传播主张、源归档撤权、参考范围隔离、关联事件/摘要/今日背景清理、时效续期、重建、工具及最终请求预算裁剪和失败/重试记账。忽略项为默认不运行的真实个人数据库精度诊断，本轮未运行它。
5. 最后边界复核：记忆/生命周期/迁移 76 passed；新增工具预算裁剪在真实工具分发路径单独复核 1 passed。后者还验证有向授权历史包按工具结果预算整体省略，不把省略说成无证据或记为读取。
6. 生成链路串行回归另 4 passed，覆盖辅助请求独立记账、完成回复检查、流式取消及停止执行器范围；与上述本机 HTTP 2 项分批运行，不将重叠回归数量相加宣称独立场景数。无真实供应商调用。
7. 4 个中英文提示词 JSON、205 处本地文档链接、同一 Agent Note 的结构/格式及限定范围空白检查通过。未宣称全工作区格式化通过。

实际命令：

```sh
cargo test --lib -- storage::personal_memory::tests migration_applies_goals_table ai::memory:: ai::tool::tests ai::approval knowledge:: ai::budget --skip knowledge::event_reading::tests::final_tests::http_tests --nocapture
cargo test --lib -- knowledge::event_reading::tests::final_tests::http_tests --test-threads=1
cargo test --lib -- ai::generation::tests --test-threads=1
cargo test --lib -- storage::personal_memory::tests storage::new_migrations::tests ai::memory:: ai::tool::tests ai::approval knowledge:: ai::budget storage::sync::snapshot::tests::frame_completeness --skip knowledge::event_reading::tests::final_tests::http_tests --skip snapshot_carries_every_table_or_explicitly_excludes_it --nocapture
cargo test --lib -- storage::personal_memory::tests migration_applies_goals_table v93_preserves_existing_memory_content_decisions_and_usage ai::memory:: ai::tool::tests ai::approval knowledge:: ai::budget storage::goal_data_reset::tests --skip knowledge::event_reading::tests::final_tests::http_tests --nocapture
```

当前全工作区存在并行的存储同步、画像/提示词和 Flutter 改动，本轮未宣称全量测试或 Flutter 门禁通过。旧轮次失败分类见[历史验证记录](2026-11-22-memory-history-recall.md)；不能把旧的全量数字当成本轮结果。迁移版本断言随本次 schema 更新至 93，增加从 92 升级保留原有确认、版本、人工决定和回执的测试。

## 专用管理页增量验证

入口为「设置 → 知识 → 记忆管理」，无需配置模型。列表按全部本地记忆分页，可组合关键词、会话类型、状态、记忆类型和排除状态；详情展示有界正文、当前来源、目标关联及人工决定。覆盖确认、固定/取消、排除/恢复使用、人工显著性、目标关联和生命周期操作。维护报告仍限于所属会话可见的最近至多 200 条，不等于全库检查。

管理接口与自动检索权限分离：人工查看不会给模型授予跨会话读取许可，也不留下当前聊天输入的排除查看例外。修改绑定详情指纹与版本，经既有待审阅提案批准后执行；拒绝取消，不自动发送聊天后续或调用模型。已归档或不可写的所属会话仅可查看。

- Rust 定向回归 **156 passed、0 failed**，包括新增 6 项管理测试：超过 200 条的分页、组合筛选、准备前/批准前来源变化、失效详情、隔离与只读、排除项手动查看、精确 token 批准、拒绝、幂等和不触发聊天后续。与之前的 214 项集合重叠，不能相加。
- Flutter **13 passed、0 failed**：11 项 widget 测试、1 项真实隔离桥接测试、1 项设置入口与真实桥接路由测试。覆盖桌面/窄屏、分页筛选、异步详情竞态、失败状态、原 token 审阅、取消、过期拒绝、目标原因、替代版本与维护回链。隔离桥接验证真实数据库与审阅执行器；测试种子仅写入测试助手创建的临时库。
- `cargo check --lib`、桥接重新生成及 release 动态库构建通过；新增页面、入口、仓库方法及测试的限定范围 Flutter 静态检查无问题。
- 检查 widget 渲染的桌面、380×844 窄屏和批准对话框截图；这是合成渲染检查，不代表桌面窗口控制、真实触控、系统字体或设备实机验收。

管理页最终复核命令（Flutter 命令在 `ui/` 执行）：

```sh
cargo check --lib
cargo test --lib -- storage::personal_memory::tests ai::memory:: ai::approval ai::tool::tests --nocapture
./scripts/regen.sh
fvm flutter test test/personal_memory_screen_test.dart test/personal_memory_bridge_test.dart test/personal_memory_settings_entry_test.dart
fvm flutter analyze lib/screens/personal_memory_screen.dart lib/screens/personal_memory_parts lib/screens/settings_screen.dart lib/screens/settings_parts/skills.dart lib/bridge/rust_bridge_repository.dart test/personal_memory_screen_test.dart test/personal_memory_bridge_test.dart test/personal_memory_settings_entry_test.dart
```

## 未完成与验收边界

### 管理页与对话供给的一致性核验

用户进一步反馈页面为空后，核对发现：管理页与 `search_memories`、`claims::refresh` 中的结构化主张使用同表、同 ID/版本，但对话还存在 Wiki 认知背景、个人画像、历史/摘要、事件及任务背景通道，页面并未覆盖。因此不能宣称两者完全一致，也不能从空列表推断没有任何历史上下文。[FR-PES-007](../requirements/product/FR-PES-007-统一记忆与使用可见性.md)明确统一来源/范围与全通道使用记录的后续需求；入口沿用用户当前的「设置 → 记忆」。

本次新增 2 项隔离数据库测试：管理列表为零时，真实 `SimpleMemory::prepare_context` 仍能装入合成今日事件；经管理接口准备并批准候选后，列表/详情、搜索及自动主张背景的 ID/版本相同，排除后仍可管理而不再自动供给该主张。连同既有预算/重试/失败回执测试共 **3 passed、0 failed**，命令为 `cargo test --lib -- management_runtime_ request_receipts_distinguish_omitted_retry_and_failed_transports --nocapture`。未重跑全量或 Flutter，未读个人库、未调真实模型，未修改运行时。这是差异核验，不是统一模型已实现或模型实际采纳的证明。

### 持续保留的限制

- 首次空列表复核：候选仅通过聊天工具 `propose_memory` 创建，管理页没有从旧聊天或 Wiki 自动抽取/补录的路径，后台自审只处理已有主张。空状态已区分未创建、筛选无结果和请求失败；新增未创建/空白查询/实际筛选切换测试，更新真实设置入口断言，Flutter 管理回归现为 **14 passed、0 failed**，限定范围静态检查无问题，380×844 空状态截图已检查。本次未改 Rust，也未重跑 Rust 测试；没有读取个人数据库，不将代码路径判断当作用户数据库条目数的实测结果。
- 等模型、等预算的回答正确性/相关性、跨会话引用质量、真实材料至少 7 天观察与至少 3 个确切模型配置没有执行，固定 24 题仍仅衡量证据召回，不是回答成绩。
- 专用记忆管理页已实现并完成上述自动化检查；桌面/移动端实机审阅操作和跨设备协议未验收。重建指现有本地元数据的派生索引重建，不是从全部历史自动抽取/重新发现候选。
- 当前为有界词法检索，不保证无公共词的语义召回、任意自述分类或任意同义主张的去重/抑制；全历史扫描、来源检查和元数据增长的大规模性能未测。
- M4 未做收益对照，默认关闭。M5 首版已接入经审阅的固定/排除、人工显著性、目标关联、有界维护线索与回执清理，仍需实测排序收益和管理体验；没有自动遗忘、模型显著性写入或语义合并，不把低访问当成已确认偏好失效依据。报告仅统计最近至多 200 条有权条目，不代表整个知识库；共享来源的整段历史抑制采取保守策略，可能同时减少其他关联内容召回。
