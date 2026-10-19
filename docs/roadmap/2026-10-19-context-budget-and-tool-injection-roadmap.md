# 上下文预算与工具注入修复路线图

> 2026-10-19。定位：Gitea MCP 事故（[事故档案](../notes/incidents/2026-10-19-mcp-gitea-tool-call-review-and-budget-incident.md)）
> 暴露的三层结构性问题的统一修复计划——工具 schema 全量注入、token 估算失真、
> 审批后无续跑闭环。前两层的修复方案已立项（见关联提案），第三层随本路线图立项。
> 本路线图是 [2026-10-17 上下文管理路线图](2026-10-17-context-management-roadmap.md)
> 的事故驱动补丁：那条线做「上下文里放什么」，这条线修「上下文有多大、怎么测、
> 工具结果怎么回到对话」。

## 背景与决策记录

- **事故三重因果**（证据见事故档案 §3-§5）：
  1. 未受信 MCP 调用被拦截后向模型返回伪成功 tool_result（`src/ai/tool/mod.rs:716-745`），
     模型据此向用户承诺「确认后就执行」；
  2. 用户确认后 `execute_claimed_proposal`（`src/ai/approval.rs:25-50`）只执行动作写回执，
     **不恢复 agent 循环**——真实工具输出永远回不到对话，承诺无法兑现；
  3. 改 trusted=1 后，53 个 Gitea 工具 schema（34KB）每轮全量注入 `tools` 字段，
     叠加 `text.len()` 字节退化估算（`src/ai/budget.rs:480`），记为 40,032 tokens，
     打爆 deepseek-flash 的 64K 窗口（第二轮必要上下文 60,356 > input_limit 58,164）。
- **外部调研结论**（`ref/opencode`、`ref/pi`，已写入两份提案的外部参考节）：
  - opencode 的 MCP 工具默认 codemode：不进 tools 字段，渲染为 2K token 预算的
    catalog 文本 + 沙箱内 search 兜底；skill 列表放 Instructions 段并做增量 diff 下发；
  - pi 的估算锚定最近 assistant usage 真实值、只估新增尾巴（误差不累积）；
    超限靠 ~25 家 provider 报错正则检测 + 压缩重试一次兜底；
  - 两家都没有本地预算预检——Elsewhen 的预检方向本身是对的（面向任意 OpenAI
    兼容端点 + 小窗口模型），坏的是测量精度和工具注入体积。
- **决策**：保留本地预检作主防线，按「测量 → 体积 → 闭环」顺序修；
  不做 opencode 式 JS 沙箱（execute）——调用面不动，只借 catalog 渲染思路。

## 关联文档

| 文档 | 角色 |
|---|---|
| [事故档案](../notes/incidents/2026-10-19-mcp-gitea-tool-call-review-and-budget-incident.md) | 全部证据（SQL/日志/代码行号）、OpenCode 对比、P0/P1/P2 原始分析 |
| [MCP 工具延迟加载提案](../notes/proposed/architecture/2026-10-19-mcp-tools-deferred-injection.md) | R2 的完整方案（catalog 格式、缓存失效、兼容闸、验收指标） |
| [token 估算校准提案](../notes/proposed/architecture/2026-10-19-token-estimation-calibration.md) | R1 的完整方案（usage 锚定、分层比率、逃生门、回归保护） |

## 分期

| 期 | 内容 | 对应事故条目 | 验收 |
|---|---|---|---|
| **R1 测量校准** | `text_tokens` 分层估算（族分词器/校准比率/保守兜底 + provider 级 ratio 覆盖逃生门）；请求预算估算锚定最近一轮真实 usage、只估新增尾巴 | 事故 P2 | Gitea schema 估算落 [8K,16K]（原 34K）；中文历史 1000 字估 1.8K-2.4K（原 3K）；已知超限样本仍 local_rejection（保守方向不破）；`cargo test` 全绿 |
| **R2 工具瘦身** | MCP 工具接入 defer/enabled 框架：L1 catalog 常驻 enable_tools 描述（签名+首行描述截 120 字符，2K token 预算），L2 按工具全名激活注入；tools/list 缓存 + 配置变更失效；未启用调用回错指路 | 事故 P1 | 事故场景重放：Round 0 tools_tokens < 8,000（原 40,032），Round 1 总输入 < input_limit，正常列出 10 条 issue；未启用调用 MCP 工具回「先 enable_tools」 |
| **R3 审批续跑闭环** | 审阅卡接受后恢复 agent 循环：真实工具输出以 tool_result 注入上下文并触发下一轮生成，模型基于真实结果作答；回执弹窗摘要化（不再 raw JSON 整段糊出）；readOnlyHint 只读 MCP 工具免逐次审批（或会话级允许） | 事故 P0（两处）+ P1 只读条目 | 未受信场景重放：用户确认后，对话里出现模型基于真实 list_issues 结果的回答（而非「已确认请继续」）；回执展示为摘要 + 可展开详情 |
| **R4 超限兜底** | provider 真实超限的识别与恢复：报错正则检测（参考 pi overflow.ts 正则表）→ 压缩历史后重试同 provider 一次，再失败才 failover | 事故 P2 延伸（防估算漏网） | 模拟上游返回超限错误文本时：先 compact-retry 一次，成功则正常作答；二次失败才走 provider 轮换 |

## 依赖与顺序理由

- **R1 先于 R2**：R2 的验收指标（tools_tokens < 8,000）依赖 R1 的测量口径才有意义；
  且 R1 是纯测量层改动，风险最低、可独立发布。
- **R2 独立于 R3**：体积问题和闭环问题互不相阻塞；但 R3 重放验收用的正是 R2 修好的
  场景，建议 R2 先落地。
- **R4 最后**：估算校准 + 工具瘦身后，本地预检的漏网概率大降，R4 是纵深防御而非主线；
  其「压缩历史」能力可复用摘要管线现有机制。
- R3 的「只读工具免审批」依赖 MCP `readOnlyHint` 标注的可靠性——Gitea MCP 的
  `list_issues` 实测未标 readOnlyHint（见事故档案 §3.1 对比表），落地时需按
  server 配置级白名单补充，不能盲信上游标注。

## 红线（实施全程）

- 不绕过 `Store::insert_event()` 提交边界；审批语义（untrusted → pending_action）不变，
  R3 只改「确认之后发生什么」，不改「何时需要确认」；
- 估算宁可偏高不可偏低（偏低会把超限请求真发给 provider，烧费用）；
- MCP catalog/缓存是本机运行时状态，不进同步域；
- 无手动后台触发入口；不新增 CLI/按钮形式的预算旁路。

## 实施记录

（待填）
