# Agent Note: 按需翻译（知识页与情报条目共用内核）

Status: proposed

## Problem

当前系统里所有外文材料都只能整段读原文：知识页的 `imported` 素材（60 页）多数是英文，情报条目的产品描述（`Privacy-first ad blocker` 这类）也全是英文，模型产出又要求按 `zh-Hans` 落库。用户想看懂某一页、某一条时没有任何入口，只能把文本复制到外部工具——这既绕过了现有 provider 配置，也把内容送出本机。

已有能力里没有任何一处做「源文本 → 目标语言文本」的转换：

- `src/language/runtime.rs` 的 `ResponsePolicy` 决定的是**回复用哪种语言写**，以及知识落库用哪种语言，不转换已有内容。
- `src/language/policy.rs` 只声明「哪个 locale 受支持」（`zh-Hans` / `en`），不是翻译器。
- `src/storage/knowledge_language.rs` 的 `knowledge_language_artifacts` 记录「某个产物是以哪种 locale 生成的」这一来源证据，同样不产出译文。

换句话说，语言策略已经齐了，缺的是执行体。

## Proposal

做一个**按需触发、可随时调用**的翻译能力，两个入口共用同一个内核：

1. **知识页**：在页面详情上点一次，产出该页的完整译文。
2. **情报条目**：在情报工作区对可见条目点一次，只把「这个产品是干什么的」那一个字段译成中文（产品名不译，见 §6）。

两处都要求「点一下就有」，不做后台批量预翻、不做定时任务、不做自动翻译开关。理由：翻译的触发理由永远是「我现在想看这一条」，预翻会把成本和审阅负担提前堆到用户没要求的时刻。

内核是一次参数化的模型调用；两个入口的差别只在**产物落到哪里**：

| | 知识页译文 | 情报条目译文 |
|---|---|---|
| 触发 | 页面详情按钮 | 情报工作区按钮 |
| 落点 | **派生产物**（`area=derivative`，`content_type='译文'`，`based_on=原页`） | 本地缓存表，只在界面替换显示 |
| 是否入知识库 | 是，进派生产物列表，走既有的版本链 | 否，不产生页面、不进主列表 |
| 是否参与同步 | 是（它就是一张 `wiki_pages` 行） | 否（本机缓存） |
| 原页是否被改 | **永不** | 不适用 |

## 设计

### 1. 为什么知识页译文必须是派生产物，不能是修订

这是本提案最重要的一条边界。

现有页面任务已经有一套「用户说改就改」的自动升级：`src/ai/page_tasks.rs:98` 的 `escalate_page_task` 会把 `local_question` / `full_summary` 升级成 `local_edit` / `full_revision`，触发词表里含 `改写`、`改一下`、`改成`（`src/ai/page_tasks.rs:20` 的 `PAGE_REVISION_TRIGGERS`）。用户说「翻译这一页」在语义上非常接近「把这一页改成中文」，一旦落进 `full_revision`，结果就是**一份待审的正文修订，接受后原文被替换**——英文原文消失。

因此：

- 翻译**不新增** `PageTask.kind`，也不进 `escalate_page_task` 的可升级集合（`src/ai/page_tasks.rs:102` 的 `matches!` 已经天然排除了非 `local_question` / `full_summary` 的类型，新增的翻译入口只要不经过 chat 任务分发就不受影响）。
- 翻译走**独立 API 函数**，不复用页内对话的消息管线，从根上避开触发词判定。
- 产出一律经 `create_derivative`（`src/storage/wiki/modify.rs:12`）：`area=derivative`、`based_on=原页 slug`、`content_type='译文'`。

这条与既有纪律一致：prompt bundle 里的 `page_protected` 写的就是「受保护原文，只能草拟 derivative」。`imported` 与 `derivative` 页永远只能生出另一份产物。

