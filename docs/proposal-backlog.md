# Proposal Backlog（未完成提案总索引）

2026-11-08 启动与手工主题修复：后台版本检查去除内存库全迁移，20次开库1869ms→1ms；迁移76修复实际75缺少产物语言表，迁移77保留原地址/正文恢复可确认手工创建页的编辑权限。创建主题改走topic接口，导入原材料仍只读。[记录](notes/implemented/bug-fix/2026-11-08-startup-schema-and-topic-editing.md)。

2026-11-08 后台预算与升级修复：消化按实际Provider完整请求预算缩批次/选材，failover重新准备，未提交事件归队不扣尝试；单事件超窗明确保留而不截断确认。新增迁移75修复已执行74缺少`analysis_jobs.prompt_generation`，保留任务与原文。Rust全量1141 passed / 20 ignored；[预算记录](notes/implemented/bug-fix/2026-11-08-digest-context-budget.md)、[升级记录](notes/implemented/bug-fix/2026-11-08-analysis-prompt-generation-upgrade.md)。多语言Goal仍暂停，L7未推进。

2026-11-08 知识消化预算修复：相关索引限制为24页/8KB，中文正文按UTF-8字节截取；160页长中文完整请求回归及消化28项通过，release通过。备用网关503仍依原退避处理。[修复记录](notes/implemented/bug-fix/2026-11-08-digest-context-budget.md)。提示词诊断按钮默认隐藏，连续点击标题10次显示；3项UI/Bridge回归通过。多语言Goal按用户要求暂停，本次不推进L7。

2026-11-08 L6工程完成：2018项中英资源、同步/设置/维护剩余入口、稳定错误与原始详情、平台文案及AST/ARB防漏门禁完成；Flutter600项、Rust1132 passed / 20 ignored、lib/tool analyze、release通过，FRB hash -479447412。Android原生arm64编译通过；ADB无设备，L7真模型/设备/多设备验收保持未完成。最新覆盖与命令见多语言路线及覆盖清单，以下为历史阶段记录。

2026-11-07 L6知识库第三批：推文预览的 AI Markdown 整理改为后端双语 Prompt 资源，整理任务记录 `preserve_source` 策略，原文语言不被聊天回答语言设置覆盖；整理仅生成预览，仍需用户点击保存才写入来源和派生产物。推文讨论、原文复制、保存/失败、窄屏输入与热切换接入双语。派生版本列表、正文按需读取、模型/流程/生成指令、采用/撤销采用、比较/复制/分页和错误详情接入双语；实体合并/撤销、结构化事实、来源事件、关联待办、相关指导及反馈错误接入双语，窄屏合并操作改为换行布局。原始事实、来源、生成指令、wire值和实际修订 ID 保持不变。

新增导入整理的稳定数据边界测试：原文作为 user 数据发送，系统指令按当前 bundle 选择中英文，整理请求不携带工具，不写事件或知识页；空正文返回 `field_required(content)`。Prompt 运行记录显示“保留来源原文语言”，不伪装成 `zh`/`en` 回答策略。脚本生成器跳过重复 FVM 安装，仍由项目 FVM 生成桥接和 release。

本批资源达到1102项，候选1207项/93文件；ARB/参数/ICU检查通过。Flutter全量**594 passed**（`/tmp/i18n-l6-preview-ui-all-delivery.log`），本批推文/派生/实体专项32项（`/tmp/i18n-l6-preview-domain-tests-clean.log`），Prompt专项32项（`/tmp/i18n-l6-preview-prompt-regression.log`），Rust全量**1130 passed / 20 ignored**（`/tmp/i18n-l6-preview-all-targets.log`），整理策略专项1项（`/tmp/i18n-l6-import-format-policy-rust-final.log`），9入口analyze无问题（`/tmp/i18n-l6-preview-delivery-analyze.log`），release通过（`/tmp/i18n-l6-preview-policy-release-final.log`）。桥 hash 已更新为 `-479447412` 两侧一致。L6剩余同步配对/owner、Provider/知识页冲突卡、数据/运行日志、规则/关系/情绪、搜索/代理/技能/MCP设置及Android原生验证；L7真实模型/设备仍未做。

2026-11-07 L5工程交付：迁移74、签名数据集知识语言/增量与快照v10、应用能力和接管门禁、任务与缓存策略隔离、产物语言metadata及设置修改接通。Rust all-targets库1128+集成2（1130 passed / 20 ignored）、Flutter全量572项、真实Bridge3项、analyze和release通过；语言切换不整库重跑，旧工作继续原bundle。L6全应用文案/错误与L7真实模型和设备仍未完成，Goal继续。

2026-11-07 L6首批推进：待办/习惯提醒、首批稳定错误码与详情、托盘/Android通知标签及对象同步冲突接通，资源896项，覆盖/ICU检查入CI。Rust完整1131 passed / 20 ignored、Flutter最终581项、同步冲突/托盘/通知增量20项、11入口analyze及release通过。Android Kotlin编译受embedding依赖与Google Maven超时阻塞，未算通过。L6其余知识库/同步/设置与L7真实验收保持未完成。

2026-11-07 L6知识库增量：分页列表/筛选/阅读状态、导入表单/负知识与新版本确认/网页预览、页签和正文编辑/标签/复制/沉浸操作接入双语。知识页编辑错误改为稳定code，保留乐观锁与显式覆盖；英文窄屏溢出已修正，切语言保留搜索/标签/未保存正文。资源1018项，候选1327项/98文件；Rust全量1131 passed / 20 ignored、Flutter全量588项、知识库专项35项、14入口analyze及release通过。推文专用预览/整理、实体/派生/维护、其余同步与设置继续，L6和L7仍未完成。

> 集中列出 `docs/notes/proposed/` 下尚未完成的功能提案。本文件是「还有哪些没做」的唯一权威总览。
>
> **维护规则（必须遵守）：**
> 1. 新增提案时，在对应分类下加一行（链接到 note 文件）。
> 2. 提案完成 / 推进时，立即更新其「状态」列，并同步该 note 头部的 `Status` 字段。
> 3. 发现「标记滞后」（note 仍写 `proposed` 但实际已完成）时，先修正 note 状态，再更新本表。
> 4. 每次状态变更附日期与一行证据（commit / 测试结果），不写「空头完成」。
> 5. 已完成的提案不留在「待实现」表里，移到「已完成（归档）」或直接删除行；状态以各 note 头部 `Status` 为准。

