# Proposal Backlog（未完成提案总索引）

更新：2026-11-23（记忆生命周期与受控跨会话工程）；最近实现提交：`15d3487`。本文件只记录当前缺口与归属；逐批进度、旧测试数字和历史方案见[历史快照](reference/proposal-backlog-history-2026-11-08.md)，实现依据见各条链接。

当前共 **6 项工程待办、15 项验收事项、3 项后置增量、3 项方向讨论、1 项搁置**。工程完成不等于真实模型或真人设备验收通过。超长事件分块消化已交付，真实验收范围及限制见实现记录；多语言 Goal 按用户要求暂停，L7只保留验收。

维护规则：新增缺口先查已有归属，避免同一工作重复排期；推进时同步具体提案或路线，记录证据；已交付能力从工程待办移除，验收缺口单列。本文分类描述当前进度，历史提案的 `proposed` 状态不等于其全部内容尚未实现。

## 工程待办（6 项）

按建议优先顺序排列，尚未开始的新项明确标注；不把人工验收算作工程开发。

记忆管理补充（2026-11-23）：入口沿用用户当前的「设置 → 记忆」。当前只管理已有结构化主张；候选需由聊天工具创建，旧聊天/Wiki 的候选发现与回填未实现。对话另有历史、画像、Wiki、事件等供给通道，空列表不能代表对话没有历史信息。[FR-PES-007 统一记忆与使用可见性](requirements/product/FR-PES-007-统一记忆与使用可见性.md)已补充来源/范围分离、三类对话统一资格、候选发现/回填和本轮使用记录，仍为 Draft、未实施。该缺口归属下面的 Agent 记忆增强项，不能用页面已完成代替数据生成和可见性闭环；表内初版知识设置入口描述为历史交付位置。

| 项目 | 当前缺口 | 优先级 |
|---|---|---|
| [Agent 记忆增强：来源、纠错与任务召回](notes/proposed/architecture/2026-11-21-agent-memory-anthropic-research.md) | **生命周期、受控跨会话、M5 保守策略及专用管理页已接线（2026-11-23）**：本地主张/来源/决定、冲突与替代、否定删除防复活；有向消息快照授权及撤回；最终请求/重试资格检查；限额自审生成审阅卡；默认关闭的 8 项/800 字符索引；访问回执。迁移 93 新增经审阅的固定/排除、人工显著性、活跃目标关联、有界重复/断链线索与回执清理，保留普通自审名额，不自我强化或自动删除。「设置 → 知识 → 记忆管理」通过专用桥接提供分页筛选、来源详情、单项控制及目标关联，沿用原版本/token 审批且不触发模型或聊天后续。[验证与限制](testing/2026-11-23-personal-memory-controls.md)。**待办**：M0/M2 真实模型等预算质量与跨会话基线、7 天/多模型/实机验收；任意自述/同义主张语义判定，M4 及 M5 排序收益对照。新表仅本地，跨设备记忆协议未实现。整体 FR 未验收完成 | 中 |
| [个人规模同步简化](notes/proposed/architecture/2026-11-12-personal-scale-sync-simplification.md) | **设计提案，未排期**：2026-11-12 现场互锁（GC 重快照标记 × 配对 request id 校验）暴露同步子系统约 44,500 行、711 处失败关闭点、9 张 `sync_*` 状态表远超个人场景需要。提案三条主张：用“快照永远可用”替代增量追赶+GC、名册恒等于实际设备数、单 owner 降级处理器移交。P0 修复（GC 不标记配对中设备、卡死 building 导出可接管，迁移 v81）已交付并合入本条；P1 按 §3.1→§3.2→§3.3 推进 | 高 |
| [待办讨论与 Wiki 结果](notes/proposed/architecture/2026-11-10-work-item-discussion-and-wiki-outcomes.md) | **设计草案，未实施/排期**：建议 A 先分离专属讨论与参考知识、修实时状态及旧关联，覆盖同步与过程页隔离；B 后置可选执行结果录入与已有消化/依据回链。保留 todos，不再默认推进[旧提案](notes/proposed/architecture/2026-09-22-work-items-as-first-class-pages.md)的全量页面化/导航重构 | 中 |
| [Wayland 窗口恢复](notes/proposed/bug-fix/2026-10-10-window-geometry-wayland.md) | 关闭时补存已实现。恢复侧仍需 compositor 实机取证后决定尺寸/位置策略，不以重复 `setBounds` 认定修复 | 中 |
| [数据监控与情报采集](notes/proposed/product/2026-11-15-watcher-data-collection-and-analysis.md) | **商机研究工程推进中（2026-11-20）**：发现页补来源多选、跨源有界分页去重、自适应卡片与相对拉取时间；主对话速览已收敛为最多三条产品信号，无可靠新证据保持安静；[O0–O7 路线](roadmap/2026-11-17-scout-opportunity-research-roadmap.md)已接通成功观察/续传、产品归并与窗口趋势、发现/个人研究、人工证据/可选 AI 候选、手动主题和主动偏好；O0.2/O0.3/O1.3/O3.4/O6.2 工程项完成，其余缺口逐项保留。专项 Workers 58、Rust Scout 17 + 同步 2、Flutter 36 通过，1 万产品/100 万观察本地查询已测；全量仍有失败。三源单轮通过，不代替连续 7/30 天、20 真实模型审阅、真人/用户实验与正式发布；见[验收记录](testing/2026-11-17-scout-research-acceptance.md)。自动评价/主题建议与部分研究回链待补；多 token/多服务器及未批准方向讨论仍按原案后置。 | 中 |
| [按需翻译：知识页与情报条目共用](notes/proposed/architecture/2026-11-17-on-demand-translation.md) | **设计提案，未排期（2026-11-17）**：知识页译文落派生产物（`area=derivative`、`content_type='译文'`、`based_on=原页`）而不进正文修订，避开 `escalate_page_task` 的「翻译→改写」误升级；情报条目**实际只有 GitHub `description` 与 Product Hunt `summary` 两个源有可翻文本（631/994 条）**，CWS / Gumroad / App Store 的 payload 只有专名、数值、日期与 URL，不为它们写分支；产品名一律不译。，块级缓存按 `(source_hash, target_locale, prompt_dependency)` 命中，迁移 88。**评价正文当前未采集**（库里只有 `ratings_average` / `ratings_count`），故翻译评价目前是空操作；若将来采也只翻正文且不得进 `fields`（会造假 changed）。首版用现有 LLM provider，不引入专用翻译引擎，判断依据与四条重估条件见提案 §11。尚未开始实现；前置是先做真实材料质量基线 | 中 |

