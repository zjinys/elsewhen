# 中英文支持 Roadmap（界面、AI 对话与知识语言）

- 建立：2026-11-06。
- 状态：**L0–L6工程完成；L7待验收；Goal 按用户要求暂停（2026-11-08）**。
- 恢复后下一步：L7真实模型、桌面/Android及多设备验收；本轮 backlog 整理不恢复 Goal。
- 产品依据：[多语言可行性复核](../notes/proposed/product/2026-11-04-i18n-feasibility-assessment.md)。
- 运行契约：[版本化 Prompt 与多语言策略](../notes/proposed/architecture/2026-11-05-versioned-prompt-management.md)。承接该设计未完成的 P3，并包含 Flutter 全应用本地化；不重做已交付的中文入库。
- 进度归属：[主路线](2026-09-17-personal-cognition-main-loop-roadmap.md)的跨模块子路线；[backlog](../proposal-backlog.md)只保留一条多语言待办，本文件记录具体执行进度。

## 目标与交付范围

让用户在简体中文与英文之间切换，界面、编辑器、AI 对话和后台知识处理各自遵循明确的语言策略。首期支持 `zh-Hans` 与 `en`，其他语言标签只按已定义规则回退，不据此宣称完整支持。

| 交付 | 范围 | 完成条件 |
|---|---|---|
| A：主界面与对话中英闭环 | 导航、设置、主对话、目标讨论、页内对话、运行状态与审阅；AI 资源与对话行为 | L1–L4 工程及专项验证通过；知识产出保持中文，覆盖范围明确 |
| B：知识语言与多设备 | 数据集知识语言可修改，后台处理、缓存与 owner 切换一致 | L5 通过后开放设置；已有内容保留原文 |
| C：应用完整中英支持 | 其他业务页面、错误、桌面壳层及平台通知等用户可见入口 | L6 完成覆盖，L7 双语真实模型和设备验收通过 |

工程完成、真实模型质量与真人设备验收分别记录。A/B 是可独立验证的阶段成果，不代替 C 的全应用验收。

## 核心约束

1. 界面、指令、回答、知识产出四类语言独立决策。切换界面不会改变数据集知识语言，也不改写历史内容。
2. 升级保留现有中文体验；默认知识语言仍为 `zh-Hans`。新库离线可启动，资源随应用发布，不依赖在线翻译。
3. 工具名、JSON 字段/枚举、slug、持久化状态和 `[工具调用]`、`[规则提议]`、`[规则本质]` 保持原值；仅翻译说明与展示标签。
4. 原始事件、页面原文、用户 profile 内容、验证引文与代码保持原文。已有页面的普通修订保留该页语言，明确翻译沿用派生稿/审阅流程。
5. 自定义 Prompt、激活项与完整 bundle 留本机；跨设备复制业务数据不会激活另一设备的模板。语言回退可见，不静默套用另一语言的用户覆盖。
6. 一个运行的多轮工具、分块、摘要和 provider failover 使用固定语言策略与模板；设置变更影响新运行，不能在旧作业中途切换。
7. 后台只依据数据集知识语言及本机明确的指令覆盖；语言变更不触发整库付费重跑。维持自动、有界调度，不新增后台手动触发按钮。
8. 英文交流沿用现有审批、证据与原文保护。自然语言确认/拒绝不直接执行或丢弃待审写入；分类器只提供对话理解信号。
9. token 预算继续检查完整序列化请求、工具 schema 和输出预留；语言设置不放宽预算。

## 语言设置与优先级

| 设置 | 归属与入口 | 默认与优先级 |
|---|---|---|
| 界面语言 | 本机应用设置 → 外观 | 中文 / English / 跟随系统；缺设置保留中文；不进入业务同步 |
| AI 回答语言 | 本机模型设置 | 自动 / 中文 / English；本轮明确语言要求优先，然后会话/本机策略 |
| 指令语言 | 本机提示词页高级选项 | 未覆盖时，交互跟随界面、后台跟随知识语言；可独立固定 |
| 知识产出语言 | 数据集设置，owner 可改、成员只读 | 初始中文；模型 → 提示词中提供修改，受数据集owner与成员能力检查约束 |

`auto/system` 使用模式字段或空值表达，不充当 locale。回答自动模式按用户本轮交流语言，短追问/问号/代码沿用最近可靠会话语言，否则用运行固定的回退语言；引用与工具结果不改变判断，不额外请求模型检测语言。自然语言要求未被确定解析时，不伪造“已检测输出语言”的运行证据。

保留已有脚本/地区回退约定，例如 `en-GB → en`；明确繁体请求可说明缺译并回退内置资源，不误用简体自定义修订。恢复当前语言默认仍屏蔽父语言用户覆盖。

## 阶段与依赖

| 阶段 | 内容 | 前置 | 状态 |
|---|---|---|---|
| L0 | 现状复核、范围与契约 | 中文 Prompt 首版 | 已复核，见完成记录 |
| L1 | 偏好持久化与本地化基础 | L0 | 已完成（2026-11-06，证据见下） |
| L2 | 主界面、编辑器与日期格式 | L1 | 已完成：781项中英资源，Flutter全量569项通过 |
| L3 | AI 资源、语言策略与运行固定 | L1 | 工程完成：双语资源、实际运行/修复/分块/故障切换快照及版本设置 |
| L4 | 英文对话行为与主链路回归 | L2、L3 | 工程完成：同组中英场景、审批/事实/原文纪律与否定变异验证通过 |
| L5 | 知识语言同步、任务与缓存 | L3、L4 | 工程完成：策略同步/兼容门禁、任务固定/缓存与设置回归通过 |
| L6 | 其余页面、错误与平台文案 | L2；全量覆盖依赖 L5 | 已完成：2018项资源、错误/平台接线、AST及ARB门禁，证据见L6最终记录 |
| L7 | 双语真实模型、设备与发布验收 | L4–L6 | 未开始 |