- 建立日期：2026-10-04
- 最后更新：2026-11-06（**多语言本轮验证收口**）：Rust all-targets共1087 passed / 20 ignored，Flutter全量572 passed，相关6入口analyze通过，release/FRB及资源检查通过。L1–L2、L3.1/3/5已完成；L3剩余动态指令与L4–L7继续实施，[路线](roadmap/2026-11-06-multilingual-support-roadmap.md)。
- 最后更新：2026-11-06（**多语言L3.3**）：37内置工具/95参数说明及45条技能目录/启用资源固定到运行，7内置技能、文本/原生schema保持一致，MCP/用户Skill原文不译。工具50项、协议17项通过，前一交付点Rust全量1084 passed / 20 ignored；最新界面收口中。剩余修复/页内任务等指令继续L3.2/4/6，详见同一[路线](roadmap/2026-11-06-multilingual-support-roadmap.md)。
- 最后更新：2026-11-06（**多语言L3运行推进**）：模板中英独立编辑/启用默认、回答/指令偏好与实际运行语言查看接通；创建即固定语言、实际多轮HTTP请求与中途变更隔离通过。动态上下文59项、工具37项/参数95项接入；存储13/上下文32/语言3/工具50/真实Bridge4专项通过，6入口analyze通过。L3剩余修复/页内等动态指令继续实施，[路线](roadmap/2026-11-06-multilingual-support-roadmap.md)、[记录](notes/implemented/architecture/2026-11-06-multilingual-runtime.md)。
- 最后更新：2026-11-06（**多语言L2收口/L3.1**）：辅助材料选择与建议卡/弹窗补齐，累计781项中英资源；Flutter全量569项、19入口analyze通过。九份Prompt英文资源及独立注册/回退接通，Rust专项8项通过；实际AI语言策略继续L3.2–6，知识同步及其余业务覆盖仍待L5/L6。[路线](roadmap/2026-11-06-multilingual-support-roadmap.md)、[记录](notes/implemented/architecture/2026-11-06-multilingual-runtime.md)。
- 最后更新：2026-11-06（**多语言L2主要界面迁移**）：导航/设置/Provider密钥/主对话/目标/页内AI/审阅接入614项中英资源，外观语言入口与热切换保留草稿通过真实Bridge回归；英文审批/原文复制/日期回归接通。Flutter全量550项、修改文件analyze通过。L2辅助入口/格式尚待补齐，AI双语运行仍待L3；[路线](roadmap/2026-11-06-multilingual-support-roadmap.md)、[记录](notes/implemented/architecture/2026-11-06-multilingual-runtime.md)。
- 最后更新：2026-11-06（**多语言L1基础交付**）：安装本机偏好、ARB/框架delegates、启动/系统变化、Prompt语言偏好CAS与只读知识语言API接通；编辑器语言与日期同步。Rust语言3项/Prompt5项、Flutter专项28项/全量541项通过，analyze无问题，桥接/release完成。下一步L2文案迁移；[路线](roadmap/2026-11-06-multilingual-support-roadmap.md)、[实现记录](notes/implemented/architecture/2026-11-06-multilingual-runtime.md)。
- 最后更新：2026-11-06（**建立中英文实施路线**）：用户确认建立[多语言 Roadmap](roadmap/2026-11-06-multilingual-support-roadmap.md)，按L0–L7交付主界面/AI、知识语言同步、全应用覆盖与验收；L0复核完成，L1–L7未实施。多语言只保留一条待办，阶段进度以新路线为准。
- 最后更新：2026-11-06（**多语言可实施性复核**）：Prompt存储/回退/运行固定基础可复用，专项4项通过；中英文支持从“分析/方向”移入待实现。剩余为界面本地化、语言偏好与AI运行接线、英文对话行为、知识语言同步和全应用错误迁移，见[更新评估](notes/proposed/product/2026-11-04-i18n-feasibility-assessment.md)。本轮未改运行行为。
- 最后更新：2026-11-06（**目标演进Phase 1工程完成**）：统一计划/步骤树导航、去黑话与完整详情；Flutter536项、Rust记忆31项/计划工具5项通过，analyze无问题、release编译通过。Phase 2–4继续待实现，见[记录](notes/implemented/feature/2026-11-06-goal-navigation-and-plain-language.md)。
- 最后更新：2026-11-05（**预算瘦身与Prompt中文入库交付**）：默认固定指令1948 token；修订/启用/回滚/运行固定及核心处理管线接通。Rust全目标库1063+集成2通过，Flutter535通过，新增桥接专项2通过。详见[实现记录](notes/implemented/architecture/2026-11-05-prompt-database-and-budget.md)。
- 最后更新：2026-11-05（**洞见浮现与面板改造复核**）：洞见浮现 P0–P2 已接通，移入收尾；面板改造补记已有 bottom sheet 与内嵌审阅，剩余非模态工作区需按当前入口重定范围。证据见各提案复核记录。
- 最后更新：2026-11-05（**命名密钥库 P1–P3 工程交付**）：见收尾与[实现记录](notes/implemented/architecture/2026-11-05-named-secrets-runtime.md)。
- 最后更新：2026-11-05（**MCP 工具意图关联落地**）：迁移 72 增加服务级关键词，设置框/卡片、Rust/Dart 桥接、配对快照、L1 catalog 与系统提示接通；修改关键词即时作废旧清单缓存，修正缓存串入其他服务工具的问题，目录头和省略提示共同受 2000 token 上限约束。`cargo test --all-targets` 库 1054 passed / 20 ignored、集成 2 passed；MCP Flutter 回归 2 passed，修改文件 Dart analyze 无问题。真实模型触发率待场景观察；可选 nudge 未启用。见[实现记录](notes/implemented/architecture/2026-11-04-mcp-intent-keywords.md)。
- 最后更新：2026-11-05（**R2.5 通用恢复 + 洞见浮现 P0–P2 落地**）：预算层补交「本轮超大工具结果带披露截断」——只剩最近两轮仍超限时不再整轮拒绝，真·必死局才报错且含可操作建议；知识库洞见浮现落地候选台账（迁移 71）、每日 1 条频控（07:00–22:00 静默/72h 过期/owner 单写）、认知画像切片注入与主对话四重闭环卡片。证据见各自 roadmap/提案落地记录与提交。
- 最后更新：2026-11-05（**立项目标系统演进：认知画像 × 知识库素材 × 航向守护**）：复盘目标系统演进历程与痛点（单句目标过粗 → 切片黑话生硬难用 → AI 拆解假大空且 UI 列表套娃截断）；回归初心「日常感知用户在做什么并温柔提醒偏离」；确立四大支柱：个人认知画像（显式档案 + 隐式精力，杜绝通用模板）、知识库素材联动（调取方法论/案例作为推演武器库）、去黑话活路线档案（剔除“抓手”等工单术语，左栏极简树状融合消除重复，主区舒展展现）、日常对话航向守护闭环；见[提案](notes/proposed/product/2026-11-05-goal-system-evolution-profile-knowledge-alignment.md)。
- 最后更新：2026-11-05（**多语言提示词管理设计**）：扩展同日评估为可实施设计：七张核心表按 key/locale/修订管理，区分界面/指令/回答/知识产出语言，定义缺译回退、按语言激活、运行固定、后台跨 tick/owner 切换和缓存依赖。仅交付设计，未迁移业务库或修改当前行为；见[设计](notes/proposed/architecture/2026-11-05-versioned-prompt-management.md)。
- 最后更新：2026-11-05（**优先级复核与首批实现**）：原“待实现 21 项”实际有 23 行，混有已交付/取代项。本轮合并为 9 项真实工程待办，保留人工验收与测量缺口；首先完成命名密钥提案的 P0：MCP 环境变量不出快照、旧快照接收不安装远端环境，21 项快照回归通过且导出/接收双向变异变红。全量 `cargo test --all-targets`：库 1040 passed / 20 ignored、集成 2 passed，0 failed。完整命名密钥库仍未完成。
- 最后更新：2026-11-05（**页内总结全文覆盖修复**）：自然语言总结请求自动采用 full_summary；明确局部范围保留局部，新导入长文无完整阅读缓存时走本轮连续分块。页面任务 22 项、对话协议 17 项回归通过，含 7000 多字持久/旧式入口全文覆盖与缓存回退；见[修复记录](notes/implemented/bug-fix/2026-11-05-page-summary-full-coverage.md)。
- 最后更新：2026-11-05（**立项知识库消化洞见浮现与行动闭环**）：针对后台消化提炼的 Wiki 页面与认知洞察“写入即封存（Write-and-Forget）”痛点立项；坚决排除顶部双横幅冲突方案，主推「主对话流轻量气泡卡片（即时感知与四重闭环：采纳规则/开启习惯问候/转待办/忽略）」+「系统提示词画像切片注入（长效唤醒）」双轨架构；制定高价值类型准入、每日最多 1 条与过期自动静默的严苛频控纪律；遵循 Owner 主设备写入与多设备同步规范。见 [提案](notes/proposed/product/2026-11-05-surfacing-digest-insights.md)）
- 最后更新：2026-11-05（**知识目录回灌超预算修复**）：实际失败来自 list_wiki_pages 一次回传 204 页，摘要已可用。目录改为 SQL 元数据分页（默认 10 条、最多 20 条）并限制标题/摘要长度；移除按历史条数猜测摘要未收敛的错误提示；R2.5 状态纠正为部分完成。`cargo test --all-targets`：库测试 1032 passed / 20 ignored，集成测试 2 passed，0 failed；见[修复记录](notes/implemented/bug-fix/2026-11-05-wiki-directory-context-budget.md)。
- 最后更新：2026-11-05（**立项命名密钥库与占位符引用**）：用户反馈多个 MCP 共享 token 难以维护且轮换繁琐；立项将「密钥」作为一等概念抽象出命名密钥库（`named_secrets`，AEAD 密封，凭据不离本机），配置中使用 `{{secret:name}}` 占位符运行时动态插值；解决多服务同 key 集中维护，彻底根除 MCP stdio `env_json` 明文随快照外溢的安全隐患；设置页集中管理与反向引用审计。见 [提案](notes/proposed/architecture/2026-11-05-named-secrets-and-references.md)）
- 最后更新：2026-11-05（**AI 为中心的目标拆解·入口修订：AI 立项/拆解移入「目标」页**）：新增保留会话「目标工作室」（`goal-studio`：固定 id、不绑定目标、不进普通对话清单与界面）；目标页未选中目标时即工作室，「和 AI 拆解 / 立新目标」与航向校准「聚焦讨论」改为在目标页预填草稿（不再跳对话页）；`system_context` 新增目标工作室幕僚分支（确认制与零落库红线不变）；无 schema 变更；`cargo test --all-targets` **1030 / 0 / 20**、analyze 0 error
- 最后更新：2026-11-04（**MCP / Skill 工具意图关联立项**）：针对第三方 MCP 工具因名称与描述固定而导致 AI 无法在日常意图讨论中主动联想触发的痛点立项；机制同构分析 MCP 与 Skill，MCP 采用客户端声明 `keywords` 列，在 L1 花名册分组头渲染「关联意图」并增加提示词引导，Skill 优先由用户编辑 SKILL.md 描述；读类直调、写类确认，不设反问中间态。见[实现记录](notes/implemented/architecture/2026-11-04-mcp-intent-keywords.md)。
- 最后更新：2026-11-04（**多语言支持可行性评估**）：全项目多语言现状盘点，确认核心成本不在 Flutter UI（~2,300 处字符串易于 ARB 化），而在 Rust AI 提示词、标记解析器、错误回流与 578 处测试断言；结论为在产品国际化定位决策前暂不盲目投入基建，先做用户定位决策。见[评估](notes/proposed/product/2026-11-04-i18n-feasibility-assessment.md)。
- 最后更新：2026-11-04（**AI 为中心的目标拆解 P4（主动复盘与偏离扩展）工程完成——roadmap 四阶段全部收口**：迁移 69 提醒携带计划上下文 + 计划层 7 天降频；复盘提议沿用 advance_plan_step 确认制（提议零落库）；`cargo test --all-targets` **1029 / 0 / 20**、`fvm flutter test` **522 / 0**、analyze 0 error。真模型与真人 GUI 验收沿用既有验收路径）
- 最后更新（同日先前）：2026-11-04（**AI 为中心的目标拆解 P3（执行循环与计划看板）工程完成**：`advance_plan_step` 推进工具（复盘 → 完成 → 激活 / 追加，满额零拒绝排队）；「直接建一条目标」自动包成单步计划（无独立目标概念）；「目标」页左栏计划看板（意图 / 当前步 / 进度 / 下一步 + 选中联动）；`cargo test --all-targets` **1027 / 0 / 20**、`fvm flutter test` **522 / 0**、analyze 0 error）
- 最后更新（同日先前）：2026-11-04（**AI 为中心的目标拆解 P2（AI 拆解主对话入口）工程完成**：确认制工具 `create_plan` / `add_plan_steps` / `list_plans`（拆解方法论与假设清单纪律进提示词）；审批执行全通路（计划 + 全部步骤单事务落库，第一步激活其余排队）；审阅卡草案专属渲染与文本字段纠正；`cargo test --all-targets` **1024 passed / 0 failed / 20 ignored**、`fvm flutter test` **521 passed / 0 failed**、`fvm flutter analyze` 0 error。真模型与真人 GUI 验收沿用既有人工门）
- 最后更新（同日先前）：2026-11-04（**AI 为中心的目标拆解 P1（存储与协议）完成**：migration 68 + `goal_plans` + `PlanRecord` 同步域 + 快照 v9 + FRB/Dart 适配；桥 hash 两侧一致 `-1437813334`；`cargo test --all-targets` **1019 passed / 0 failed / 20 ignored**、`fvm flutter test` **521 passed / 0 failed**、`fvm flutter analyze` 0 error。见 [roadmap P1 落地记录](roadmap/2026-11-04-ai-centric-goal-decomposition-roadmap.md)）
- 最后更新（同日先前）：2026-11-04（**AI 为中心的目标拆解重设计提案**：用户实机使用反馈门槛过高，重设计为「一句话意图 → AI 拆解出长程计划（Plan）与逐步 goal，AI 起草一切、用户只确认」；评审中进一步修订：**目标/计划不设上限**，注意力保护移驻「同时推进的步骤数」（缺省 3，AI 调度并发闸）；确认制 / 零打扰红线不变。见 [提案](notes/proposed/product/2026-11-04-ai-centric-goal-decomposition.md)；同日转 [roadmap](roadmap/2026-11-04-ai-centric-goal-decomposition-roadmap.md)（P0–P4）开工；评审三问定案（软时间窗 / AI 自决拆解 / 无独立目标、存量随迁移删除），P0 已完成）
- 最后更新（同日早前）：2026-11-04（**认知型目标 UI 收拢 + 满额创建语义**：目标工作台整体进左栏 `GoalWorkbenchPanel`（标题「目标」/右上常驻「新建」/过滤多选单行/清单），新增表单改底部面板；主区「详情 | AI 讨论」左右并排；**活跃已满 3 条时新建目标自动以「暂停搁置」创建**（storage 满额落 paused、触发器仍兜底，AI 提议回执同步说明）。测试已重写/新增（含 `cargo test goals` 断言），**已验证（2026-11-04）**：`cargo test` **1011 passed / 0 failed / 20 ignored**、`fvm flutter test` 全量 **521 passed / 0 failed**、`fvm flutter analyze` 0 error；顺带修复 bridge part 扩展静态绑定缺陷——`createGoal` 移回类内保证测试 Fake 虚分派覆写）
- 最后更新（前次）：2026-11-02（**立项认知型目标与长程航向对齐**：建立分支 `feat/cognitive-goals-and-alignment` 与 [新 roadmap](roadmap/2026-11-02-cognitive-goals-and-long-term-alignment-roadmap.md)；将目标从单句愿望升级为 4 个认知切片（Why/Focus/Blocker/Done）、状态机升级为 active/paused/completed 三态读档复活、解除对话禁言、引入 3~5 天脱靶航向校准卡片）
- 最后更新（前次）：2026-10-27（**配对快照交接真机验收通过**：[收敛性 roadmap](roadmap/2026-10-26-enrollment-snapshot-convergence-roadmap.md) 的 C4-4/C5-2 全绿 —— 桌面导出 `af56a582` 到 `acknowledged`（`transport=blob`、`blob_index=173`、快照 9,069,001 B / 173 片、上传事件仅 1 条无重传），手机从「等待成员授权与初始同步」走到「成员设备」，数据完整落地（全新安装的手机出现了知识库 8 条、两天的历史对话、3+ 条 AI 提供商配置），relay 残留 0。同一批暴露并另立 roadmap 三条生命周期缺陷：D1 `requires_repair` 无出口、D2 未配置时裸读单例、**D3 交接完成后手机 UI 不收敛（需重启 app）**。遗留：relay 侧累计写入字节未采样（严格 ≤1.05× 待第 4 次复跑专项采样）；一条回执被 `sync_receipt_identity_mismatch` 丢弃（已并入 D3 候选根因，见生命周期边界条目））
- 最后更新（同日早前）：2026-10-26（新增该 roadmap：真机配对卡「等待成员授权与初始同步」定性为 **mail 传输缺三条不变式**（有界在途/进度反馈/停滞推进），硬时限当日收口，含删功能止损预案 C5）
- 最后更新（本轮之前）：2026-10-25（relay v3 roadmap **R0 ~ R6 全部完成**、review 修复 **R7-P0 ~ R7-P6 全部完成（含二次复核残留）**：数据面/信令面分离落地，验收 6 项指标一次性复核通过；`docs/review_logs/2026-10-25/review_20261025_155152.md` 的 6 条发现与 12 条小刺全部有结论）
- 口径变更 2026-10-24：relay 只保留 standalone 单实现（Cloudflare Worker 下线），功能面收窄为「信箱 + blob」，不再承担 APK 分发。

