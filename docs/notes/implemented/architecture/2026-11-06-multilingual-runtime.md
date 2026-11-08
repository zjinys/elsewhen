# Agent Note: 中英文运行基础与本机语言偏好

Status: implemented

## Problem

AppSettings.language只是内存占位，不能显式清空覆盖，也没有持久化与启动接线；应用缺少本地化资源，编辑器delegate和静态文案强制中文并修改Intl全局语言。数据集切换还要求本机显示选择独立于远端业务配置。

## Decision

本记录承接[中英文支持Roadmap](../../../roadmap/2026-11-06-multilingual-support-roadmap.md)，当前覆盖L1–L6工程。实际请求、后台任务和同步知识策略已固定语言；L7真实模型/设备验收尚未完成，交付C不提前关闭。

界面偏好保存在AppConfig.data_dir根目录的`language.json`，不进入数据集数据库或快照。缺文件/非法内容使用zh-Hans，显式null表示跟随系统；固定值支持zh-Hans/en，英文地区别名归一为en。写入使用唯一临时文件与替换，Unix创建权限0600；失败不覆盖原选择。数据集选择只改变数据库路径，显示偏好保持不变。

Prompt指令/回答偏好沿用已有prompt_language_preferences表，读空行得到跟随/auto，保存规范化标签并按generation原子校验；旧generation或非法组合不会改写。知识语言仅提供只读查询，当前zh-Hans，不提供缺少同步保障的修改接口。

Flutter使用ARB生成应用资源和Material/Widgets/Cupertino delegates。默认显示中文，系统模式解析支持语言并稳定回退；启动在进入主界面前加载偏好，设置操作串行提交，失败后后续操作仍可继续。有效locale通过业务API传给Rust进程，L3在创建运行时复制到bundle；worker不再读取中途变化的偏好。

AppFlowy静态l10n与delegate加载同一locale并预初始化日期符号。加载串行，已正确加载的同语言请求同步复用，避免重复异步加载及旧请求覆盖；系统变化也同步编辑器与Intl。该层只处理显示与格式，不改原文、协议、时间戳或审批语义。

### L2：主要界面与双语行为回归

中英文ARB当前有812个同名资源项。导航/最近页面/主窗入口、启动/锁屏、设置标签与外观、Provider/密钥库/提示词表单、主对话输入/搜索/消息菜单/队列/运行状态、目标树/详情/创建/编辑/讨论、页内AI面板与提议审阅详情已接入资源。语言入口放在外观设置，支持中文、English、跟随系统，使用L1持久化链路。输入内容、密钥代号/引用、知识材料、任务/工具名与JSON字段保持原值。

显示语言按BuildContext解析，独立组件宿主无应用delegate时保留中文默认；本地日期资源同步注册以供独立组件预览，生产启动仍先完成编辑器加载。日期分组、复制文本时间与相对时间使用locale规则，数量/单复数由资源处理。锁屏和队列错误保存语义状态并在build时本地化；生成中的随机短语保存索引，切语言不会生成新的运行或重新请求模型。

英文标签长度暴露设置窄导航溢出，改为在可用宽度内换行。热切换测试验证未保存Provider密钥文本不丢，锁屏错误状态不变、只改变显示语言；真实Bridge验证密钥库选择/手工输入/保存/重开双语行为。审阅测试仍绑定原修订与approval token，英文版遇到失效版本不会自动授权新版本。消息复制和日期分组由原固定中文断言改为双语参数化，继续核验全部原文与时间。

L2辅助入口也已接入：知识侧栏空态/数量/最近更新时间，材料范围/版本/Unicode选区，航向/习惯/洞见/联系人卡，知识预览、存为主题和反馈弹窗。页面标题、正文、习惯自定义字段保持原文；已有洞见消息的中文结构标记仍按原协议解析，只把外层标签本地化。习惯/存为主题/选区校验保存语义状态，热切换重新渲染语言；航向360px卡按钮允许换行，点击仍只预填指定目标草稿。日期和数值显示使用locale；存储日期与mask/字符偏移不变。习惯读取/保存委托从extension移回仓储实例以保留虚分派，生产API和revision通知语义不变。首次加载延后到首帧，避免同步失败时在initState读取本地化依赖。

L2收口验证：原材料版本/习惯状态/洞见解析与知识预览测试参数化为中英两组，保留原文、状态/版本和行为断言；新增360px航向卡及习惯弹窗热切换/校验/保存字段测试。全量Flutter **569 passed**、19入口analyze无问题，日志`/tmp/i18n-l2-complete-all.log`、`/tmp/i18n-l2-complete-analyze.log`。其他设置/业务页面及统一错误码归L6；Prompt语言变体UI和实际运行归L3，不以781个资源项声称全应用迁移完成。