执行顺序为 L1 → L2 → L3 → L4 → L5 → L6 → L7。L6 的文案盘点可提前进行；不提前开放缺少运行或同步支持的设置。

## L0：现状复核与契约

- [x] 复核现有 locale 修订/回退、运行 bundle 与缓存依赖基础。
- [x] 明确三次交付与四类语言语义；协议字面保持不变，知识语言修改以同步为前置。
- [x] 确认 UI、动态指令、工具说明、对话词表和错误回流均属于实施范围。

证据：2026-11-06 [可行性复核](../notes/proposed/product/2026-11-04-i18n-feasibility-assessment.md)及本文；同日复核执行 `cargo test storage::prompts::tests --lib -- --test-threads=4`，4 passed / 0 failed，日志 `/tmp/i18n-prompt-foundation-tests.log`。这是中文首版基础验证，不是英文功能验收。

## L1：偏好持久化与本地化基础

- [x] L1.1 建立界面语言的本机持久化读写与生效状态，明确与数据集切换的关系，修复 `AppSettings.copyWith` 无法显式清空语言覆盖的问题；“跟随系统”能够设置、保存和恢复。
- [x] L1.2 增加应用 ARB、本地化生成配置及 Material/Widgets/Cupertino delegates，接通支持语言、系统变化和缺译回退；启动先加载偏好，避免进入主界面后闪回另一语言。
- [x] L1.3 接通已有 `prompt_language_preferences` 的读写/API与 generation 并发校验；数据集知识语言本阶段只读，仍固定中文。新增 API 后生成 FRB，保持 Dart/Rust/release 库一致。
- [x] L1.4 定义前台实际 locale 到 Rust 运行准备的传递边界；后台在无前台 UI 时仍能独立决策。界面偏好不依赖同步初始化，不从业务快照恢复远端显示语言。有效locale已接入进程状态，实际bundle采用仍由L3完成。
- [x] L1.5 验证升级库/新库、无设置/非法值、显式切换/跟随系统、重启及数据集切换；桥接测试使用隔离临时目录。

主要落点：`ui/lib/main.dart`、`ui/lib/models/settings.dart`、`ui/lib/providers/settings_provider.dart`、`ui/lib/screens/settings_parts/`、`src/api/prompts.rs`、`src/storage/prompts.rs`；按实际存储归属新增语言配置模块。

退出证据：真实 Bridge 保存后重开能读回；widget 能在两种 locale 下构建；Rust 保存与读取规则一致，空库离线可用。语言控件只在对应运行能力已接通后开放。

2026-11-06证据：[实现记录](../notes/implemented/architecture/2026-11-06-multilingual-runtime.md)。Rust语言专项3项、Prompt专项5项通过；Flutter专项28项、全量541项通过；修改文件analyze无问题。`./scripts/regen.sh`及release编译通过，桥hash两侧均为`-1499826736`。日志为`/tmp/i18n-l1-{rust,prompts,flutter-tests,flutter-all,analyze,regen}.log`。

## L2：主界面、编辑器与格式

- [x] L2.1 将导航、启动/锁屏、外观/模型设置、Provider/密钥/提示词相关界面迁入资源；名称输入、代号和用户数据不翻译。
- [x] L2.2 迁移主对话、目标工作区、页内对话、运行状态、工具/建议卡及审阅菜单文案；保持既有布局与审批行为，检查英文长文本和移动端溢出。
- [x] L2.3 将 AppFlowy 的固定中文 delegate 与静态 `AppFlowyEditorL10n.current` 改为同一生效 locale；初始化相应日期符号，处理异步切换竞争，避免旧加载覆盖新选择。（2026-11-06，编辑器中英切换与日期回归、真实Bridge快速切换/系统语言变化通过，见L1证据）
- [x] L2.4 日期、相对时间、数量/复数和数字展示使用 locale 规则；数据存储/传输时间戳、机器可读数字格式不变。
- [x] L2.5 将核心 UI 测试从单一中文匹配改为稳定 key 与双语文案/参数断言，保留文案内容和动作正确性的检查。

主要落点：`ui/lib/utils/editor_localization.dart`、`ui/lib/screens/`、`ui/lib/widgets/message_area_parts/`、`goal_*`、`chat_run_status.dart`、`chat_proposal_parts/` 及相关测试。

退出证据：中文/英文导航、设置、编辑器、对话和审阅均可用；修改语言不丢草稿、不重建会话、不改变页面原文；日期及编辑器语言一致。

2026-11-06阶段进度：614个中英资源key，主要导航/外观/模型/Provider/密钥/提示词表单、主对话输入/搜索/消息菜单/队列/状态、目标树/详情/讨论、页内AI/审阅详情已接入。新增英文审批版本绑定、原文复制/日期、锁屏错误保留、真实Bridge密钥选择/保存重开、语言热切换保留未保存表单回归。剩余知识侧栏空态、材料范围选择器、航向/习惯/洞见/联系人卡片、部分辅助弹窗/错误/格式继续归L2；保持L2.1/2/4/5未全勾选，不以资源数量代替全覆盖。实际AI语言仍待L3。