多语言最新验证：Flutter全量572项通过，相关6入口analyze无问题；最新release实际双轮工具语言与设置桥接3项通过。剩余动态指令/英文行为、知识同步及全应用/真实模型设备验收继续按[路线](roadmap/2026-11-06-multilingual-support-roadmap.md)推进。

## 待实现与部分完成（8 项，按本轮执行顺序）

2026-11-05 对照代码与各路线完成记录复核；工程完成但缺真人验收的项目移入收尾，合并重复事故/子提案，不把同一缺口重复计数。

| Proposal | 仍缺内容与执行顺序 | 优先级 |
|---|---|---|
| [目标系统演进：认知画像 × 知识库素材 × 航向守护](notes/proposed/product/2026-11-05-goal-system-evolution-profile-knowledge-alignment.md) | 2026-11-06 Phase 1 工程完成：计划/步骤统一树导航、过滤/展开/选中、标题换行、移除独立进度与重复看板、目标/审阅/AI提示去黑话。见[记录](notes/implemented/feature/2026-11-06-goal-navigation-and-plain-language.md)。2026-11-06 Phase 2/3 首版接通：AI主动检索/阅读/引用知识素材（已撤掉手动参考资料入口）、日常对话归集到知识页profile/me（无专门入口、无问卷，明确事实按字段更新）、目标上下文注入与检索纪律；[记录](notes/implemented/feature/2026-11-06-goal-grounding-and-profile.md)。Phase 4复用既有提醒并补证据/不评判纪律，更强语义偏离判断及真实模型/多设备/GUI验收仍待做 | 高 |
| [对话框 → 抽屉改造](notes/proposed/product/2026-10-04-chat-dialog-to-drawer.md) | 2026-11-05 核实部分完成：待办/目标快捷查看/审阅列表已统一 bottom sheet，审阅详情与批量操作内嵌。当前 showModalBottomSheet 仍模态，非模态右侧工作区未实现；目标主工作区已独立，旧草稿/生成历史入口已不存在，需重定范围，不照旧清单重做 | 高 |
| [同步生命周期遗留 D6/D7](roadmap/2026-10-27-sync-lifecycle-boundaries-roadmap.md) | C0–C12、D8/D9 已实现，原“零改动”描述失效。仍缺旧数据集清理出口（D6，先明确用户确认与保留边界）、录入期尚未激活名册的业务帧推迟处理（D7）；累计字节采样归入收尾，不重复实现 blob 传输 | 中 |
| [单流主题后续演进](notes/proposed/product/2026-10-05-single-stream-topic-follow-ups.md) | 主题生命周期/状态排序、分析队列摘要、同名候选确认、建议归拢 | 中 |
| [工作项一等页面](notes/proposed/architecture/2026-09-22-work-items-as-first-class-pages.md) | 已有 open_todo_work_item 及讨论入口；尚未把 todos 收敛为页面的兼容投影，也未完成历史迁移与统一业务模型 | 中 |
| [Linux/Wayland 窗口位置记忆失效](notes/proposed/bug-fix/2026-10-10-window-geometry-wayland.md) | 关闭时补存已落地；需恢复侧与具体 compositor 实机取证，不能靠重复 setBounds 宣称修复 | 中 |
| [中英文支持：界面、AI 与知识语言](roadmap/2026-11-06-multilingual-support-roadmap.md) | L1–L6工程完成，2018项资源；Flutter600项、Rust1132项、静态检查/release及Android原生arm64编译通过。L7真模型、设备和多设备验收未完成；交付C与Goal继续 | 中 |
| [Today 每日回顾工作台](notes/proposed/product/2026-09-20-today-daily-review-workbench.md) | 保持后置；现有回顾读面与周期统计不等于该工作台全部设计落地 | 后置 |

