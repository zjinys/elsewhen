# Agent Note: 对照 RikkaHub Desktop 聊天层，确定上下文稳定性改造顺序

Status: proposed

## Problem

> **2026-09-30 执行口径更新：** 本文保留为早期技术比较与调查过程，下面原有的 P0/P1 编号和部分缺陷归因不再作为实施依据。当前目标是借鉴成熟产品完善 AI 对话体验，方向与取舍见 [AI 对话体验方案](2026-09-30-ai-chat-experience.md)，任务顺序和验收见 [AI 对话体验优化 Roadmap](../../../roadmap/2026-09-30-ai-chat-experience-roadmap.md)。
>
> 本次源码核查已澄清：工具循环 usage 按请求累加；压缩只改内存、片段分支可达，不永久删除历史；当前预算有整轮工具交换保护；参考实现的 SSE 背压包含断连重同步，快照仅冻结部分 prompt，`compaction_boundary` 不等于技术切点；RikkaHub 有检索和引用能力。上述旧结论保留作调研过程记录，不应再按“现存 bug”照单修复。具体动作确认仍值得优先改进，但它服务于明确、可控的产品交互，而非给两套系统整体打分。

Elsewhen 的聊天层（`src/ai/conversation.rs` 3030 行 + `src/ai/memory.rs` 831 行 + `src/ai/budget.rs` 146 行）在**内容质量**上强于参考实现：检索排序、不可伪造的引用、真实 BPE 分档计数、显式预算与拒绝语义，这些都是 RikkaHub Desktop 完全没有的。

但 RikkaHub 在**上下文稳定性**上有三处设计缺口，成本低、收益直接：

1. 每轮从变动的数据库重算装配面，没有快照，第 N 轮模型究竟看到了什么无法复现。
2. `compress_context` 是删除 + 每条裁到 160 字符，却标注为"（自动压缩的较早对话，仅供回忆）"——标签承诺了摘要，实际是截断，长会话单调退化到裸标记且不可恢复。
3. tool_call / tool_result 配对安全是**偶然**成立的（整段 drain user→user 区间），不是显式不变量。

RikkaHub 对应地有三样东西：会话级字节冻结的上下文快照、LLM 摘要压缩配 `compaction_boundary` 注解、`alignContextStart` 两条配对对齐规则。

关键判断：**RikkaHub 的优势集中在"让已装配的上下文稳定、可复现"，Elsewhen 的优势集中在"装进去的东西本身有来源、可验证"。两者不冲突。** RikkaHub 强在稳定性而内容无出处，Elsewhen 强在出处而每轮重算。可叠加而非二选一。

## Proposal

按"改动量 / 收益"排序，先做不依赖任何前置条件的三条。

### P0 — 把 tool 配对安全从偶然变成不变量

移植 RikkaHub `inference-engine/message-enrichment.ts:195-228` 的两条规则：

- **R1**：当前窗口含已执行 tool → 往前回溯找到对应的纯 call（未执行 tool），把它一起纳入。
- **R2**：当前窗口起始是纯 call → 往前归并到最近的 USER（工具链入口）。

它防的具体故障是孤立 `tool_result` 被 OpenAI / Claude 400，且**因前缀稳定会连续多轮 400**（对方注释原话）。Elsewhen 现在靠 `budget.rs:112` 的整段 drain 恰好安全，但没有对应规则来覆盖"从持久化前缀恢复会话"或"加缓存后截断"这两种场景。

### P1 — 会话级快照冻结

移植 `inference-engine/context-snapshots.ts`（61 行，自包含）。按 `(conversationId, assistantId, 三个开关位打包)` 冻结已装配的 system 段与跨对话背景，有界 LRU（`MAX_SNAPSHOTS = 200`，命中即 touch），唯一失效钩子只在用户手动改设置时调用，**工具写入刻意不失效**。

