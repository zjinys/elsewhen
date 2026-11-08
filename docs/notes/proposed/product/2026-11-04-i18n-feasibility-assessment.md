# Agent Note: 多语言支持可行性评估（对话块为重点）

2026-11-08 L6工程完成：2018项中英资源、同步/设置/维护剩余入口、稳定错误与原始详情、平台文案及AST/ARB防漏门禁完成；Flutter600项、Rust1132 passed / 20 ignored、lib/tool analyze、release通过，FRB hash -479447412。Android原生arm64编译通过；ADB无设备，L7真模型/设备/多设备验收保持未完成。最新覆盖与命令见多语言路线及覆盖清单，以下为历史阶段记录。

Status: proposed

2026-11-07 L5工程交付：迁移74、签名数据集知识语言/增量与快照v10、应用能力和接管门禁、任务与缓存策略隔离、产物语言metadata及设置修改接通。Rust all-targets库1128+集成2（1130 passed / 20 ignored）、Flutter全量572项、真实Bridge3项、analyze和release通过；语言切换不整库重跑，旧工作继续原bundle。L6全应用文案/错误与L7真实模型和设备仍未完成，Goal继续。

2026-11-07 L6首批推进：待办/习惯提醒、首批稳定错误码与详情、托盘/Android通知标签及对象同步冲突接通，资源896项，覆盖/ICU检查入CI。Rust完整1131 passed / 20 ignored、Flutter最终581项、同步冲突/托盘/通知增量20项、11入口analyze及release通过。Android Kotlin编译受embedding依赖与Google Maven超时阻塞，未算通过。L6其余知识库/同步/设置与L7真实验收保持未完成。

2026-11-07 L6知识库增量：分页列表/筛选/阅读状态、导入表单/负知识与新版本确认/网页预览、页签和正文编辑/标签/复制/沉浸操作接入双语。知识页编辑错误改为稳定code，保留乐观锁与显式覆盖；英文窄屏溢出已修正，切语言保留搜索/标签/未保存正文。资源1018项，候选1327项/98文件；Rust全量1131 passed / 20 ignored、Flutter全量588项、知识库专项35项、14入口analyze及release通过。推文专用预览/整理、实体/派生/维护、其余同步与设置继续，L6和L7仍未完成。


- 首次评估：2026-11-04；当前复核：2026-11-06。
- 范围：Flutter UI、Rust AI 与错误回流、多设备知识语言。
- 关联：[版本化 Prompt 与多语言策略](../architecture/2026-11-05-versioned-prompt-management.md)，中文首版已实施，P3 待做。
- 实施归属：2026-11-06 用户确认建立[中英文支持 Roadmap](../../../roadmap/2026-11-06-multilingual-support-roadmap.md)，L0复核完成，L1–L7待实施；阶段任务与证据统一记录于该路线。
- 实施进度：2026-11-06 L1与L2.3已交付，本机偏好/ARB/框架/启动、Prompt偏好CAS、编辑器/日期接线通过验证；Flutter全量541项、Rust语言3项/Prompt5项通过，见[记录](../../implemented/architecture/2026-11-06-multilingual-runtime.md)。下文代码盘点保留复核时口径，最新状态以路线为准，L2其余及L3–L7继续实施。
- 同日后续：L2主要界面已接614项中英资源，外观语言选择及热切换保留表单接通；全量Flutter550项、修改文件analyze通过。材料范围/辅助卡片/部分格式与错误仍待补，AI实际语言运行归L3，详见同一记录。

- 同日后续收口：L2辅助入口补齐，累计781项资源，Flutter全量569项、19入口analyze通过；L3.1九份双语模板目录/回退接通，Rust专项8项。实际语言策略/动态指令继续L3.2–6，L4–L7保持未完成。

- 后续运行接线：L3.5模板变体/独立编辑/启用/默认、回答与指令偏好及运行语言查看完成；动态上下文59条、工具37/参数95接入固定bundle，多轮HTTP/设置变更隔离回归通过，详见实现记录。L3其余动态指令及L4–L7未完成。