## 进行中（0 项）

## 本轮核实已交付或已取代（不重复排期）

| 项目 | 核实结论 |
|---|---|
| [上下文预算与工具注入](roadmap/2026-10-19-context-budget-and-tool-injection-roadmap.md) | 2026-11-05 R2B/R7 默认固定指令瘦身完成：BASE1767+指南165+消息开销16=1948 token，测试约束完整固定默认<3000。独立OpenAI兼容请求保留完整指令及稳定前缀，增量diff不作为当前线协议待办；动态上下文/schema仍受整体预算约束。自定义Prompt体积单独计预算 |
| [MCP 工具意图关联](notes/implemented/architecture/2026-11-04-mcp-intent-keywords.md) | 2026-11-05 工程完成：迁移 72、服务级关键词设置/列表、Rust/Dart 桥接、快照兼容、catalog 关联意图与提示词均接通；关键词修改即时更新缓存，目录头与省略提示计入 2000 token。真实模型触发效果仍需场景观察，可选 nudge 未启用 |
| relay 存储放大与配额 | [v3 路线](roadmap/2026-10-24-relay-v3-dataplane-roadmap.md) R0–R7 已交付；原待办正文已附全部完成证据 |
| 配对快照交接收敛 | [blob 唯一数据面](roadmap/2026-10-26-enrollment-snapshot-convergence-roadmap.md)已交付且真机配对通过；剩累计写入字节精确采样列入收尾 |
| 决策辅助闭环与今日记忆检索 | [D0–D6](roadmap/2026-10-14-decision-support-loop-roadmap.md)实现与反馈/回访/今日背景均已存在；修正路线头部 D6“未开始”的滞后状态 |
| AI 目标拆解、认知目标与航向对齐 | [P0–P4](roadmap/2026-11-04-ai-centric-goal-decomposition-roadmap.md)及目标工作室均工程完成；真人质量验收保留在收尾 |
| MCP 延迟加载、token 估算与事故主修复 | catalog/激活/缓存、分层估算/usage 锚定、审批续跑与摘要降级均已实施；剩余缺口只在预算路线维护 |
| 摘录孵化闭环 | 存储/快照以及 Flutter 孵化分组、纳入/否定按钮已存在；原索引“未实施”及“Flutter 后置”失效 |
| 知识页持续更新与关系网 | 提案已记 P0–P3 实施，代码存在后台整理、关系存储与详情入口；整理队列专用设置面板仍属后续增量 |
| save_knowledge_draft 来源发现 | 错误指路与目录 slug 已落地；真实模型是否摆脱循环仍待验证，移入收尾 |
| [快速捕获旧 UI 切换故障](notes/proposed/bug-fix/2026-10-10-quick-capture-no-ui-switch.md) | 2026-11-01 已删除 Capture 模式与入口分支，旧故障不再适用；不为过期方案补 mode provider |