验证：`fvm flutter test --timeout 45s --concurrency=4` **550 passed**，修改文件analyze无问题；资源key/生成与diff检查通过。日志`/tmp/i18n-l2-all-final.log`、`/tmp/i18n-l2-final-analyze.log`。此前默认并发全量549 passed/1失败为同步worker400ms时限内未观察到调用，单独复验通过；本轮并发4全量含该用例全部通过，未改弱其调用次数/暂停断言。

2026-11-06补齐L2：知识侧栏、材料范围/版本选择、航向/习惯/洞见/联系人建议、知识预览、存为主题及反馈弹窗接入，累计781项中英资源。习惯校验保存语义状态，热切换不丢草稿/自定义问法/按钮/记录句；明确刷新才更换材料版本；360px航向卡按钮换行。相关日期与数字按locale展示，机器时间/选区与JSON不变。辅助专项51项、Flutter全量**569 passed**，19入口analyze无问题。日志`/tmp/i18n-l2-aux-tests.log`、`/tmp/i18n-l2-habit-dialog-test.log`、`/tmp/i18n-l2-complete-all.log`、`/tmp/i18n-l2-complete-analyze.log`。其他设置/业务页面与统一错误契约归L6，实际Prompt语言和运行策略归L3，均不以L2完成替代。

## L3：AI 资源、语言策略与运行固定

- [x] L3.1 扩展九份内置模板的资源目录、注册身份与 resolver，内置修订区分 key/locale/内容版本；新增英文资源不覆盖中文或用户修订。固定协议资源仍只读。
- [x] L3.2 迁移目标/profile、知识导师、引用要求、分析/摘要/阅读/消化/页内任务、最终答复/修复等动态指令的自然语言部分；事实材料与模板变量严格分离，缺变量明确报错。
- [x] L3.3 将内置工具与参数 description 接到运行语言资源；工具名、参数 key、schema 类型/枚举及授权保持原样。第三方 MCP/Skill 原始说明保留来源文本，应用生成的目录/引导说明本地化。
- [x] L3.4 将四类语言策略保存到运行 bundle 并用于实际请求；工具多轮、摘要、分块与 failover 复用固定快照。缺译回退保留实际 resolved locale 与原因，回答/产出语言指令独立生效。
- [x] L3.5 提示词页支持选择语言变体、独立草稿/启用/恢复默认及运行语言查看；标题改用稳定本地化 key。打开设置/预览不调用模型，翻译副本不自动启用。
- [x] L3.6 验证中英独立激活、显式默认屏蔽父覆盖、地区/脚本/缺译、不兼容修订；切语言/改模板不改变旧 run，新 run 使用新值，协议解析保持一致。

主要落点：`resources/prompts/`、`src/storage/prompts.rs`、`src/api/prompts.rs`、`src/ai/memory/`、`src/ai/tool/`、`src/ai/summary.rs`、`src/ai/page_tasks.rs`、`src/api/analysis_queue.rs`、`src/knowledge/reading.rs`、`src/wiki/digest.rs`、`ui/lib/widgets/prompt_settings.dart`。

退出证据：英文界面可自动回答或固定中文/英文，中文界面也能固定英文；运行详情反映实际模板/语言策略，编辑另一语言不影响当前激活；双语完整请求均受预算约束。

2026-11-06 L3.1：九份模板新增en资源，内置修订ID包含key/locale/hash，注册只追加且保留历史bundle。标题使用稳定资源key，内置默认预览使用请求语言的内置版本；地区回退、显式默认屏蔽父自定义、跨脚本缺译只选内置、不兼容覆盖回退及固定协议只读均通过。`cargo test storage::prompts::tests --lib -- --test-threads=4` **8 passed**，日志`/tmp/i18n-l3-catalog-tests.log`。本项不表示实际运行已采用英文，L3.2–6继续推进。

2026-11-06后续：L3.5模板语言变体/独立编辑启用默认、回答与指令偏好、实际运行语言查看完成；当前12份双语模板中，chat.context含59条严格变量资源，tool.descriptions含37工具/95参数说明。创建运行即固定四类语言，实际HTTP多轮工具保持旧策略，新run使用新策略；后台依赖不随UI/回答偏好改变。专项存储13、上下文32、语言3、工具50、真实Bridge4通过，6入口analyze通过；Flutter在前一模板设置交付点全量571通过，后续全量收口单独记录。[实现记录](../notes/implemented/architecture/2026-11-06-multilingual-runtime.md)列明日志与限制。技能目录/启用引导45项已接入并保留第三方原文，修复/最终答复和部分页内/后台任务指令仍待补，L3.2/4/6保持未全勾选；L4–L7继续实施。

L3.3补齐：技能目录/7份内置技能/enable引导与文本回退清单45条资源固定入bundle，MCP/用户Skill原文不译，翻译后的目录头尾继续计入2000 token预算。工具专项50项、协议17项通过；此前运行/资源交付点Rust全量1084 passed / 20 ignored。最新界面与release收口继续登记，[记录](../notes/implemented/architecture/2026-11-06-multilingual-runtime.md)区分各验证点。

最新UI收口：Flutter全量572 passed，6入口analyze无问题；最新release的实际双轮工具/英文enable目录及旧run固定、新run采用新策略与设置独立编辑，追加3项专项通过。日志`/tmp/i18n-l3-final-ui-all.log`、`/tmp/i18n-l3-final-ui-analyze.log`、`/tmp/i18n-l3-final-runtime-bridge-tests.log`。L3.2/4/6仍继续，不把合成provider验收算作真实模型质量通过。

