# Agent Note: save_knowledge_draft 来源发现死循环

Status: 待验证

> 2026-10-08：两层修复已落地（见文末「修复落地记录」），待真实模型复现验证后转 completed。
> 验证点：让模型在仅有 list_wiki_pages 列表、未 search/get 的情况下尝试 save_knowledge_draft，
> 观察是否 (a) 不再死循环猜 slug，(b) 被新错误消息/输出提醒引导去 search_knowledge_base 或 get_wiki_page。

## Problem

模型在对话中调用 `save_knowledge_draft` 引用一个知识库页面时，连续失败并最终放弃保存，知识点丢失。

实际日志（`agent-debug.log`）中的完整失败序列：

1. 模型调用 `list_wiki_pages` → 返回纯文本列表（`- anvil（project）：anvil 此前定位`），**不含 slug**
2. 模型猜测 slug 为 `anvil`，调用 `save_knowledge_draft` → 失败："来源 anvil 未提供给当前对话，请先检索或打开后再保存"
3. 模型改用 `save_wiki_revision` → 失败："知识库没有 slug=anvil 的页面"
4. 模型再猜 `project/anvil`，重试 `save_knowledge_draft` → 同样的"未提供"错误
5. 模型放弃，最终回复中丢失该知识点

## Root Cause

证据绑定的安全设计本身是对的：`draft_sources`（[authoring.rs](../../../../src/knowledge/authoring.rs)）要求草稿只能绑定"实际提供给对话的证据"（`available`），防止模型凭空编造来源。

问题出在**错误消息指错了路**：

- 错误消息说"请先检索或打开后再保存"，但没有说明**哪些工具的输出能解锁证据绑定**
- `available` 只有两个来源：
  - `candidates_in_context`：解析 `search_knowledge_base` / `get_wiki_page` 工具返回的 **JSON**（`knowledge_candidates` 字段）
  - `previous_answer_sources`：上一轮 assistant 回答中已绑定的引用
- `list_wiki_pages` 返回的是**纯文本**，不会被 `candidates_in_context` 解析——它不是认识的格式
- 模型最容易想到的检索工具恰好是走不通的那条路，被错误消息引进了死胡同

叠加因素：`list_wiki_pages` 输出只给标题和摘要，**不展示 slug**。模型只能从标题猜，猜了 `anvil` 和 `project/anvil` 两次都失败后，没有信号告诉它"slug 猜对了但工具用错了"vs"slug 本身就不对"。

## Proposed Fix

两层，改动都很小：

### 第一层（治本）：错误消息指明具体工具

`src/knowledge/authoring.rs:71`：

```rust
// 现在
"来源 {slug} 未提供给当前对话，请先检索或打开后再保存"

// 改为
"来源 {slug} 未提供给当前对话，请用 search_knowledge_base 或 get_wiki_page 获取后再保存（list_wiki_pages 的列表不能作为来源）"
```

### 第二层（治标但实用）：`list_wiki_pages` 输出带 slug

```
- anvil（project，slug: project/anvil）：anvil 此前定位
```

模型即使用 `list_wiki_pages` 探路，也能拿到正确 slug 去调 `get_wiki_page` 解锁证据。

## Alternatives

- **让 `list_wiki_pages` 也输出可解析的 JSON 候选**：改动更大，且 `list_wiki_pages` 的定位是"概览"而非"证据检索"，让它承担证据解锁职责会模糊工具分工。不推荐。
- **在系统提示里教模型工具分工**：可以缓解，但错误消息是模型失败时的最后指导，应该自带正确指引。两层修复后提示词层面的补充可选。

## Verification

改完后在真实对话中验证：让模型尝试保存一篇引用现有知识库页面的草稿，观察它是否能在第一次失败后通过 `search_knowledge_base` 或 `get_wiki_page` 成功解锁证据绑定，而不是陷入重试循环。
