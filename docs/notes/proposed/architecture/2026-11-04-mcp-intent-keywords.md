# MCP / Skill 工具意图关联（用户声明触发词，让 AI 主动联想第三方工具）

- 日期：2026-11-04
- 状态：`proposed`（讨论已收敛，方案未实施）
- 范围：对话中 MCP 第三方工具与用户 skill 的自动触发/联想机制；不改 MCP 协议、不动既有 defer/enable_tools/L1-L2 框架
- 性质：产品 + 架构小改进（一次迁移 + 一个设置字段 + catalog 渲染一行）
- 关联：[MCP 工具接入延迟加载与按需暴露](2026-10-19-mcp-tools-deferred-injection.md)（R2A catalog 机制，本文是其触发面的延伸）、[token 估算校准](2026-10-19-token-estimation-calibration.md)（catalog 预算）

## 0. Skill 与 MCP：同一个问题，两套发现规则

需求场景里 MCP 和 skill 是并列的，两者在本问题上**本质同构**：都是「能力如何被模型在正确的时机发现并调用」。项目里两者已统一在同一套 defer/enable_tools 框架下——L1（title/tags/brief）常驻花名册，模型语义匹配后 `enable_tools` 拿 L2 完整定义，同轮可用（skill 见 `src/ai/skills.rs` 头注，MCP 走 `mcp_catalog_text()`），下节的漏触发分析对两者同样成立。

但两者的**发现规则与可干预点不同**，这决定了治理方案的差异：

| | MCP server | 用户 skill |
|---|---|---|
| 描述来源 | `tools/list` 自报，第三方代码写死，**用户改不了** | SKILL.md frontmatter（name/description），**用户可编辑** |
| 花名册渲染 | 按 server 分组，工具行 = 短名(参数) // 首行描述 120 字符截断 | `- id（用户技能）：brief`，brief 截断到 200 chars |
| 数量治理 | catalog 内联预算 2000 token，超出只计数 | `USER_SKILL_LIMIT = 30`，超出截断 |
| 触发词治理 | **本文主题：加 keywords 列**（唯一干预点） | 首选直接改 SKILL.md description（零机制成本，见位置一） |

因此：下文以 MCP 为主线展开（它是改不了描述的那半边），skill 作为对称面提及——若 skill 也出现「意图词与描述不重叠」的漏触发，用同一套思路处理。

## 1. 需求（用户原话归纳）

> 正常对话提到社媒或一些想法，比如「我想在小红书卖 xx」，如果用户配置了相关的 MCP/skill，系统会自动调用去搜索相关信息；或者系统可以问用户「你配置了 skill/mcp，是否需要我用这些工具去查查看」。

拆成两层：

1. **自动联想**：用户没点名工具，只表达了意图（卖货/种草/查资料），模型应主动把意图匹配到已配置的 MCP 工具并调用（读类），或提议调用（写类）。
2. **确认式兜底**：联想不确定时，系统提示用户「你配置了 X 工具，要不要我去查」。

## 2. 现状：机制已内建，缺的是相关性信号

现有链路（R2A 后）：

```
用户消息 → 模型看 enable_tools 花名册（MCP catalog 按 server 分组）
        → 模型语义匹配 → enable_tools 登记全名 → 同轮调用 mcp__<server>__<tool>
```

模型「知道」的只有两样：**server 名**（如 `xx_xiaohongshu_xx`）与**工具描述首行**（`tools/list` 自报，catalog 里 120 字符截断，`src/ai/tool/mod.rs:487` catalog_line）。联想全靠模型拿用户消息与这两样做模糊匹配。

会漏触发的情形：

- 意图词与工具名/描述不重叠（server 叫 `social_media_kit`、描述是英文 `search notes on RED`，用户说「小红书」联想不到）
- 模型保守，倾向先凭记忆泛答
- catalog 2000 token 内联预算截断，工具根本不在花名册里
- 内置 skill（web-search 等）先被匹配，抢走了更具体的领域工具

核心缺口：**系统没有「用户为什么装这个工具」的显式关联定义**。子串肉眼可见（`xx_xiaohongshu_xx` 含「xiaohongshu」）不等于模型能可靠联想中文意图。

