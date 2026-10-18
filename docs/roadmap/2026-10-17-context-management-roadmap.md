# 长会话上下文管理（吸收 AIcove「短期记忆」层）路线图

> 2026-10-17。来源：吸收计划 P0-1（`2026-10-16-aicove-uptake-plan.md` §1）。
> 参考实现：`ref/AIcove/.../docs/04_功能模块规范/记忆系统/README.md`（ADR0038），
> 及对应 Dart 源码（`context_window_policy.dart` / `runtime_context_service.dart` /
> `chat_send_api_runner.dart`）。本 roadmap 是研读源码后的落地设计，不再是转述。

## 背景与决策记录

- **现状缺口**：主对话流的长会话只有一档兜底——`compress_context_for_model`
  （运行时压缩，不落库），每轮重复压缩、细节随手丢、下轮重来。200+ 条长会话
  每轮交 30k+ 输入。
- **两条已付学费的教训写进设计**：
  - `0225b11` 滑窗撤销：历史配额归零 → 记忆静默失效。**retain 永不为 0**，
    且丢弃必须按"整单元可预期"而非随机截断。
  - 慢对话解剖（`37adeec`）：预算三种东西在烧——工具 schema、系统提示、历史。
    历史绝不从砍内容拿；砍表述（摘要）没问题。
- **手动档先于自动档**（顺序即护栏）：第一版只做用户显式触发的翻篇，摘要
  预览可编辑，压缩质量由人工兜底；自动触发线跑顺了再挂。
- **数据先行**：明细排期等明天装机验证的 `prompt_cached_tokens` 报表——
  如果长会话压缓存/爆窗口不构成真实摩擦，本 roadmap 不抢预算。

## AIcove 机制解剖（源码级，直接可抄）

### 预算模型（47 行的 `ContextWindowPolicy`）

```text
window  = min(用户配置, 模型窗口)
trigger = min(window × 80%, window − output_reserve(2048))   // 发送前检查线
retain  = window × 16%                            // 自动压缩保留的近期原文预算
```

- 窗口是**容量不是额度**：输出预留 + 估算误差先扣掉。
- 全链路字符级估算（不跑 tokenizer），误差由 20% 垫子兜。
- `renderedContextTokens` 量**装配后**的全量：消息 + tools schema 一起算。

### 四级梯度降级（发送一轮 + 工具循环内）

| 级 | 动作 | 代价 | 重试上限 |
|---|---|---|---|
| ① | 量长度 < trigger → 直接发 | — | — |
| ② | 截工具结果（保留最近 3 次，旧换占位） | 无损 | — |
| ③ | 自动压缩：闭合单元从旧到新压成摘要 | 有损，落库 | 2 遍（1: 压到 retain；2: 压到 0） |
| ④ | 服务端仍报超长 → force 运行时压缩 | 有损，不落库 | 恰好 1 次 |

工程细节（都有护栏价值）：

- **闭合单元是压缩安全单位**：assistant 回复与其 tool_calls/tool_results 成组
  摘除（孤儿 tool_result 直接 API 报错）；含最新 user 消息、含 system 的单元永不参与。
- **强制恢复也至少压一个最旧单元**——防"全是新消息没得压"死锁。
- **真没得压**就抛明确异常（"没有可安全压缩的历史"），不静默截断。
- **服务端超长识别** = 报错文本字符串表：`context_length_exceeded` /
  `prompt is too long` / `input token limit` / `上下文超限` …（各家网关话术不同）。
- **摘要与边界不配套**（跨设备同步来的、来源被改）→ 现场补整理边界前 24k 原文，
  **失败就不发送**——宁可不发也不发错记忆。

## 落到 Elsewhen 的设计

### 分层与落点

| 层 | AIcove | Elsewhen 对应 |
|---|---|---|
| 短期记忆 = 上下文管理器 | context_manager | `budget.rs` + conversation.rs 装配卡点 |
| 量长度卡点 | 每次请求前 | **现有 `compress_context_for_model` 调用点（conversation.rs:419）** |
| 摘要存储 | `context_summaries` | 迁移 42：`context_summaries`（口径见下） |
| 长期记忆 | memory_items + FTS | **不新建**——wiki/事件/digest 就是我们的长期链路 |
| 压缩产物二次利用 | 摘要只喂回上下文 | **超过原版**：翻篇段落登记成事件 → digest → wiki |

### 数据模型（迁移 42）

```sql
CREATE TABLE context_summaries (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL,
  covered_until_message_id TEXT NOT NULL,  -- 话题边界：此消息（含）之前被摘要覆盖
  kind TEXT NOT NULL,                      -- manual / auto
  summary TEXT NOT NULL,                   -- 预览过/可编辑的摘要正文
  source_message_sha256 TEXT,              -- 覆盖段指纹（配套性检查用）
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_context_summaries_conv ON context_summaries(conversation_id);
```