### L3.1：双语模板目录与解析

九份内置模板都有zh-Hans/en资源，固定协议自然语言可翻译、工具标记/字段/枚举保持原值。目录版本为prompts-v9，内置ID包含key/locale/内容hash；追加新修订，旧修订及bundle不覆盖。标题保存稳定资源key，默认正文预览独立于激活覆盖，按请求locale解析内置。

resolver按同语言由具体到一般解析；每候选先激活再内置。显式恢复默认或不兼容修订，从当前候选起仅选内置，防止意外回落到另一个父语言自定义。繁体缺译最终只能用简体内置，不能套简体用户覆盖。回退原因使用parent_locale/missing_translation/incompatible_revision稳定标记。草稿base_default_revision_id指向同语言可用内置；parent仍要求key和locale一致。

验证：Prompt存储专项**8 passed**，覆盖18份资源幂等注册、双语独立覆盖/默认、父语言/脚本、旧版不兼容、协议标记/审批纪律及历史运行固定。日志`/tmp/i18n-l3-catalog-tests.log`。此为L3.1初版证据；后续运行接线见下，L3.2–4/6仍部分待补。

L2验证（2026-11-06）：`fvm flutter test --timeout 45s --concurrency=4`全量**550 passed**；修改的界面、工具函数和测试共37个检查入口analyze无问题；资源生成、614项key一致性与`git diff --check`通过。日志`/tmp/i18n-l2-all-final.log`、`/tmp/i18n-l2-final-analyze.log`。默认并发的一轮为549 passed/1 failed：同步worker测试在400ms内未观察到调用；单独复验与最终并发4全量均通过，测试暂停/次数要求未弱化，记录于`/tmp/i18n-l2-worker-recheck.log`。本轮无Rust公开API变更，不需要重新生成桥；L1的bridge/release结果继续有效。真实模型和GUI/多设备验收仍未进行。

### L3：运行语言、动态资源与模板设置

运行创建与语言bundle在同一事务中完成，早于worker dispatch。指令语言默认：聊天跟随已解析UI、后台跟随知识语言；明确覆盖独立保存。回答策略：本轮确定解析的要求优先，其次固定偏好，auto时使用可靠本轮语言/最近12条用户消息/固定回退。短追问、代码、引用不作为新的可靠语言信号，不发检测请求；无法确定解析的要求不记录为explicit_request。source明确为explicit_request/fixed_preference/current_message/conversation/fallback，未虚构模型检测结果。知识语言目前仍为数据集zh-Hans，只读。

每个bundle记录UI、requested instruction、回答locale/source/mode/fallback、知识locale/策略版本以及每项requested/resolved/fallback。聊天模板与工具说明使用交互指令；summary/reading/analysis/digest模板使用后台指令。产出语言独立指令注入实际请求；原文/引文/名称/代码/slug不译，普通页面更新保留原页语言，明确翻译是派生稿。运行内多轮工具复用同一bundle；重启可重读，变更设置/模板影响新运行。背景依赖hash包含后台指令/实际知识输出/策略版本，UI或回答偏好不使其失效。L5任务身份/多设备策略仍待实现。

新增只读chat.context.fixed_contract（中英各59条）：目标讨论、工作室、profile/me检查/归集、自动检索参考、知识导师、规则背景、跨对话/今日原文与认知背景、引用纪律及常驻工具指南。文本与显式变量分开；缺失/多余/重复变量报错，插入值不再解释成模板。旧catalog v1/v2缺此key时使用冻结的legacy资源，保留历史中文行为；不借用当前语言。现有目标/profile实现的无专门入口、每轮一两问、保留原文、自动找资料与审批约束同时保留。

新增只读tool.descriptions.fixed_contract：37个工具说明及95处参数description有中英版本。组装仅替换应用内置description；名字、properties/required/type/enum、默认值和ToolPolicy不改。MCP原始说明/schema保持字节内容，延迟启用返回的内置工具schema也使用所固定版本。技能目录与enable说明现有45条固定中英资源（含7份内置技能的标题/tags/简述/说明），随bundle固定；目录预算含翻译后的标题/意图/省略说明。原生schema、文本回退和enable结果使用一致语言；第三方正文/schema和用户意图保留原文。默认/旧manifest使用冻结中文兼容资源。最终答复/协议修复、部分页内/分析阅读任务自然语言尚未全部迁移，L3.2保持未完成。

