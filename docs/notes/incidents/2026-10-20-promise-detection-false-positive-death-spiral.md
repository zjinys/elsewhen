# 事故档案：规则抽象请求被承诺检测误杀，四轮死亡螺旋

**日期**：2026-10-20  
**Run id**：`2b0ac282-1a48-4763-aa7b-c41ad976ce7e`  
**对话**：「主对话流」（`93196d4f`）  
**涉及模块**：
- `src/ai/conversation/claim_detection.rs`（`future_promise_detected` 词表）
- `src/ai/generation/draft.rs`（DraftBuffer 封锁）
- `src/ai/conversation/agent.rs`（回炉/空回复重试/finalizing 逻辑）
- `src/ai/conversation/continuity.rs`（`progress_fallback` 诚实失败文案）

---

## 一、现象

用户在对话中要求「这个规则不够抽象，或者不够普适」——一个纯生成任务（把刚讨论出的立项规则进一步抽象化）。AI 经过 4 轮内部循环后，交付的不是规则正文，而是 `progress_fallback` 诚实失败文案：

> 关于你刚才的要求「这个规则不够抽象，或者不够普适」，这一轮还没有完成：连续几轮都没有产出可用内容，本轮到此为止；你的问题还挂着，说「继续」或换个问法即可，不需要重新输入原话。另外，我之前说过要接着处理这件事，这次同样没有做到；它还挂着，不会被当作已完成。

用户感知：「记忆/上下文理解有问题」。实际诊断：上下文充足（20K/58K），模型理解完全在线，是**收敛层把一个好回答活活掐死了**。

## 二、四轮死亡螺旋

| 轮 | completion tokens | 模型实际输出 | 系统判定 |
|---|---|---|---|
| 0 | 383 | `list_rules` 工具调用（合理：先看现有规则避免重复） | 执行工具 |
| 1 | 334 | 「你说得对，上一条还停留在车价、月收入这类具体参照上，本质上只是讲『定价要看参照』的一个版本。**那就不绕了，规则直接给你。**」 | `future_promise_detected` 命中「**这就**」→ 回炉 |
| 2 | 317 | 「先说状态：我刚查了规则库（list_rules），现有的 5 条里没有跟立项、定价沾边的，所以这次确实是新内容。」「审阅卡里那两条更早的规则提议还挂着，等你处理。」 | 每句含「**工具**」→ DraftBuffer 封锁 → clean 为空 → 空回复重试 |
| 3 | 355 | finalizing 轮（FINAL_ANSWER_NUDGE 已注入） | clean 仍空 → `progress_fallback` |

### 根因 1：`future_promise_detected` 词表误杀

「**那就不绕了，规则直接给你**」被「我**这就**」词表命中。这是修辞性承接——模型下一句就要给规则正文了。守卫词表（「请确认/具体如下/你看这样」）全是**结尾**措辞，挡不住「正文还没说完就被整段判定」的情况。

词表的设计假设是「命中承诺词 = 整段回复只有预告」。但真实世界里，「那就不绕了，规则直接给你」是**预告+交付的混合体**——正文就在后文，只是恰好 383 个 completion token 在「规则直接给你。」这句被截断。

### 根因 2：DraftBuffer 对「工具」一词过敏

round 2 模型老老实实交代行为（「我刚查了规则库」），每句都含「工具/已」类词 → `unsafe_sentence` 全封锁 → `clean` 为空 → 空回复重试。**模型说了实话反而被当成没说话。**

### 根因 3：finalizing 轮也没救回来

round 3 注入了 FINAL_ANSWER_NUDGE + turn_grounding，但模型可能继续提到「工具」或「提议」，clean 仍空，`progress_fallback` 只能给诚实失败。

## 三、为什么用户看到的是「记忆/理解有问题」

实际上 round 2 那两段 text_delta 已经是**有效的进度答复**（承认理解 + 报告规则库现状 + 提醒 pending 提议），只是没走到「规则正文」那步。用户看到的 fallback 文案「连续几轮都没有产出可用内容」完全误导了问题本质——不是模型不理解，是收敛层不允许它说。

## 四、修复方向

修 `future_promise_detected` 词表是打地鼠。结构性修法：

1. **命中承诺词时，检查后文是否还有实质内容**（≥50 字的非承诺正文），有才交付就不回炉
2. **DraftBuffer 的「工具」词表过于宽泛**——「工具」作为日常名词（「我用工具查了一下」）不该触发封锁，只有协议标记（`<tool_call>`、`[工具调用]`）才该封
3. **finalizing 轮 DraftBuffer 应放宽**——finalizing 轮已经注入了 FINAL_ANSWER_NUDGE，模型知道必须交付，此时 DraftBuffer 的过度封锁反而帮倒忙

## 五、证据

```
-- chat_run_requests（4 轮全部 completed）
154736f1|completed|9370|383|7296   -- round 0: list_rules
f8b1bc68|completed|9971|334|9728   -- round 1: 「规则直接给你」被误杀
91b01b2b|completed|10365|317|10112 -- round 2: DraftBuffer 封锁→空
65a566c5|completed|6042|355|1536   -- round 3: finalizing 也空→fallback

-- text_delta（round 1-2 有实质输出但没收敛）
「你说得对，上一条还停留在车价、月收入这类具体参照上…」
「那就不绕了，规则直接给你。」  ← 被「这就」误杀
「先说状态：我刚查了规则库（list_rules）…」  ← 被「工具」封锁
「审阅卡里那两条更早的规则提议还挂着，等你处理。」  ← 被「工具」封锁
```

预算结构：20K input / 58K 限额，远不挤。上下文完整。
