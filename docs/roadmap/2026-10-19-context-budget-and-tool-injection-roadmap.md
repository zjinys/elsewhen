# 上下文预算与工具注入修复路线图

> 2026-10-19。定位：Gitea MCP 事故（[事故档案](../notes/incidents/2026-10-19-mcp-gitea-tool-call-review-and-budget-incident.md)）
> 暴露的三层结构性问题的统一修复计划——工具 schema 全量注入、token 估算失真、
> 审批后无续跑闭环。前两层的修复方案已立项（见关联提案），第三层随本路线图立项。
> 本路线图是 [2026-10-17 上下文管理路线图](2026-10-17-context-management-roadmap.md)
> 的事故驱动补丁：那条线做「上下文里放什么」，这条线修「上下文有多大、怎么测、
> 工具结果怎么回到对话」。

> **设计目标（2026-10-19 追加）**：**「上下文超限」不应是用户可见的死局**。
> 参考 opencode/pi 的三层防线（system prompt 增量 diff、工具 catalog 化、
> 超限压缩重试），Elsewhen 应做到：固定地板 < 5K、80% 提前压缩、
> 裁剪后仍超限 → 强制压缩重试（永不拒绝）。
> 用户看到的永远是「正在整理记忆」，而不是「请缩小任务范围」。

> **第二目标（2026-10-19 追加）**：**「不崩」≠「好用」**。
> 裁剪到 0 后模型只记得最近 4 条消息——不是「对话不顺畅」，是**模型失忆**。
> 需要 R5 智能裁剪（保留主题/结论）、R6 对话纪要（压缩历史为可读摘要常驻）、
> R7 system prompt 动态化（只注入当前激活工具的说明书），让模型在长对话中
> 知道「我们在讨论什么、之前做了什么决定、进行到哪一步」。

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
  - **pi 的 system prompt 分 sections + `diffSystemPromptSections` 增量下发**：
    每轮只发变化的段，不变的不重复发（对 prefix caching 友好）；
  - 两家都没有本地预算预检——Elsewhen 的预检方向本身是对的（面向任意 OpenAI
    兼容端点 + 小窗口模型），坏的是测量精度、工具注入体积和 system prompt 组织方式。
- **决策**：保留本地预检作主防线，按「测量 → 地板 → 兜底 → 闭环」顺序修；
  不做 opencode 式 JS 沙箱（execute）——调用面不动，只借 catalog 渲染思路。
- **stable_setup 巨石发现**（`src/ai/memory.rs:106-174`）：
  `SYSTEM_PROMPT_BASE` 本体 ~10K tokens，包含全部工具说明书 + 行为规则 + 情绪回应
  + 个人经验规则库引导，标记 `Required` 不可裁、每轮全量重发。
  工具说明书（web_search/fetch_page 等延迟工具的详细用法）不该常驻——只在
  `enable_tools` 激活后才需要。参考 pi 的 sections + diff，拆成常驻核心 + 按需渲染。
- **模型失忆问题**（2026-10-19 数据验证）：212 条历史 + 53 工具 + 64K 窗口的会话，
  裁剪后 history 只剩 783 tokens（4 条 `whole_earlier_turn` 尾巴），stable_setup 10,982 原封不动。
  模型「知道」全部工具用法、待办、规则、时钟、最近 4 条消息；「不知道」212 条历史里
  说了什么、之前讨论过什么话题、做过什么决定。这不是「对话不顺畅」，是**模型失忆**。
  需要 R5 智能裁剪 + R6 对话纪要 + R7 prompt 动态化让模型在长对话中保持连贯性。

## 关联文档

