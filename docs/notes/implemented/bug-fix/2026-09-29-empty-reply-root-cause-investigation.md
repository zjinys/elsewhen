# 空回复根因调查（第二轮：记忆层假设的验证与推翻）

**状态**：调查完成，**根因未定**。已落地的修复是上一轮那三处（见
[backlog 的断链节](2026-09-26-deep-review-remaining-todo.md#对话断链空头承诺被当成最终答案2026-09-29已实现)）。
本文档记录的是**在用户反问「这个空回复不是记忆的问题吗」之后**做的第二轮调查，三个结论：

1. 记忆压缩层与空回复**无因果**，已用真实数据排除；
2. 但记忆层**另有一个真缺陷**——4096 的 token 预算形同虚设，请求稳定超支 1/3；
3. 空回复的机制线索指向一条**上一轮修复完全没覆盖**的代码路径。

---

## 触发：一句我上一轮没测过的反问

上一轮我把「上游返回空」写进文档时标的是「未能确证，不猜」——理由是
`debug_eprintln!` 不落盘、工具调用不持久化，无证可查。用户不接受这个「查不出来」，
直接给了个我没排除过的具体假设：**记忆层**。

这个质疑是对的，因为它指向的是**本仓库自己写的、会改写请求体的代码**
（`SlidingWindowMemory` + `compress_context`），而我上一轮压根没量过上下文体积，
只凭「上游返回空」就把它当成了外部问题。

**教训：把机制缺失归给外部之前，先量自己这侧的实际输入。**

---

## 测法：临时测试跑活库副本，不碰活库

`build_system_prompt` 是私有的，且要 `store` + `conversation_id` 才能求值，没法在仓外
量。所以临时往 `src/ai/memory.rs` 的测试模块塞一个测量用例：

- 把活库及其 `-wal` / `-shm` **复制**到临时目录再打开，全程不碰原文件；
- 定位 `note-106bf538` 的会话，构造真实 `prepare_context`，量压缩前后的估算体积；
- 打印完删掉，`git diff` 确认 `memory.rs` 无残留。

```
MEASURE system_prompt chars=7082 est_tokens=3043
MEASURE prepare_context msgs=11 est_before=3820 est_after_compress=3820 msgs_after=11
MEASURE dropped_msgs=0
MEASURE   [system] chars=7082 est=3043
MEASURE   [user] chars=48 est=21
MEASURE   [assistant] chars=264 est=113
MEASURE   [user] chars=15 est=7
MEASURE   [assistant] chars=161 est=73
MEASURE   [user] chars=1891 est=472
MEASURE   [assistant] chars=129 est=57
MEASURE   [user] chars=1 est=0
MEASURE   [assistant] chars=18 est=9
MEASURE   [user] chars=32 est=16
MEASURE   [assistant] chars=18 est=9
```

---

## 结论一：记忆压缩层是清白的

`compress_context(&mut context, 4096)` 判 `est_before = 3820 ≤ 4096`，**直接 return，
一条消息都没裁**（`dropped_msgs = 0`）。

对话本身也极小——最长的一条是用户贴的英文 prompt，1891 字符 ≈ 472 估算 token；
页面正文（`note-106bf538` 的 `content_md`）只有 580 字符。

**压缩层在这场对话里从未生效，因此不可能是空回复的成因。** 这条不是推理，是量出来的。

---

## 结论二：但记忆层确实有个真 bug——4096 的预算形同虚设

量到估算值之后第一件事是去查**真实的** `prompt_tokens`。`token_usage` 表记的是
provider 自己上报的数字，属于地面真值，不需要我估：

| 时间 | 估算 | **provider 实收** | 预算 | 超出 |
|---|---|---|---|---|
| 02:13:32 | ~4300 | **7254** | 4096 | +77% |
| 02:15:31 | 3820 | **5452** | 4096 | **+33%** |
| 02:16:45 | 3820 | **5497** | 4096 | +34% |

根因在 `estimate_tokens`（`memory.rs:269`）：

```rust
ascii / 4 + wide / 2
```

系统提示 7082 字符里绝大多数是中文，真实 tokenizer 下汉字约 1 字 1 token，代码按
2 字 1 token 算——**单是系统提示就少算约 1300**。实测低估比 **1.43×**（5452 / 3820）。

代码注释里承认了「2× 低估是可接受的折中」，所以 1.43× 落在自认容忍范围内。**但后果
是结构性的**：

- 估算永远够不到 4096 → `compress_context` 永远提前 return；
- 每次请求实际都比预算**多带三分之一**的 token；
- 而系统提示单独就占掉估算预算的 **74%**（3043 / 4096），留给真实对话的余量极小。

也就是说，**这个预算是软的，从来没真正约束过任何东西**。它现在的作用仅止于
「上下文大到离谱时兜个底」。

附带查到：全部 5 个 provider 配置的 `max_tokens` **都是 NULL**，所以请求根本不带
输出上限——输出侧也完全没设防。

**这个缺陷与空回复无因果**（超预算不等于返回空），但它是真的，且会放大其他症状。

---

## 结论三：空回复的线索不在「上游返回空」

同一张 `token_usage` 表还记了 `completion_tokens`：

| 时间 | prompt | **completion** | 用户实际看到的 |
|---|---|---|---|
| 02:13:32 | 7254 | **611** | 那句「稍等片刻」（129 字符） |
| 02:15:31 | 5452 | **218** | 「抱歉，模型没有返回内容」（18 字符） |
| 02:16:45 | 5497 | **104** | 同上 |

**`completion_tokens` 是 218 和 104，不是 0。** 而入库的兜底只有 18 字符（≈9 token）。

且代码路径排除了「正文被解析器吃掉」的可能：循环末尾有
`if content.is_empty() { content = parse_tool_call_envelope(&last_raw).0; }`，
`last_raw` 非空就不会走兜底。所以**最终那一轮 `reply.content` 确实是空的**。

合起来就是：**provider 报告生成了约 200 token，但其中没有一行进入 `content`。**
上一轮「上游返回空」的说法太粗——不是「什么都没生成」，是**生成了但没走到 `content`**。

---

## 最吻合的解释（**未证实**）

读完循环代码，`conversation.rs:683` 这一段是唯一能吞掉正文的地方：

```rust
// 原生 tool-calls：回传 assistant(tool_calls) + 工具结果
if !reply.tool_calls.is_empty() {
    protocol = Some(ToolProtocol::Native);
    context.push(ContextMessage::assistant_with_tool_calls(
        reply.tool_calls.clone(),
        reply.reasoning_content.clone(),   // ← reply.content 一个字都没进来
    ));
    for call in &reply.tool_calls { ... dispatch(...) ... }
    continue;                              // ← 只有最后一轮的 content 会返回
}
```

**模型在同一条回复里同时给了正文和 `tool_calls` 时，正文被整个丢弃，只有最后一轮的
`content` 会返回给用户。** 而 `continue` 意味着中间轮次的正文全都进了上下文、用户永远
看不到。

把它和这页的处境拼起来：

- 该页 `area = imported`（素材原文，三层锁），`save_wiki_revision` 走 `revision` 会被
  `tool/mod.rs:1716` 的 `force_derivative` 直接 `bail`；
- `dispatch`（`tool/mod.rs:294`）把它包成 `ToolResultMsg::err` **继续循环**，不中断；
- 于是：工具被拒 → 模型再试 → 再被拒 → 4 轮预算烧完 → `content` 仍空 → 兜底道歉；
- **全程零写入**，所以 `wiki_log` / `wiki_revisions` 干干净净——与第一轮观察完全吻合；
- 200 token 花在反复构造工具参数上，也解释了 `completion_tokens` 为何不为 0。

02:13 那条能正常显示，是因为最后一轮模型放弃了工具、只回文字；而那句「稍等片刻」
是**第 0 轮的正文，被这一段吞了**。

**这条没能证实。** 工具调用不落库，`debug_eprintln!` 也不落盘。同样无法排除的还有：
推理 token 计入 `completion_tokens`、正文落在 provider 响应的其他字段里。

---

## 这暴露了上一轮修复的缺口

上一轮治了三处，**这一处不在其中**：

| 上一轮治的 | 本轮发现的漏网 |
|---|---|
| 空头预告（零工具调用时）被当成答案 | **正文与 `tool_calls` 同回时，正文被吞** |
| 空回复不重试、直接收敛 | 工具持续被拒时，重试仍会被同样地吞 |
| 兜底文案污染上下文 | （这条仍成立） |

更糟的是**修复会失效**：`future_promise_detected` 是在「本轮零工具调用」的前提下才
触发的回炉。模型一旦同回正文 + 工具，承诺检测根本不会被执行——被吞掉的正文里哪怕
写着「稍等片刻」，也没人看。

而且现有手段救不回来：`MAX_EMPTY_RETRIES = 1` 只是把同一条请求原样重发一遍，工具照样
被拒、正文照样被吞。**这类场景需要的是「把本轮正文透给用户」，而不是「再问一次」。**

---

## 为什么查不下去：需要什么才能查清

要证实或推翻上面那条假设，只需要每轮的 `reply.content` 长度和 `tool_calls` 数量——
而这两样**恰好就是 `debug_eprintln!` 已经在打的**（`conversation.rs:676-680`），
只是不落盘、且默认关闭。

`scripts/debug-run.sh`（上一轮做的）就是为此存在的：`ELSEWHEN_DEBUG=1` + `tee` 到
`agent-debug.log`。**端到端仍未验证**，因为它要启第二个应用实例、和你正在用的那个
抢活库。下次重启应用时跑一次即可闭环。

---

## 附带发现：共享工作树把两个 agent 的提交混在了一起

调查过程中撞上一件与 AI 无关但更值得记的事。

另一个 AI agent（codex，分支 `codex/llm-wiki-completion`，worktree 在
`/tmp/elsewhen-llm-wiki`）**与本 agent 共用同一个工作树**。事故实录：

1. `11:59:59` 对方把 `codex/llm-wiki-completion` fast-forward 合并进 `main`（`1817f1f`），
   带来 `migrations/04-knowledge/`，活库 `user_version` 随之从 3 升到 4；
2. 我在 12:2x 用 `git mv` 把 5 个 `.sh` 移进 `scripts/`；
3. 对方在 `12:31:32` 提交 `e25ec25`（`test(wiki): 对齐 v4 迁移断言并完成闭环验收记录`），
   **用的是 `git add -A` 式全量暂存，把我这 5 个重命名一并卷了进去**：

```
e25ec25  test(wiki): 对齐 v4 迁移断言并完成闭环验收记录
  R100  elsewhen-capture.sh → scripts/elsewhen-capture.sh   ← 本 agent 的
  R100  elsewhen.sh         → scripts/elsewhen.sh           ← 本 agent 的
  R100  flutter-wrapper.sh  → scripts/flutter-wrapper.sh    ← 本 agent 的
  R100  regen.sh            → scripts/regen.sh              ← 本 agent 的
  R100  test_bridge.sh      → scripts/test_bridge.sh        ← 本 agent 的
  M     src/storage/new_migrations.rs                        ← 对方的
  + 4 个 wiki 文档                                            ← 对方的
```

**提交信息一个字没提文件搬家。**半年后没人查得清这 5 个文件为什么在这条提交里。

本 agent 随后的两次提交改为**逐个显式 `git add` 具体路径**，不再用 `-A`，未再卷入。

**教训**：共享工作树下，`git add -A` / `git commit -a` 是不可控的——它提交的不只是
「我改的」，而是「此刻工作区里全部未提交的东西」。两个 agent 各写各的，最终会被
彼此的提交信息错误归因。

---

## 待办

按优先级：

1. **[P1] 透出被吞的本轮正文**。原生协议分支在 `reply.content` 非空时把它返回给
   用户（与 `tool_calls` 并存），而不是丢弃。**这是本轮调查唯一指向代码缺陷的结论**，
   上一轮的承诺检测与空回复重试都救不了工具被拒的场景。
2. **[P2] 修 `estimate_tokens` 的中文权重**。至少让 `wide` 按 1 字 1 token 估，并把
   预算从「估算 token」改成「估算 token × 安全系数」，否则 4096 永远不生效。
   同时该定一个合理预算值——系统提示单独就吃掉 74%。
3. **[P2] `max_tokens` 全为 NULL**，输出侧无上限。是否要设默认，要先确认各 provider
   对「思考型模型」的计费口径。
4. **[P1] 跑通 `scripts/debug-run.sh` 端到端**，用真实日志证实或推翻「正文被吞 +
   工具被拒烧轮数」这条假设。**在此之前，第 1 项的修法方向是推断而非定论。**
5. **[流程] 共享工作树隔离**。要么给另一个 agent 独立 worktree 并禁止它碰主工作树，
   要么约定提交纪律（禁止 `-A`、按路径显式暂存）。见下节。

其余（页内 AI 对话导出、`export_wiki` 孤儿函数等）见
[backlog](2026-09-26-deep-review-remaining-todo.md)，此处不重复。
