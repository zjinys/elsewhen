# 事故档案：主对话流摘要管道永久失败 → AI「忘记」三分钟前的承诺

**日期**：2026-10-19　**状态**：根因已定位到模块级；P0-a 已落地（真实错误入事件，含脱敏截断）、知识健康巡检已落地（FRB `knowledge_health`，纯 Rust 无 UI）；P0-b 已落地（2026-10-20，`87d4931`：fit 砍轮下限 turns<=2 + preselection 最近交换硬保留，同一口径）；P0-c 已落地（`generation.rs::honest_summary_degradation`：completed run 查本 run 的 `summary_unavailable` 事件，给最终回复加确定性系统标注前缀，不走 failure_notice 避免污染 P1-A 指纹）；P1 归并根节点修复未动，失败点待 P0-a detail 积累后定位
**涉及代码**：`src/ai/summary.rs`（apply_summary / Builder::build）、`src/ai/memory.rs`（preselection_earlier_turn）、`src/ai/budget.rs`（whole_earlier_turn 裁剪）
**关联事故**：与 [2026-10-18 顶包事故](2026-10-18-intent-bypass-progress-fallback.md) 同一会话（93196d4f「主对话流」）的预算问题家族，但触发机制不同——那次是 run 失败后 progress_fallback 顶包，这次是摘要失败后静默砍历史。

## 一、现象时间线（主对话流 93196d4f，UTC）

| 时间 | 谁 | 内容 |
|---|---|---|
| 01:26:40 | user | 「这个待办怎么建？如何建？建什么？」 |
| 01:27:56 | assistant | 正常回答，结尾明确承诺：「说『建』，我就把这三条提交成待确认草稿」 |
| 01:29:42 | user | 「建」 |
| 01:30:56 | assistant | ❌「前面的对话内容我这边没有留存，看不到你原本要『建』的是什么——是要建一条待办、建知识页，还是建联系人或项目记录？」 |

用户体感：AI 让自己回复「建」，回复之后 AI 又不认了。

## 二、物证（chat_runs / chat_run_events，桌面本地库）

出事 run：`a6f4729c-7a26-41fe-9974-2cf97b4007ca`（status=completed，request_count=7）。

1. **会话规模**：340 条消息，完整历史约 115K tokens；deepseek-v4.1-flash 输入上限 58164（`context_window=65536`）。
2. **摘要从未成功**：该会话 `summary_ready` 事件 **0 次**；`summary_unavailable` 每轮都有。最近 8 轮的失败开销（requests/cache_hits）：
   - 01:29（出事轮）5/87、01:26 7/81、01:23 16/65、01:21 9/57、10-18 12:33 10/48、10-18 08:08 16/32、更早两轮 16/16、16/0。
   - 即：缓存节点在持续累积（250 个 `chat_summary_nodes` 行），但**顶层归并节点永远建不出来**，每轮白白烧 5–16 次模型请求（出事轮约 1.4 万 prompt tokens 作废）。
3. **失败后的降级路径**（`summary.rs:446-456`）：`Err(_)` 分支往上下文注入 system 免责声明「较早历史的派生摘要尚未完成或已失效…不得声称已完整记住全部历史」（:449），事件只记 `reason:"incomplete_or_invalid_basis"`，**真实错误被丢弃**。
4. **主请求预算装配**（run 事件 33/37 的 `request_budget` 报告）：
   - `preliminary_omissions`：332 条历史消息（115198 tokens）在选材阶段整轮排除（`preselection_earlier_turn`）；context 里只剩最近 7 条历史消息。
   - `tools_tokens: 40039`——工具 schema 吃掉了 65536 窗口的 2/3，只剩约 15K 给全部 system 段 + 历史。
   - 第一次主请求（事件 34，publish_draft 前的 attempt）：segment 10–16 保留，但模型只输出了 67 tokens 后 `draft_reset`（怀疑返回了 tool_call，未坐实）。
   - 第二次主请求（事件 37–39）：segment 10–16 全部打上 `omission_reason:"earlier_turn"`、`tokens_after:0`——**最近 6 条历史（含 01:27:56 那句承诺和「建」之前的全部上下文）被整轮裁掉**。模型最终只看到：stable_setup(11K) + 若干 system 段 + 免责声明 + segment 17（当前输入「建」，43 tokens）。
   - 「前面的对话内容我这边没有留存」正是 :449 那句免责声明的转述。

## 三、根因分解（按修复优先级）

### P0-a 与健康巡检（2026-10-19 晚已落地）
- **P0-a**：`summary.rs` 的 `Err(_)` → 捕获错误，`to_string()` 截 500 字符记入 `summary_unavailable` 事件的 `detail` 字段（归并/校验错误是结构性信息，不含用户原文）；下次复现即可拿到真实失败点（P1 的前提）。测试：`health_report_counts_reflect_pipeline_state` 断言 `last_summary_error` 能读到 detail。
- **健康巡检**（链路八个环节的仪表盘，纯查询）：`knowledge.rs::health_report` + FRB `knowledge_health(days_lookback)`。覆盖：编译（建议 pending/accepted/rejected、后台任务成功/失败）、库存（活跃/归档/无摘要降级面/stale/从未露面）、投递采用（knowledge_mounted 事件数与去重页数、usage 记录、被引页数）、增强通道（search/get_wiki_page 调用、summary_ready/unavailable、最近一次真实错误）。投递留痕：确定性摘要挂载新增 `knowledge_mounted` 事件（`relevant_knowledge_mount`，run 身份未就绪时自然空操作）。缺 instrument 的环节后续补事件即可，SQL 已聚合。

