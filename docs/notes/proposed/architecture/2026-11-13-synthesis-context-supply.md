# 合成型问题的上下文供给：分层检索、工具内证据提取与词法索引

- 日期：2026-11-13
- 状态：**实施中**。P0 `68ad97a`、P1 `4d079d5`、P2 已提交；P4、P3 按[实施路线](../../../roadmap/2026-11-14-synthesis-context-supply-roadmap.md)推进，进度与各期落地记录以路线为准（实现与本稿不同处也记在路线里）
- 范围：知识对话的检索、读取与综合链路（`src/knowledge/select.rs`、`src/ai/tool/read_tools.rs`、`src/ai/conversation/agent.rs`、`src/knowledge_background/tldr.rs`、`src/ai/budget.rs`）；新增一个只读合成工具与一套词法索引。不改事件存储、消化管线、审阅契约与同步协议
- 性质：实测失败驱动的架构设计
- 输入：仓库根目录的 `local_first_ai_context_engine_recommendations.md`（外部通用建议，未提交）；提交 `e0823de`、`d06dc2a`；工作区未提交的 `ToolContext.provider` 改动
- 关联：[上下文管理路线](../../../roadmap/2026-10-17-context-management-roadmap.md)、[预算与工具注入路线](../../../roadmap/2026-10-19-context-budget-and-tool-injection-roadmap.md)、[对话支撑 LLM Wiki 路线](../../../roadmap/2026-11-09-llm-wiki-conversation-support-roadmap.md)、[对话知识增量的召回与依据边界](../../implemented/architecture/2026-11-09-wiki-conversation-evidence.md)、[token 估算校准](./2026-10-19-token-estimation-calibration.md)、[规模评估](../../../llm-wiki-scale-and-refactoring.md)、[记事本页](../product/2026-10-19-notepad-memo-pages.md)

## 0. 结论与需要拍板的事

**结论**：外部建议的方向（先做上下文工程、暂不上 Embedding）成立，但它描述的病因与本仓库的实测不符，它建议新建的模块、表和预算比例表大多已有等价物。真正缺的是三件事，按代价从小到大：

1. 已提交的页级 TL;DR 有 6 处缺陷（§5.1），其中一处会无退避地重复付费请求，一处把记事本正文发给模型。
2. 读取没有"预算前移"：工具不知道本轮还剩多少输入预算，取回 8000 字符后再被预算层带披露截断（§5.2）。
3. 通读长页或综合多页在主对话上下文里**按构造不可行**（§1.3 的算术），需要把"分块提取证据"移到工具内部的子请求里（§5.3）。

FTS5 目前没有，本设计把它作为独立的 P3（§5.5），**P0–P2 不依赖它**：当前个人库的规模下词法扫描不是瓶颈。

**需要拍板**（不拍板不影响 P0）：

| 编号 | 决策 | 我的建议 |
|---|---|---|
| D1 | 合成工具每次最多发起几次子请求 | 6 次；一次合成的真实输入约 4–11 万 token，需要在工具说明和用量统计里可见 |
| D2 | 是否修订"估算只记低估、不记高估"的红线 | 先按 §5.6 用现有数据量出偏差，再决定；不凭注释里的系数下结论 |
| D3 | FTS5 何时做 | 有数据触发再做：知识页超过约 2000 页，或 `select_knowledge` 的 p95 超过 150 ms |
| D4 | `MAX_TOOL_ROUNDS` 是否调整 | 不调；合成工具把"多次翻页"压成一轮，四轮够用 |

## 1. 问题定义

### 1.1 实测失败

两个提交说明里记录了两次真实运行：

| Run | 现象 | 直接原因 |
|---|---|---|
| `564fe4bb` | 合成型问题四轮全部耗在 `get_wiki_page` 翻页上，没有产出答案 | 每次只返回 2400 字符，5 万字手册要 20 多次 |
| `8fd94ade` | 多轮工具结果累积后本地拒绝（41189 > 29492） | 聊天与结构化生成共用 32K 输出预留，64K 窗口的输入预算被砍到 29492 |

`e0823de` 把聊天输出预留降到 16K（输入预算回到 45,876），`d06dc2a` 把单次读取提到 8000 字符并加了页级 TL;DR。这两处修的是症状，§1.3 说明为什么还不够。

### 1.2 外部建议与仓库现状

| 外部建议 | 仓库现状 | 判断 |
|---|---|---|
| 病因是"多篇全文拼进一次请求" | 自动挂载上限 3000 字符/3 页，搜索 4000 字符，读页 8000 字符；没有全文拼接 | 病因不符，见 §1.3 |
| 新建 Token Budget 模块、按 10/10/55/5/20 分配 | `ai/budget.rs` 按完整序列化请求（含工具 schema）计量，逐级降级，截断都带披露 | 已有；固定比例表不适用于按段省略的现有模型 |
| 分批阅读再归并（Map-Reduce） | `knowledge/reading.rs`：6000 字符分块、四块一组递归归纳、逐字摘录校验、可续跑。只服务后台整理 | 已有一半，缺的是面向问题的提取和接到聊天 |
| Query Planning 独立阶段 | `search_knowledge_base` 接受原查询加最多 3 个模型改写，倒数排名融合 | 有改写，没有按分析维度拆分 |
| 可追溯 Evidence 表 | `KnowledgeCitation` 带来源快照、版本、内容哈希；回答引用过白名单校验 | 页级可追溯已有，缺字符级定位 |
| SQLite FTS5 / BM25 是现成能力 | **没有**。`select_knowledge` 在标题、标签、摘要、正文、适用条件上逐词 `instr` | 建议写错了前提，见 §5.5 |
| Wiki Links + 反向链接扩展 | `knowledge_relations` 只有确定性 `mentions` 和人工边，限 1 跳 | 可选增量，§9 |
| 新模块树 `retrieval/`、`synthesis/` 等 | 对应职责已在 `src/ai/`、`src/knowledge/` | 不照搬目录 |
| 十几张新表 | 来源、快照、阅读缓存、依赖、关系均已有 `knowledge_*` 表 | 不照搬 |
| 暂不上 Embedding | 11-09 路线已明确排除 | 一致 |