- 最新收口：Rust all-targets 1087 passed / 20 ignored，Flutter全量572项与6入口analyze通过；L3剩余动态指令和L4–L7继续，不以自动化替代真实模型/设备验收。

2026-11-07 L6知识库第三批：推文预览的 AI Markdown 整理改为后端双语 Prompt 资源，整理任务记录 `preserve_source` 策略，原文语言不被聊天回答语言设置覆盖；整理仅生成预览，仍需用户点击保存才写入来源和派生产物。推文讨论、原文复制、保存/失败、窄屏输入与热切换接入双语。派生版本列表、正文按需读取、模型/流程/生成指令、采用/撤销采用、比较/复制/分页和错误详情接入双语；实体合并/撤销、结构化事实、来源事件、关联待办、相关指导及反馈错误接入双语，窄屏合并操作改为换行布局。原始事实、来源、生成指令、wire值和实际修订 ID 保持不变。

新增导入整理的稳定数据边界测试：原文作为 user 数据发送，系统指令按当前 bundle 选择中英文，整理请求不携带工具，不写事件或知识页；空正文返回 `field_required(content)`。Prompt 运行记录显示“保留来源原文语言”，不伪装成 `zh`/`en` 回答策略。脚本生成器跳过重复 FVM 安装，仍由项目 FVM 生成桥接和 release。

本批资源达到1102项，候选1207项/93文件；ARB/参数/ICU检查通过。Flutter全量**594 passed**（`/tmp/i18n-l6-preview-ui-all-delivery.log`），本批推文/派生/实体专项32项（`/tmp/i18n-l6-preview-domain-tests-clean.log`），Prompt专项32项（`/tmp/i18n-l6-preview-prompt-regression.log`），Rust全量**1130 passed / 20 ignored**（`/tmp/i18n-l6-preview-all-targets.log`），整理策略专项1项（`/tmp/i18n-l6-import-format-policy-rust-final.log`），9入口analyze无问题（`/tmp/i18n-l6-preview-delivery-analyze.log`），release通过（`/tmp/i18n-l6-preview-policy-release-final.log`）。桥 hash 已更新为 `-479447412` 两侧一致。L6剩余同步配对/owner、Provider/知识页冲突卡、数据/运行日志、规则/关系/情绪、搜索/代理/技能/MCP设置及Android原生验证；L7真实模型/设备仍未做。

## Problem

用户已明确需要多语言，中文 Prompt 入库也已完成。旧评估中的“没有任何基础”和“先决定是否需要”已不适合作为当前排期依据，需对照代码明确可实施范围。

多语言最新验证：Flutter全量572项通过，相关6入口analyze无问题；最新release实际双轮工具语言与设置桥接3项通过。剩余动态指令/英文行为、知识同步及全应用/真实模型设备验收继续按[路线](../../../roadmap/2026-11-06-multilingual-support-roadmap.md)推进。

## Proposal

2026-11-06 结论：**现在可以进入实施，先支持简体中文与英文。** 复用已有 Prompt 修订、语言回退、运行固定与缓存依赖基础；没有发现必须等待其他 backlog 才能启动的结构性依赖。完整应用本地化仍有明确工程量，不能仅添加译文文件就宣称完成。

### 当前代码依据