提示词页可独立浏览中英模板、保存草稿、启用、历史与恢复默认；不会因浏览/翻译副本自动激活或调用模型。选择页还提供回答auto/固定中英和指令默认/固定中英，generation用于CAS；未知持久化标签保留显示。近期运行展开可查看实际语言与每项回退。编辑对话框持有并释放自己的controller，覆盖关闭动画，修复原showDialog返回即dispose造成的失效引用；请求期间禁重复保存/启用。UI累计806项中英资源。

验证：`cargo test storage::prompts --lib -- --test-threads=4` **13 passed**（含严格渲染、59条变量一致性、旧bundle读面、12份双语资源与独立启用、创建即固定/重启）；`cargo test ai::memory --lib -- --test-threads=4` **32 passed**（含双语goal/profile上下文与用户事实不再解释）；语言策略专项3项、工具说明/权限/schema/MCP原文所在工具专项50项通过。隔离真实Bridge专项**4 passed**，验证实际HTTP请求的基础模板/回答语言/英文参数说明、工具第二轮仍沿旧策略、中途改设置后新run采用新策略，以及独立模板编辑/默认与语言偏好CAS。相关日志`/tmp/i18n-l3-runtime-prompt-tests.log`、`/tmp/i18n-l3-context-tests.log`、`/tmp/i18n-l3-language-policy-tests.log`、`/tmp/i18n-l3-tool-localization-tests.log`、`/tmp/i18n-l3-runtime-ui-tests.log`；6入口analyze无问题，`/tmp/i18n-l3-final-ui-analyze.log`。这些是合成provider协议证据，不是L7真实模型质量验收。

预算全量暴露旧大结果折半停在2001–3999字符而无法继续收敛的问题。已截断的大结果现在可以收敛到原定2000字符下限；未截断的小回执仍完整保留，披露后缀不叠加，不放宽请求窗口或输出预留。原接近64K的204页目录HTTP回归通过，预算29项通过，见`/tmp/i18n-l3-budget-recovery-test.log`和`/tmp/i18n-l3-budget-tests.log`。

公开运行详情字段已执行`./scripts/regen.sh`并重建release，日志`/tmp/i18n-l3-runtime-regen.log`；后续资源变化执行release增量编译`/tmp/i18n-l3-final-release.log`。当前桥hash两侧仍为-1499826736。全量Flutter在模板语言/独立编辑交付点为**571 passed**，日志`/tmp/i18n-l3-catalog-all.log`；新增动态资源与多轮fixture之后的全量收口继续记录，不把此前结果当作之后改动的最终全量。

后续L3.3证据：`cargo test ai::tool --lib -- --test-threads=4` **50 passed**，覆盖37工具/95参数说明不改schema/策略、完整技能目录与所有7技能启用、MCP描述/schema原文保留；`cargo test protocol_tests --lib -- --test-threads=4` **17 passed**。日志`/tmp/i18n-l3-all-tool-tests.log`、`/tmp/i18n-l3-final-protocol-tests.log`。运行策略/上下文/工具说明交付点Rust全量`cargo test --lib -- --test-threads=4` **1084 passed / 20 ignored**，日志`/tmp/i18n-l3-final-rust-all.log`；此后语言回退字段与工具metadata的增量专项另列，不把这份全量作为之后改动的最终全量。最初沙箱运行25个回环端口测试遭Operation not permitted，按权限重跑后确有预算边界失败，修复并回归，而非跳过失败用例。

最新界面收口：`fvm flutter test --timeout 45s --concurrency=4` **572 passed**，`fvm flutter analyze`检查6个相关入口无问题；实际双轮工具HTTP/英文enable目录/旧策略固定与新run设置，以及真实模板编辑/启用/默认和回答/指令偏好保存，随后3项专项再次通过。日志`/tmp/i18n-l3-final-ui-all.log`、`/tmp/i18n-l3-final-ui-analyze.log`、`/tmp/i18n-l3-final-runtime-bridge-tests.log`。完整Prompt/技能/工具资源在release中，资源key/变量/链接及`git diff --check`通过。当前知识语言修改仍未开放，真实模型/GUI/多设备验收均保持未完成。

最新Rust收口：`cargo test --all-targets -- --test-threads=4`通过，库1085项、集成2项（共1087 passed / 20 ignored），其余example target无测试；日志`/tmp/i18n-l3-final-all-targets.log`。最新release与生成桥接一致，Flutter全量572项、相关6入口analyze通过。

