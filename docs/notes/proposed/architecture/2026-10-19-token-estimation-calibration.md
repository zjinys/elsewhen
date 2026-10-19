# 非 GPT 模型 token 估算校准（告别 text.len() 字节退化）

- 日期：2026-10-19
- 状态：已实施（2026-10-20；分层估算经 R1 落地，usage 锚定按方案 C 进程内单槽落地——低估值才记录并收紧预算，不落库、重启回落启发式；偏差见 roadmap 实施实录）
 范围：`src/ai/budget.rs` 的 `text_tokens` / `input_tokens` 估算口径；不改请求裁剪策略与 input_limit 公式
- 性质：事故驱动的架构改进
- 关联：[Gitea MCP 工具调用失效与上下文预算熔断事故档案](../../incidents/2026-10-19-mcp-gitea-tool-call-review-and-budget-incident.md)、[MCP 工具延迟加载提案](./2026-10-19-mcp-tools-deferred-injection.md)、[主对话流摘要管道永久失败事故](../../incidents/2026-10-19-summary-pipeline-permanent-failure.md)

## 1. 现象与问题

事故实测：`deepseek-flash`（非 GPT 前缀模型）下，34,204 字节的 Gitea MCP 工具 schema 被 `text_tokens` 记为 **34,204 tokens**；全量 tools 字段被记为 **40,032 tokens**，把 64K 窗口挤到只剩 2K 余量。而按 BPE 经验比率（英文/JSON 约 1 token ≈ 3.5~4 bytes，UTF-8 中文约 1~1.5 字节/字但中文 token 率远低于字节率），这段 schema 在真实分词器下约 **9,000~12,000 tokens**——估算虚高 3~4 倍。

后果不只是这一次的误报熔断：**每一次**预算检查（裁剪决策、`tool_reserve` 预扣、`selection_budget` 历史窗口、摘要管线压缩判定）都在用被放大 3~4 倍的数字做决策，导致：
- 历史对话被过度裁剪（「较早对话已省略」过早出现）；
- 摘要管线被过早触发甚至打满请求上限（见 2026-10-19 摘要事故，64 次摘要请求）；
- 可用上下文被系统性浪费，64K 模型实际只当 ~20K 用。

## 2. 现状代码证据

`src/ai/budget.rs:468-482`：

```rust
pub fn text_tokens(model: &str, text: &str) -> usize {
    if model.starts_with("gpt-4o") || model.starts_with("gpt-5")
        || model.starts_with("o1") || model.starts_with("o3") || model.starts_with("o4") {
        tiktoken_rs::o200k_base_singleton().encode_ordinary(text).len()
    } else if model.starts_with("gpt-4") {
        tiktoken_rs::cl100k_base_singleton().encode_ordinary(text).len()
    } else {
        text.len()   // ⚠️ 非 GPT 模型：UTF-8 字节数 1:1 当 token
    }
}
```

`input_tokens`（`budget.rs:687`）对整个序列化请求（含 messages、tools、协议外衣）调 `text_tokens`，再加每条消息 12 token 的框架余量。

模型名来源：`ai_provider_configs.model`（用户自由填写，如 `deepseek-flash`、`glm-5.3-flash`、`kimi-k3`、`deepseek-v4.1-flash`）——估算器面对的是任意字符串，白名单匹配必然大面积落空。

## 3. 方案

### 3.0 估算锚定真实 usage（外部调研结论，优先级最高的单点改进）

pi（`ref/pi/packages/ai/src/utils/estimate.ts:88-113`）的做法：**找最近一条 assistant 响应里的 `usage` 块（provider 返回的真实 input/output token 数），只对其后的新增消息用字符估算（`len/3.5`）补上**——估算误差被锚定在上次真实值上，不随历史长度累积。Elsewhen 的 `event_analyses`/`chat_usage`（若记录）或 OpenAI 兼容响应的 `usage` 字段可做同样锚定：`estimate_request_tokens = last_usage.input + Σ 之后消息的估算`。即便保留现有字节估算，锚定后误差也只影响最近一两轮新增内容，不会把 212 条历史的误差全部带进预算判定。

opencode 的对照：完全不做本地估算，靠 provider 报错后处理（其模型以 Anthropic/OpenAI 为主，报错文案规整）。pi 则额外维护 ~25 家 provider 的超限报错正则（`utils/overflow.ts`），检测到后压缩重试一次。Elsewhen 可保留本地预检做主防线，同时引入「usage 锚定 + 超限报错正则检测」做双保险——当前 failover 遇到上游真实超限时只换 provider，没有「压缩后重试同 provider」的路径。

### 3.1 分层估算策略（按准确度与依赖成本排序）

