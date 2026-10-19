# MCP 工具接入延迟加载与按需暴露（工具 Schema 瘦身）

- 日期：2026-10-19
- 状态：核心已实施（2026-10-20：catalog 化+两层激活+调用拦截经 R2A；tools/list 缓存同日落——配置指纹键+TTL，未引入 ToolsChanged 事件，偏差见 roadmap 实施实录）
- 范围：第三方 MCP server 工具在对话上下文中的注入策略；不改动内置工具 defer 机制与 skill 包的 L1/L2 结构
- 性质：事故驱动的架构改进
- 关联：[Gitea MCP 工具调用失效与上下文预算熔断事故档案](../../incidents/2026-10-19-mcp-gitea-tool-call-review-and-budget-incident.md)、[主对话流摘要管道永久失败事故（P2 工具 schema 40K 瘦身条目）](../../incidents/2026-10-19-summary-pipeline-permanent-failure.md)、[非 GPT 模型 token 估算校准提案](./2026-10-19-token-estimation-calibration.md)

## 1. 现象与问题

事故档案实测：接入 `gitea-mcp`（53 个工具）后，每轮对话请求的 `tools` 字段序列化达 **40,032 tokens**（含内置常驻工具），占 64K 上下文窗口的 **61%**，且这笔钱**每一轮、每一个 provider 重试都全额付出**。即便把所有历史对话裁剪到 0，固定开销（系统基底 ~16K + tools 40K + 本轮 tool_result ~3K）仍击穿 `deepseek-flash` 的 58,164 input_limit，触发 `local_rejection`（"本轮必要上下文超过预算"）。

对比之下，内置的 14 个低频工具走 `deferred()` 延迟加载，常驻开销只有 `enable_tools` 一个元工具加几百字花名册——**同一套机制，MCP 完全没有用上**。

## 2. 现状代码证据

### 2.1 每轮全量拉取 + 无条件注入

`src/ai/conversation.rs:246-251`：每轮生成都调用 `mcp::available_tools(store)` 对每个启用 server 发起 `tools/list` RPC，然后 `registry.set_mcp_tools(mcp_tools)` 全量登记。

`src/ai/tool/mod.rs:310-315`（`provider_specs_partitioned`）：

```rust
.filter(|tool| !self.defer_tools || !tool.deferred() || enabled.contains(tool.name()))
...
.chain(self.mcp_tools.iter().map(|t| ToolSpec { ... }))  // ⚠️ chain 在 filter 之外，无条件注入
```

内置工具的 defer 过滤对 `mcp_tools` 完全不生效；`eager_specs()`（空 enabled 集，`mod.rs:319`）也把 MCP 算进「常驻集」参与 `tool_reserve` 预算预扣。

### 2.2 已有的可复用机制

- `ToolRegistry.defer_tools` / `is_deferred` / `enabled_tools: HashSet<String>` 登记集（`conversation.rs:897`）；
- `enable_tools` 元工具：description 常驻花名册（内置 `BUILTIN_SKILLS` + 用户技能 `user_skills_roster()`，`mod.rs:334-477`），`enable_result()` 回传方法论 + 完整 schema 并激活；
- 延迟工具守卫：未启用即调用时回友好错误「先调用 enable_tools」（`conversation.rs:1362-1372`）。

### 2.3 与内置 defer 的差异点（不能直接复用的原因）

1. **MCP 工具清单是运行时 RPC 拉取的**，不像 `BUILTIN_SKILLS` 是编译期常量——花名册、激活登记都要支持动态清单；
2. **粒度是 server 还是 tool 待定**：Gitea 53 个工具按一个包启用依然超预算，需要 tool 级或「server 内子集」级；
3. **调用侧拦截**：`dispatch_with_knowledge` 里 MCP 分支（`mod.rs:689`）走的是 `registry.get()` 未命中后的 `mcp__` 前缀特判，不经过 defer 守卫，需要补「未启用 MCP 工具拒绝执行并指路 enable_tools」的对称行为；
4. **每轮 RPC 成本**：`available_tools` 每轮都发 `tools/list`；若改为花名册常驻，清单可缓存（server 配置变更时失效）。