### 2026-11-07：运行追加指令与英文对话行为

新增`chat.execution.fixed_contract`（55项）、`task.processing.fixed_contract`（16项）和`chat.reply.fixed_contract`（25项），目录共15组、30份双语内置修订。前两组分别随交互/后台指令语言，reply组随实际回答语言，快照中记录各自requested/resolved；事实变量采用严格单次渲染，不重新解释占位符或密钥引用。协议/最终答复/空回复/预告/完成声明修复、待审事实清单、短追问、review状态、页内选区/全页分块/相邻原文、后台阅读/分析/规则判定/摘要限制/消化包装已接入。摘要与翻篇的原body保持原文，显示/注入说明按资源渲染。

临时内容对话使用`preview`归属和`content_chat:UUID`身份，保留现有数据库owner_kind约束，无新增迁移；根据最后一条用户消息、历史用户语言和本机策略生成固定bundle，不写临时消息/事件，只保留运行资源证据及原有用量。原生协议回落文本后保留已执行结果与引用绑定；中文历史前缀继续可读，英文工具结果及历史包装可解析。

英文信号模块只辅助理解与事实核验，不授权写操作。完成声明按句核验，否定/示例/引文/代码不作为成功；写/页修订请求按动词和否定/询问守卫判断；continue/any updates等短追问回到原请求。应用失败/待审回复使用回答语言，用户事实与草稿原文保留。旧失败提示曾要求再说“保存”，已改为审阅卡重试，并保留原要求/未生效状态回归断言。

验证：Rust完整交付点1093 passed / 20 ignored（`/tmp/i18n-l3-execution-processing-all.log`），之后英文14项通过（`/tmp/i18n-l4-english-behavior.log`）；双Provider实际HTTP故障切换1项通过（`/tmp/i18n-l3-language-failover.log`），设置从en变为zh-Hans后备用Provider仍收到原system和工具定义。移除英文否定守卫后`Save nothing`用例确实失败，原源码恢复（`/tmp/i18n-l4-negation-mutation.log`）。专项覆盖页内完整覆盖/原文不变/只产待审、临时对话HTTP与无消息落库、阅读摘录校验/英文修复/重启同bundle、分析原始事件不变与知识输出仍中文。最新完整检查/release/Flutter结果在收口后另追加，早期全量不替代新改动。

范围仍有剩余：知识整理/跨资料整合等动态指令、英文搜索/直查与全部场景、知识策略同步/身份和缓存、其余应用文案及真实模型/设备验收继续按L3–L7推进。

2026-11-07界面与运行库收口：release构建成功（`/tmp/i18n-l4-current-release.log`），FRB两侧hash仍为`-1499826736`；新增资源共809项，中英key/变量与文档链接检查通过。Flutter全量**572 passed**（`/tmp/i18n-l4-current-ui-all.log`），3入口analyze无问题（`/tmp/i18n-l4-current-ui-analyze.log`）；新版release实际模板设置与多轮语言Bridge **3 passed**（`/tmp/i18n-l4-final-runtime-bridge.log`）。额外确认英文拒绝词只按整句匹配，`notes/noted/notice/nothing`不会因以`no`开头被当作拒绝，专项1项通过（`/tmp/i18n-l4-acknowledgements.log`）。

最终Rust交付点：`cargo test --all-targets -- --test-threads=4`库1102项、集成2项通过（**1104 passed / 20 ignored**），其余example目标无测试，日志`/tmp/i18n-l4-current-all-targets.log`。之后唯一生产增量为英文拒绝词只按整句匹配，新增1项专项通过；无需因该局部守卫重复全库测试，release随后增量重建。L3.6和L4.2/3/5已勾选，L3.2/4、L4.1/4与L5–L7保持未完成。

### 2026-11-07追加：知识处理与英文搜索/直查

目录更新为`prompts-v8`，17组定义/34份内置修订。新增`knowledge.processing.fixed_contract`26项与`chat.lookup.fixed_contract`11项；前者按后台指令语言，后者按交互指令语言固定。知识提炼/修订/修复、跨资料整合/修复、洞察、主题合并拆分、重新整理、每日回顾、联系人关系提取与习惯建议均建立scope并附知识输出指令。后台提炼/整合/洞察账本记录实际bundle。来源JSON、原文、逐字引文、页面身份和人工修订basis继续由原校验执行，语言资源不改写这些值。旧v1–v6的知识处理、旧v1–v7的直查分别使用冻结兼容包，不从当前locale借资源。