### 根因 A（体验杀手）：降级路径不区分「老历史」和「最近几轮」
预算紧张时裁剪策略把 segment 10–16（最近 3 轮对话原文）与 332 条老历史同等对待，按 `whole_earlier_turn` 从新到旧砍。摘要失败 + 40K 工具 schema 挤压 → 连 AI 三分钟前自己的承诺都保不住。
**正确行为**：最近 N 轮（≥2 轮 user/assistant 对）原文必须是不可裁剪的 Required 段；宁可砍更老的历史、砍 stable_setup 的可选部分，也不能砍刚说完的话。这是直接消除用户体感的改动，风险最小。

### 根因 B（系统性 bug）：摘要归并确定性失败且原因被吞
- 叶子/中间节点都在正常提交（缓存命中从 0 涨到 87），说明 `Builder::leaf` / 低层 `merge` 是好的；失败发生在 `build()` 的归并收敛阶段或收尾校验（`merge` 根节点 / `validate_history_spans` / 摘要 allowance 检查 / `NODE_TOKEN_LIMIT`）。
- `summary.rs:446` 的 `Err(_)` 把真实错误丢弃，事件只有笼统的 `incomplete_or_invalid_basis`，导致这个 bug 无法从日志自愈定位、且每一轮重复烧钱。
- 修法：失败分支把 `error.to_string()`（脱敏后）记进 `summary_unavailable` 事件；然后重放一轮定位确切校验点。注意 MAX_REQUESTS=16 在多轮里被打满（16/16），说明归并树规模已逼近上限，失败点可能随消息数增长而固化。

### 根因 C（放大器）：工具 schema 占 40K tokens
`tools_tokens: 40039` 使 58K 可用窗口常年只剩 ~15K，任何长对话都会频繁撞上根因 A/B。需要按需加载（工具检索/分组懒加载）或精简描述。这与 10-18 事故的「预算前移」修复方向一致，但那一轮修的是补充材料挂载，没动工具 schema。

6. **档案外进展（2026-10-19 晚，知识投递侧）**：与 10-18 事故复盘合流后确认「投递侧不设防」是两次事故的共同背景板，确定性摘要挂载已先行落地（详见 10-18 档案「投递侧不设防的统一结论」小节）：`knowledge.rs::summary_mount` + `conversation.rs::relevant_knowledge_mount`，AI 摘要、≤3 页、3000 字符封顶、OptionalBackground 可让路段。注意它不改变本档案根因 A/B/C 中的任何一个：摘要失败时最近轮仍会被裁（P0-b 的 Required 标记仍未做），归并失败错误仍被吞（P0-a 未做）。

## 四、复现方法

1. 桌面启动 app，用同一个「主对话流」（340 条消息）。
2. 任意发一句话触发新 run。
3. 查 `chat_run_events`：`SELECT kind, payload_json FROM chat_run_events WHERE run_id=<新run> AND kind IN ('summary_unavailable','summary_ready')` —— 预期仍然 `summary_unavailable`，100% 复现。
4. 补日志后（根因 B 修法落地）重放，拿到真实错误。

## 五、修复方向预案（按序评估）

1. **P0（治标，直接消体感）**：
   - a. `apply_summary` 失败分支记录真实错误（根因 B 的日志部分，半天工作量，是定位 B 的前提）。
   - b. 预算选材保证「最近 N 轮原文」不可裁剪（根因 A）。改 `memory.rs` 的 preselection 与 `budget.rs` 的 `whole_earlier_turn` 裁剪顺序，把最近 2 轮 user/assistant 对标记为 Required。
   - c. 失败时的免责声明文案改为对用户诚实可见的降级回复前缀（类似 10-18 事故 P1-B 的诚实失败消息思路），而不是只在 system 里写一句然后让模型自由发挥——模型把免责声明转述成「没有留存」就是现状。
2. **P1（治本）**：拿到真实错误后修归并根节点失败点；评估 MAX_REQUESTS=16 对 340 条消息会话是否过小；考虑对已积累 250 个缓存节点的会话做一次「离线归并收敛」而不是每轮在线重试。
3. **P2（放大器）**：工具 schema 瘦身/按需加载（40K → 目标 <10K）。
4. **测试**：summary/tests.rs 补「归并收敛失败时最近轮原文保留」与「失败事件含真实错误」两例；budget 测试补「最近 N 轮不可裁剪」。

## 六、用户侧临时绕过

新话题开新对话——短历史不会触发摘要流程，根因 A/B 都不生效。主对话流在修复前会持续出现「AI 忘记刚说过的话」。