## 工程完成，待验收（15 项）

每行只列尚缺证据；验收失败再登记具体工程缺陷，不重新开发已有功能。

| 项目 | 已实现及剩余验收 |
|---|---|
| [合成型问题的上下文供给](notes/proposed/architecture/2026-11-13-synthesis-context-supply.md) | P0–P4 工程完成（2026-11-16），各期落地记录见[路线](roadmap/2026-11-14-synthesis-context-supply-roadmap.md)：P0 `68ad97a` 页级 TL;DR 修正（迁移 83）；P1 `4d079d5` 工具按下一次请求的余量返回内容、事件按词检索；P2 `29ebb32` 只读合成工具 `synthesize_knowledge`（工具内分批读原文，返回带页内位置的逐字证据与未读范围）；P4 `47ac931` 估算偏差测量；P3 知识检索的 FTS5 词法索引（迁移 84，结果与扫描相同，十万页合成语料检索从秒级降到 2–540 毫秒）。Rust 库测试 1264 通过／0 失败／26 忽略；未改公开 API 与 Dart。**待验收**：全程未调用真实模型——TL;DR 摘要质量、模型是否会主动调用合成工具、证据提取质量、耗时与用量，以及原失败场景（run `564fe4bb`／`8fd94ade`）的重放都没有做；未在个人数据库和 Android 上验证索引。**估算校准（2026-11-17，用户决定）**：实测原 token 估算约为真实的 2.2–2.4 倍（64K 档实际只装到约 2 万真实 token），已对有实测数据的模型族（deepseek／glm／kimi／moonshot／gemini／gpt-oss）把汉字系数从 2.0 降到 1.2，并把这批模型族里的标识符串改按每字符 1 token 计；未知与本地模型不变。下调后的请求级偏差尚未重新测量。**未做的小项**：界面上合成工具的步骤名与证据展开；浏览页搜索、历史否定提醒和事件检索仍是扫描 |
| [对话支撑 LLM Wiki](notes/proposed/product/2026-10-05-single-stream-topic-follow-ups.md) | 多问法召回、修订新增依据与依赖、相近新建／无增量去重及中英文依据预览已交付；Rust1212项单元＋2项集成、Flutter612项、Release和门禁通过。用户按[现有页面优先测试计划](testing/2026-11-09-wiki-conversation-support-manual-test.md)先做E1–E4，再用已有材料覆盖T1–T10；虚构样例可选。已按[现有三问反馈](testing/2026-11-10-wiki-existing-page-quality-review.md)优化短页全文/长页分段与版本保护、引用记账及证据边界规范，Rust1227项单元＋2项集成和Release通过；新增独立负知识提醒（历史原因与当前条件核对，不作正面引用或草稿依据）；修复后原问法在新会话/长会话的忠实性、负知识避坑效果、写入与失效闭环仍待验收，不重复排主题状态／排序 |
| [对话伴随工作区](notes/proposed/product/2026-10-04-chat-dialog-to-drawer.md) | 当前待办／目标速览／审阅已接入桌面并排工作区，窄屏底部，跨宽窄保留草稿；Flutter610项通过。待真人桌面与移动触摸／返回体验验收，范围及门禁见[路线](roadmap/2026-11-09-chat-companion-workspace-roadmap.md)，不恢复草稿／生成历史旧入口 |
| [中英文支持](roadmap/2026-11-06-multilingual-support-roadmap.md) | **L1–L6 工程完成，Goal 暂停**。2018 项资源及资源门禁已接通；L7 真实模型、桌面/Android 实机、多设备语言/owner 验收未完成，交付 C 未关闭。原文、协议及运行固定约束继续有效 |
| [Prompt 入库与版本管理](notes/implemented/architecture/2026-11-05-prompt-database-and-budget.md) | 修订、CAS 激活/回滚、运行 bundle、核心处理接线已完成；待真人编辑/回滚/运行版本体验验收。两个诊断按钮默认隐藏，连续点击标题 10 次显示。多语言验收只在上行维护 |
| [洞见浮现与行动闭环](notes/proposed/product/2026-11-05-surfacing-digest-insights.md) | P0–P2 已实现；待真实模型端到端质量及真机多设备验证 |
| [命名密钥库](notes/implemented/architecture/2026-11-05-named-secrets-runtime.md) | 密钥存储、Provider 选择/手填、引用、轮换、审计与删除已实现；待真人 GUI 操作验收。旧未密封安装及带 Bearer 前缀的存量处理边界见实现记录；不再保留旧测试批次失败作为当前结论 |
| [配对快照累计写入采样](roadmap/2026-10-26-enrollment-snapshot-convergence-roadmap.md) | blob 数据面与真机收敛已有证据；仍缺累计写入字节 ≤1.05× 的精确现场采样 |
| [目标系统演进](notes/proposed/product/2026-11-05-goal-system-evolution-profile-knowledge-alignment.md)／[拆解与航向对齐](roadmap/2026-11-04-ai-centric-goal-decomposition-roadmap.md) | Phase 1–4 已接通；[语义守护](roadmap/2026-11-09-semantic-compass-phase4-roadmap.md)真实模型9场景、Rust1202项单元及2项集成、Flutter605项及Release／FRB／本地化门禁通过。剩余为自动检索／建议原有真实模型质量、真人GUI／真机同步及长期真实记录观察；合成场景不替代这些验收，继续使用profile/me与自动知识检索 |
| [草稿来源发现](notes/proposed/bug-fix/2026-10-08-save-draft-source-discovery-loop.md) | 错误指路和目录 slug 修复已实现；待真实模型原场景复验，确认没有继续陷入来源发现循环 |
| [记事本页](notes/proposed/product/2026-10-19-notepad-memo-pages.md) | `memo`、AI 隔离、创建/追加与 Flutter 入口已实现；待真人 GUI 验收。旧 Capture 模式已删除，不恢复该验收项 |
| [定时提醒](notes/proposed/architecture/2026-10-19-timed-reminders.md) | 持久提醒、5 秒 tick、稍后提醒、解析及确认制工具已实现；待真人 GUI 验收。系统通知为后置增量 |
| [习惯主动问候](notes/proposed/product/2026-10-22-proactive-habit-checkins.md) | 问候、聊聊预填、限流/忽略反馈、设置与 owner 单写已实现；待真人 GUI 和多设备验收 |
| [多设备同步 S06](notes/proposed/architecture/2026-09-30-multi-device-sync.md) | 核心同步与后续 relay 改造已有交付记录；综合真人门禁按[人工测试路线](roadmap/2026-10-14-sync-manual-test-roadmap.md)与[S06 清单](roadmap/2026-10-14-s06-human-gate-checklist.md)完成。D6/D7 已补齐旧副本显式清理与合法加入期消息等待，Rust1222单元+2集成、Relay49项及15项变异验证完成；Flutter617项、analyze与Release／FRB／本地化门禁通过，证据见[生命周期路线C13–C17](roadmap/2026-10-27-sync-lifecycle-boundaries-roadmap.md)。按[双端测试步骤](testing/2026-11-10-sync-d6-d7-manual-test.md)验收系统密钥、清理恢复和录入期间加入收敛；2026-11-19 增补稳定快照续传、owner 转发逐跳确认、AI/同步交替与 UI 竞态修复，工程证据见[同步修复记录](notes/implemented/bug-fix/2026-11-19-sync-resume-routing-and-scheduling.md)，真人门禁不变 |
| [AI 对话体验](notes/proposed/architecture/2026-09-30-ai-chat-experience.md) | C00–C09 工程及合成场景完成；待[人工验收 S1–S6](roadmap/2026-10-04-ai-chat-manual-acceptance-roadmap.md) |