英文知识提炼第一次响应非法时，经另一连接把偏好改为中文，第二次仍为原英文指令/中文知识产出，来源不变且只生成pending建议。知识回归72项、扩展相关95项通过（2 ignored），资源17项与新知识专项1项通过，日志`/tmp/i18n-l3-knowledge-tests.log`、`/tmp/i18n-l3-knowledge-complete-tests.log`、`/tmp/i18n-l3-knowledge-prompt-tests.log`、`/tmp/i18n-l3-knowledge-language-tests.log`。追加前一完整交付点Rust库1105+集成2通过（1107 passed / 20 ignored，`/tmp/i18n-l3-knowledge-final-all-targets.log`）；之后搜索/直查/习惯建议与更完整资源检查不冒用该全量结果。

英文搜索区分直接联网要求与时效词，否定/引文/代码和newsletter不误命中；timely仍要求模型先区分本地记录任务。直查支持英文规则/用量/目标问法，双语包装保留原始规则，并检索中文目标与英文goal页面。搜索实现位于web_search模块，复用语言模块的纯文本过滤，未建立web_search到conversation的新依赖。对应专项均通过。

该交付点剩余L3包括补充材料与其余上下文包装盘点，以及任务/缓存策略在L5中的统一；L4仍需完整中英文参数化场景与全部边界盘点；界面资源当前811项。

### 2026-11-07追加：补充材料与证据包装

新增`chat.supplement.fixed_contract`11项，目录`prompts-v9`、18组定义/36份内置修订；旧v1–v8缺组时读取冻结兼容包。用户补充、@wiki引用摘要展开/缺页/原话/截断说明、建议反馈、上游知识含人工修订、完整阅读结果与空证据提示采用严格变量渲染。补充context现在返回Result，资源错误在组装阶段明确失败；队列/撤回/继承/预算预检/仅安全边界注入保持原语义。`知识选材：`及其JSON作为既有证据协议前缀保留，由bind.rs解析。

原补充专项14 passed / 1 ignored（`/tmp/i18n-l3-supplement-tests.log`），新增英文补充快照/原话/拒绝授权/缺页/截断1项通过（`/tmp/i18n-l3-english-supplement-tests.log`）；上下文迁移后的知识95 passed / 2 ignored、英文19项通过（`/tmp/i18n-l3-context-final-knowledge.log`、`/tmp/i18n-l3-context-final-english.log`）。中文全量摘要意图专项保留，新增英文远距离否定/示例/代码守卫，日志`/tmp/i18n-l4-summary-guards.log`及后续页内专项。新增补充资源的变量一致性与旧v8兼容回归一起纳入最终检查。

前一交付点完整Rust库1108+集成2通过（1110 passed / 20 ignored，`/tmp/i18n-l4-expanded-all-targets.log`），release与3项实际Bridge/3入口analyze通过（`/tmp/i18n-l4-expanded-release.log`、`/tmp/i18n-l4-expanded-bridge.log`、`/tmp/i18n-l4-expanded-analyze.log`）。本次补充/context增量的最终证据随后追加；不把上一份全量当作新增代码的最终结果。

阶段归属：L3.2/4及L4.1已勾选；L3工程收口，L4.4统一场景仍待完成。JSON证据信封、工具/规则标记、来源原文、自定义模板和第三方说明保留原值；应用生成的少量回复/错误展示继续按L6迁移，知识策略跨设备与缓存任务身份归L5。这里的工程完成不包含真实模型质量或设备验收。

### 2026-11-07交付A：L1–L4工程完成

统一参数化场景9项通过（`/tmp/i18n-l4-bilingual-final-scenarios.log`），涵盖问答、检索、目标/profile引导及明确事实写入profile/me、全文分块与原文覆盖、修订待审、拒绝保留草稿/无执行回执、继续/进展、工具失败和双HTTP Provider故障切换。统一场景发现旧中文“不要保存”未被写意图否定词表覆盖，导致多一次修复请求；加入守卫后通过，不靠放宽期望次数或隐藏失败。失败回执断言保留真实工具错误；工具实际错误仍中文，后续按L6稳定错误契约迁移。

完整最终检查：Rust all-targets库1110+集成2（**1112 passed / 20 ignored**），`/tmp/i18n-l4-stage-final-all-targets.log`；Flutter最新release全量**572 passed**，`/tmp/i18n-l4-stage-final-ui-all.log`；3入口analyze无问题，`/tmp/i18n-l4-stage-final-analyze.log`；release构建通过，`/tmp/i18n-l4-stage-final-release.log`。随后中文否定守卫增量由双语9项与对话全组回归验证，release再编译（`/tmp/i18n-l4-final-conversation-tests.log`、`/tmp/i18n-l4-final-delivery-release.log`）。FRB两侧hash仍为-1499826736；812项中英界面资源及8组JSON文本资源key/变量检查通过，文档链接与git diff --check通过。