### 1.3 真正的约束：一笔算术

约束有三个，相乘之后决定了能读多少：

- **轮次**：`MAX_TOOL_ROUNDS = 4`（`src/ai/conversation.rs`），最后一轮只收尾（`finalizing`），所以能调工具的是 3 轮。协议回落、空回复重试、承诺回炉也消耗这 4 轮。一轮内模型可以并行发多个调用。
- **每轮输入预算**：`input_limit = 窗口 − 输出预留 − 余量`（余量为窗口的 5%，夹在 256–4096 之间）。
- **估算口径**：非 tiktoken 族按每个汉字 2.0 × 1.12 = 2.24 token 估算（`CJK_TOKENS_PER_CHAR`、`HEURISTIC_SAFETY`）。

| 模型族 | 窗口 | 聊天输出预留 | 输入预算 | 一页 8000 汉字的估算 | 能同时留在上下文里的页数（未扣固定开销） |
|---|---:|---:|---:|---:|---:|
| deepseek 现役 | 65,536 | 16,384 | 45,876 | 17,920 | 2 |
| GLM / 通义 / Kimi / 豆包 / Claude / Gemini | 131,072 | 16,384 | 110,592 | 17,920 | 6 |
| 未知模型 | 32,768 | 8,192 | 22,938 | 17,920 | 1 |

5 万字的手册是 7 页，共约 125,000 估算 token，**超过表里任何一档的输入预算**。128K 窗口最多放 6 页即 4.8 万字。所以"让模型把长页翻完再回答"不是轮次给少了，而是容量上放不下；把轮次加到 10 轮只会更早触发预算层的截断。

多页综合同理：三页各 8000 字符在 64K 模型上就超限，预算层会依次做早期工具结果占位、补充材料减半、砍整轮、近期工具结果减半（`RequestPolicies::fit`），每一步都如实披露，但模型拿到的是被截掉一半的材料。

**推论**：长页通读和多页综合必须在主上下文之外完成，主上下文只接收提取后的证据。这是 §5.3 的依据。

估算口径本身可能偏高，这会放大上面的问题，但不改变结论（真实容量即使大 2–3 倍，5 万字仍然吃掉大半个窗口）。口径问题单独在 §5.6 处理。

## 2. 目标与非目标

**目标**

- G1 综合 3–8 页、或通读一篇 5 万字的页，在 4 轮内给出带逐字出处的回答，不触发本地拒绝，也不依赖"截断后披露"。
- G2 每条进入回答的证据能定位到页面版本和字符区间；模型的归类和转述不冒充原文。
- G3 每次模型请求（含工具内子请求）都经过现有的取消、租约、预算、记账通道。
- G4 检索第一跳的每页成本降到约 150 token 级别，同时不丢失"命中在页内哪里"。
- G5 增量落地：每一期单独可发布、可回退，不引入手动后台触发入口。

**非目标**

- 不引入 Embedding、向量库或 Reranker 模型。
- 不新建与现有 `knowledge_*` 表职责重叠的表；不持久化证据（先看复用率，§9）。
- 不做关键词式的"合成型问题识别"。10-20 事故（承诺检测词表误杀）说明对用户文本做词表分类的失败代价高；是否合成由聊天模型通过调用工具来表达。
- 不改变孵化闸门、引用白名单、审阅确认契约。
- 不为这项能力提高 `MAX_TOOL_ROUNDS`。

## 3. 可复用的现有部件

| 部件 | 位置 | 本设计怎么用 |
|---|---|---|
| 请求预算与降级 | `ai/budget.rs`：`input_limit`、`RequestPolicies::fit`、`tool_result_halved` | 子请求照常经过；新增"剩余预算"查询供工具前移 |
| 子请求通道 | `ai/generation.rs::auxiliary_request` | 工具内 map 请求的唯一出口（取消、续租、用量、预算事件） |
| 引用资格 | `knowledge/select.rs::citation_for_page`、`citation_for_page_explicit` | 合成工具读哪些页完全沿用 |
| 候选信封与引用绑定 | `knowledge/bind.rs::candidates_in_context`、`validate_answer_citations` | 合成结果放进同一个 `knowledge_candidates` 信封 |
| 读取覆盖标识 | `knowledge_reading`：`full` / `summary` / `excerpt` | 新增 `evidence` 一档 |
| 分块参数与逐字校验 | `knowledge/reading.rs`：`CHUNK = 6000`、摘录必须是原文子串 | 分块边界对齐；校验规则照搬 |
| 阅读树缓存 | `knowledge_source_readings`（`level`、`part`、`start_char`、`end_char`、`summary`、`quotes_json`、`is_root`） | 根节点归纳作长页 TL;DR 的输入；叶节点归纳作提取失败时的带标注兜底 |
| 多问法融合 | `select_knowledge_variants` | 合成工具的选页直接调用 |
| 负知识提醒 | `knowledge/negative.rs::select_warnings` | 合成结果同样附带 |
| 版本化 Prompt 与语言策略 | `store.render_processing_instruction`、`prompt_for_run`、`output_language_directive` | 所有新提示词走这里，不写死在代码里 |
| 后台账本 | `knowledge_background_runs`（租约、失败退避） | TL;DR 回填接入 |