最新Rust收口：`cargo test --all-targets -- --test-threads=4`通过，库1085项、集成2项（共1087 passed / 20 ignored），其余example target无测试；日志`/tmp/i18n-l3-final-all-targets.log`。最新release与生成桥接一致，Flutter全量572项、相关6入口analyze通过。

2026-11-07推进：新增运行修复/页内处理55项、后台阅读/分析/摘要/消化16项、按回答语言生成的兜底与待审状态25项资源；目录为`prompts-v6`，15组双语定义只追加修订，旧v1–v5运行使用冻结兼容资源。临时内容对话按当前要求/历史用户语言创建独立preview快照，不保存临时消息；普通页面修订保留原文语言和审阅门禁。英文工具结果及原生转文本后的历史结果继续绑定已核验引用；摘要正文/来源引文原样保留，仅翻译包装说明。实际HTTP故障切换验证设置中途变化不改备用Provider的指令/输出策略。前一交付点Rust全量1093 passed / 20 ignored，日志`/tmp/i18n-l3-execution-processing-all.log`；后续英文专项14项通过，日志`/tmp/i18n-l4-english-behavior.log`。完整收口证据追加于实现记录，L3.2/4仍保留其余知识处理路径和缓存/同步工作，不提前勾选。

## L4：英文对话行为与主链路回归

- [x] L4.1 补确认/拒绝、继续/进度追问、用户写意图、完成声明、未来承诺与诚实失败处理的英文支持；语言判断不成为执行授权。
- [x] L4.2 补页内修改意图的英文否定/疑问/引用边界，如 `don't edit this` 与 `how would you edit this?` 不升级成修改任务；真实编辑仍产生待审稿。
- [x] L4.3 对 `saved/created` 类完成表达核验真实工具结果；只有草稿时清楚表达待审，失败/否定/示例不误判成成功或执行请求。修复次数继续受已有上限约束。
- [x] L4.4 参数化同一组中英场景：普通问答、目标/profile 引导与归集、知识检索、全文总结、编辑草案、明确拒绝、短追问、执行失败与 provider failover。
- [x] L4.5 对关键行为回归做针对性变异检查，例如移除英文否定守卫或允许自然语言执行后用例必须失败；不为逐条译文编写镜像实现测试。

主要落点：`src/ai/conversation/content_chat.rs`、`continuity.rs`、`claim_detection.rs`、`src/ai/page_tasks.rs` 及协议/工具测试。

退出证据：英文与中文的审批/事实语义一致；追问承接未完成原任务；否定不触发动作，空头完成声明能被发现。L1–L4 完成后登记交付 A 的工程证据和真实模型待验项。

2026-11-07 L4部分实现：英文完成声明、空头预告、写/改页请求、确认/拒绝与进展追问已接入保守信号；代码、引用、示例、否定和疑问不自动升级编辑；自然语言不执行或丢弃待审提议。英文虚报保存按真实待审状态纠正，短追问承接原请求；应用兜底按固定回答语言渲染，指令中文/回答英文独立。移除否定守卫的变异检查使`Save nothing; explain it instead.`回归失败，原代码已恢复；日志`/tmp/i18n-l4-negation-mutation.log`。其余英文搜索/直查、完整参数化场景及真实模型验收继续实施，L4复选框保持未完成。

2026-11-07界面收口：新release编译成功，809项中英key/变量检查通过；Flutter全量572项、3入口analyze、真实语言/模板Bridge3项通过，日志`/tmp/i18n-l4-current-ui-all.log`、`/tmp/i18n-l4-current-ui-analyze.log`、`/tmp/i18n-l4-final-runtime-bridge.log`。最终Rust全量与剩余范围见[实现记录](../notes/implemented/architecture/2026-11-06-multilingual-runtime.md)。

2026-11-07专项收口：L3.6完成，独立激活/默认/缺译/脚本/不兼容与旧run/新run、修复/分块/真实HTTP故障切换均有回归。L4.2/3/5完成：中英文普通确认/拒绝均不执行或删除提议；英文明确改页请求保留否定/询问/引文/代码守卫；虚报保存按真实页/草稿状态纠正，修复轮次仍有上限；否定守卫变异用例确实失败。`cargo test --all-targets -- --test-threads=4` **库1102项+集成2项通过（1104 passed / 20 ignored）**，日志`/tmp/i18n-l4-current-all-targets.log`。随后英文整句确认/拒绝专项1项通过，日志`/tmp/i18n-l4-acknowledgements.log`；L4.1英文搜索/直查盘点与L4.4完整参数化场景继续，不把部分完成算作交付A结束。

2026-11-07知识处理追加：知识提炼/修订、跨资料主题整合、洞察、主题合并拆分、重新整理、每日回顾、联系人关系提取与习惯建议的26项双语资源接入，后台账本关联实际bundle。新增英文搜索（含否定/引文/代码守卫）和规则/用量/目标直查，直查11项包装资源保留原始规则/事实。目录现为`prompts-v8`、17组定义/34份内置修订，旧v1–v7缺组时读取对应冻结兼容包。知识处理95 passed / 2 ignored、资源存储17 passed及英文知识重试/搜索/直查专项通过，日志`/tmp/i18n-l3-knowledge-complete-tests.log`、`/tmp/i18n-l3-knowledge-prompt-tests.log`、`/tmp/i18n-l3-knowledge-language-tests.log`、`/tmp/i18n-l4-search-final-tests.log`、`/tmp/i18n-l4-lookup-tests.log`。其后完整检查与release/界面结果另列，补充材料等剩余包装和L4完整场景仍不算完成。

