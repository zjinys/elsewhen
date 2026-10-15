# Agent Note: 工具清单预算

Status: implemented

## Problem

R2 给对话加一个新工具（`query_event_stats`，最紧凑形态 ~590B）后，两个旧测试失败：
`请求输入约 27265 token，超过当前输入预算 27034`。

根因是一个此前没人显式看见的架构约束：

1. **预算引擎对未知模型按字节数估 token**（`budget.rs` `text_tokens` 的
   `else { text.len() }` 分支）——1 字节 ≈ 1 token，中文场景实际高估 ~2x，
   这是「宁小勿大」的有意保守。
2. **对话请求每次全量注入所有工具的 JSON schema**（`ToolRegistry::default()`
   24 个工具 ≈ 14KB ≈ 14k 预算字节），占 32k 保守窗预算的一半以上。
3. 输入限额 = `window - output - clamp(window/20, 256, 4096)`
   （`budget.rs` `input_limit`）。32k 窗 + 4096 输出预留 → **27034**。
4. R2 之前的工具清单已经把这个限额吃到只剩 **244B 余量**——任何新工具
   （哪怕 150B）都必然破线。这不是新工具太肥，是清单没有预留扩展位。

## Measured

| 场景 | 无新工具 | R2 上线未压缩 | 极限压缩后 |
|---|---|---|---|
| short（单用户消息 32k 窗） | 26281 | 27265 | 26985 ✅ |
| natural（+规则命中+待确认动作） | ≤27034 | 28221 | 27641 ⚠️ |

natural 场景（fixture 含 pending actions + rule 的重上下文）在新工具压缩后
仍超 333B。判读：该测试的本意是「短回复不得授权」的**行为纪律**，不是预算
装载；把 fixture 模型名换成 `kimi-*`（映射 131k 默认窗）恢复本意，预算装载
护栏由 short 场景单独看守。

## Decision

1. **新工具描述极限压缩**：语义集中一句话、schema 只留 `key + type`（枚举
   值写进 description，不重复 required——schema 里已有）。tool JSON 的每个
   字节都是**全体对话、每次请求**的固定开销。
2. **护栏测试断言用派生式**：`short_new_conversation_fits_conservative_32k_budget`
   里 `assert!(count <= budget::input_limit(32768, 4096))`，而不是硬编码
   快照值——公式微调时自动对齐，清单膨胀仍会显式失败。
3. **行为纪律测试与预算测试分离**：验「模型行为」的 fixture 用大窗模型名，
   不叠加 32k 边际预算约束。
4. **已知限制显式记录**（roadmap R2 债务注记）：unknown-32k 长尾模型 +
   「规则命中/待确认动作」重上下文会保护性拒绝（报错文案自带指引）；
   deepseek/qwen/kimi/glm/claude/gemini 等 64k+ 主流不受影响。

## 给后续加工具的人（必读）

- **加工具前先跑**
  `cargo test --lib ai::conversation::tests::short_new_conversation_fits -- --nocapture`
  （ 它的 eprintln 带 `conservative input=`；基线现值 ~26.9k / 限 27.0k）。
- 新工具描述先按「一句话语义 + 裸 schema」写，跑不进预算再想办法，不要
  先写长文案再压。
- 若破线：先找**等量**的清单瘦身，不要动 `input_limit` 公式或放宽护栏
  ——公式是运行时行为（真实会拒绝请求），不是测试常数。
- 根治方向（单独立项）：对话场景分层工具清单（知识页对话/主对话各取所需，
  roadmap 已暗示「知识页对话不必」）、或 schema 结构复用；工具数继续增长
  时这是唯一的出路。

## Anchors

- 预算公式：`src/ai/budget.rs` `input_limit` / `text_tokens`（未知模型 1B=1t）
- 清单注册：`src/ai/tool/mod.rs` `ToolRegistry::default()`
- 护栏测试：`src/ai/conversation/protocol_tests.rs`
  `short_new_conversation_fits_conservative_32k_budget`
- R2 的极限压缩实例：`src/ai/tool/stats.rs`（description 一句话含全部枚举语义）