| 部分 | 已有基础与剩余工作 | 依据 |
|---|---|---|
| Prompt | 按 locale 的修订/启用、父语言回退和显式恢复默认已存在；内置资源注册与解析仍只有中文，需扩展为真正的多语言目录 | [存储与解析](../../../../src/storage/prompts.rs) |
| 运行与缓存 | bundle、跨 tick 阅读固定及管线依赖已接通；捕获语言、bundle 元数据、依赖 hash 和阅读策略仍硬编码 zh-Hans，需一并贯通 | [运行固定](../../../../src/storage/prompts.rs) |
| 语言设置 | 两张语言策略表已建；prompt_language_preferences 尚无设置/运行接线，dataset_content_language 仅初始化中文。Flutter 的 language/updateLanguage 只是内存占位，提示词编辑页固定中文 | [迁移73](../../../../migrations/73-prompt-management/up.sql)、[设置状态](../../../../ui/lib/providers/settings_provider.dart)、[提示词设置](../../../../ui/lib/widgets/prompt_settings.dart) |
| UI 与编辑器 | 主应用无 ARB 和 supportedLocales 接线；AppFlowy 已有译文，但 delegate 与静态 l10n 固定中文，并影响 Intl.defaultLocale | [应用入口](../../../../ui/lib/main.dart)、[编辑器本地化](../../../../ui/lib/utils/editor_localization.dart) |
| 动态指令与工具 | 核心九份模板已抽取；目标/profile 上下文、分析任务契约、工具描述/参数说明、修复指令仍有中文硬编码 | [动态上下文](../../../../src/ai/memory/system_context.rs)、[工具注册](../../../../src/ai/tool/mod.rs)、[分析](../../../../src/api/analysis_queue.rs) |
| 对话行为 | 确认只有少量英文，拒绝、进度追问、保存声明与未来承诺检查主要使用中文词表；页内编辑的英文触发词也需补否定语境回归 | [确认/拒绝](../../../../src/ai/conversation/content_chat.rs)、[连续性](../../../../src/ai/conversation/continuity.rs)、[写入声明](../../../../src/ai/conversation/claim_detection.rs)、[页面意图](../../../../src/ai/page_tasks.rs) |
| 同步与预算 | 知识语言当前被排除在同步快照之外，不能直接开放本机修改。预算已使用 BPE 或 ASCII/CJK/其他字符分层估算，无须因多语言重写，但要验证双语完整请求 | [同步边界](../../../../src/storage/sync/snapshot/records.rs)、[预算](../../../../src/ai/budget.rs) |

### 语言规则与实施顺序

四类语言语义沿用专项设计：界面语言为本机设置；AI 回答默认跟随用户交流语言、也可固定；知识产出语言属于数据集、由 owner 修改并同步；指令语言是提示词页的高级选项，交互默认跟随界面、后台默认跟随知识语言。缺译回退不能改变目标回答/产出语言，引用和工具结果不应干扰自动语言判断。

常用设置无需四个同级下拉框：界面语言放外观，回答语言放模型，知识语言归数据集，指令语言留在提示词编辑/预览处。升级时保留现有中文体验，提供中文、英文、跟随系统选项；存量用户内容不自动翻译。

1. **语言基础与主界面**：接持久化、ARB/delegates、编辑器静态 l10n、日期/数量格式；先迁导航、设置、对话、目标讨论与状态/审阅入口，同时盘点托盘和桌面入口。测试以稳定 key 和参数化文案验证双语。
2. **AI 中英闭环**：扩展内置资源注册/resolver，补英文模板、动态上下文、工具说明、修复与语言指令，贯通偏好到每次运行的固定 bundle；补英文拒绝、继续/追问、保存声明及否定/引用语境回归。此阶段知识产出仍固定中文。
3. **知识语言与同步**：补 owner 写入、版本化同步、初始快照与兼容检查，再开放产出语言修改。输出语言/策略版本进入 bundle、管线指纹和新任务身份；新策略不复用不兼容阅读缓存，旧作业按原策略完成，显式重做建新任务；没有原 bundle 的新 owner 重新领取工作。已有页面不自动翻译。
4. **全应用覆盖与验收**：迁移其他业务页面和常见错误；Rust 以稳定错误码/参数跨桥传递，Flutter 本地化并保留诊断详情，不按中文字符串替换 e.toString()。完成文案清单与双语真模型/多设备验收，才称为完整应用中英支持。

阶段1/2可先交付明确范围的主界面与对话支持，无须等待所有历史 Rust 错误改完；阶段3的同步要求是开放知识语言修改的前置条件。

## Alternatives considered

- **只翻译 UI 或只添加英文 Prompt 文件**：无法自动接通语言选择、缓存与对话行为，不作为完整中英闭环。
- **翻译协议标记**：不采用。[工具调用]、[规则提议]、[规则本质]、工具名、JSON 字段、枚举、slug 保持原值；英文指令解释原标记即可，无须更换协议。
- **继续等待其他 backlog 完成**：未发现这样的结构依赖，Prompt 基础已经可以复用。
- **一次翻译全部存量内容、开放任意语言**：不采用。先用中英验证链路，保留用户内容原文，更多语言沿资源/策略扩展。