| 层 | 适用 | 方法 | 预期误差 |
|---|---|---|---|
| L0 精确 | 已知 tokenizer 的模型族 | 本地分词器精确编码（见 3.2） | <5% |
| L1 族估算 | 能识别族但无本地 tokenizer | 按族校准的字节比率（JSON/代码/中文混合各一比率） | 15~30% |
| L2 兜底 | 完全未知模型 | 保守比率（仍优于 1:1 字节），并允许 provider 配置覆盖 | <50% |

**比率校准基准**（离线测一批代表性样本固化到代码里，附测试断言）：
- JSON/代码（工具 schema、tool_result）：英文为主，~3.5 bytes/token；
- 中文自然语言（用户输入、历史对话）：~1.2~1.5 bytes/token（deepseek/qwen/glm 族中文约 0.6~0.8 字/token）；
- 混合段按内容启发式判别（ASCII 占比阈值）取对应比率。

### 3.2 L0 精确分词的落地路径（可选增强，不阻塞 L1/L2）

- `tiktoken-rs` 已有 o200k/cl100k；deepseek 族实测与 cl100k/o200k 偏差在可接受范围（同 BPE 家族），Qwen 有公开 vocab；
- 落地方式：模型名 → tokenizer 映射表（`deepseek*` → cl100k、`qwen*` → qwen vocab、`glm*` → 比率估算兜底），未知模型走 L1/L2；
- 依赖控制：保持 `tiktoken-rs` 单依赖，不为小语种 vocab 引入大文件；引入前用 `cargo tree` 评估体积。

### 3.3 配置逃生门

`ai_provider_configs` 增加可选列（或复用现有 JSON 配置位）：`token_ratio_override: Option<f64>`，用户在设置页可手动指定 bytes/token 比率——为自部署/网关改名模型（如 `kunyou.asia` 的 `glm-5.3-flash`）提供无需发版的校准手段。UI 上做高级折叠项，默认留空走自动。

### 3.4 不变量与回归保护

- **保守方向不变**：估算宁可偏高不可偏低（偏低会把超限请求真的发给 provider，触发上游 400 与费用浪费）；L1/L2 比率在样本基准上加 10~15% 安全系数；
- **预算报告口径不变**：`BudgetReport.counting` 字段注明实际使用的估算层（如 `family_ratio:json=3.5,text=1.3;safety=1.1`），便于事后审计；
- 现有预算裁剪测试（`budget.rs` 内 `context_overflow_tests` 等）全部保持通过；新增样本基准测试：Gitea 53 工具 schema 样本的估算值须落在 [8K, 16K] 区间而非 40K；
- 不改 `input_limit`/`safety_margin`/`output_reserve` 公式——只校准分子，不动分母。

### 3.5 与 MCP 延迟加载提案的关系

两者独立可并行：MCP 瘦身（40K→~5K）解决**绝对体积**问题，本提案解决**测量精度**问题。即便 MCP 提案先落地，内置工具 + 长历史 + 中文对话场景下字节退化仍会系统性浪费上下文（历史消息里的中文按 3 字节/字计 token），本提案收益独立成立。

## 4. 验收指标

| 指标 | 现状 | 目标 |
|---|---|---|
| Gitea schema 估算（34,204 字节） | 34,204 tokens | 8K~16K |
| 中文历史消息估算（1,000 字 ≈ 3,000 字节） | 3,000 tokens | 1,800~2,400 |
| 事故场景重放（Round 1 总输入） | 60,356（local_rejection） | <45,000（ready） |
| 回归：已知超限样本 | local_rejection | 仍 local_rejection（保守方向不破） |

## 5. 外部参考

| 项目 | 机制 | 代码位置 | 可借鉴点 |
|---|---|---|---|
| pi · 估算 | `CHARS_PER_TOKEN = 3.5` 一刀切字符估算，**锚定最近 assistant usage 真实值**只估尾巴；无模型族区分、无本地分词器依赖 | `packages/ai/src/utils/estimate.ts:20,88` | 3.0 的锚定机制；证明粗粒度比率 + 锚定已够用，L0 精确分词非必需 |
| pi · 超限恢复 | ~25 家 provider 超限报错正则表 → 检测后剔除失败响应、compaction 压缩历史、重试一次（`_overflowRecoveryAttempted` 闸门），再失败如实报错；另有 `shouldCompact(tokens > contextWindow - 16K reserve)` 提前压缩 | `packages/ai/src/utils/overflow.ts`、`coding-agent/src/core/agent-session.ts:3033`、`compaction/compaction.ts:267` | 本地预检之外的事故兜底：Elsewhen 可复用其正则表思路做「上游超限 → 压缩重试同 provider」，而非仅 failover |
| opencode | 不做本地估算与预检，直接发 | — | 反面对照：在模型以 Anthropic/OpenAI 为主时可行；Elsewhen 面向任意 OpenAI 兼容端点+小窗口模型，预检仍必要 |