2026-11-07补充材料接入：`prompts-v9`共18组/36份双语修订，新增补充材料与证据包装11项，含@wiki摘要/缺页/截断/用户原话、建议反馈、上游修订与阅读结果说明；原材料与消费/撤回/审阅状态不变，既有`知识选材：`JSON信封继续按协议保留。补充14项、英文补充1项、知识95项和英文19项专项通过。前一完整Rust1110 passed / 20 ignored，最新版补充增量的完整收口记录于实现说明。L3.2/4与L4.1/4仍未勾选，统一缓存/策略身份、同步/owner兼容归L5。

本阶段后续落点：L3.2收拢规则提议等少量应用提示及时间/证据包装例外清单；L3.4复核所有独立作业与跨tick/缓存身份（后者依L5）；L4.1完成其余词表盘点，L4.4把目标/profile、检索、全文任务、编辑、失败和继续等组织为同一组中英场景。知识语言修改仍只读，L5需要同时处理策略同步、应用端能力/owner门禁、逻辑任务与缓存身份，不能只增加一个本机语言开关。

2026-11-07阶段确认：L3.2/4与L4.1完成。L3明确列出的目标/profile、导师、引用、分析/摘要/阅读/消化/页内、最终/修复指令以及补充/知识处理已纳入运行固定资源；回归覆盖多轮、分块、模板/语言中途修改、重启读取与真实HTTP故障切换。`知识选材：`JSON信封和工具/规则标记保留为协议，来源原文/用户自定义Prompt与第三方说明按原值保留；少量应用生成回复/错误展示继续归L6，不是新执行授权。L4确认/拒绝/写意图/完成核验/未来承诺/进展和失败英文信号专项通过；同组完整中英场景的L4.4仍未完成。任务跨owner策略同步/缓存身份依原分工归L5，不将L3运行内固定误报为知识语言多设备已交付。

2026-11-07交付A工程完成：L4.4同一组中英输入/脚本场景验证普通问答与检索、目标/profile信息不足引导和明确事实归集、全文总结全字符覆盖、编辑草案/原文不变、明确拒绝不执行或丢弃、短追问承接与真实待审状态、缺页工具失败、双HTTP Provider故障切换/中途改设置。9项参数化专项通过（`/tmp/i18n-l4-bilingual-final-scenarios.log`）。统一场景还发现并修正旧中文“不要保存”漏否定守卫，原测试保留实际工具失败回执与审批状态断言。

最终完整交付点Rust all-targets库1110+集成2（1112 passed / 20 ignored，`/tmp/i18n-l4-stage-final-all-targets.log`），Flutter最新release全量572通过（`/tmp/i18n-l4-stage-final-ui-all.log`），3入口analyze无问题（`/tmp/i18n-l4-stage-final-analyze.log`），release通过。其后唯一生产增量为中文“不要保存”守卫，双语9项及后续对话全组回归验证，增量release重建；详见实现记录。本阶段无新增公开Rust API，FRB hash两侧保持`-1499826736`。交付A仅工程，真实模型、GUI和多设备不以合成HTTP/Bridge代替；继续L5–L7。

## L5：知识语言同步、任务与缓存

- [x] L5.1 将 `dataset_content_language` 从本机配置排除项迁为版本化数据集配置，接通 owner 写入、成员只读、增量操作、初始快照和已有冲突/epoch 裁决；追加迁移，不改已发布迁移73。
- [x] L5.2 实现应用端处理策略版本能力检查与 owner 接管门禁。未知/不支持策略暂停相应产出并可见；无能力声明的旧端不能静默成为非默认语言处理者，无法证明兼容时不开放修改。中继 blob 能力不代表应用语言能力。
- [x] L5.3 将实际输出语言、策略版本及相关指令/契约/渲染依赖纳入摘要、阅读、分析、消化指纹和逻辑任务身份；只改聊天风格或界面不失效后台缓存。
- [x] L5.4 旧任务跨 tick/同机重启保留 bundle；新策略创建独立任务/节点，不混旧树；跨设备 owner 没有本机 bundle 时基于已同步来源与策略重新领取工作，不假称原样续跑。
- [x] L5.5 保留历史 PageMark/摘要/产物可读性，正常调度只处理有界新工作；同一页面普通更新保留语言，原文/引文不译；产物携带必要的语言/策略 metadata，完整模板继续留本机。
- [x] L5.6 完成两/三份隔离库的增量、首次同步、重复/乱序、过期 owner、接管和不兼容版本回归后，开放知识语言设置；非 owner 只能查看。

主要落点：`src/storage/prompts.rs`、`src/storage/sync/`、`src/sync/protocol/`、`src/knowledge/queue.rs`、`src/knowledge/reading.rs`、`src/knowledge_background.rs`、`src/storage/digest.rs`、`src/wiki/digest.rs` 及数据集设置。

2026-11-07 L5接入点复核（尚未实现）：`dataset_content_language`仍在快照排除表，当前快照版本9；后台领取/提交使用`sync_background_permission`数据库视图与运行租约；owner/processor交接依赖签名roster、final_sequence和pending_takeover，roster结构拒绝未知字段。下一步需追加迁移与版本化策略记录，并明确应用能力声明及旧端隔离方式；不能把中继`blob`能力当作应用语言能力，不能直接给签名结构加字段而沿用旧签名/旧快照版本。已有Prompt/bundle仍留本机，策略进入同步后再调整来源阅读/摘要/维护的逻辑任务和缓存身份并开放修改入口。