## 4. 总体设计：三层供给

```text
用户问题
   │
   ▼  聊天模型自己决定走哪一层（不做词表分类）
┌──────────────────────────────────────────────────────────────────┐
│ L0 发现   search_knowledge_base                                  │
│   每页：标题 + TL;DR(≤300字，生成摘要) + 命中片段(≤240字，原文)  │
│         + hit_offset + total_chars + content_hash                │
│   命中长页或多页时附一句确定性提示：综合请用 synthesize_knowledge│
├──────────────────────────────────────────────────────────────────┤
│ L1 定位读取   get_wiki_page(slug, offset=hit_offset, content_hash)│
│   返回量 = min(8000 字符, 本轮剩余输入预算能容纳的量)            │
│   用于核对一两处具体要求                                         │
├──────────────────────────────────────────────────────────────────┤
│ L2 合成   synthesize_knowledge(question, sub_questions, …)        │
│   工具内部：选页 → 分块 → 词法选块 → 分批子请求提取              │
│             → 逐字校验 + 定位 → 去重装配                         │
│   返回：已校验证据(≤6000字符) + 覆盖范围声明                     │
│   主上下文只看到证据，原文不进主上下文                           │
└──────────────────────────────────────────────────────────────────┘
   │
   ▼  收尾轮：聊天模型基于证据作答，引用走现有白名单校验
```

典型的四轮：第 0 轮搜索，第 1 轮合成，第 2 轮按需用 L1 核对一处细节，第 3 轮作答。模型已知页面时可以第 0 轮直接合成。

## 5. 详细设计

### 5.1 P0：修正已提交的页级 TL;DR

`d06dc2a` 的实现有 6 处问题。前两处涉及费用和隐私边界，应先修。

| # | 问题 | 证据 | 修法 |
|---|---|---|---|
| 1 | **失败无退避，每个 tick 重复付费请求，并饿死其他页** | `run_page_tldr_backfill` 对失败只 `continue`；`pages_missing_tldr` 固定取 `last_seen_at` 最新的 2 页。模型返回超过 600 字符或空串时 `commit_page_tldr` 报错，下个 tick 还是这 2 页。模块注释说有账本 `task='page-tldr'`，代码里没有写账本 | 接入 `knowledge_background_runs`：`input_key = page_id:content_hash`，失败按 `reading.rs` 的退避（2、4、8、16 分钟，第 5 次起 6 小时）。超长时截到句末并保存，不当失败 |
| 2 | **记事本正文被发给模型** | `pages_missing_tldr` 只排除 `archived` 和空正文。`memo` 是人类专属页，约定只在用户引用时按需读取；孵化中的 `source`/`note` 和已否定的页也会被取走 | 候选条件改为"能通过 `citation_for_page` 的页"。TL;DR 只在 `select_knowledge` 里、且在 `citation_for_page` 成功之后被读取，给不可引用的页生成摘要既越界又浪费 |
| 3 | **页面更新后永不重建** | `pages_missing_tldr` 的条件是 `t.page_id IS NULL`。页面是原地 UPDATE，旧行还在但哈希不匹配：`page_tldr` 读不出来，回填也不会选中它。迁移和函数注释都写"等待重建"；测试 `page_tldr_roundtrip_and_hash_expiry` 只断言读不出来 | 迁移 83 加触发器：`wiki_pages.content_md` 变化时删除对应 `page_tldr` 行。纯 SQL，覆盖全部写路径（含同步应用） |
| 4 | **生成摘要被标成原文摘录** | 搜索工具按 `excerpt == content_md` 判 `full`，否则一律 `excerpt`；TL;DR 落在 `excerpt` | `select` 另返回每页的摘录种类，搜索工具据此写 `coverage: "summary"`。不给 `KnowledgeCitation` 加字段，避免动桥接 DTO |
| 5 | **短页从全文退化成摘要** | 原来不超过 1000 字符的页由 `matching_excerpt` 整页返回（`coverage: full`，11-10 的 F2 修复）。现在有 TL;DR 就替换 | 只对超过 1000 字符的页生成和使用 TL;DR |
| 6 | **丢了命中位置，长页只看开头** | TL;DR 取代了词窗摘录；生成时只喂前 6000 字符（注释说"开头 + 小节骨架"，代码只取开头）。问题命中长页深处时，结果里既没有相关片段，也没有该从哪读 | 结果同时给 TL;DR、命中片段（不超过 240 字符的原文）和 `hit_offset`（`matching_excerpt` 已算出最佳窗口起点，现在丢弃了）。生成输入：来源有阅读树根节点时用根归纳（全文覆盖，零额外阅读）；否则用开头加 Markdown 标题骨架，并在 `page_tldr.basis` 记 `reading-root` / `head-outline` / `full` |

另外三处较小：