**冻结的对象是"这一轮用了哪些候选"，不是"这一轮重新查了什么"** —— 所以检索、引用校验、防伪造剥离全部照常工作，缓存照样命中。这是唯一一处两边优势可叠加的地方。

一个改动同时买三样：前缀缓存资格、逐轮 prompt 可复现、装配面可回归测试。

### P1 — LLM 摘要压缩 + 边界注解

`compress_context` 之上增加真正的摘要路径，移植 `conversations/auxiliary.ts:500-602` 的四个要点：

- **half-keep 兜底**：按条数保留对"少而长"的会话不成立（对方内测 bug 原话：20 条超长消息因条数 ≤ 默认保留 32 条被整体划进保留区）。
- 摘要写成 **USER 消息**替换压缩前缀，不塞进 system。
- 尾部打 `compaction_boundary` 注解，阻止后续截断跨越接缝。
- 覆盖前快照 `messages` 引用与长度做审计——压缩是破坏性整数组替换，宁可作废重来不可丢用户数据。

压缩本身必须接受中止信号（对方 `compressConversation(..., signal?)`）。

### P2 — 对话路径的两道 token 网收成一条

见下方「验证结果」。实际只有两道，都在对话路径，单位相同但保护规则不同、互不知情：

| 位置 | 单位 | 触发时机 | 保护对象 | 超预算行为 |
|---|---|---|---|---|
| `memory.rs:412` `compress_context` | token | 静态装配后 | 主 system + 最新 user 轮及其后；后续 system 只计费不裁剪 | 摘要降级（可恢复） |
| `provider.rs:360` / `:612` `fit_request` | token | 完整序列化后 | `role == "system"` 全保 + 当前工具交换 | 整组硬删（不可恢复）→ 只剩 1 轮时硬报错 |

第三道 `bound_model_context`（char 24000）**不在对话路径** —— 只在 `api/mod.rs:696`（每日回顾）与 `:943`（分析）调用，`conversation.rs` 零调用。原先把它算进对话路径是错的。

合并为一条带显式保护集的裁剪链，且让摘要降级优先于硬删。

## 不借的部分

以下 RikkaHub 能力经评估后明确不引入，记录理由以免半年后重议：

- **消息树 / regenerate 保留兄弟分支**（`MessageNode.selectIndex`、`truncateConversationForRegenerate`）。Elsewhen 是个人认知系统不是多分支聊天客户端，这套为"用户反复调教同一 prompt"设计，不匹配。
- **工具循环上限提到 256**。Elsewhen 的 4 轮配 `FINAL_ANSWER_NUDGE` 是收敛设计不是遗漏，改大会让"过度承诺"那类 bug 回来。
- **多厂商编码层**（一个 enriched tree 四个 encoder）。Elsewhen 单 OpenAI 兼容端点是刻意的收窄。

> 撤销：原「传输层无关」一节把 Elsewhen 的轮次边界确认门记为「刻意产品选择（配合 CLAUDE.md 的『AI 只提议，核心裁决』），代价是用户必须多说一轮」。按原则 5（副作用必须绑定单一意图）重审后判定为误判——该设计用关键词匹配 + 全量 pending 循环执行，构成安全缺陷而非风格差异。详见下方「追加调查」P-1。**「不借的消息树 / 256 轮」两条排除理由仍然成立**，但「不借 in-loop 审批门」这条撤销。

## 传输层无关、当前就能抄的三条

流式化尚未启动，但以下三条与传输无关，可独立先行：

1. **`TokenUsage` 合并语义**：新值 > 0 才覆盖，否则保留旧值。当前 `provider.rs:408` 与 `conversation.rs:1653` 是 `#[derive(Default)]` 整体覆盖，在 4 轮工具循环中会把前几轮已知的值清零。需先验证再修。
2. **压缩可中止 + 边界幂等**：加 LLM 摘要压缩时必须同时上，否则会出现"压缩被 kill 但边界标记已打，下次重复压"。
3. **增量落库 + 退出全链 flush**：现在整个生成只有 `conversation.rs:428` 一个写库点且跑在循环之后，崩溃丢整条回复。不需要流式也能先做——至少每个工具轮结束后落盘。