## 3. 方案空间（讨论结论）

### 位置一：改描述源头 —— skill 首选，MCP 常不可行

MCP 协议里 description 本就是给客户端模型的触发信号，自己写的 server 改首行（加「当用户提到小红书/种草时使用」）零客户端成本。但多数 server 是第三方（npx 拉取），改不了上游。

**skill 则没有这个限制**：SKILL.md 的 description 就在用户磁盘上，用户（或经 AI 引导）直接编辑 frontmatter 即可补触发词——这是 skill 侧的零机制成本方案，**优先级高于给 skill 也建 keywords 字段**。

### 位置二：客户端配置加「意图关键词」字段（✅ 推荐）

`mcp_servers` 表加一列：

```
migration 5x: ALTER TABLE mcp_servers ADD COLUMN keywords TEXT NOT NULL DEFAULT ''
```

用户在设置 → 集成 → MCP 编辑对话框里填（选填，逗号分隔，如 `小红书,种草,卖货,笔记,博主`）。

消费点两处：

1. **catalog 渲染**（`mcp_catalog_text()`，src/ai/tool/mod.rs:446）：server 分组头从
   `- xx_xiaohongshu_xx（3 个工具）：`
   变为
   `- xx_xiaohongshu_xx（3 个工具）【关联意图：小红书/种草/卖货】：`
   花名册是模型每轮都看的，这一处即解决「系统知道相关性」。
2. **（可选增强）确定性 nudge**：消息合成时用 keywords 对用户消息做包含匹配，双向命中（消息含词 + 用户声明过该词）才注入提示，仿 `PROTOCOL_REPAIR_NUDGE` 机制。绕过语义匹配的随机性，且匹配依据是用户显式声明而非拍脑袋规则。

配套提示词（SYSTEM_PROMPT_BASE 加一条）：花名册中的「关联意图」是用户声明的触发信号，用户消息命中时应主动调用（读类）或提议调用（写类）；内置 skill 只做通用兜底，用户配置的领域工具优先。

改动清单：migration、`src/storage/mcp_servers.rs` 读写（含快照同步域字段进出，参照 platform/trusted 先例）、`ui/lib/screens/settings_parts/integrations.dart` 编辑对话框加输入框、`mcp_catalog_text()` 拼接、既有快照测试同步。

### 位置三：per-tool 级关键词（❌ 过度设计）

粒度更细但 catalog 行会挤爆 2000 token 预算；用户也分不清 server 级/tool 级该填什么。server 级够用。

### 「问用户要不要查」形态（❌ 不做通用机制）

读类搜索加确认步骤反而打断对话流：用户说「想在小红书卖手工皂」，期待的是带情报的回答，不是先被反问。项目里确认卡同构物（habit 问候、chat proposal）都是写操作/定时动作，确认闸合理。统一规则：

> **读 = 直接调（readOnlyHint + allow_readonly_auto 已有审批框架），写 = 确认（pending-action 审批卡已有）。**

不需要「问一下」这个中间态。

## 4. 实施建议与顺序

1. **先做零代码版验证**：SYSTEM_PROMPT_BASE 加引导 + 改描述源头（skill 改 frontmatter；MCP 只有自有 server 可改），真实对话观察漏触发率（成本最低，可能已够）。
2. **不够再上位置二**：keywords 列 + 设置 UI + catalog 渲染。nudge 增强视 keywords 落地后的效果再决定做不做。skill 侧不建 keywords 字段，引导用户改 SKILL.md。
3. 位置三与「问用户」形态明确不做。

## 5. 风险与注意

- **误触发**：聊天顺口提到「小红书」也触发查询。缓解：nudge 只注入提示不强制；提示词写明「顺带提及且无查证必要时不必调用」。用户声明 keywords 本身就是用户对误触发的知情同意。
- **catalog 预算**：关键词行加进分组头会略增 token，仍远小于 per-tool 注入；2000 上限内的截断逻辑不变。
- **快照同步**：keywords 属「定义随人走」域（同 platform），进快照、不含敏感信息，无 trusted/headers 那样的本机隔离问题。