## 后置增量（3 项）

| 项目 | 边界 |
|---|---|
| [Today 每日回顾工作台](notes/proposed/product/2026-09-20-today-daily-review-workbench.md) | 无当前排期；已有回顾读面和周期统计不能代表完整工作台交付 |
| [提醒的系统通知](notes/proposed/architecture/2026-10-19-timed-reminders.md) | 应用内提醒已实现；系统通知通道、后台送达及平台权限另行设计与验收，不与 Android 同步保活通知混为一项 |
| [记事本的系统通知入口](notes/proposed/product/2026-10-19-notepad-memo-pages.md) | 旧提案保留的后续增量，未排期；Capture 模式及快捷落点已删除，不借此恢复旧架构 |

## 方向讨论（3 项）

| 项目 | 当前结论 |
|---|---|
| [Scout 方向调整：发现→周报→按需下钻](notes/proposed/product/2026-11-17-scout-discovery-direction.md) | **2026-11-17 讨论定稿，待用户批准轻路径**：用户三需求（每天打开/赛道升温判断/创作者研究）暴露现有 7 源三层结构问题——采集端代码预设绑死、GitHub 查询是错的发现形态（史前巨兽榜）、缺综合层。四条调整：①源设置化（Gumroad 分类/CWS 清单进 /admin，不再写死代码）；②GitHub 去语言化改「90 天内新星仓库」发现型查询；③周报综合层（确定性分桶 + AI 归纳三档升温判断 + 证据回链，复用 digest 管线）；④人物层定位为**社媒创作者按需研究**（出海平台 TikTok/YouTube/X，小红书否决；seller 索引否决为无信号价值），监测是研究输出不是输入。轻路径先验证周报再投入 O1–O3 重工程；Glasser 定位为下钻工具不进采集。O0 工程缺口不推翻但优先级被改写 |
| [规则生命周期后续设计](notes/proposed/product/2026-10-19-rule-lifecycle-decoupling-discussion.md) | 核心解耦已交付；规则字段、影子判定纠正、适用结果消费与成本上限尚未定案，不直接生成开发任务 |
| [Elsewhen 作为 Agent Harness](notes/proposed/2026-10-02-elsewhen-as-agent-harness.md) | 现状对照与方向清单，不是已批准实施路线 |