2026-11-07交付B工程完成：追加迁移74（迁移73不改），策略带generation/roster版本/数据集与owner签名；增量使用head/CAS及待审冲突，快照升为v10并验证签名。应用能力由本设备安装stream认证声明，每次roster变更重新确认；缺能力成员不能接管非默认知识语言，未知策略在数据库领取闸门暂停。模板/激活/完整bundle与能力证据不由快照伪造或激活。

分析/消化领取时固定bundle，重试/重启沿用；阅读跨tick复用旧策略，新资料与明确新任务分离generation，调度不因设置变化重新处理已完成来源；摘要树策略包含输出/版本/generation/相关资源与渲染依赖，子节点不得混用，旧PageMark仍可读。语言metadata附于同步分析和知识页修订，完整模板不出本机；已有页面普通更新保留语言，原事件/引文/slug不译。设置接通CAS，非owner只读，能力不足/不支持可见。

验证：`cargo test --all-targets -- --test-threads=4`库1128+集成2（**1130 passed / 20 ignored**，`/tmp/i18n-l5-delivery-all-targets.log`）；Flutter全量**572 passed**（`/tmp/i18n-l5-ui-all-tests.log`），隔离真实Bridge设置/CAS/独立偏好**3 passed**（`/tmp/i18n-l5-ui-bridge-final-tests.log`），3入口analyze无问题，release完成。另有7项owner/冲突专项、189项同步全组、迁移73升级和带分析metadata的签名首次快照验证；最后设置门禁断言增量另列。FRB两侧hash为`1894755173`。编译脚本固定FVM子命令及真实C编译器，避免旧SDK与包装递归。真实设备不同UI语言配对与真模型质量继续按L7验收，不把隔离库/Bridge当作真机通过。

退出证据：两/三库增量/首次快照/重复乱序、成员权限/不兼容/接管/冲突、同机重启任务和无全库重跑回归通过，登记交付B工程。

## L6：其余页面、错误与平台文案

- [x] L6.1 完成用户可见文案清单：知识库/导入/派生稿、待办、规则/习惯/提醒、数据/运行日志、同步配对/冲突/owner、搜索/代理/技能/MCP等设置及空态/菜单/错误。
- [x] L6.2 建立 Rust → Dart 稳定错误 code/参数的展示契约并保留诊断详情；先覆盖常见错误，逐域迁移剩余用户错误。未知错误有本地化通用提示与可查看详情，不按中文字符串替换。
- [x] L6.3 补桌面托盘/窗口菜单/退出确认，以及实际存在的移动端保活通知等平台文案；更新展示 locale 不改变通知/同步生命周期。
- [x] L6.4 增加资源 key/占位符/复数规则一致性检查，清点剩余硬编码；排除生成文件、vendor、测试夹具、用户内容、机器协议及原始第三方说明，逐项记录合理例外。
- [x] L6.5 完成各模块双语 widget/Bridge/错误参数回归和布局检查；资源缺漏必须可发现，不能通过减少内容断言或隐藏入口取得通过。

2026-11-07 L6首批实现：待办列表/编辑/菜单/优先级/日期与习惯提醒设置、周计划/状态/反馈接入双语；英文380px优先级溢出改为换行，未发送输入热切换保留。Rust→Dart采用`elsewhen-error-v1` JSON信封（version/code/params），首批覆盖待办/提醒/语言设置及对象冲突；未知错误本地化回退，主动查看/复制保留诊断，禁止按中文字符串猜测翻译。真实Bridge验证错误code与参数及设置CAS。

平台接线：托盘菜单串行热更新，Android保活通知标题/频道/内容来自ARB，更新仅调用notify，保留原持有者和启停逻辑；7项生命周期专项通过。Android Kotlin尝试编译但缺当前engine的embedding缓存，联网重试Google Maven超时（`/tmp/i18n-l6-android-kotlin-online.log`），不得记为原生编译或真机通过。对象冲突卡新增知识语言类型/原因并双语化，原文、审阅时updated_at和执行授权不变；其余同步页面仍待迁移。

验证交付点：Rust all-targets库1129+集成2（**1131 passed / 20 ignored**，`/tmp/i18n-l6-domain-all-targets.log`）；Flutter全量**580 passed**（`/tmp/i18n-l6-current-ui-all-tests.log`）；待办/习惯/错误18项、真实Bridge3项、托盘/通知7项专项和release通过。最新对象冲突增量的Rust5项通过，其后同步冲突/共用外壳/托盘/通知增量20项通过（`/tmp/i18n-l6-sync-ui-tests-final.log`），release再编译通过（`/tmp/i18n-l6-sync-release-final.log`）；最终Flutter全量**581 passed**（`/tmp/i18n-l6-delivery-ui-all-tests.log`），11入口analyze无问题（`/tmp/i18n-l6-delivery-analyze.log`）；FRB hash保持1894755173。资源896项，同key/参数类型/ICU分支检查及失败变异检查接入CI；[覆盖清单](../development/localization-coverage.md)保留1520个候选字符串/109文件的归属与例外说明。知识库与其余同步/设置、剩余错误域和Android原生验证未完成，L6.1–5均不提前勾选。

退出证据：文案覆盖清单无未归属的应用用户提示，错误的 code/参数两侧一致，英文界面无意外中文残留；保留的原文/协议例外明确可解释。