## 验证结果：`compress_context` 位置问题（2026-09-30 实测）

上面的 P2 曾把问题描述为「三道网互不知情 + 压缩看不到最终 payload」。逐行核对后**归因需要修正**——主预算闸门没有失效，缺陷是降级发生在错误的层。

### 事实 1 — 覆盖完整 payload 的闸门是 `fit_request`，约束未被绕过

`memory.rs:401-415`（`SlidingWindowMemory::prepare_context`）内部顺序：

```rust
context.insert(0, build_system_prompt(...)?);                                  // :408
if let Some(background) = recent_background(...)? { context.insert(1, ...); }   // :409
compress_context(&mut context, self.max_tokens);                                // :412 只见 [system, background, 历史]
```

返回后 `conversation.rs` 再 push 8 条 system（`:152` feedback、`:158` rules、`:167` executed writes、`:220` direct_query、`:237` wiki page、`:264` citation、`:268` candidates、`:280` followup）。而 `fit_request` 在 `provider.rs:360` / `:612` 序列化完整请求时才执行——**它看到的是完整 payload，主预算约束一直生效**。

### 事实 2 — `compress_context` 几乎总是提前 return，摘要降级实际不可达

它只对静态部分求和（`memory.rs:304`），未超预算即 `return`（`:305`）。默认配置走 SlidingWindow，预算 = `window - (output + window/20)`（`conversation.rs:130-132`），65536 窗口约 58k token——静态部分极少触顶。

真正超预算时干活的是 `fit_request`，而它的降级是**硬删**：整组 `drain` user→user 区间（`budget.rs:113`），只 splice 回 `role == "system"` 的消息（`:114`），插一句 `较早对话因请求预算已省略`（`:117`）；删到只剩 1 个 user 轮次则 `Err`（`:107-108`）。

**净效果：对话路径只有硬删降级，永远走不到 `compress_context` 的摘要路径。** 长会话单调退化为裸标记——但原因不是摘要能力不足，是它被放在了够不着的位置。

### 事实 3 — 对话路径没有 char 网兜底

`bound_model_context`（char 24000）只在 `api/mod.rs:696`（每日回顾）与 `:943`（分析）调用，`conversation.rs` 零调用。

### 事实 4 — wiki page body 无上限，是最大溢出源

`conversation.rs:237-255` 把 `page.content_md` **完整**塞进 system，无截断、无长度检查。一篇长知识页可把 `fit_request` 逼到只剩 1 轮 → 硬报错。

两道网都救不了：它们只保 `role == "system"` 与 user 轮次之间的组（`budget.rs:114`），**wiki page 那条 system 永远计费、从不裁剪**。`compress_context` 有同样策略且是刻意的（`memory.rs:338-343` 注释 "Never truncate source evidence"），代价是长页面无兜底。

### 修复顺序（按杠杆）

| 优先级 | 动作 | 改动量 |
|---|---|---|
| P0 | `compress_context` 移到 `conversation.rs:281` 之后 | 非纯一行移动：`max_tokens` 藏在 `config.memory_type` 里，需一并带出 `prepare_context` |
| P0 | 给 wiki page body 加上限，超出部分保留但标注截断 | 独立小改，先做这个 |
| P1 | 两道 token 网合并，摘要降级优先于硬删 | — |

### 修正记录

本节初稿曾称「三道网单位不同、互不知情」并把 `bound_model_context` 算进对话路径。核对 `api/mod.rs:696` / `:943` 与 `conversation.rs` 后确认第三道不在对话路径，P2 表格已同步改正。**主预算闸门未失效**这一条也与初稿判断相反，特此留痕。

## Alternatives considered

### Why not 整体移植 RikkaHub 的聊天层？