- 提示词写死在 `tldr.rs` 里并要求"中文摘要"，绕过了版本化 Prompt 和知识语言策略（英文数据集会得到中文摘要）。改走 `render_processing_instruction` 和 `output_language_directive`。
- 回填没有调用 `record_token_usage`，这部分消耗不在每日用量里。
- `select_knowledge` 不止搜索工具在用（`knowledge_background.rs` 也导入了它）。TL;DR 现在会随 `excerpt` 流到这些调用方，需要逐个确认它们是否把 `excerpt` 当原文用。**这一条我没有核实**。

#### P0 实施记录（2026-11-14）

实现与上面的草案有几处不同，以代码为准：

- **退避用独立小表 `page_tldr_backoff`**，没有接 `knowledge_background_runs`。后者是只增的运行日志，每次成功生成都会多一行，并且维护界面对未知任务名回落显示成「洞察」。小表每页至多一行，页面正文变化时由同一个触发器清掉。它记两种情况：生成失败（`attempts` 递增，按 2、4、8、16 分钟、之后 6 小时退避），以及页面当前不可引用（`attempts` 不变，一小时后再核对）。后者是草案没想到的：不记下来，每个 tick 都会重读同一批不合格的页，并且它们会挡住排在后面的页。
- **失败后整个回填冷却 10 分钟**。只按页退避不够：上游故障或模型持续返回空内容时，每个 tick 会换一页再请求一次，直到所有页都进入退避。
- **摘要不进 `excerpt`**。`excerpt` 始终是逐字原文；搜索工具在 `knowledge_reading[slug]` 里另给 `generated_summary` 和 `hit_offset`，`coverage` 仍按原文摘录判定为 `excerpt`。这样不动 `KnowledgeCitation`（桥接 DTO），也不需要新的覆盖档位。
- **摘要计入搜索预算**。`generated_summary` 的长度与引用的序列化长度合计不超过原来的 4000 字符；否则 6 页各带 600 字符摘要会让一次搜索结果多出 3600 字符。
- **只有搜索工具这条路径使用 TL;DR**（`select_knowledge_variants`）。有摘要的长页把原文窗口缩到 240 字符；`select_knowledge` 的其他调用方（自动挂载、洞察取材）拿到的仍是 1000 字符原文词窗。上文"其他调用方是否把摘要当原文"的疑问因此不成立。
- **`reading-root` 只在页面正文与来源快照哈希相同（页面就是来源原文）时使用**；派生页的正文与来源不同，用来源的归纳会概括错对象。
- **提示词 bundle 按提示词与语言策略的状态共用**，不是每页一个；提示词或知识语言策略变化后自动换新。

其余按草案：迁移 83（`basis` 列、清空旧行、过期触发器）；候选限定为超过 1000 字符且通过 `citation_for_page` 的页；写入时校验生成前的正文哈希，生成期间页面被改则丢弃这份摘要且不算失败；超长回复截到句末保存；提示词移入 `task.processing.fixed_contract`（`tldr_system`、`tldr_input`）并遵循知识语言策略，Prompt 目录升到 `prompts-v15`；回填用量记入每日统计；搜索工具说明补了摘要与命中偏移的用法。

未做：知识语言策略切换后，已有的 TL;DR 不会重新生成（仍是旧语言）；没有写 `knowledge_language_artifacts` 语言来源记录。

验证（在 `d06dc2a` 加本次改动的独立副本上，不含工作区其他未提交改动）：`cargo test --all-targets` 库测试 1240 通过、23 忽略、1 项失败，集成测试 7 项通过。失败的是 `short_main_stream_input_survives_424_with_32k_fallback`：本机回环 HTTP 夹具有 10 秒等待上限，并行负载下会超时，单独运行通过。它不是本次改动引入的：未改动的 `d06dc2a` 并行运行 `ai::conversation` 组 3 遍同样失败 2 遍；串行运行两边都全过，耗时相当（`d06dc2a` 32.9 秒，工作区 33.3 秒）。同类的另外两项（`observable_run_checkpoints_and_cancels_without_final_message`、`natural_replies_never_execute_or_discard_unbound_proposals`）在 `d06dc2a` 上也偶发失败。

新增 6 项测试：存储往返、过期与候选资格；只读可引用长页且搜索摘录逐字；失败退避与全局冷却、超长回复截断；长页输入范围（`head-outline`／`reading-root`）；摘要计入搜索预算；截断函数。

没有调用真实模型，TL;DR 的摘要质量未验收。没有重跑 release 构建和 Flutter 测试（未改公开 API 与 Dart 代码）。

迁移 83 的草案如下，实际内容（含 `page_tldr_backoff`）见迁移文件：

```sql
ALTER TABLE page_tldr ADD COLUMN basis TEXT NOT NULL DEFAULT 'head-outline';
-- 82 版的行来自只读开头的生成，且可能包含不应生成的页；整表清空，由后台重建。
DELETE FROM page_tldr;
CREATE TRIGGER page_tldr_expire AFTER UPDATE OF content_md ON wiki_pages
WHEN OLD.content_md IS NOT NEW.content_md
BEGIN
  DELETE FROM page_tldr WHERE page_id = NEW.id;
END;
```

### 5.2 P1：定位读取与预算前移

**命中定位**。搜索结果每页增加 `hit_offset` 和 `total_chars`。`get_wiki_page` 的 `offset` 参数已存在，`content_hash` 也已在引用里，所以模型可以直接从命中处读，不用从头翻。工具描述相应补一句。