## 收尾（11 项）

| Proposal | 现状 |
|---|---|
| [Prompt 入库与版本管理](notes/implemented/architecture/2026-11-05-prompt-database-and-budget.md) | 2026-11-05 中文首版工程交付：迁移73、不可变修订、CAS启用/回滚、固定协议只读、运行bundle与请求关联、摘要依赖隔离、跨tick阅读固定、分析/消化方法接入；设置→模型→提示词提供编辑/历史/回滚/运行版本查看。中文首版真人验收待做；P3多语言工程统一由上文“中英文支持”承载 |
| [知识库消化洞见浮现与行动闭环](notes/proposed/product/2026-11-05-surfacing-digest-insights.md) | 2026-11-05 对照代码确认 P0–P2 工程完成：迁移 71、候选登记、worker tick、画像注入、主对话卡片及四出口接通；存储/画像 7 项和组件 6 项已有覆盖。仅剩真实模型端到端与真机多设备验收，不重复排工程开发 |
| [命名密钥库与占位符引用](notes/implemented/architecture/2026-11-05-named-secrets-runtime.md) | 2026-11-05 P0–P3 工程实现：本机密封存储、MCP/搜索运行时引用、轮换作废会话、设置管理/引用审计/删除确认、编辑器引用插入与显式精确值提取。密钥专项 5 项、Flutter 全量 534 项通过，修改界面与桥接入口 analyze 无问题；Rust 全量旧批次 4 项失败均已修正/单独复验通过，未获同批次全绿。真人 GUI 待验收。未密封安装沿用明文列，env/headers 仍全量留本机；Bearer 前缀存量需手动替换 |
| [配对快照累计写入采样](roadmap/2026-10-26-enrollment-snapshot-convergence-roadmap.md) | blob 唯一数据面与真机收敛已通过；严格累计写入字节 ≤1.05× 的精确现场采样仍缺，不能用“上传一次”替代字节账 |
| [目标拆解与航向对齐](roadmap/2026-11-04-ai-centric-goal-decomposition-roadmap.md) | P0–P4 及目标工作室工程完成；仍需真实模型质量与真人 GUI 签收，不重复开发已交付的计划/步骤/复盘能力 |
| [save_knowledge_draft 来源发现](notes/proposed/bug-fix/2026-10-08-save-draft-source-discovery-loop.md) | 两层工程修复已落地；真实模型的原场景复验尚缺，不把自动测试当成模型循环问题已获真人签收 |
| [记事本页（随手记，不经 AI）](notes/proposed/product/2026-10-19-notepad-memo-pages.md) | 已实现（2026-10-19，`0665af1`）：`memo` kind + 三层白名单隔离（AI 可写排除 / digest 上下文排除 / 人类专属）+ `create_wiki_page` / 单事务 `append_wiki_page_content`；Flutter 记事本入口 + 快记框。基线 Rust **849** / Flutter **403** / analyze 0 error。剩余：真人 GUI 验收；系统通知通道与 Capture 落点为后续增量 |
| [定时提醒（"20 分钟后提醒我"）](notes/proposed/architecture/2026-10-19-timed-reminders.md) | 已实现（2026-10-19，`0665af1`）：migration 51 加 `todos.reminder` + 设备本地 `todo_reminder_fires`；pop 挂 5s tick 最前（故障隔离）；snooze = due_at 推后 + fire 删除（单事务）；本地解析（7 单测）+ `create_reminder` AI 工具（确认闸）。基线 Rust **849** / Flutter **403** / analyze 0 error。剩余：真人 GUI 验收；`flutter_local_notifications` 系统通知为后续增量 |
| [习惯主动问候](notes/proposed/product/2026-10-22-proactive-habit-checkins.md) | 已实现（2026-10-22，`36a88fc` + A 方案）：migration 57 + 计划 CRUD/确定性推断 + 5s tick + 主对话 `habit_nudge`/`habit_checkin` + 定制按钮/未读点 + 后台 failover + 设置页「习惯」集中列表（暂停/恢复/调整）；**多设备只有 owner 主设备触发**，其他设备用同步 checkin 判断已回答、不显示无本机 fire 的「晚点再问」。验证：Rust **914** / Flutter **429** / analyze 0 error（既有 133 条 info/warning 不计）。已修 digest 漏写 kind 前缀导致的重复 habit 页，并清理真实库中的 `after-lunch-walk` 重复。2026-10-23 补齐：确认卡「推断依据」展示、问候卡「聊聊/聊聊原因」预填输入框（不自动发送）、全局限流每日 2 条（补问不占额度）、连续 3 天忽略后确定性建议卡（改时间走确认对话框、暂停经用户点击才落库）。验证：Rust **916** / Flutter **429** / analyze 0 error。剩余：真人 GUI 多设备验收 |
| [多设备同步](notes/proposed/architecture/2026-09-30-multi-device-sync.md) | 开发关闭（2026-10-14，`e9254d1`）：S00–S05 交付，基线 `f7f6e74`（迁移 34、快照 v3，Rust **711** / Flutter **372** / analyze 0 error）。唯一阻塞项为 S06 真人门禁——按[人工测试路线 M0–M7](roadmap/2026-10-14-sync-manual-test-roadmap.md) 执行后回填 [S06 清单](roadmap/2026-10-14-s06-human-gate-checklist.md)；证据见 [Handoff §1–§31b](roadmap/2026-10-07-multi-device-sync-handoff.md) |
| [AI 对话体验](notes/proposed/architecture/2026-09-30-ai-chat-experience.md) | C00–C09 工程 + 合成场景完成；真人 GUI 签收移交[人工验收 roadmap](roadmap/2026-10-04-ai-chat-manual-acceptance-roadmap.md)（S1–S6） |