它栈不同（Tauri + Bun + React vs Rust + Flutter + FFI），且核心能力方向与 Elsewhen 相反：它无检索无引用，Elsewhen 无流式无中止。整体移植会同时丢掉 Elsewhen 已有的不可伪造引用与真实 BPE 计数。只取稳定性设计。

### Why not 先做流式化？

流式确实是最大缺口（`provider.rs:596` 显式 `stream: false`，UI 侧 `conversation_provider.dart:136` 单个阻塞 `Future<String>`），但它改动面大、会连带签名与渲染形态。**追加调查后此判断需要修正**：P-1（关键词批量放行写操作）已在造成不可逆写入且用户无从察觉，其优先级高于流式。先做低杠杆项止血更划算——但"低杠杆"里最紧急的是 P-1，不是流式。

### Why not 照抄 RikkaHub 的动态模型目录查询？

它在实时查 models.dev 上吃过大亏——第三方数据漂移导致构建失败，最终退回"带日期的快照 + 一手文档证据"。Elsewhen 现在用硬编码窗口常数本质上在做同样的事，缺的只是把出处和日期写下来。抄结构时不要把这个教训一起丢掉。

## Acceptance criteria

- `compress_context` 不再把截断结果标注为"自动压缩"，或摘要路径接管该标签。
- 第 N 轮实际送出的 system 段可复现：给定同一 `(conversation, flags)` 与被冻结的快照，重放得到逐字节相同的 system 段。
- 存在一条测试锁住"裁剪后不存在孤立 tool_result"——从持久化前缀恢复会话的场景必须覆盖，而不仅是 `fit_request` 的整段 drain。
- 三个裁剪上界合并为一条，或至少共享同一份显式保护对象清单。
- `TokenUsage` 在多轮工具循环中不丢前轮已知字段，有测试锁定。

## Risks

- **快照会引入陈旧数据面**：记忆写入后本轮不再反映。缓解手段是失效钩子只在用户显式动作时触发（工具写入不失效），代价是工具写入的记忆要等下一轮才进上下文。这个取舍需要产品侧确认——若用户期望"AI 刚存的东西立刻影响本轮"，则该机制不适用。
- **前缀缓存收益依赖 provider**：若走便宜中转而 OpenAI / Claude 缓存不生效，该项收益归零。但"逐轮可复现"与"装配面可回归测试"与 provider 无关，仍成立。不要把缓存当唯一理由。
- **LLM 摘要压缩引入新的失败模式**：摘要本身可能失真或丢失关键事实，且是破坏性写入。必须有 `compaction_boundary` 注解 + 覆盖前审计，且摘要请求可中止。
- **工具配对规则可能过冲**：R1/R2 都只向前扩展窗口，需确认在极端情况下不会把窗口撑破上限（RikkaHub 用 `visited` 集合与"索引只向下移"约束，移植时必须一并带上）。
- **wiki page body 截断会与"不裁剪证据"的既有策略冲突**：两道网都刻意选择"永不裁剪 source evidence"（`memory.rs:338-343`），给 wiki page 加上限等于在这条策略上开一个口子。需确认截断标注足够让模型知道内容不完整，否则会引入"模型引用了被截断的片段却以为看到了全文"的新错误模式。
- **把 `compress_context` 移出 `prepare_context` 会改变 trait 语义**：该函数当前对 `MemoryProvider` 的两个实现语义不一致——`SimpleMemory::prepare_context`（`memory.rs:247-267`）根本不调用 `compress_context`。移出后需明确 `max_tokens` 由谁持有，否则 `Simple` 路径的裁剪行为会静默改变。
- **参考仓库不可用**：`ref/` 在 `.gitignore` 中且非 submodule，本 Note 不含指向它的相对链接；对照结论一旦采纳，需把具体规则与行号固化进代码注释，否则半年后无从复核。

---

## 追加调查：按 AI 聊天系统基本原则评估（2026-09-30）