**预算前移**。现在的顺序是工具取回 8000 字符，预算层发现超限后再减半并披露。改成工具事先知道能返回多少：

- `AiProvider` 增加一个只读方法，返回当前上下文和工具清单下的剩余输入预算（估算 token）。两个具体 provider 已经有窗口和输出预留，内部就是 `input_limit − input_tokens(当前请求)`。
- 代理循环在分发前算出剩余预算，为收尾轮留出固定余量后，通过 `ToolContext.result_budget_tokens` 交给工具。同一轮并行多个调用时按调用数平分。
- `get_wiki_page` 的返回量取 `min(8000 字符, 预算换算出的字符数)`，下限 1500 字符；`next_offset` 照实返回。低于下限时返回错误并指路 `synthesize_knowledge`，而不是返回一个注定被截断的片段。
- 没有预算信息时（测试、无 provider）保持 8000。

这不改变 `fit` 的降级顺序，只是让它更少被触发。

**事件检索按词匹配**。`search_knowledge_base` 对事件用的是 `instr(lower(raw_text), lower(?1))`，把整个查询串当一个子串，多词查询基本匹配不到。改为复用 `query_terms` 做任一词匹配并按命中词数排序，仍取最近的若干条、仍过可记录性过滤。

### 5.3 P2：合成工具 `synthesize_knowledge`

#### 接口

```json
{
  "question": "用户问题的完整复述",
  "sub_questions": ["分析维度 1", "…"],
  "page_slugs": ["已知要读的页"],
  "queries": ["检索词"]
}
```

- `sub_questions` 1–6 条，由聊天模型在工具参数里写出。这就是"问题规划"，不另起一次规划请求。
- `page_slugs` 最多 8 个；`queries` 最多 4 条，缺省时用 `sub_questions`。两者至少给其一。
- 策略为只读（`ToolPolicy::Read`），不延迟加载。描述里写明适用场景（跨页比较、归纳，或通读长页）和成本（会额外发起若干次模型请求）。
- L0 的搜索结果在命中页总字符数超过 12,000 或可引用页不少于 3 张时，附一条确定性提示指向本工具。提示由结果数据触发，不看用户文本。

#### 流程

1. **选页**。`page_slugs` 里的页走显式通道（`citation_for_page_explicit`，与 `get_wiki_page` 一致）；检索出的页走默认通道（孵化闸门生效）。拿不到引用的页不读，列入 `skipped` 并给出原因，模型仍可用 `get_wiki_page` 单独打开。上限 8 页。
2. **分块**。每页按 6000 字符切块，起点对齐 `reading.rs` 的 `CHUNK`，每块向后多带 300 字符的重叠尾巴，避免句子被切断。不超过 6000 字符的页是一块。
3. **选块**。每块对全部子问题的词项（`query_terms`）计数打分，纯本地、确定性。按分数取前 N 块，并保证每个子问题至少有其最高分的一块入选。零分的块不读。
4. **分批提取（map）**。每次子请求放 `clamp(⌊0.7 × input_limit ÷ 单块估算⌋, 1, 3)` 块（单块含重叠尾巴约 6300 字符，估算 14,112 token）：64K 模型 2 块，128K 模型 3 块，未知模型 1 块。子请求**最多 6 次**（D1），所以一次合成最多读 6–18 块，即 3.6–10.8 万字符。
5. **校验与定位**。每条证据的 `quote` 必须是所在块原文的精确子串，否则丢弃并计数。通过的证据在页内定位出 `start_char` / `end_char`。一批的输出无法解析时带错误信息重试一次，与 `reading.rs` 一致。
6. **装配（reduce）**。确定性处理，不调用模型：同页区间重叠的引文合并；按子问题轮转排序以保持各维度均衡；总量上限为 `min(6000 字符, 输入预算的 30%)`，超出的按选块分数从低到高丢弃并计入 `omitted`。真正的综合由聊天模型在收尾轮完成。

提取请求的输出契约：

```json
{"evidence": [
  {"q": 0, "chunk": 1, "kind": "fact|opinion|data|conclusion|limitation|counter",
   "quote": "逐字原文，8–300 字符", "note": "为什么与该子问题相关，≤80 字"}
]}
```

每块最多 6 条。`kind` 和 `note` 是模型的判断，在返回给聊天模型的结果里明确标注为"模型归类，未经校验"；系统只为 `quote` 及其位置作保。

#### 返回

```json
{
  "knowledge_candidates": [ /* 有证据入选的页，KnowledgeCitation，供引用白名单使用 */ ],
  "knowledge_reading": {
    "method/relay": {"coverage": "evidence", "total_chars": 51200,
                     "read_ranges": [[0, 12000], [30000, 36000]]}
  },
  "evidence": [
    {"q": 0, "kind": "limitation", "page_slug": "method/relay",
     "start_char": 31240, "end_char": 31302, "quote": "…", "note": "…"}
  ],
  "coverage": {"pages_read": 3, "chunks_read": 9, "chunks_total": 14,
               "unread": [{"page_slug": "…", "ranges": [[12000, 30000]]}],
               "rejected_quotes": 2, "omitted": 0, "failed_batches": 0},
  "skipped": [{"page_slug": "…", "reason": "来源待复核"}],
  "knowledge_warnings": [ /* 负知识提醒，同搜索工具 */ ]
}
```