## Acceptance criteria

1. 中文/英文/跟随系统可持久化切换；主界面、编辑器与日期/数量格式一致，用户数据原文保留。
2. 回答可自动或固定语言；目标/profile 引导、工具调用和审阅正常，实际模板回退可查。
3. 英文拒绝与否定编辑不产生意外动作，继续/追问承接原任务，“已保存”依真实执行结果核验；自然语言确认不绕过现有写入审批。
4. 改设置不改变已固定运行；新运行使用新策略，阅读/摘要/分析/消化缓存按相关语言与指令隔离。
5. 开放知识语言前完成快照、增量、owner 切换及旧版本兼容；不支持策略时暂停相关后台产出并明确提示。
6. 两种语言的真实模型验收覆盖日常/目标/页面对话、工具/修复与后台产出；预算检查使用完整请求。
7. 全应用发布前覆盖其他页面和常见错误；阶段性成果明确标示范围。

本轮验证：2026-11-06 执行 `cargo test storage::prompts::tests --lib -- --test-threads=4`，**4 passed / 0 failed**，覆盖修订/启用、语言回退与独立默认、运行/阅读固定、缓存依赖及只读协议。日志 `/tmp/i18n-prompt-foundation-tests.log`。证明现有基础可用，不代表英文运行或 UI 国际化已实现；本轮仅更新评估与 backlog。

## Risks

- 容易遗漏的是行为词表、动态上下文与 Rust 错误回流，它们不会随 ARB/模板翻译自动改变。
- AppFlowy 静态 l10n 修改 Intl 全局 locale，切换时须避免编辑器与应用格式互相覆盖。
- 中文自定义模板回退必须明确呈现；新知识语言须先补同步及任务身份，再开放设置。
- 当前专项测试不证明模型双语质量，需合成回归与真实模型验证；英文词表也不能保证覆盖任意表达。

## 2026-11-04 历史盘点（不代表当前实现）

下文保留当日规模和判断以便追溯，数量未在2026-11-06重新测量。旧“没有任何基础”“需要更换协议标记”“按语言重做预算”和“先决定是否需要”的建议，以本次复核为准：协议标记保持原值，现有分层预算继续使用，当前进入分阶段实施。

### 1. 现状盘点

#### 1.1 基础设施

| 项 | 现状 |
|---|---|
| `l10n.yaml` / ARB 文件 | ❌ 无（`third_party/appflowy_editor` 自带 30+ 语言 ARB，但只是编辑器依赖，主应用未接入 delegate） |
| `MaterialApp` 配置 | 只有 `DefaultMaterialLocalizations` + 中文 AppFlowy delegate（`lib/main.dart:167-177`），未设 `supportedLocales`，跟随系统 locale 但 UI 不响应 |
| 设置项语言切换 | 无（`settings_screen.dart` 及 `settings_parts/` 下无 locale/language 相关配置） |
| 复数/性别处理 | 无（`Intl.plural` 零使用） |
| 日期格式 | 已用 `DateFormat` 但符号写死中文（`'yyyy年MM月dd日'`、`'M月d日'` 与 ISO 混用），需 locale 感知初始化 |

#### 1.2 硬编码中文分布

| 层 | 规模 |
|---|---|
| Dart UI（单引号字符串） | 约 **2,300+ 处**：`lib/screens` 286、`lib/widgets` 1,737、providers 36 |
| Rust 用户可见中文 | 约 **~5,700 行**；其中 `anyhow!/bail!/format!` 错误消息 **571 处** |
| AI 提示词/工具描述 | `src/ai/` 含中文行 3,427：`SYSTEM_PROMPT_BASE` 全中文人格、4 条协议修复 nudge、工具 schema description（`write_tools.rs` 26 / `read_tools.rs` 71 / `mod.rs` 131）、`CITATION_INSTRUCTIONS`、摘要 `INSTRUCTIONS` |
| 测试断言 | **53 个测试文件、578 处** `find.text('中文')`——i18n 改造的隐性耦合大头 |