2026-11-07 L6知识库第二批：分页检索/分区/类型/阅读状态/标签/批量审阅结果，导入表单/负知识确认/原文版本确认/网页预览，以及页签关闭保护、页面标签/编辑/复制/路径/关系/沉浸操作接入双语。显示locale热切换保留输入、原文与未保存正文；筛选wire值、原始标签、修订审计原因和导入snapshot CAS不改。英文380px筛选和保存按钮溢出已在回归中发现并修正。

知识页编辑冲突不再匹配中文错误；使用`wiki_page_changed`稳定码，空正文/素材只读/超长/缺页同样返回版本化契约。列表/读取/导入/批量操作的未知错误采用本地化回退，并保留主动查看诊断。导入的两项仓库委托移回可覆写的类方法，让假仓库验证保留输入与明确写入边界，生产委托行为不变。资源1018项（本批+122），候选1327项/98文件；保留审计/关系wire例外，见覆盖清单。

验证：知识库专项35项通过（`/tmp/i18n-l6-knowledge-tests-final.log`）；Flutter全量**588 passed**（`/tmp/i18n-l6-knowledge-ui-all-tests.log`），含导入表单2项与隔离真实Bridge知识页错误/CAS验证；Rust正文保存专项2项（`/tmp/i18n-l6-wiki-rust-tests.log`）与release（`/tmp/i18n-l6-wiki-release.log`）通过。FRB hash仍为1894755173，无公开Rust API变化。剩余推文专用整理/实体/派生/维护与同步/设置、Android原生及L7真模型/设备验收仍未完成，不提前勾选L6。

最终验证：Rust all-targets库1129+集成2（**1131 passed / 20 ignored**，`/tmp/i18n-l6-knowledge-all-targets.log`），Flutter全量**588 passed**（`/tmp/i18n-l6-knowledge-ui-all-tests.log`），14入口analyze无问题（`/tmp/i18n-l6-knowledge-analyze-clean.log`），release、1018项资源检查、文档链接/状态头和`git diff --check`通过。静态检查曾发现测试无用import，移除后重跑通过；失败记录保留，未减少功能断言。

2026-11-07 L6知识库第三批：推文预览的 AI Markdown 整理改为后端双语 Prompt 资源，整理任务记录 `preserve_source` 策略，原文语言不被聊天回答语言设置覆盖；整理仅生成预览，仍需用户点击保存才写入来源和派生产物。推文讨论、原文复制、保存/失败、窄屏输入与热切换接入双语。派生版本列表、正文按需读取、模型/流程/生成指令、采用/撤销采用、比较/复制/分页和错误详情接入双语；实体合并/撤销、结构化事实、来源事件、关联待办、相关指导及反馈错误接入双语，窄屏合并操作改为换行布局。原始事实、来源、生成指令、wire值和实际修订 ID 保持不变。

新增导入整理的稳定数据边界测试：原文作为 user 数据发送，系统指令按当前 bundle 选择中英文，整理请求不携带工具，不写事件或知识页；空正文返回 `field_required(content)`。Prompt 运行记录显示“保留来源原文语言”，不伪装成 `zh`/`en` 回答策略。脚本生成器跳过重复 FVM 安装，仍由项目 FVM 生成桥接和 release。

本批资源达到1102项，候选1207项/93文件；ARB/参数/ICU检查通过。Flutter全量**594 passed**（`/tmp/i18n-l6-preview-ui-all-delivery.log`），本批推文/派生/实体专项32项（`/tmp/i18n-l6-preview-domain-tests-clean.log`），Prompt专项32项（`/tmp/i18n-l6-preview-prompt-regression.log`），Rust全量**1130 passed / 20 ignored**（`/tmp/i18n-l6-preview-all-targets.log`），整理策略专项1项（`/tmp/i18n-l6-import-format-policy-rust-final.log`），9入口analyze无问题（`/tmp/i18n-l6-preview-delivery-analyze.log`），release通过（`/tmp/i18n-l6-preview-policy-release-final.log`）。桥 hash 已更新为 `-479447412` 两侧一致。L6剩余同步配对/owner、Provider/知识页冲突卡、数据/运行日志、规则/关系/情绪、搜索/代理/技能/MCP设置及Android原生验证；L7真实模型/设备仍未做。

### 2026-11-08：L6最终工程验证

L6.1–5完成。同步配对/加入/owner与名册/移交、数据集切换、Provider/知识页冲突、设置搜索/代理/DoH/技能/MCP/规则/关系/情绪、知识维护/后台日志等剩余入口接入双语；工具进度和待显示的通知在渲染时解析当前locale。英文380px设备信息标题溢出修正；显式审批仍绑定原预览token。知识重整返回稳定状态/数量回执，人工保护分支保持原操作语义；原始说明完整可查看。未知错误使用本地化回退和诊断详情，不按中文字符串猜测翻译。

资源2018项；AST覆盖含相邻字符串与插值内部表达式，剩余39处中文常量为34种逐项协议/身份/审计/诊断例外。缺译/参数/ICU及新增文案、重复/失效例外负例纳入CI，详见[覆盖清单](../development/localization-coverage.md)。

Flutter全量 **600 passed**（`/tmp/l6-ui-delivery.log`）；Rust all-targets库1130+集成2，合计 **1132 passed / 20 ignored**（`/tmp/l6-rust-delivery.log`）；应用lib/tool静态检查无问题（`/tmp/l6-analyze-delivery.log`）；release（`/tmp/l6-release-delivery.log`）、FRB双侧hash `-479447412`、ARB/AST检查通过。18项收口专项验证热切换、完整诊断、窄屏审批、Provider冲突及队列原文，最终全量包含新增稳定错误码测试。