| 文档 | 角色 |
|---|---|
| [事故档案](../notes/incidents/2026-10-19-mcp-gitea-tool-call-review-and-budget-incident.md) | 全部证据（SQL/日志/代码行号）、OpenCode 对比、P0/P1/P2 原始分析 |
| [MCP 工具延迟加载提案](../notes/proposed/architecture/2026-10-19-mcp-tools-deferred-injection.md) | R2 工具 catalog 化的完整方案（catalog 格式、缓存失效、兼容闸、验收指标） |
| [token 估算校准提案](../notes/proposed/architecture/2026-10-19-token-estimation-calibration.md) | R1 的完整方案（usage 锚定、分层比率、逃生门、回归保护） |

## 分期

| 期 | 内容 | 对应事故条目 | 验收 |
|---|---|---|---|
| **R1 测量校准** | `text_tokens` 分层估算（族分词器/校准比率/保守兜底 + provider 级 ratio 覆盖逃生门）；请求预算估算锚定最近一轮真实 usage、只估新增尾巴 | 事故 P2 | Gitea schema 估算落 [8K,16K]（原 34K）；中文历史 1000 字估 1.8K-2.4K（原 3K）；已知超限样本仍 local_rejection（保守方向不破）；`cargo test` 全绿 |
| **R2 地板重构** | **A. 工具 catalog 化**：MCP 工具接入 defer/enabled 框架，L1 catalog 常驻 enable_tools 描述（签名+首行描述截 120 字符，2K token 预算），L2 按工具全名激活注入；tools/list 缓存 + 配置变更失效；未启用调用回错指路。**B. system prompt 按需渲染**：`SYSTEM_PROMPT_BASE` 拆成 `SYSTEM_PROMPT_CORE`（常驻 < 2K：角色+铁律+风格）+ `TOOL_GUIDANCE`（按需：仅激活工具的说明书注入）；conversation_state/goals/rules 等变化时参考 pi `diffSystemPromptSections` 做增量 system message 追加而非全量重发 | 事故 P1 + stable_setup 巨石 | 事故场景重放：Round 0 tools_tokens < 8,000（原 40,032），stable_setup < 3,000（原 10,982），Round 1 总输入 < input_limit，正常列出 10 条 issue；未启用调用 MCP 工具回「先 enable_tools」；prefix cache 命中率提升（可观测） |
| **R2.5 兜底压缩（永不拒绝）** | 裁剪后仍超限时，**不返回 local_rejection**，而是：用 `store.list_messages()` 原始消息（非裁剪后上下文）触发 `apply_summary` 强制压缩（跳过 `users.len() < 3`、`selected_tokens < 512`、`complete_originals` 三个早退闸）→ PageMark 落库 → 用压缩后上下文重试本轮生成。真·必死局（压缩后仍超）才报错，但给出具体可操作建议（如「请断开 X 个 MCP server」）而非「缩小任务范围」 | 消灭用户可见的 local_rejection | 事故场景重放：212 条历史 + 53 工具 + 64K 窗口下，第一轮触发兜底压缩，第二轮正常通过，用户看到「正在整理记忆」而非「请缩小任务范围」；压缩后仍超的极端 case 报错含具体建议 |
| **R3 审批续跑闭环** | 审阅卡接受后恢复 agent 循环：真实工具输出以 tool_result 注入上下文并触发下一轮生成，模型基于真实结果作答；回执弹窗摘要化（不再 raw JSON 整段糊出）；readOnlyHint 只读 MCP 工具免逐次审批（或会话级允许） | 事故 P0（两处）+ P1 只读条目 | 未受信场景重放：用户确认后，对话里出现模型基于真实 list_issues 结果的回答（而非「已确认请继续」）；回执展示为摘要 + 可展开详情 |
| **R4 超限兜底** | provider 真实超限的识别与恢复：报错正则检测（参考 pi overflow.ts 正则表）→ 压缩历史后重试同 provider 一次，再失败才 failover | 事故 P2 延伸（防估算漏网） | 模拟上游返回超限错误文本时：先 compact-retry 一次，成功则正常作答；二次失败才走 provider 轮换 |
| **R5 智能裁剪** | 裁剪不再是「整段砍」，而是「保留主题/结论，裁掉细节」：对长 assistant 回复提取要点（1-2 句），对工具调用只保留调用名和结果摘要（不保留完整 JSON），对用户消息保留原文（用户的话通常短） | 模型失忆问题 | 裁掉 70K 后，模型仍然知道「我们在讨论 MCP 工具超限问题，已经定位到三个根因」而非「只记得最近 4 条消息」 |
| **R6 对话纪要** | 每 N 轮（或窗口到 60%）自动生成「对话纪要」段：几百 tokens 概括「到目前为止讨论了什么、做了什么决定、待办什么」。纪要作为 `Required` 常驻，被裁掉的历史由纪要替代 | 模型失忆问题 | 模型不失忆，知道对话主线；212 条历史压缩后，模型仍能回答「我们之前讨论过什么」 |
| **R7 system prompt 动态化** | `SYSTEM_PROMPT_BASE` 按当前激活工具动态裁剪：只注入当前 `enabled_tools` 里工具的说明书；`time_context` 改为「只在用户提到时间相关词时注入」或「只在第一条和最后一条消息注入」 | stable_setup 巨石 + 信噪比 | 10,982 tokens 的巨石降到 2-3K；模型被迫在 10K tokens 工具手册里找「当前对话相关信息」的问题消除 |