`coverage.unread` 是关键：模型和用户都能看到哪些范围没读，回答规范要求在结论依赖未读范围时说明。`knowledge_reading.coverage` 新增 `evidence` 一档，含义是"若干段已校验的逐字摘录，不是全文"，与 11-10 确立的"摘要不足以核对具体要求"规则同级。

#### 接线

- **子请求通道**。`ToolContext` 需要同时携带 provider 和它的名字（`auxiliary_request` 要 `provider_name` 写请求记录）。工作区未提交的改动只加了 `provider`，建议改成一个小结构体 `{ provider, name }`。`auxiliary_request` 目前把阶段写死为 `summarizing`，需要加一个可传阶段标签的变体，合成用 `synthesizing` 并带 `done/total`。**未核实**：Flutter 端对未知阶段字符串的处理。
- **事务**。只读策略下 `dispatch_for_turn` 不开写事务，所以子请求期间不持有写锁。这是硬约束：本工具不得声明为写策略。
- **文本协议**。`candidates_in_context` 在文本协议下按固定前缀识别工具结果，目前只认 `search_knowledge_base` 和 `get_wiki_page`。需要为新工具补中英文两个前缀，否则回落文本协议后证据页不可引用。
- **取消**。`auxiliary_request` 内部已做 `generation::check`；批与批之间再查一次。运行被停止时错误原样上抛，不包装成工具失败。
- **失败**。单批失败（上游暂时错误、两次都解析不了）计入 `failed_batches`，其余批照常返回。全部失败时返回工具错误并指路 `get_wiki_page`。本地预算拒绝时把该批对半拆开重试一次。
- **换 provider 重跑**。外层换 provider 会从同一基底上下文重新执行工具。加一个进程内缓存，键为会话、参数哈希和各页内容哈希，保留 10 分钟，避免重跑时重复付费。不落库。
- **提示词**。新增 `synthesis.policy` 和 `synthesis.fixed_contract`，随下一个内置 Prompt 目录版本发布；`quote` 保持原文语言，`note` 遵循输出语言策略。
- **预算处理对证据的影响**。`prune_old_tool_results` 保留最近 3 个工具结果，合成结果在收尾轮通常在其中。若仍被减半，现有披露尾巴会破坏 JSON，`candidates_in_context` 解析失败后该结果整体不作为引用依据。这是安全方向的失败，但应该避免，所以证据总量按输入预算的 30% 封顶。

#### 可观测

每次合成写一条运行事件，只含计数和耗时：页数、总块数、已读块数、子请求数、保留与拒绝的证据数、失败批数。不写原文。子请求的用量由 `auxiliary_request` 记入每日用量。

### 5.4 轮次

不改 `MAX_TOOL_ROUNDS`。修复类分支（协议回落、空回复、承诺回炉）与工具轮共用这 4 轮，一次回落就少一轮，这是既有风险。建议在 P1 顺手从运行事件里统计带知识工具的运行的 `rounds_used` 分布，有数据后再讨论只读工具轮是否单独计数。

### 5.5 P3：词法索引（FTS5）

#### 现状与必要性

`select_knowledge` 的查询是对每个词项做 `instr(lower(标题||标签||摘要||正文||适用条件), ?)`，按命中词数加权排序后取 80 行。规模评估的合成数据：1 万页约 370 ms，10 万页约 3.9 s；多问法检索最多跑 4 路。个人库在 9 月 30 日是 49 页，这个量级下扫描在毫秒级。

所以 FTS5 现在解决不了任何用户可感知的问题。它的价值在两处：页数上千之后的延迟，以及 BM25 对长短页的归一化（现在的计数排序偏向长页）。按 D3，有数据触发再做。下面是做的时候的方案。

#### 中文分词的四个选项

| 方案 | 行为 | 结论 |
|---|---|---|
| 默认 `unicode61` | 连续的汉字串被当成一个词元，"知识库检索"只能整串匹配 | 不可用 |
| 内置 `trigram` | 任意子串匹配，但查询至少 3 个字符。两字词（现有检索的基本单位）查不到 | 不可用 |
| 自定义分词器（结巴、ICU） | 分词质量最好；需要 unsafe FFI 注册，且每个打开数据库的连接都要注册，否则对 FTS 表的任何操作报"no such tokenizer"，外部工具（`sqlite3`、`stamp-legacy-db.sh`）受影响 | 不采用 |
| **应用层二元组预分词 + `unicode61`** | 写入时把文本转成空格分隔的二元组，查询时用同一函数生成词项 | **采用** |

采用方案的关键性质：现有 `query_terms` 对非 ASCII 词已经是按相邻两字切分。文档侧用同一个切分函数，那么"正文包含某两字子串"与"索引里有该二元组词元"对中日韩文本**完全等价**，召回不变。ASCII 词从子串匹配变成前缀匹配（`"retry"*`），词中间的命中会丢，这是唯一的行为差异，需要在回归里明示。

#### 结构

```sql
CREATE VIRTUAL TABLE wiki_fts USING fts5(
  title, tags, applicable, summary, body,
  content='', contentless_delete=1, tokenize='unicode61'
);
-- rowid 取 wiki_pages.rowid；无内容表只存索引，不重复存正文
CREATE TABLE wiki_search_state (
  page_rowid INTEGER PRIMARY KEY, indexed_hash TEXT NOT NULL
);
CREATE TABLE wiki_search_dirty (page_id TEXT PRIMARY KEY);
```

**维护方式：触发器只标脏，索引在 Rust 里惰性刷新。**