**派生链不允许再派生**：对一张 `content_type='译文'` 的页再点翻译，直接返回它本身（或提示已存在），避免「译文的译文」。判断依据是 `based_on` 非空且 `content_type='译文'` 时拒绝新增，要重译就走覆盖同一 `(parent_id, content_type)` 的新版本——`knowledge_artifact_versions` 的版本号正是按这个键递增的（`src/storage/wiki/modify.rs:97`）。

### 2. 分块：必须换一套切法，不能复用 `reading_ranges`（仅知识页）

`src/ai/page_tasks.rs:381` 的 `reading_ranges` 带 **700 字重叠**，因为归纳要跨块接上下文。翻译复用它会得到**重复的译文段落**（重叠区被译两遍并拼接两遍）。

翻译的分块规则：

- 按段落边界切，**零重叠**；段落超长时退到句末标点。
- 目标块长约 `CHUNK`（3000 字，`src/ai/page_tasks.rs:15`），块数上限沿用 `MAX_PARTS`（32）。超过 32 块直接报错并给出「拆分后再译」，不静默截断——这是既有纪律。
- 每块额外附**上下文而不附材料**：前一块末尾一句、后一块开头一句，显式标注「仅作衔接参考，不要翻译这一行」。上下文进 prompt 但不进结果校验。

必须原样保留、永不翻译的区段：