前两轮都是"缺什么、补什么"视角。本轮反过来——先立几条不可让步的原则，再看两个系统各自违反了哪些。结论：**Elsewhen 在写路径上有一条比 RikkaHub 严重得多的原则性缺陷，且方向与前几轮所有结论相反。**

### 采用的原则

1. **用户的话必须完整送达模型** —— 当前轮不得被静默截断
2. **模型说的话不得被伪造** —— 用户看到的即模型生成的
3. **状态必须可恢复** —— 崩溃/重开不丢不损坏
4. **停止必须立即生效** —— 用户永远能打断
5. **副作用必须可见且绑定单一意图** —— 不能由模糊输入触发
6. **引用必须可验证** —— 可溯源
7. **成本与延迟必须可控** —— 流式而非阻塞
8. **错误必须诚实** —— 不静默降级

### 违规对照

| 原则 | RikkaHub | Elsewhen |
|---|---|---|
| 1 完整送达 | 局部违规（见下） | 合规 |
| 2 不伪造 | 合规 | **严重违规（见 P-1）** |
| 3 可恢复 | 合规（200ms 脏标记 + 退出全链） | **违规（见 P-2）** |
| 4 停止即生效 | 合规（真 AbortController） | **违规（见 P-3）** |
| 5 副作用绑定意图 | 合规（见下） | **严重违规（见 P-1）** |
| 6 引用可验证 | 不适用（无引用层） | 合规（`knowledge.rs:439-468` 剥除未知 slug） |
| 7 延迟可控 | 合规（33ms 增量 + 背压） | **违规（`stream: false`）** |
| 8 错误诚实 | 合规 | 合规（`progress_fallback`、`ToolResultMsg.success`） |

Elsewhen 在 8 条里违规 5 条，但**性质完全不同**：第 1、6、8 条它是全场最好，违规集中在"写操作"与"过程可见性"两类。

### P-1 — 副作用由模糊关键词触发，且跨动作无差别（最严重）

`conversation.rs:1685-1747` 的 `is_confirmation` 词表含 31 个词，其中：

```
"记", "加", "存", "保存", "行", "嗯", "是", "就这样", "确认"
```

匹配规则有三层放宽：整句精确匹配（`:1723`）、确认词后跟 ≤2 个语气字且整句 ≤5 字（`:1731-1741`）、**确认词 ≥2 字且整句 ≤6 字的前缀匹配（`:1743-1746`）**。

触发点在 `conversation.rs:1843-1868`：

```rust
let confirmed = matches!(&last_user, Some(m) if is_confirmation(&m.content));
for pa in &pendings {                    // :1848
    if confirmed {
        match execute_pending_action(store, pa) {   // :1850  逐个执行全部 pending
```

三个问题叠加：

**（a）意图与动作无绑定。** `pending_actions_for_conversation` 返回该会话**全部**待确认动作，循环逐个执行。用户对 A 动作说"好"，B、C 动作一并执行。若上一轮已积累多个 pending，一次"好"批量放行。

**（b）无时效窗口。** `last_user` 取的是该会话**最后一条** user 消息，不检查它是否是对上一轮提议的回应。隔一天回来输入"嗯"，昨天积压的三个写操作立即全部执行。

**（c）"好"作为默认答案。** 用户看到提议后最自然的回应就是"好"——而这恰好是"确认全部待办"的通用触发器。原则 5 要求副作用绑定**单一意图**，此设计恰好相反。

对照组 RikkaHub 的做法（`api/handlers/conversations.ts:776-782`）：

```ts
if (part.type !== "tool" || part.toolCallId !== body.toolCallId) return part;
```

批准**必须携带具体 `toolCallId`**，精确匹配单个工具 part。点哪张卡批哪次，无模糊通道、无批量语义、无跨轮残留。同一套代码还处理了孤儿审批（`:801-804`）与副作用来源（`:811` 按引擎 `resumeSemantics` 决定是否重触发，不旁路推断）。