- `wiki_pages` 的 INSERT、DELETE，以及 `title`/`tags`/`summary`/`content_md`/`status` 的 UPDATE，加上 `knowledge_metadata` 的写入，由纯 SQL 触发器往 `wiki_search_dirty` 插一行。页面写路径至少有 6 处（含同步应用和快照导入），触发器一次覆盖，不会漏。
- 触发器不调用任何应用自定义 SQL 函数，所以外部工具改库不会报错，最多是索引暂时落后。
- 刷新在 Rust 里做：取脏行，重新分词，在一个事务里替换 FTS 行并更新 `wiki_search_state`。两个时机：每次检索前同步清掉脏行（保证读到自己刚写的内容），后台 tick 批量清。
- 检索前的同步刷新有上限（例如 50 页）。脏行超过上限时，本次查询**回落到现有的 `instr` 路径**，后台继续追。升级后的第一次搜索不会卡住，旧路径也因此长期保留作为保底。

#### 查询

- 词项生成沿用 `query_terms`（含同义词扩展），拼成 `"词1" OR "词2" …`，ASCII 词加前缀星号。
- FTS 只负责出候选：按 `bm25(wiki_fts, 8, 6, 6, 3, 1)` 取前 200 个 rowid，回表套用参与资格谓词（`sql_participating`）。
- **第一版保留现有的 Rust 重打分和排序**（主题命中 ×4 加正文命中），只替换候选生成。这样排序行为不变，可以直接对比新旧候选集。改用 BM25 排序是之后的独立一步，需要评测集证明不退化。
- 负知识检索（`select_warnings`）是同样的 `instr` 模式，第二步再迁。事件检索另建只增不改的索引（事件不可变，不需要标脏机制）。

#### 迁移与同步

- 迁移建表、建触发器，并把全部页面标脏。回填由后台和首次检索惰性完成。
- 同步快照的覆盖门禁要求每张表要么携带、要么显式排除。FTS5 的影子表（`wiki_fts_data`、`_idx`、`_docsize`、`_config`）在 `sqlite_master` 里是普通表，需要连同 `wiki_fts`、`wiki_search_state`、`wiki_search_dirty` 一起登记为本机派生数据。**未核实**：门禁测试枚举表的具体方式，登记项以它为准。
- 新设备导入快照时页面经 INSERT 落库，触发器自动标脏，索引本机重建。快照版本不变。

#### 前置检查

- 打包的 SQLite 是否启用了 FTS5、版本是否不低于 3.43（`contentless_delete` 的要求）。**未核实**。`rusqlite` 用的是 `bundled` 特性，各平台编的是同一份源码，预期两项都满足，但需要一条测试钉住：`SELECT sqlite_compileoption_used('ENABLE_FTS5')` 和 `sqlite_version()`。
- 这条测试必须先于迁移合入。`CREATE VIRTUAL TABLE` 在缺模块的构建上会让迁移失败，应用无法启动。

#### 验收

- 等价回归：固定语料上，新候选集包含旧候选集（中日韩词项严格相等，ASCII 词项的差异逐条列出并确认可接受）。
- 规模基准：沿用 `synthetic_wiki_scale`，报告 1 万/10 万页的检索耗时、索引体积、单页更新耗时。不预先承诺数字。

### 5.6 P4：估算口径（先量，再决定）

仓库里关于中文 token 率的两处说法互相矛盾：

- `budget.rs` 的注释："真实 deepseek/qwen 中文约 0.6~0.8 **token/字**，取 2.0 保守放大 2.5~3 倍"。
- [估算校准提案](./2026-10-19-token-estimation-calibration.md) §3.1："deepseek/qwen/glm 族中文约 0.6~0.8 **字/token**"，即每字 1.25–1.67 token。

按前者，估算偏高约 3 倍；按后者，约 1.4–1.8 倍。两者对"64K 窗口实际能用多少"的结论差一倍。现有的 usage 锚定只在低估时收紧（系数夹在 1.0–1.5），高估方向从不记录，所以运行时不会自己纠正。

**第一步不需要写代码**。每次请求已经落了两个数：发送前的估算（`request_budget` 事件里的 `input_tokens_after`）和上游返回的真实 `prompt_tokens`（`chat_run_requests`）。按 `request_id` 关联，分模型统计"真实 ÷ 估算"的分布，就能知道偏差是多少、是否稳定。

如果中位数低于约 0.7 且分布集中，再讨论两个方案：

- A：按模型族设中文系数。简单，但依赖模型名前缀，自部署改名的模型落空。
- B：双向锚定。按模型持久化"真实 ÷ 估算"的 p95，样本不少于 20 条才启用，乘 1.1 的余量，系数下限 0.6。上游真实拒绝后的窗口减半重试（R4）继续兜底。

B 修订了"只记低估"的红线，属于 D2，需要明确同意。它的保守性体现在用 p95 加余量，而不是用均值。

这一期与 P0–P3 独立，量的那一步可以随时做。

## 6. 数据模型变更汇总

| 期 | 变更 | 同步归类 |
|---|---|---|
| P0 | 迁移 83：`page_tldr.basis` 列、过期触发器、清空旧行、退避账本 `page_tldr_backoff` | 两张表均登记为本机派生 |
| P1 | 无 | — |
| P2 | 无（证据不落库；缓存在进程内） | — |
| P3 | `wiki_fts` 及影子表、`wiki_search_state`、`wiki_search_dirty`、标脏触发器 | 全部登记为本机派生，快照版本不变 |
| P4 | 方案 B 需要一张按模型的校准表 | 本机运行状态 |