## 3. 方案

### 3.1 分层接入（与 skill 同构，L1 参考 opencode catalog 格式）

外部调研（`ref/opencode`，见下「6. 外部参考」）：opencode 的 MCP 工具默认**不进 `tools` 字段**，而是渲染成按 namespace 分组的 **catalog 文本**——每个工具一行 `签名 // 首行描述（120 字符截断）`，且有 **2,000 token 内联预算**，装不下的由沙箱内 `search()` 检索兜底。53 个 Gitea 工具在该机制下只占几百 token。

Elsewhen 的 L1 花名册按此格式升级：

```
L1 常驻：enable_tools 元工具的 description 追加 MCP catalog
         按 server 分组：
         - gitea-mcp（53 个工具）
           - list_issues(owner, repo, state?, page?) // 列出仓库 issue
           - search_repos(query, ...) // 搜索仓库
           …（每工具一行：TS 风格签名 + 首行描述，120 字符截断）
         catalog 整体设内联 token 预算（参考值 2,000），装不下的工具只在
         「还有 N 个工具，enable 后可看完整定义」中计数
L2 按需：模型 enable_tools(["mcp__gitea-mcp__list_issues", ...]) 后
         → enabled_tools 登记全名 → 下一轮起该工具完整 schema 进 tools 字段
         → 调用侧 dispatch 放行（MCP 分支补 enabled 检查）
```

- 与 opencode 的差异：opencode 用 JS 沙箱 `execute` 统一调用并可在沙箱内组合多次调用；Elsewhen 不动调用面（仍是原生 tool call），只借其 **catalog 渲染与预算**思路。沙箱化（execute）作为后续可选方向，不在本期。
- 激活粒度：单工具全名（`mcp__<server>__<tool>`），一次可多个。
- 预算记账：`eager_specs()` 常驻集不再包含未启用 MCP 工具，`tool_reserve` 自然回落；激活后下一轮由 `provider_specs_partitioned` 的 filter 放行（MCP 分支补 `enabled.contains()` 判断）。

### 3.2 缓存与失效（参考 opencode 事件驱动 reconcile）

现状 `available_tools` 每轮对每个启用 server 发 `tools/list` RPC（`mcp.rs:443`），既慢又在每轮请求前增加固定延迟。参考 opencode：发现一次后缓存，之后靠 `ToolsChanged` 事件 debounce 100ms  reconcile（`ref/opencode/packages/core/src/tool/mcp.ts:132-148`）。

- `tools/list` 结果按 server 行缓存（内存或 DB 表列），server 增删/启停/transport 配置变更时失效；
- 缓存行带拉取时间与错误状态：拉取失败走现有 `mcp_errors` 事件通道，不静默降级为空花名册（红线：宁可显式失败不静默少工具——对模型侧表现为 catalog 中标注「拉取失败」）；
- **不引入快照数据集**：MCP catalog/缓存是本机运行时状态，与 provider active 同理不进同步域。

## 6. 外部参考