**这条不借是错的。前几轮把它记为"产品选择不匹配"，属于误判——它不是风格差异，是安全模型差异。**

### P-2 — 生成中途崩溃丢失整条回复

`conversation.rs:428` 是整个生成过程唯一的写库点，跑在循环之后：

```rust
// Save reply to database (no parent_message_id for AI replies in main thread)
store.send_message(conversation_id, "assistant", &content, None)?;
```

工具副作用（事件、待确认动作）过程中已落库，但**对话状态零持久化**。崩了就是：用户输入在库（UI 先写），AI 回复、工具结果、轮次状态全无。用户重开会看到一个自己问过但完全没有回答的问题。

RikkaHub 是 200ms 脏标记增量 flush + 退出时对所有生成中会话做全量 reconcile（`server.ts:453-459`）+ `checkpointConversationsDb` WAL TRUNCATE + 干净退出标记。

工具轮已经跑完 3 轮才崩的概率不高，但**这是长上下文 + 多轮工具调用下的常态**，不是边缘情况。

### P-3 — 无法中止，用户被锁在转圈里

`provider.rs:596` 显式 `stream: false`；`conversation_provider.dart:136` 单个阻塞 `Future<String>`。全库无 `AbortController` / `CancelToken`。

违反原则 4。叠加后果：4 轮工具循环 + 58k 上下文，用户可能等数十分钟且**无任何手段取消**。上游黑洞（`reqwest` 连接建立后不返回数据）时 `.timeout()` 只覆盖总时长，但用户仍无中途退出路径。

RikkaHub 有 `STREAM_IDLE_TIMEOUT_MS = 120_000` 看门狗 + 真 `AbortController` + `POST .../stop`。

### P-4 — RikkaHub 侧的局部违规：增量帧丢内容

`api/sse.ts:258-291` 双轨帧设计本身正确（能增量发 `text_delta`，否则退全量 `node_update`），但 `:137` 的背压策略是直接丢帧：

```ts
const MAX_SSE_BACKLOG_FRAMES = 256;
if ((client.desiredSize ?? 0) <= -MAX_SSE_BACKLOG_FRAMES) { /* 丢帧 */ }
```

慢客户端下丢弃的是**增量帧**。若丢的是某个 `text_delta` 而后续关键帧未到达，该段文本永久缺失——违反原则 2。Elsewhen 无流式，暂无此问题；**将来做流式时不能照抄这段**，必须保证丢帧只发生在可重发的关键帧上，或丢弃后触发全量重同步。

### 修复顺序（修正）

P-1 升为最高优先级，理由不是它最容易，而是它**已经在造成不可逆的数据写入**，且用户无从察觉（回复会显示"已保存"，看起来是成功的）。

| 优先级 | 动作 | 违反原则 | 备注 |
|---|---|---|---|
| **P0** | 批准动作绑定具体 action id，禁止关键词批量放行 | 5、2 | 最小修法：确认时携带 id，只执行该条 |
| **P0** | 给 pending 加时效窗口或显式过期 | 5 | 上一轮提议之外的"好"不得触发 |
| P1 | 每个工具轮后增量落库 + 退出 flush | 3 | 参考 `server.ts:434-469` 链路 |
| P1 | 流式 + 可中止 | 4、7 | 改动面最大，但违反两条原则 |
| P2 | `compress_context` 位置（见上节） | 1（间接） | 降级质量而非正确性 |
| P2 | wiki page body 加上限 | 1 | 长页面可逼到硬报错 |

### 修正记录（本节）

前两轮把 RikkaHub 的 in-loop 审批门记为"Elsewhen 是刻意产品选择，不匹配"。按原则 5 重审后判定为**误判**：Elsewhen 现状不是"另一种设计"，是"模糊输入可触发无差别副作用"。`is_confirmation` 的三层放宽匹配 + 全量 pending 循环执行，两者叠加构成原则性缺陷。本节已把它提为 P0，并撤销「Why not 借审批门」这条排除理由的原有理由。