P0–P2 不改公开的 Rust API，不需要重新生成桥接。P2 的阶段标签如果要在界面上显示进度，属于 Dart 侧的小改动。

## 7. 分期与验收

| 期 | 内容 | 工程验收 | 真实模型验收（单列，不以工程通过替代） |
|---|---|---|---|
| P0（已实现） | §5.1 的 6 处修正和 3 处小项 | 失败退避与不饿死；`memo`、孵化、否定页不进候选；页面更新后重建；搜索结果的 `excerpt` 是逐字原文，摘要另放 `generated_summary` 并计入预算；1000 字符以内的页仍是全文；结果含 `hit_offset` | 回填一轮后抽查 TL;DR 是否反映全文而非开头 |
| P1 | 预算前移、事件按词检索（命中定位已随 P0 交付） | 64K 口径下连续读页不触发工具结果减半；预算不足时返回指路错误；多词事件查询有命中 | 原失败问法：模型是否从命中处读而不是从头翻 |
| P2 | `synthesize_knowledge` | 回环测试：5 万字合成页在 64K 口径下 4 轮内完成、无本地拒绝；全部证据是原文子串且区间正确；伪造引文被丢弃并计数；单批失败返回部分结果；停止运行即时生效；文本协议下证据页可引用；写事务不被持有 | 重放两个失败场景；3–5 个真实跨页问题，人工核对证据忠实性、未读范围是否被如实说明、总耗时与用量 |
| P3 | FTS5 词法索引（D3 触发后） | 前置检查测试；等价回归；规模基准；脏行超限时回落旧路径 | 换问法召回不退化 |
| P4 | 估算偏差测量，按 D2 决定是否校准 | 已知超限样本仍被本地拒绝 | 不同中转下无新增上游超限 |

每期完成时更新[提案索引](../../../proposal-backlog.md)和本文状态。

## 8. 红线

1. 生成内容不冒充原文：TL;DR 只以 `generated_summary` 出现、不进 `excerpt`，证据的 `kind`/`note` 标为模型归类，只有通过子串校验的 `quote` 算原文。
2. 工具内的模型请求只走 `auxiliary_request`，不直接调用 provider。
3. 合成工具只读，不持有写事务，不写知识页、不建提案。
4. 读哪些页只由现有资格函数决定；`memo`、孵化页、否定页不因新路径而进入自动处理。
5. 未读范围必须出现在返回里；不允许"读了一部分但当作全部"。
6. 不新增手动后台触发入口；索引和 TL;DR 只由后台 tick 和检索路径维护。
7. 估算口径的任何放松都要有本机实测分布支持，且保留上游拒绝后的重试兜底。

## 9. 后置与被否决的方案

**后置（有数据再做）**

- 证据持久化。先看进程内缓存的命中率；同一页同一类问题被反复合成时再考虑落库，键为页面哈希、块号、子问题哈希。
- 模型参与的二级归并。证据经常超过上限、且丢弃的是有用内容时再加。
- 沿 `knowledge_relations` 做 1 跳扩展选页。
- 子请求并行。需要先确认 provider 对象能否跨线程使用。
- Embedding。触发条件：评测集显示词法检索加模型改写仍有稳定漏召回。

**否决**

- 照搬外部建议的模块树和表清单：与现有 `knowledge_*`、`ai/` 职责重叠，迁移成本没有对应收益。
- 独立的 Query Planning 请求和合成型问题分类器：多一次请求和延迟；分类器的误判代价见 10-20 事故。
- 提高 `MAX_TOOL_ROUNDS` 让模型自己翻完：§1.3 说明容量上放不下。
- 把证据提取放到后台预计算：提取是面向问题的，离线只能做与问题无关的归纳，而这部分 `reading.rs` 已经在做。
- 用阅读树的归纳直接当证据：它是生成摘要，不是原文，也不针对当前问题。只用作 TL;DR 输入和带标注的兜底。

## 10. 风险与未决问题

- **耗时**。子请求串行，6 次约 1–3 分钟，而单轮运行有 10 分钟的墙钟上限。需要阶段进度让用户知道在做什么。
- **成本**。一次合成的真实输入约 4–11 万 token。工具描述要写明，模型才不会在简单问题上调用。
- **选块是词法的**。相关段落与子问题没有共同词时会漏。缓解：子问题由模型撰写，通常包含关键术语；`coverage.unread` 让遗漏可见。
- **固定宽度分块**。表格和代码块可能被切开。300 字符重叠只能缓解句子级的切断。
- **模型不调用工具**。合成工具是否被用取决于模型。L0 的确定性提示是唯一的引导，不强制。
- **`select_knowledge` 的其他调用方**是否受 TL;DR 替换影响（§5.1 小项），未核实。
- **Flutter 对新阶段标签的处理**，未核实。
- **FTS5 的两项前置条件**（编译开关、SQLite 版本）和**快照门禁的枚举方式**，未核实。

## 11. 核实范围

本文的代码事实来自 2026-11-13 阅读源码（`e0823de`、`d06dc2a` 之后，含工作区未提交的 `ToolContext.provider` 改动）。没有运行测试，没有调用模型，没有访问个人数据库。§1.3 的数字由 `budget.rs` 的常量算出，不是实测；§5.5 的 FTS5 行为来自 SQLite 文档的既有知识，实施前需按"前置检查"验证。