L1–L4工程完成后，知识语言仍中文且只读。L5必须先交付知识策略同步、旧应用能力/owner门禁和任务缓存身份；L6剩余页面/错误/平台提示、L7真实模型与设备验收仍未完成。Goal保持active，不以本阶段完成代替整个roadmap。

### 2026-11-07交付B：知识语言同步与运行身份

迁移74把知识语言提升为数据集owner签名策略（generation/roster revision/dataset绑定），增量由既有head/CAS与待审冲突裁决；快照v10包含签名策略及小型产物语言metadata。完整Prompt、激活、bundle留本机。能力声明只接受已认证origin device/stream，不能由快照或relay blob能力证明；roster变化必须重声明，缺声明旧端不能接管非默认产出。不支持策略通过连接注册的SQLite函数在后台领取视图失败关闭，降级到旧可执行文件也不能静默生成。

分析和消化领取即固定bundle，失败重试与同机重启复用；显式重新分析增加逻辑代次。阅读已开始的snapshot继续旧bundle，新来源/明确新任务各自创建generation策略。普通维护调度按来源成功账本去重，不因设置改动付费重跑整库；每tick仍有界。摘要树策略附带知识输出、版本/generation及相关资源/渲染hash，合并验证同策略，旧PageMark继续读取。分析、阅读、摘要、消化、来源维护记录本机manifest关联；分析与wiki修订的必要语言metadata随业务产物同步，接管设备以同步来源和策略重新领取而不冒充旧本机bundle。

模型→提示词现可修改知识语言，非owner只读，缺成员能力与不支持策略有中英文提示。保存使用generation CAS，与界面/回答/指令偏好独立；保留已有页面更新语言及来源原文。新增5项资源，共817项。真实Bridge验证保存/重载/陈旧版本拒绝和偏好隔离。

证据：Rust all-targets库1128+集成2（1130 passed / 20 ignored，`/tmp/i18n-l5-delivery-all-targets.log`），同步全组189项、owner/冲突专项7项及带分析metadata的签名首次快照通过；Flutter全量572项（`/tmp/i18n-l5-ui-all-tests.log`）、真实Bridge3项（`/tmp/i18n-l5-ui-bridge-final-tests.log`）、3入口analyze无问题。FRB hash1894755173两侧一致，`cargo build --release`通过（`/tmp/i18n-l5-final-release.log`）。生成器曾因全局旧SDK与过期FVM链接停滞，脚本改为FVM子命令；C编译器包装曾递归，已提前固定真实路径后构建完成。失败日志保留，不将失败运行记为通过。

本阶段完成L5工程。L6全应用文案/错误/平台提示、L7真实模型/GUI/多设备验收继续，Goal保持未完成。

### 2026-11-07 L6首批：错误契约、待办/习惯与平台

Rust错误通过`UserError`的版本化JSON信封携带稳定code和有限参数，anyhow链保留诊断；Dart严格识别版本/结构/类型，未知码回到本地化通用文案。域参数仅按已知字段/资源映射，原始路径/SQL/第三方说明不进入普通提示。详情需用户主动打开，可查看和复制原诊断。首批待办/提醒、语言设置CAS/owner/能力与对象冲突接通；其他错误继续逐域迁移。

待办列表、空态、编辑/菜单/优先级/截止日期双语，英文窄屏优先级Wrap避免溢出；原始标题/补充不译，热切换保留未发送文字。习惯集中列表/状态/周计划与操作反馈双语，保留用户问题和按钮。托盘菜单排队热更新，动作key/show/quit不变；Android保活通知标题/频道/内容使用ARB，语言更新只刷新已存在的notification，不重新start、不释放持有者、不重获wake lock。旧API的显式自定义通知文字仍保留原值。

同步对象冲突双语化，并展示知识语言策略冲突；原updated_at绑定、原文与显式裁决纪律不变。Controller保留诊断，不自行拼接中文；视图选择本地化错误。Provider/知识页冲突卡与配对设置继续迁移，共用外壳的loading/徽标/刷新已双语。