## 搁置（1 项）

| 项目 | 原因 |
|---|---|
| [移动端执行委托](notes/proposed/architecture/2026-10-18-mobile-execution-delegation.md) | 已明确 Deferred；场景/设备形态与成本不匹配，无排期 |

## 已交付能力与归属

这里只指向实现或已有主线，不重复生成待办。更早的归档项和逐批证据见[历史快照](reference/proposal-backlog-history-2026-11-08.md)。

| 项目 | 归属与当前边界 |
|---|---|
| [超长事件分块消化](roadmap/2026-11-09-long-event-digestion-roadmap.md) | 持久阅读、预算／合并、原子提交、worker与中英文只读进度已交付；真实gpt-4o完成100,663字合成事件，17叶／3合并、来源与原文保留。39次调用及失败／短事件空提议／无多设备实机范围见[验收记录](testing/2026-11-09-long-event-real-model-acceptance.md) |
| [预算与工具注入](roadmap/2026-10-19-context-budget-and-tool-injection-roadmap.md) | 固定指令瘦身、MCP 延迟注入/缓存、估算与请求边界已实现；更广的路线缺口在该路线维护。消化动态缩批次/选材见[修复记录](notes/implemented/bug-fix/2026-11-08-digest-context-budget.md)；单事件超窗由上方已交付分块消化覆盖 |
| [MCP 意图关键词](notes/implemented/architecture/2026-11-04-mcp-intent-keywords.md) | 工程完成；真实模型触发率仍可在对话场景观察，可选 nudge 未启用 |
| [relay v3](roadmap/2026-10-24-relay-v3-dataplane-roadmap.md) | R0–R7 已交付；累计字节采样单列验收，不重复实现传输 |
| [决策辅助闭环](roadmap/2026-10-14-decision-support-loop-roadmap.md) | D0–D6 与今日记忆检索/反馈/回访已有实现，不再独立排“今日记忆检索” |
| [摘录孵化](notes/proposed/product/2026-10-19-clip-incubation-and-negative-knowledge.md) | 存储、快照、孵化分组与纳入/否定入口已实现 |
| [知识持续更新与关系网](notes/proposed/architecture/2026-10-19-knowledge-page-reconsolidation-and-relations.md) | P0–P3 及整理队列/维护展示已存在；语言覆盖见 L6，不再登记“队列面板未实现” |
| [启动、缺表升级与主题编辑](notes/implemented/bug-fix/2026-11-08-startup-schema-and-topic-editing.md) | 迁移 75–77 相关修复及后台版本检查优化已提交；新构建启动自动升级。性能数字不等同整机启动时间 |
| [旧快速捕获故障](notes/proposed/bug-fix/2026-10-10-quick-capture-no-ui-switch.md) | Capture 模式已删除，旧故障不再适用 |