## 分析/方向（2 项，不产生直接待办）

| Proposal | 说明 |
|---|---|
| [规则生命周期与主对话解耦后续设计](notes/proposed/product/2026-10-19-rule-lifecycle-decoupling-discussion.md) | 核心解耦已由 e7e9e4b 实施；遗漏的后续讨论为规则字段方向、影子判定纠正、适用结果消费与判定成本上限。原提案明确未拍板，先收敛设计；预算瘦身/diff 仍由预算路线承载，不重复计数 |
| [Elsewhen 作为 Agent Harness](notes/proposed/2026-10-02-elsewhen-as-agent-harness.md) | 现状对照与缺口分析，产出方向清单 |

## 搁置（1 项，无排期）

| Proposal | 原因 |
|---|---|
| [移动端执行委托](notes/proposed/architecture/2026-10-18-mobile-execution-delegation.md) | 2026-10-18 评估后明确 Deferred：场景与设备形态错位，回合挂起与双向委托成本高；保留设计，不纳入开发队列 |

## 已完成 / 已归档（不在待办内）

以下 note 已标为非 `proposed` 状态，不作为待办（状态以各 note 头部 `Status` 为准）；2026-10-14 起已实现类物理移入 `docs/notes/implemented/{architecture,feature,simplification}/`，superseded/archived 类移入 `docs/notes/archived/`（文件名未变）：