资源896项；脚本`python3 scripts/check-localization.py`与CI检查key/参数/type、ICU选择/复数other，允许中文直接插值而英文使用复数；负例拒绝缺other、重复selector、参数失配。`--inventory`输出Dart字符串候选，已迁移本轮六类入口的候选为0；剩余1520候选/109文件按[覆盖清单](../../../development/localization-coverage.md)继续复核，分类例外不当作已翻译。

验证：Rust完整1131 passed / 20 ignored（`/tmp/i18n-l6-domain-all-targets.log`），Flutter完整580项（`/tmp/i18n-l6-current-ui-all-tests.log`），待办/习惯/错误18项、真实Bridge3项、托盘/通知7项专项及release通过。最新对象冲突增量Rust5项通过；同步冲突/共用外壳/托盘/通知增量20项通过（`/tmp/i18n-l6-sync-ui-tests-final.log`），release再编译通过（`/tmp/i18n-l6-sync-release-final.log`）；最终Flutter全量581项通过（`/tmp/i18n-l6-delivery-ui-all-tests.log`）、11入口analyze无问题（`/tmp/i18n-l6-delivery-analyze.log`），FRB两侧hash1894755173。Android离线缺engine embedding，联网Google Maven连接超时，编译未到应用Kotlin；保留未通过事实（`/tmp/i18n-l6-android-kotlin-online.log`）。无真机/真实模型结论。

### 2026-11-07 L6知识库：导入、阅读与编辑

迁移列表分页/筛选/阅读状态、批量审阅、负知识与原文版本确认、网址/文本表单和网页预览；页签关闭/标签/正文编辑与冲突、复制/路径/关系/沉浸操作跟随locale。原始页面/标签/标题、关系wire和修订审计原因保持原值。状态保留原始异常或验证枚举，在build时选文案，避免切语言后错误停在旧语言。行内错误复用主动查看诊断组件。

编辑冲突以版本化`wiki_page_changed`替代中文字符串匹配；空正文、素材只读、长度限制和缺页也使用稳定code。既有updated_at乐观锁、取消/重载/覆盖、导入previousSnapshotId确认纪律均保留。批量结果保存当次标题与成功状态以本地化展示，失败诊断留在详情。导入两个薄委托放回RustBridgeRepository类中以保留虚分派，假仓库可验证明确写入，生产仍委托同一API。

增加122项资源至1018项；清单1327候选/98文件。已迁移列表/阅读/导入确认与表单为0候选，编辑2项为修订审计，关系4项为既有wire映射。英文筛选和保存按钮在窄屏测试暴露溢出，改为有界布局；验证保留内容与行为断言。Flutter全量588项、知识库专项35项、Rust保存专项2项、release通过，日志见路线。推文专用整理、实体/工作项、派生/历史/维护和其他同步/设置继续，不把L6或Goal标为完成。

最终验证：Rust all-targets库1129+集成2（**1131 passed / 20 ignored**，`/tmp/i18n-l6-knowledge-all-targets.log`），Flutter全量**588 passed**（`/tmp/i18n-l6-knowledge-ui-all-tests.log`），14入口analyze无问题（`/tmp/i18n-l6-knowledge-analyze-clean.log`），release、1018项资源检查、文档链接/状态头和`git diff --check`通过。静态检查曾发现测试无用import，移除后重跑通过；失败记录保留，未减少功能断言。

### 2026-11-07 L6第三批：推文、派生与实体

2026-11-07 L6知识库第三批：推文预览的 AI Markdown 整理改为后端双语 Prompt 资源，整理任务记录 `preserve_source` 策略，原文语言不被聊天回答语言设置覆盖；整理仅生成预览，仍需用户点击保存才写入来源和派生产物。推文讨论、原文复制、保存/失败、窄屏输入与热切换接入双语。派生版本列表、正文按需读取、模型/流程/生成指令、采用/撤销采用、比较/复制/分页和错误详情接入双语；实体合并/撤销、结构化事实、来源事件、关联待办、相关指导及反馈错误接入双语，窄屏合并操作改为换行布局。原始事实、来源、生成指令、wire值和实际修订 ID 保持不变。

新增导入整理的稳定数据边界测试：原文作为 user 数据发送，系统指令按当前 bundle 选择中英文，整理请求不携带工具，不写事件或知识页；空正文返回 `field_required(content)`。Prompt 运行记录显示“保留来源原文语言”，不伪装成 `zh`/`en` 回答策略。脚本生成器跳过重复 FVM 安装，仍由项目 FVM 生成桥接和 release。