## 合并提案的查找入口

| 原提案 | 归属 |
|---|---|
| [个人认知主循环](notes/proposed/architecture/2026-09-17-personal-cognition-main-loop.md) | [主路线](roadmap/2026-09-17-personal-cognition-main-loop-roadmap.md)；Today 单列后置 |
| [今日记忆检索](notes/proposed/2026-09-15-daily-memory-retrieval.md)、[决策辅助设计](notes/proposed/architecture/2026-09-19-decision-support-loop.md) | 决策辅助路线 |
| [目标表与偏差检测](notes/proposed/architecture/2026-09-28-goal-schema-and-deviation.md)、[周期偏差报告](notes/proposed/architecture/2026-09-30-periodic-deviation-report.md)、[AI 目标拆解](notes/proposed/product/2026-11-04-ai-centric-goal-decomposition.md) | 目标已交付能力与“目标系统演进／拆解与航向对齐”验收项 |
| [MCP 延迟工具注入](notes/proposed/architecture/2026-10-19-mcp-tools-deferred-injection.md)、[token 估算](notes/proposed/architecture/2026-10-19-token-estimation-calibration.md) | 预算与工具注入路线 |
| [relay 存储放大](notes/proposed/architecture/2026-10-24-relay-storage-amplification-and-quota.md)、[R4](notes/proposed/architecture/2026-10-25-relay-v3-r4-windowed-snapshot-plan.md)、[R5](notes/proposed/architecture/2026-10-25-relay-v3-r5-drop-and-invariants-plan.md)、[R6](notes/proposed/architecture/2026-10-25-relay-v3-r6-deliver-state-plan.md) | relay v3 路线；采样单列验收 |
| [密钥提案](notes/proposed/architecture/2026-11-05-named-secrets-and-references.md) | 命名密钥库实现记录及真人验收 |
| [多语言评估](notes/proposed/product/2026-11-04-i18n-feasibility-assessment.md)、[Prompt 多语言设计](notes/proposed/architecture/2026-11-05-versioned-prompt-management.md) | 多语言路线 L1–L6 工程完成，L7 验收暂停；不重复排 Prompt 入库 |

[超长事件原始提案](notes/implemented/architecture/2026-11-08-long-event-chunked-digestion.md)的执行进度统一由上方roadmap维护。