Android原生arm64 Kotlin编译通过（`/tmp/l6-android-arm64.log`，116 tasks），命令见覆盖清单。未声明完整APK、其他ABI或真实设备通过。2026-11-08 ADB无连接设备；L7真模型/真人设备/多设备仍未验收，交付C保持未完成，Goal继续。

## L7：双语模型、设备与发布验收

- [ ] L7.1 完成下表场景的合成协议/状态回归，再跑真实模型；记录 provider/model、输入、实际 bundle/语言策略、结果与未通过项，不记录密钥。
- [ ] L7.2 桌面与 Android 实机检查语言切换/重启、输入法、编辑器菜单、英文长文本、错误及平台文案；其他平台按现有 CI/可用设备记录验证范围。
- [ ] L7.3 两设备以不同界面语言配对，验证相同知识语言、owner 修改/移交、进行中任务与旧版本不兼容行为；同时确认自定义模板未同步。
- [ ] L7.4 运行相关 Rust/Flutter 全量检查、FRB/release 一致性及资源检查；回填各阶段证据，更新提案/主路线/backlog，登记交付 C。

| 场景 | 验收关注点 |
|---|---|
| 自动回答，中英文长消息/短追问/代码/引文 | 语言稳定，继续原任务，不额外请求语言检测 |
| 界面、指令、回答语言分别设置 | 独立生效，运行固定，可见回退 |
| 目标讨论与 profile/me | 缺信息温和追问，归集明确事实，保留已有资料与审批纪律 |
| 检索、全文总结与页内编辑 | 引文/slug不变，覆盖首中尾，否定/疑问不升级编辑，草稿待审 |
| 工具调用、拒绝、失败与完成声明 | JSON/协议可解析，无越权执行或虚报保存 |
| 多轮工具/分块/provider failover 中途改语言 | 原运行固定，新运行使用新策略，完整请求预算通过 |
| 后台阅读/摘要/分析/消化及 owner 切换 | 固定知识语言，任务/缓存不混用，不支持版本暂停 |
| 历史内容与已有页面更新 | 原文可读、页面语言稳定，无整库自动翻译/重跑 |

退出证据：自动化、真模型、GUI/多设备分别有记录；失败项保持未完成，不能以 mock/Bridge 通过替代真实质量验收。

## 验证与进度维护

- 每完成一个子步骤立即勾选，追加日期、修改范围和可复核命令/日志；失败或外部限制写明事实及剩余项。
- 每阶段先跑相应存储/协议/widget/隔离真实 Bridge 测试；跨模块收口再跑 `cargo test --all-targets -- --test-threads=4`，以及在 `ui/` 执行 `fvm flutter analyze`、`fvm flutter test`。新增公共 API 时先执行 `./scripts/regen.sh`，本地化资源按项目生成配置生成，最终编译 release。
- 自动测试不读取个人数据目录；真实模型请求使用隔离场景和必要材料。原始记录不变、审批、同步、任务身份和预算属于重点回归边界。
- 本文件管理执行状态，产品评估与 Prompt 设计管理契约；每次阶段交付同步 backlog/主路线和相应实现记录，避免多份独立待办相互漂移。

## 完成记录

### 2026-11-06：L2主要界面迁移与双语行为回归

主要界面接入614个中英文资源项；语言控件位于外观，热切换保留未发送表单与对话状态。日期/复制/相对时间改为locale展示，英文审批保持原token/版本，原文与机器字段不变。Flutter全量550项通过，修改文件analyze无问题。[实现记录](../notes/implemented/architecture/2026-11-06-multilingual-runtime.md)列出覆盖范围与剩余辅助入口，L2仍部分完成。

### 2026-11-06：L1与编辑器语言接线完成

工作区根目录`language.json`保存本机显示偏好，独立于数据集数据库/快照；缺配置保留中文，显式null跟随系统。串行保存/加载与有效locale同步接通启动，Prompt偏好使用独立generation检查。应用本地化生成与框架delegates接通，AppFlowy/Intl随语言加载且避免异步旧请求覆盖。全量Flutter541项与专项验证通过，细节见[实现记录](../notes/implemented/architecture/2026-11-06-multilingual-runtime.md)。当前尚未开放语言设置控件，主要业务文案、实际AI语言运行和知识语言修改继续在L2–L7推进。

### 2026-11-06：建立实施路线

承接现有可行性复核与 Prompt P3，建立 L0–L7、三次交付、依赖与逐阶段验收范围，并在产品提案、Prompt 设计、主路线和 backlog 建立归属链接。本次为路线文档交付，L1–L7 未实施；未把之前中文首版测试计作多语言功能通过。

2026-11-07最后增量验证：中文“不要保存”守卫和中英统一场景之后，对话全组 **83 passed / 1 ignored**（`/tmp/i18n-l4-final-conversation-tests.log`），release增量编译成功且无新增警告（`/tmp/i18n-l4-final-delivery-release.log`）。完整1112项Rust与572项Flutter、额外参数化9项及83项专项分别对应上述交付点；资源812项/18组目录、桥hash、变量与文档链接/空白检查通过。当前没有进行中的验证进程；Goal保持active，下一阶段L5。

2026-11-07 L6首批最终收口：Rust全量1131 passed / 20 ignored之后，冲突错误码增量5项通过；Flutter最终581项、错误Bridge3项、同步/平台20项与11入口analyze、release通过。896项资源校验及负例、文档链接和空白检查通过。Android Gradle离线缺embedding，联网Google Maven超时，编译未通过；L6余项与L7真实模型/设备继续，路线未完成。