本批资源达到1102项，候选1207项/93文件；ARB/参数/ICU检查通过。Flutter全量**594 passed**（`/tmp/i18n-l6-preview-ui-all-delivery.log`），本批推文/派生/实体专项32项（`/tmp/i18n-l6-preview-domain-tests-clean.log`），Prompt专项32项（`/tmp/i18n-l6-preview-prompt-regression.log`），Rust全量**1130 passed / 20 ignored**（`/tmp/i18n-l6-preview-all-targets.log`），整理策略专项1项（`/tmp/i18n-l6-import-format-policy-rust-final.log`），9入口analyze无问题（`/tmp/i18n-l6-preview-delivery-analyze.log`），release通过（`/tmp/i18n-l6-preview-policy-release-final.log`）。桥 hash 已更新为 `-479447412` 两侧一致。L6剩余同步配对/owner、Provider/知识页冲突卡、数据/运行日志、规则/关系/情绪、搜索/代理/技能/MCP设置及Android原生验证；L7真实模型/设备仍未做。

### L6最终：展示语义、处理回执及持续覆盖检查

2026-11-08，2018项中英资源覆盖其余同步/设置/维护入口。同步控制器存储版本化资源身份及参数，渲染时按当前locale解析，支持嵌套时长；未知错误以诊断信封保留全文，显示本地化回退，并提供查看/复制。页内/目标恢复失败、密钥表单和旧对话错误同样保留诊断。工具结果包装按当前locale重新渲染，原始内容不重写。

Rust页面重整在实际执行分支返回`elsewhen-result-v1`回执（version/code/count/detail），不通过旧中文提示推断状态；人工编辑保护与证据计数有语义回归。来源变化与审阅依据变化返回稳定code，原有CAS/正文保护断言继续保留。

AST门禁检查手写应用Dart（含l10n辅助代码）、相邻字符串/插值内表达式；精确例外清单按路径/文本/次数绑定39处协议/审计/身份/诊断。新增入口和失效例外均失败，负例进入CI。此前正则扫描遗漏的插值中文已纳入。英文复数显式插入数量，避免生成器按字面显示`#`。这层不翻译用户内容、第三方说明或历史bundle。

Flutter600项，Rust1132 passed / 20 ignored，lib/tool analyze无问题，release/FRB一致性及2018项ARB/39例外AST检查通过；Android原生arm64 Kotlin编译通过。[覆盖记录](../../../development/localization-coverage.md)保留命令与日志，真模型/真设备仍归L7。

## Alternatives considered

- 将界面语言写入所选数据集：切换数据集会影响显示选择；使用安装根目录配置，AI本机策略仍沿用其现有数据集表。
- 仅调用copyWith：不能持久化或同步编辑器/后端，也不能表达恢复跟随系统；使用显式空值语义与串行异步操作。
- 继续强制编辑器中文：会覆盖英文日期和菜单，改为共享当前生效locale。
- 立即开放知识语言修改：同步和任务身份尚未完成，保持只读，等待L5。

## Consequences

语言切换具备持久化和启动基础，主要界面已迁移，旧安装默认行为保留。代价是增加一个本机配置文件和明确的异步加载顺序，并持续维护两种语言资源；模板运行/行为/知识同步和其他业务界面继续按L3–L7实现。

验证（2026-11-06）：Rust`cargo test language --lib -- --test-threads=4` 3项通过，含本机落盘/非法配置、Prompt规范化与CAS、真实数据集选择往返；Prompt存储专项5项通过（与语言专项有重叠）。Flutter语言/编辑器/真实Bridge/设置专项28项、全量541项通过，修改文件analyze无问题；桥接再生成和release编译通过，桥hash为-1499826736。Bridge测试全部使用隔离临时目录，无个人数据依赖。日志见`/tmp/i18n-l1-rust.log`、`/tmp/i18n-l1-prompts.log`、`/tmp/i18n-l1-flutter-tests.log`、`/tmp/i18n-l1-flutter-all.log`、`/tmp/i18n-l1-analyze.log`和`/tmp/i18n-l1-regen.log`。真实模型/GUI/多设备语言验收尚未进行。

2026-11-07最后增量验证：中文“不要保存”守卫和中英统一场景之后，对话全组 **83 passed / 1 ignored**（`/tmp/i18n-l4-final-conversation-tests.log`），release增量编译成功且无新增警告（`/tmp/i18n-l4-final-delivery-release.log`）。完整1112项Rust与572项Flutter、额外参数化9项及83项专项分别对应上述交付点；资源812项/18组目录、桥hash、变量与文档链接/空白检查通过。当前没有进行中的验证进程；Goal保持active，下一阶段L5。