### 2. 对话块分层现状（重点）

对话块比其他模块多两层暴露面：Dart UI（可常规 ARB 化）之外，还有 **Rust AI 提示词层** 与 **错误回流层**。

#### 2.1 Dart UI 层（难度低，标准迁移）

- `message_area_parts/`（输入框 hint「AI 正在思考…」、now_status「今天」、message_bubble 菜单「复制/存为主题/纳入记录/重新分析」等 ~127 处）
- `goal_chat_panel.dart` 51 处、`chat_run_status.dart` 58 处、`chat_proposal_parts/`（proposal_detail/batch/review 等 28+ 处）

#### 2.2 Rust AI 提示词层（❗最难最特殊）

- **文本工具协议的解析器只认中文标记**：`[工具调用]`、`[规则提议]`、`[规则本质]` 是运行时解析的协议格式而非展示文案。提示词换语言而解析器仍匹配中文串时，工具调用会**静默失效**。两者必须同步改造，协议标记本身应语言无关化（或解析器双语兼容）。
- **AI 输出语言无控制机制**：提示词全中文 → 模型自然回中文。若 UI 换英文而提示词不变，出现「英文 UI + 中文 AI 回复」割裂。需引入「回复语言跟随用户消息语言」指令。
- **知识库内容语言漂移**：wiki 页生成语言由模型输出决定，选材指令（`CITATION_INSTRUCTIONS` 等）也是中文。需产品决策：知识库固定中文还是跟随用户。
- **工具 schema description 全中文**：发给模型的功能描述，翻译影响模型理解质量，需真模型回归测试。

#### 2.3 错误回流层（跨 FFI）

Rust 侧 571 处中文错误（「已达 3 条上限，请先归档一条」等）经 flutter_rust_bridge 原样透传，Dart 侧 `toast(e.toString())` 直显。两条路：

- **错误码化**（彻底）：Rust 返回结构化 `{code, params}`，Dart 查 ARB 渲染——改造量大
- **接受单语言**（务实）：多数桌面工具的做法

#### 2.4 深水区：token 预算与语言耦合

`src/ai/budget.rs` 中文按 0.6~0.8 token/字估算。提示词换语言后字节率完全不同，预算校准参数需按语言区分——i18n 估算时最容易漏的一项。

### 3. 若决定做：分层路线与量级

| 阶段 | 内容 | 量级 |
|---|---|---|
| 0 | **决策**：目标用户/市场是否需要非中文版；知识库内容语言策略 | 决策 |
| 1 | 基建：`l10n.yaml` + ARB + `supportedLocales` + 设置页语言项；先迁对话块 UI 层（message_area_parts + goal_chat_panel + chat_run_status + proposal 系列） | 小~中，~400 处 |
| 2 | 错误码化：Rust 错误改结构化 code，Dart 渲染 | 中，571 处 + 桥协议 |
| 3 | 提示词层：SYSTEM_PROMPT_BASE 语言包化、协议标记与解析器语言无关化、「回复语言跟随用户」指令、工具 schema 翻译 + 真模型回归 | 大，风险最高 |
| 4 | token 预算按语言分参数；日期 locale 化；测试断言迁移（578 处） | 中 |

### 4. 总体判断

对话块 UI 文案的可迁移性没有问题；真正的多语言成本集中在 Rust AI 层——**提示词、协议标记解析、工具 schema、token 预算四者相互咬合**，且没有现成测试安全网（现有测试全部假设中文交互，如 `acceptance_controls.rs` 合成验收提示词直接用中文写验收指令）。

产品定位上，docs 全中文、产品人格（情绪回应、知识库分区名）均为中文语境设计。若目标用户明确是中文用户，**i18n 的 ROI 很低**——建议先用阶段 0 的用户定位决策否决或推迟整个计划，避免为一个不存在的需求背上长期双语协议维护负担。