| 项目 | 机制 | 代码位置 | 可借鉴点 |
|---|---|---|---|
| opencode · MCP | MCP 工具默认 `codemode`：不进 tools 字段，渲染为按 namespace 分组的 catalog（签名+首行描述截 120 字符，2,000 token 内联预算，装不下靠 `execute` 沙箱内 `search()` 检索兜底）；变更事件驱动 debounce reconcile；权限走统一 Permission.Ruleset（调用前 assert，无伪成功暂存） | `packages/core/src/tool.ts:234`、`codemode/catalog.ts:46`、`codemode/tool.ts:64`、`mcp/instructions.ts`、`tool/mcp.ts` | L1 catalog 渲染格式与预算；清单缓存+事件失效；「无预算预检、直接发」也说明 schema 瘦身本身就是防线 |
| opencode · skill | 与 MCP 同构的两层：L1 = Instructions 段（system 消息）里的 `<available_skills>` XML（id/name/description 三项，`autoinvoke: false` 的不进列表），且用 `Instructions.diffByKey` 做**增量下发**——列表变了只发增删 delta，不整段重发；L2 = 常驻一个 `skill` 工具（`codemode: false` 直接进 tools 字段），模型传 id 调用后返回 `<skill_content>`（SKILL.md 正文 + 最多 10 个采样文件清单 + base directory 说明）。权限同样走 Permission.Ruleset（按 skill id 授权） | `packages/core/src/skill/instructions.ts`、`tool/plugin/skill.ts`、`skill.ts:36`（toModelOutput） | skill 列表放 Instructions 而非工具描述里，并做增量 diff 下发——Elsewhen 的 enable_tools 花名册每轮全量重复，可借鉴；`<skill_content>` 返回结构与 Elsewhen `enable_result` 的 user_skill_bodies 几乎同构，说明路线收敛 |
| pi · skill | 无独立 skill 工具/激活机制：启动时扫描技能目录（frontmatter 校验 name/description，`disable-model-invocation: true` 的只准 `/skill:name` 显式调用），把可见技能渲染成 `<available_skills>` XML（name/description/location 三项）**直接拼进 system prompt**；模型要用时用已有的 read/bash 工具自己去读 SKILL.md 文件——**加载动作复用通用文件工具，不新增专用工具** | `packages/coding-agent/src/core/skills.ts:358`（formatSkillsForPrompt）、`system-prompt.ts:180` | 「skill 加载 = 读文件」的零新工具思路：Elsewhen 的用户技能正文回传目前走 enable_result 特判，若未来统一为「知识页即技能」（roadmap 已提），可参考此模式让 read 类常驻工具直接承担加载；`disable-model-invocation` 与 Elsewhen skills_state 的 trusted/enabled 语义对应 |
| pi · 上下文 | 无 MCP 集成；估算锚定真实 usage，见 token-estimation-calibration 提案 | — | — |

### 3.3 兼容与安全闸

- `trusted` 审批语义不变：defer 只影响 schema 注入时机，执行前审批（untrusted → pending_action）照旧；
- 存量行为开关：过渡期可加 `mcp_servers.defer_injection` 列或全局配置项默认开；不设开关亦可（纯增益，无回退需求）；
- `status_only`（用户只问进展）分支的只读过滤（`conversation.rs:953`）对 MCP 工具同样生效——MCP 工具的 policy 目前无法区分读写（`McpToolInfo` 只有 name/description/schema），本期不扩展，保持现状不过滤。

### 3.4 验收指标

| 指标 | 现状（Gitea 实测） | 目标 |
|---|---|---|
| Round 0 tools_tokens（deepseek-flash，字节估） | 40,032 | < 8,000 |
| enable 后调用 `list_issues` 的完整链路 | 不存在（未启用即调用走未注册 MCP 直放） | enable → 下一轮 schema 注入 → 调用成功 |
| 未启用调用 MCP 工具 | 直接执行（trusted）或暂存审批 | 回错「先 enable_tools」并指路 |
| 事故场景重放 | Round 1 60,356 > 58,164 熔断 | Round 1 < input_limit，正常列出 10 条 issue |

## 4. 边界与不做

- **不改 token 估算算法**（字节退化的放大问题另立提案：token-estimation-calibration）；本提案做完后 40K → ~5K 即使按字节估也不再挤爆 64K 窗口；
- **不做 MCP 工具的结果截断策略调整**（现有 `truncate_result` 保留）；
- **不做 server 内工具的语义聚类/自动推荐**（花名册按 server 平铺即可，模型自己挑）；
- **审批流续跑闭环（Resume Agent Loop）不在本提案**：那是事故文档 P0 的独立问题，需在 `execute_claimed_proposal` 后唤醒会话生成，另行立项。