- `implemented`：`event-recordability-filter`、`archived-conversation-deletion`、`single-stream-topic-implementation`（`d6ca18d`）、`wiki-workflows-and-scale`、`wiki-tabs-tweet-import-refactor`、`wiki-metadata-and-reading`、`conversation-role-separation`、`editor-reading-settings-layer`、`2026-09-27-web-search-tool`（外网搜索，migration v25：多引擎 Tavily/Brave/SearxNG + DoH+ECH `src/net_doh/`，实测开 x.com）
- `superseded`：`external-directory-project-import-boundary`、`personal-event-system-boundaries`
- `archived`：`rikkahub-chat-context-comparison`
- `adopted`：`personal-cognition-main-loop`
- `done`：`refactor/api-dart-generation-migration`、`refactor/storage-and-detail-view-split`
- 已修复（历史记录）：`refactor/input-budget-token-accounting`

## 本轮新增完成（2026-10-25，R4 + R5 + R6 收尾）

- **relay v3 roadmap R4 ~ R6 全部完成，验收 6 项指标一次性复核通过**：[roadmap](roadmap/2026-10-24-relay-v3-dataplane-roadmap.md)。
  - **R4 窗口化快照**：分片走 `/blob/*` 数据面、接收端边拉边删、逐窗 `ack`，relay 峰值与快照总量解耦。三条落地修订：窗口按 `WINDOW_CHUNKS * CHUNK_BYTES_MAX ≤ RESPONSE_BYTES` 定（32 片 = 21 MiB 会撞穿 relay 16 MB 响应上限，表现为 `relay blob response too large` 死锁）；等 ack 预算改成调用方属性 `BlobUpload.window_wait_ms`；峰值采样必须紧跟 `push_due`（接收端收一窗 ack 一窗，import 之后再读 relay 账恒为 0）。R4-5 口径纠正：回执对象仍是 `snapshot-end:{id}` manifest 信封，变的是时机。
  - **R5 `drop` 与 `ack` 分家**：relay 上同为删除，但**签名域不同**（拿 ack 的签名去 drop 结构性 403），「已消费」与「已丢弃」不再共用一个词；「必表态」把死信治理从「等 7 天 TTL」变成「3 次拒绝后必须表态」，计数落表（放内存等于每次重启重数、阈值永远到不了），且**只对不可恢复错误计数**（依赖未到的 `waiting` 不计、部署错位的 `sync_protocol_unsupported` 只上报不 drop）；三条容量不变式进 CI。
  - **R6 `deliver` 状态机**：状态**推导不存储**（`relay_id`/`available_at_ms`/`blocked_reason`/`last_error` 已是真相，再存一列必然自相矛盾），唯一新增列是迁移 62 的 `last_error`。落地抓到两处语义混淆并拆开：「已送出的最小重发间隔」不是退避（backoff 判定加 `last_error <> ''`，成功路径改名 `resend_spacing_ms`）；`blocked` 只负责让人看见、不负责刹车（507 保留 15 s 再试）。结构化日志唯一入口 `wire_log`，行格式拆成 `wire_line` 以便断言「只有四个字段」。
  证据：事故同规模（`ELSEWHEN_BLOB_SNAPSHOT_EVENTS=3 ELSEWHEN_BLOB_SNAPSHOT_UNITS=400000`，174 片/密文 121.7 MB）跑 `smoke::end_to_end_send_fetch_ack_against_real_worker` **18 拍 29.51 s 绿**，`[blob] acceptance ... peak=6995630 baseline=0`（≈6.67 MiB = 一窗 10 片，与快照总量无关）；`close` 后 `blob_bytes`/`blobs` 归零；三个跑过数据的信箱 0 封/0 blob/0 片，`wal_checkpoint(TRUNCATE)+VACUUM` 后 8,486,912 → **77,824 字节**。门禁：`cargo test --all-targets` **932 passed / 0 failed / 19 ignored**（200 s）、relay `node --test` **41/41**（含两个原先 `--ignored` 的 Rust e2e 包裹用例）、`fvm flutter analyze` **0 error**、`sync_deliver_status_test` 4 例绿、`scripts/regen.sh` 桥 hash 一致。

## 本轮新增完成（2026-10-25，R3 blob 数据面）

- **relay v3 数据面（P1）落地**：[roadmap R3](roadmap/2026-10-24-relay-v3-dataplane-roadmap.md)。`relay/standalone/schema.mjs` 加 `relay_v3_blob` / `relay_v3_blob_chunk`（主键 `(blob_id,idx)` ⇒ 重复 PUT 结构性去重）并把 blob 的 30 分钟无进展 TTL 并进 `purgeExpired`；新增 `relay/standalone/blob.mjs` 实现 `open`/`chunk(PUT)`/`chunk(GET)`/`state`/`ack`/`close` 六端点，全部复用现有 `stamp()`/签名域/`nonce`/`withTx`，没有一行自创鉴权；`PUT` body 是裸二进制（新 `boundedBinary`），`GET` 返回 `[4 字节长度][密文]` 连续帧裸流（省 base64 的 1.37×）；`/status` 增 `capabilities:["blob"]` 与 `signal_bytes`/`blob_bytes` 两本账（老字段语义不变）。
  证据：新增 `relay/standalone/blob.test.mjs` 7 用例 —— 重投同片 `blob_bytes` 不变、`ack upto` 后 `10240→2048`、`close` 后 `0`、7 种非法 `blob_id`→400、`version:1`→426、未注册 sender/owner 冒签/越界索引全拒、`1MiB+1`→413、超配额→507、TTL 到期自动回收。门禁：`node --test relay/standalone/*.test.mjs relay/test/*.test.mjs` **39/39 pass**（32 基线 + 7 新增）、`cargo test --all-targets` **917 passed / 0 failed / 19 ignored**（Rust 未动）。
  两条与方案原文的偏离（已在 roadmap 与方案 note 里写明理由）：读侧多出 `blob-state`/`blob-get` 两个签名域（否则 `state` 的签名可重放 `get`）；上传侧用 sender 凭据而非 owner（owner 凭据能读能删整个信箱，不能给上传方）。