## 依赖与顺序理由

- **R1 先于 R2**：R2 的验收指标（tools_tokens < 8,000、stable_setup < 3,000）依赖 R1 的测量口径才有意义；
  且 R1 是纯测量层改动，风险最低、可独立发布。
- **R2 内部 A/B 可并行**：工具 catalog 化和 system prompt 拆分互不相阻塞；
  但 B 的增量 diff 机制对 prefix caching 有直接影响，建议优先。
- **R2.5 与 R2 可并行**：兜底压缩不依赖地板重构，但 R2 修好后 R2.5 的触发概率趋近于零。
  R2.5 是「永不拒绝」的最后一道闸，不能省。
- **R3 独立于 R2/R2.5**：体积问题和闭环问题互不相阻塞；但 R3 重放验收用的正是 R2 修好的
  场景，建议 R2 先落地。
- **R4 最后**：估算校准 + 地板重构 + 兜底压缩后，本地预检的漏网概率大降，R4 是纵深防御而非主线。
  其「压缩历史」能力复用 R2.5 的强制压缩机制。
- **R5/R6/R7 是「好用」层**：R1-R4 达成「不崩」基线后，R5-R7 解决「模型失忆」和信噪比问题。
  R5/R6 可并行，R7 依赖 R2B 的 system prompt 拆分（先拆开才能动态化）。
  R5 的「提取要点」和 R6 的「对话纪要」可复用摘要管线的现有能力。
- R3 的「只读工具免审批」依赖 MCP `readOnlyHint` 标注的可靠性——Gitea MCP 的
  `list_issues` 实测未标 readOnlyHint（见事故档案 §3.1 对比表），落地时需按
  server 配置级白名单补充，不能盲信上游标注。

## 红线（实施全程）

- 不绕过 `Store::insert_event()` 提交边界；审批语义（untrusted → pending_action）不变，
  R3 只改「确认之后发生什么」，不改「何时需要确认」；
- 估算宁可偏高不可偏低（偏低会把超限请求真发给 provider，烧费用）；
- MCP catalog/缓存是本机运行时状态，不进同步域；
- 无手动后台触发入口；不新增 CLI/按钮形式的预算旁路；
- **「上下文超限」不应是用户可见的死局**——所有分期完成后，用户看到的永远是
  「正在整理记忆」或具体可操作建议，而不是「请缩小任务范围」。
- **「不崩」≠「好用」**——R5-R7 完成后，模型在长对话中应保持连贯性：
  知道对话主线、记得之前的决定、能回答「我们之前讨论过什么」。

## 实施记录

（待填）