- 围栏代码块（```` ``` ```` / `~~~`）与缩进代码块
- 行内代码、URL、Markdown 链接地址
- 数字、日期、版本号、SPDX 标识符、文件路径

### 3. 术语漂移：会话级术语表（仅知识页）

32 块独立请求，`license` 可能在第 1 块译成「许可」、第 8 块译成「授权」。做法：

1. 第一块请求同时要求模型输出本块出现的术语对照（`[{src, dst}]`）。
2. 后续每块都带上累积术语表，并要求「表内术语必须沿用，不得改译」。
3. 术语表在同一次翻译调用内有效，**不跨调用持久化**——持久化会把一次误译固化成长期约定。

用户可在翻译前手工补充强制保留词（如 `SPDX id`、`Chrome Web Store` 不译），与术语表一同注入。

### 4. 结构守恒校验（仅知识页）

译文的失败模式是「顺手把三段并成一段」或「漏掉最后一节」。**这是 Markdown 长文才有的问题**；情报条目只有一句话，不需要结构校验，只需长度比上限（§6）。

每页翻译完成前校验：

- 标题层级与数量一致
- 围栏代码块数量及其内容**逐字相同**
- 列表项数量一致
- 译文字符数落在原文的合理区间内（防止「翻译」成摘要）

任一不通过则这一块重试一次，仍不通过就整体失败并保留已成功块的缓存，**不落半成品页面**。

### 5. 缓存：块级，不按页

分块翻译的中间失败很常见（第 20 块超时）。缓存必须按**块**粒度，才能重试时不重复计费。

新表（迁移 88）：

```sql
CREATE TABLE translation_cache (
  source_hash       TEXT NOT NULL,   -- 源块文本的 sha256
  target_locale     TEXT NOT NULL,
  prompt_dependency TEXT NOT NULL,   -- 复用 prompt_dependency(keys)
  translated        TEXT NOT NULL,
  model             TEXT,
  created_at        TEXT NOT NULL,
  PRIMARY KEY (source_hash, target_locale, prompt_dependency)
);
```

`prompt_dependency` 带上后，**改 prompt 自动失效旧缓存**，不需要手工清表——这与既有 prompt 版本机制一致。表登记进 `SNAPSHOT_EXCLUDED_TABLES`（`src/storage/sync/snapshot/records.rs:82`），理由：它是可重建的计算缓存，不是用户劳动。

情报条目的翻译同样走这张表，但**收益最大**：产品描述在每个产品的多轮快照里反复出现，没有缓存就是每轮重译同一句话。所以情报侧只翻一个字段，却恰好是缓存命中率最高的用法，这也是范围收窄后成本变得可忽略的原因。

### 6. 情报条目：实际只有两个源有东西可翻

按最近快照逐个源核对后的真实字段形态（共 994 条镜像）：

| 源 | 可翻字段 | 其余字段 | 条数 |
|---|---|---|---|
| GitHub Dart / Rust | `description` ✔ | `full_name`、`stargazers_count`、`license_spdx_id`、`topics`… | 314（其中 282 条描述 >40 字符） |
| Product Hunt | `summary` ✔ | `title`（全在 40 字符内）、`link`、`published` | 317 |
| CWS | **无** | `developer`、`name`、`rating`、`rating_count`、`size`、`updated`、`users` | 26 |
| Gumroad | **无** | `name`、`seller_name`、`price_cents`、`ratings_*`、`thumbnail_url` | 246 |
| App Store | **无** | `name`、`artistName`、`genres_0_name`、`rank`、`releaseDate` | 91 |

这是本提案必须写清楚的事实：**「翻译情报」听起来是大功能，实际只覆盖 631/994 条，且只限两个源。**CWS、Gumroad、App Store 的 payload 里只有专名、数值、日期和 URL，一条可翻的自由文本都没有。实现时不要为这三个源写分支——没东西可翻。

**需求已经确认（2026-11-17）**：使用者明确表示会读这两个字段。因此情报侧翻译不是为了凑齐两个入口而做，而是确有用例；但价值密度仍然低于知识页——它的产出是一句话，不是一份能重读的材料。

同理，产品名一律不翻：`Consensys Software Inc.` 不该变成「康森西斯软件股份有限公司」，`uBlock Origin Lite` 也不该变成「uBlock 起源精简版」。所谓「翻译情报」在本项目里的确切含义就是**把 GitHub 仓库自述和 PH 一句话介绍译成中文**。

App Store 的 `genres_0_name` 是受控词表（`Shopping` 这类 Apple 固定分类），正确做法是小写映射表而不是送模型——榜单里反复出现的就是同一小撮分类，翻错一个会污染整张榜。

明确不翻：

- `name` / `full_name` / `title` / `developer` / `artistName` / `seller_name`：专名。
- 全部 URL 字段（`link` / `url` / `html_url` / `thumbnail_url` / `artworkUrl100`）：实测这些字符串都超过 40 字符，是最容易被「长字符串就是正文」这种启发式抓住的一类。
- 全部数值与日期（`stargazers_count`、`price_cents`、`ratings_*`、`rating_count`、`users`、`rank`、`releaseDate`、`updated`）。
- `license_spdx_id` / `spdx_id` / `permalink` / `native_type` / `id`。
- `delta.diff` 结构本身（只翻 `from` / `to` 的正文，不翻数值）。

这与既有采集纪律同源：数值拆成标量是为了能算 diff，翻译数值会把 diff 毁掉。

三个必须写死的兜底，因为「源站没写描述」在 PH 侧普遍存在（`summary` 是完整 HTML，含 `&lt;p&gt;` 实体与 `Discussion | Link` 导航噪音）：

- **先用干净文本，再翻**。PH 的 `summary` 必须先剥标签、解实体、去导航句再送模型。剥不干净会被模型把 `&lt;p&gt;` 当正文翻，或把链接地址也译了。现有 `_stripHtml` / `_decodeEntities` 在 Dart 侧（`ui/lib/providers/scout_provider.dart`），翻译在 Rust 侧，**需要一份 Rust 等价实现**，两处行为必须一致。
- **空字段不得让模型编造**。没有源描述就不产出译文，界面保持空白。
- **译文必须标为译文**。情报的核心纪律是「事实、当事人陈述、确定性统计、AI 推测分开保存与展示」；译文属于本地加工，不得看起来像源站原话。UI 需有可见标识。

长度比校验（§4 只做结构、不做长度；情报侧反过来）：译文长度超过原文约 1.8 倍就丢弃并重试一次。依据是这类短描述最典型的失效不是漏译，而是模型把它「补完整」成一段营销文案。阈值宁紧勿松——短句本来就该短。

### 6.1 用户评价：现在没采，将来采了也只翻正文

`GUMROAD_FIELDS` 只有 `ratings.average` 与 `ratings.count`，即均分与数量；其余源也没有评论字段。**所以「翻译评价」目前是空操作**，不应为它预留实现。

若将来采集评价正文，边界如下：

- 评价正文属于「用户说了什么」，可以翻；但**评分、条数、时间不翻**。
- **评价正文不能进 `fields`**。它持续变动，进 `fields` 就是每轮制造假 changed——这与之前已修过的「全量假 changed」是同一类缺陷。它只能作为随条目携带的附加读取，不参与 diff。
- 翻了不代表能据此下结论。既有商机研究提案已明确「没有评价正文不生成『用户普遍抱怨』」；有正文之后也最多到「可引用具体评价」，不升级为聚合判断。

### 7. 预算档位：必须用生成档（仅知识页）

知识页的译文输出体量约等于输入，所以这一节只对知识页成立；情报条目一次批量请求的输入输出都是短句，不会撞顶。

`src/ai/provider/openai.rs:33` 的 `output_reserve_override` 默认为 `None`（生成档），聊天与摘要路径显式调用 `with_output_reserve(budget::chat_output)` 切到低预留档给输入让位。

翻译的**输出体量约等于输入**，用聊天档会触发 `fits_request` 失败，或请求被上游静默截断后产出半篇译文。翻译路径**不调用** `with_output_reserve`，保持生成档。

代价要写清楚：这会让翻译比聊天更早撞上下文上限。所以超长原文的分块上限（32 块）是硬约束，不是建议值。

### 8. 单飞与进度

一次 32 块的翻译串行发请求，可能持续数分钟。三个决定（情报条目是一次批量请求，不涉及单飞以外的顾虑）：

- **不新增后台任务表**。FRB 的同步函数在 Dart 侧返回 `Future`、跑在 Rust 线程池，UI 不会冻结；既有 `generateReply` 处理 32 块全文归纳就是这个模式，有先例。
- **单飞靠进程内守卫**：`static Mutex<HashSet<(source_hash, locale)>>` 挡住重复点击。单机单进程场景够用；崩溃不会留下数据库脏状态（缓存是幂等的 `INSERT OR REPLACE`）。
- **进度不做实时推送**：UI 显示不确定进度与已用时间；用户可再次点击，已完成的块从缓存命中、立刻返回。不做流式、不做百分比——为了一个按需动作引入推送通道不值得。

### 9. 语言门

译文写入 `knowledge_language_artifacts`（`record_language_artifact_with_output`，`src/storage/knowledge_language.rs:68`），`artifact_kind='page_translation'`，`output_locale` 填目标语言。

目标语言只接受 `src/language/runtime.rs:31` 已支持的取值（`zh-Hans` / `en`）。**默认跟随 `knowledge_locale`，不做独立的「翻译语言」设置项**——多一个语言设置就多一处不一致。

### 10. Prompt 落点

新增一个 fixed_contract 条目，与 `chat.execution.fixed_contract` 同类：

- `src/storage/prompts.rs:70` 的 `catalog()` 加 `translate.fixed_contract`
- 正文放 `resources/prompts/translate-contract.{zh-Hans,en}.json`，内含 `chunk_translate` / `glossary_extract` / `translate_repair` / `context_only` 几段
- 无需迁移：`register_prompts()`（`src/storage/prompts.rs:292`）在 `Store::open()` 时对 `prompt_definitions` 做 upsert、对 `prompt_revisions` 做 `INSERT OR IGNORE`

### 11. 引擎选择：为什么先用 LLM，什么条件下该换

首版用**现有的 LLM provider**，不引入专用翻译引擎。三条依据：

1. **凭据与预算已在本地**。provider 配置存在于 `elsewhen.db`，是既有架构的唯一真源。接 DeepL / LibreTranslate 要新增凭据、网络路径、失败模式，以及一套「用哪个引擎」的配置界面。
2. **知识页是 Markdown**。专用翻译服务的免费档基本只处理纯文本，代码块、标题层级、链接会被压平。LLM 能被明确指令「围栏代码块逐字保留」。
3. **约束是自然语言表达的**。术语表、保留词、块间衔接上下文，在 LLM 里是 prompt 里的一段话；在专用翻译服务里通常是付费功能或干脆没有。

LLM 明确**不占优**的两点，必须承认而不是绕开：

- **保真度**。专用翻译引擎在纯散文上比 LLM 更忠实。LLM 的典型失效正是它别处的长处——顺手压缩啰嗦处、合并短句、调整语气。§4 的结构守恒校验只能**事后拦截**：块数、代码块、标题数对得上，中间被并句或改语气是看不出来的。这在知识页（长文、要逐段读）上是真实风险。
- **成本模型**。但这一条在范围收窄后基本不成立了。原先担心的是「长篇英文评价逐条翻」，实际只翻一个 `description` / `summary` 字段、且按页批量提交成一个数组：一页 20 条 × 每条 100 字符 ≈ 一次请求 2000 字符。加上块级缓存（同一产品下轮轮询直接命中），成本可以忽略。**故不再把成本当作选引擎的理由。**

按场景分开看：

- **情报条目 → 同样用 LLM，不需要换便宜档**。范围收窄后每个条目只有一个短句，按页批量后一次请求就完成；且这里的真实需求是「保留专名 + 中文注解」，`Consensys Software Inc.` 该留专名，不该硬译成「康森西斯软件股份有限公司」。专用翻译引擎恰好在这一点上会做错，LLM 用指令能管住。
- **评价正文（将来若采集）→ 这才是专用翻译引擎真正合适的场景**：纯散文、要忠实、不需结构。但它在采集侧尚未立项（§6.1），不为一个不存在的字段预先选引擎。

**架构后路**：内核是一个函数签名 `translate(store, provider, text, target_locale) -> String`，两个入口依赖它而不依赖它背后用谁。换引擎改一处，缓存键里加一列 `engine` 即可让旧结果失效。

**重新评估的触发条件**（满足任一条才重开引擎讨论，否则不讨论）：

1. 用真实材料翻 3–5 页真实外文知识页 + 20 条真实 finding，人工对比发现并句、漏段、改语气或**编造源文没写的内容**。
2. 单页翻译的实测耗时明显影响日常使用（例如超过 2 分钟且无法通过分块缓存缓解）。
3. 明确需要断网可用。此时唯一候选是 Ollama 本地模型（provider 层已支持），质量需重新验收。

**即使触发，正确的第一补法也不是换引擎**，而是加一道译文与原文的逐块对照校验——这在专用翻译引擎下同样做不了。换引擎只在「保真度不足且校验补不动」时才成立。

没有真实材料的质量基线之前不选引擎：现在选是瞎猜。

## 明确不做

- **不引入独立翻译服务**（LibreTranslate / DeepL / Google Translate）。判断依据与重新评估条件见 §11；不在有基线之前预置第二套基础设施。
- **不做自动/后台批量翻译**。没有开关，没有定时任务。
- **不把译文当原文修订**，永不覆盖原文。
- **不翻译标量字段**，见 §6。
- **不做流式输出**，见 §8。
- **不把译文当证据或规则**。译文是派生产物，引用必须回到原文。
- **不做译文与原文的逐段对照视图**（首版）。首版只在派生产物里给全文译文；对照阅读等真实使用反馈。

## Alternatives considered

- **复用页内对话**（用户在页面聊「翻译这一页」，再把回复存为产物）：零 Rust 改动，但会撞上 `escalate_page_task` 的触发词升级（§1），且要求用户会打字、会再点一次「存为产物」。两步操作换零改动，不值。
- **在服务端（scout-cf）翻情报**：offline 且省客户端算力，但 AI provider 配置在那台机器上不存在——配置在 Elsewhen 本地数据库里，这是既有架构的硬约束（CLAUDE.md「数据库是 AI 配置的唯一真源」）。要这么做就得把 provider 凭据搬到服务端，与本地优先原则冲突。
- **把译文塞回 `payload`**：会让 `payload` 同时承载「源站事实」和「本地加工」，破坏快照 append-only 与 delta 计算的语义。
- **复用 `content_type='AI 总结'`**：语义不同。总结是压缩，翻译是等价重述；混用会让派生产物列表无法按类型区分，也无法对译文单独做结构守恒校验。

## Acceptance criteria

- 知识页：对任一 `imported` 页点翻译，得到一张 `area=derivative`、`content_type='译文'` 的页；**原页 `content_md` 与其 `human_edited_at` 不变**；重复点击不产生第二张译文（走同键新版本）。
- 围栏代码块内容逐字相同；标题与列表项数量一致；不通过的块不落库。
- 情报条目：只替换「产品是干什么的」那一个字段（`description` / `summary`）；`name`、`full_name`、`title`、所有 URL 字段、`star 57.2k`、`+34`、`2026-09`、`Apache-2.0` 均原样不变。
- 源文本没有描述时，界面保持空白，**不得出现模型自行编造的产品介绍**。
- 译文在 UI 上有可见的「译文」标识，不会被误认为源站原话。
- 同一 finding 重复翻译第二次不发模型请求（缓存命中）。
- 修改 `translate.fixed_contract` 后缓存自动失效、重新请求。
- 超过 32 块的长文给出明确报错，不静默截断、不落半成品。
- 全程不冻结 UI；重复点击被单飞守卫挡住。
- 完整测试：块级缓存命中/失效、结构守恒拒绝、字段取值器只取一个字段、空字段不编造、派生链禁套娃、单飞。

## Risks

- **术语漂移无法根除**。术语表只管住模型「看见过的」术语；首块没出现的术语后续仍可能不一致。缓解：接受少量漂移，不做跨调用持久化（避免把误译固化）。
- **输出预算撞顶**。生成档在超长页面上更容易失败。缓解：32 块硬上限 + 明确报错；不做静默降级。
- **译文质量无法用测试证明**。单元测试只能证明结构守恒与缓存行为；读懂读不懂必须真实模型 + 真实材料验收。**不得用 mock 测试冒充质量验收**。
- **短描述被模型「补完整」**。一条 `Privacy-first ad blocker` 很容易被扩写成一句完整的产品介绍——这是情报场景最危险的失效，因为它看起来像源站事实。缓解：prompt 明确禁止扩充，且译文带可见标识。
- **空描述被编造**。见 §6；若源站没写，宁可不展示。
- **派生产物列表膨胀**。每点一次长文翻译就多一张页。缓解：同 `(parent_id, content_type)` 走版本递增而非新建；派生产物本就不进主列表。
- **同步边界不一致**。译文页会随快照同步，但它的语言来源行（`knowledge_language_artifacts`）在 `SNAPSHOT_EXCLUDED_TABLES` 里不同步——新设备有译文但看不到它的生成凭据。这是既有语言机制的既存边界，本提案不扩大它，但验收时要确认不会因此把译文误当原文。

## 待定

- 派生产物在 UI 里是否需要独立的「译文」筛选入口，还是混在现有派生产物列表里靠 `content_type` 显示。
- 评价正文是否采集是**采集侧**的独立决定（涉及假 changed、体量、各站 ToS），不随翻译能力一起推进。

## 已定

- **情报条目翻译的批量粒度：按 Findings 现有分页单位批量提交一个 JSON 数组，不逐条点。**依据：只有两个源有字段可翻、每条就一句话，而一页有 20 条，逐条点意味着点 20 次。单条失败不影响整批已成功的部分（每块独立进缓存），失败条目降级为逐条重试。
- **需求已确认（2026-11-17）**：使用者确认会读 GitHub `description` 与 PH `summary` 的译文，情报侧不做减法。