## 本轮新增完成（2026-10-25，R1+R2）

- **relay v3 roadmap R1 + R2（P0 止血）全部完成**：[roadmap](roadmap/2026-10-24-relay-v3-dataplane-roadmap.md)。R1 修掉两处生产 bug（信箱混入配对帧被当损坏信封冒泡、`owner_confirmed` 早退吞掉 completion 回执）；R2 六项：`seq_hint` 作投递幂等键（**原地替换、保住原 id** —— `DELETE`+重插会让按 `id>after` 翻页的接收方永远卡在最新一批，`roundtrip.rs:341` 实测稳定红）、客户端 `sync_wire_seq` 单行发号器（migration 59）、传真值到两条 push 路径、`507` 语义化（`RelayQuotaExceeded` + 15s 退避 + 面板提示）、`relay_id` COALESCE、relay 卫生（`schema.mjs` 共用 DDL + `sweeper.mjs` 扫目录的 `worker_threads` 回收器 + 句柄 LRU + `VACUUM`/`wal_checkpoint`）。
  证据：真实 1.7GB 归档量出**同一信箱 801 封 / 534.8 MB 只有 175 个不同载荷 = 4.41×**，同形态重放 `seq=0` → 801 封 / 510.0MB、固定 `seq_hint` → **175 封 / 115.5MB**；`d6157af7` **0 封邮件占 66.6MB** → sweeper 后 **0.1MB**。门禁：relay `node --test` **32/32 pass**、`cargo test --all-targets` **917 passed / 0 failed / 19 ignored**、`fvm flutter analyze` 零 error。

## 本轮新增完成（2026-10-24）

- **relay 单实现化 + 移除应用内自动升级（APK 分发）**：[roadmap R0 全部完成](roadmap/2026-10-24-relay-v3-dataplane-roadmap.md)。relay 只剩 `relay/standalone/server.mjs`（零第三方依赖，`worker.ts`/`wrangler.toml`/`deploy-relay.sh`/`miniflare` 全删）；`/release/latest`、`/release/apk`、`RELEASES_DIR`、R2 桶、Dockerfile releases 挂载删除；升级链从 Android 权限（`REQUEST_INSTALL_PACKAGES`）、FileProvider、平台通道、`app_update.dart`、设置页入口，到 Rust 的 `check_app_update`/`download_update_apk`/`AppUpdateDto`/`ReleaseManifest` 与 `scripts/publish-release.sh` 整条删除。证据：`cargo test --all-targets` **917 passed**、relay `npm test` **21 passed**、`fvm flutter analyze` 零 error、bridge hash 两侧 `916603462`。附带收获：删掉 Worker 版后 standalone 的 `base64Size` 缺校验（空 payload 入库、非字符串 500）立即被契约测试打红 -> 已修。

## 合并归属补充（2026-11-05 文件逐项核对）

以下提案此前仅按项目名或路线图合并登记，补齐文件链接以便追溯，不新增重复待办。

| 提案文件 | 归属 |
|---|---|
| [拟议需求：对话里的"查看今日记忆"（真检索，非推断）](notes/proposed/2026-09-15-daily-memory-retrieval.md) | 已由决策辅助路线承载，不单独排期 |
| [Agent Note: 以个人认知主循环重组 Elsewhen](notes/proposed/architecture/2026-09-17-personal-cognition-main-loop.md) | 已采纳为个人认知主路线；Today 待办另列 |
| [Agent Note: 决策辅助闭环（定位补全）](notes/proposed/architecture/2026-09-19-decision-support-loop.md) | 已由决策辅助路线承载，不单独排期 |
| [Agent Note: 目标表设计与偏差检测的落地路径](notes/proposed/architecture/2026-09-28-goal-schema-and-deviation.md) | 已采纳为回顾/偏差路线；后续目标演进另列 |
| [Agent Note: 周期统计报告与目标偏差检测的实现路径](notes/proposed/architecture/2026-09-30-periodic-deviation-report.md) | 已采纳为回顾/偏差路线；后续目标演进另列 |
| [知识页持续更新与关系网](notes/proposed/architecture/2026-10-19-knowledge-page-reconsolidation-and-relations.md) | P0–P3 已实施，对应上文“知识页持续更新与关系网” |
| [MCP 工具接入延迟加载与按需暴露（工具 Schema 瘦身）](notes/proposed/architecture/2026-10-19-mcp-tools-deferred-injection.md) | 核心已实施，剩余统一由上下文预算路线承载 |
| [非 GPT 模型 token 估算校准（告别 text.len() 字节退化）](notes/proposed/architecture/2026-10-19-token-estimation-calibration.md) | 核心已实施，剩余统一由上下文预算路线承载 |
| [relay 传输层重构（v3）：数据面与信令面分离](notes/proposed/architecture/2026-10-24-relay-storage-amplification-and-quota.md) | 已由 relay v3 R0–R7 完成记录承载 |
| [R4 客户端窗口化快照 —— 实施范围申报（2026-10-25）](notes/proposed/architecture/2026-10-25-relay-v3-r4-windowed-snapshot-plan.md) | 已由 relay v3 R0–R7 完成记录承载 |
| [R5 `drop` 动词 + 必表态 + 不变式进 CI —— 实施范围申报（2026-10-25）](notes/proposed/architecture/2026-10-25-relay-v3-r5-drop-and-invariants-plan.md) | 已由 relay v3 R0–R7 完成记录承载 |
| [R6 `deliver` 状态机 + 结构化日志 —— 实施范围申报（2026-10-25）](notes/proposed/architecture/2026-10-25-relay-v3-r6-deliver-state-plan.md) | 已由 relay v3 R0–R7 完成记录承载 |
| [摘录孵化闭环（讨论 → 纳入知识 / 丢弃为负知识）](notes/proposed/product/2026-10-19-clip-incubation-and-negative-knowledge.md) | 首版已实施，对应上文“摘录孵化闭环” |