- 边界不另设消息表列：`covered_until_message_id` 即边界，摘要行即翻篇记录。
- 每个会话可有多条（多次翻篇），请求装配时取**最新一条**作为有效边界。
- 摘要口径（直接抄 AIcove 的措辞进提示词）：**"只继承事实、经历、约定，
  不带旧格式"——压缩的是表述，不是事实**。

### 请求装配规则

1. 有效存在最新摘要 → 历史 = 摘要（作为一条 system/summary 消息注入，尾部、
   **不打断前缀缓存**）+ 边界后原文。
2. 边界前原文照常留在库里，只是不进请求。
3. 前缀缓存友好性优先于 token 极小化：摘要放尾部（与动态背景段同位置）。

### 分发与同步

- `context_summaries` 随整库同步（同 skill_files 思路，零协议改动）。
- AIcove 的教训直接采纳：同步来的摘要可能与本机消息流不配套 → **配套性检查**
  （source_message_sha256 或消息 id 存在性），不配套就触发补整理，不硬带。

## P1 实施记录（本提交，已落地）

源码摸底后的修正：

- **级④已存在**：`streaming.rs` 曾有 400 + `context_length_exceeded` → 砍半 window
  重触装配（trigger 事件 `upstream_context_window_rejection`）。本轮只把识别抽成
  纯函数 `is_context_overflow(status, body)` 并扩表：message 文本表
  （`prompt is too long` / `input token limit` / `上下文超限`…）+
  `error` 为纯字符串的网关形态；仅 400/413/422 判定，5xx 文案不误判。
- **级①②④齐了；级③（摘要）没有**：`fit` 的既有降级是直接删整轮——占位化
  插在它前面。2 个以内的工具结果按"保留最近 3"规则全保留，直落砍轮
  （有测试固化该边界）。
- **占位化**（`prune_old_tool_results`，wire 层）：role=="tool" 消息保留最近 3 次，
  更早的换自明占位文本；消息与 tool_call_id 配对不动（exchange 校验不受影响）；
  幂等（已占位跳过）；无净收益则标记完成落回砍轮（防死循环）。
- **观测**：`BudgetReport.pruned_tool_results`（serde default，向后兼容）——
  明天装机报表可直接看降级构成。
- 测试 +4：占位即足（历史完整保留）、占位不够回落砍轮（+省略提示在位）、
  幂等、超长话术表 2 条。Rust 795 / Flutter 400。
- 报错优于静默、闭合单元（exchange 校验）、retain 概念的等价物（fit 按段裁剪）
  均为既有管道，未动。

## 分期

| 期 | 内容 | 验收 |
|---|---|---|
| P1 底座（无 UI，无 schema） | ContextWindowPolicy（trigger/retain/输出预留）进 budget.rs；装配层工具结果占位化（保留最近 3 次）；isContextOverflow 字符串表 + force 重试 1 次；闭合单元校验（孤儿 tool_result 不可出现） | 装配后 32k 窗口请求在超长时报错前完成降级；工具结果占位节省量在 chat_run_requests 里可观测 |
| P2 手动翻篇 | 迁移 42 + 翻篇 API（摘要预览可编辑）+ UI 入口（聊天页"翻篇"动作）+ 摘要转事件钩子 + 装配跳过边界前原文 | 翻篇后：新请求 token 显著下降；旧事实经知识库仍可被 AI 引用 |
| P3 自动档 | 80% 触发线自动压缩（落库 kind=auto，每话题只留最新一份）+ retain 分级（pass1→16%，pass2→0 但整单元） + 撤销手动翻篇（已写入 wiki 的不回滚） | 200+ 条长会话请求不失败；自动压缩不吞"最新一轮" |
| P4（存疑） | 摘要与边界配套性检查 + 补整理流程；多摘要（多话题）链 | 跨设备同步场景摘要不串味 |

## 红线（review 必检）

1. **retain 永不为 0**（除非 pass-2 救场，但仍按整单元推进）——`0225b11` 铁律。
2. **孤儿 tool_result 不可出现**——闭合单元校验是硬约束，不是优化。
3. **抛错优于静默**——"没有可安全压缩的历史"要作为明确错误浮到用户面前。
4. **压缩的是表述不是事实**——摘要口径进提示词，预览可编辑兜底。
5. **不打断前缀缓存**——摘要注入走尾部，不插在 system 与历史之间。

## 关联

- 吸收计划：`2026-10-16-aicove-uptake-plan.md` §1（本 roadmap 是 §1 的展开）
- AIcove 参考：`ref/AIcove/.../记忆系统/README.md`（ADR0038）、
  `context_window_policy.dart`、`runtime_context_service.dart`、`chat_send_api_runner.dart`
- 前置事实：滑窗撤销 `0225b11`、提速三件套 `37adeec`、前缀缓存观测 `4c83a85`
  ——装机报表数据决定 P2 的动手时点
