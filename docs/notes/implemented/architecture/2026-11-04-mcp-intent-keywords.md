# Agent Note: MCP / Skill 工具意图关联

Status: implemented

- 日期：2026-11-04
- 实施日期：2026-11-05
- 范围：对话中 MCP 第三方工具与用户 skill 的自动触发/联想机制；不改 MCP 协议、不动既有 defer/enable_tools/L1-L2 框架
- 性质：产品 + 架构小改进（一次迁移 + 一个设置字段 + catalog 渲染一行）
- 关联：[MCP 工具接入延迟加载与按需暴露](../../proposed/architecture/2026-10-19-mcp-tools-deferred-injection.md)（R2A catalog 机制，本文是其触发面的延伸）、[token 估算校准](../../proposed/architecture/2026-10-19-token-estimation-calibration.md)（catalog 预算）

## Problem

第三方 MCP 的工具名与英文描述未必覆盖用户日常使用的中文意图；用户说「想在小红书卖手工皂」时，模型可能只给泛化建议或选择通用搜索，无法知道用户为什么安装该领域工具。需要可由用户编辑的相关性信号，同时保持已有工具发现预算和审批边界。

## Decision

迁移 72 为 `mcp_servers` 增加选填 `keywords`，默认空串，服务级关联意图随定义同步。设置 → MCP 的新建/编辑框与服务器卡片支持输入和展示；Rust API 与生成的 Dart 桥接完整透传。保存和快照接收统一接受中英文逗号与换行、去空白去重，规范化后合计最多 256 字，控制字符拒绝落库。关键词允许任意语言文字，不绑定当前中文界面。

运行时将本机关键词附到工具清单，catalog 仅在服务分组头渲染一次。分组头、关联意图、工具行与省略提示共同受 2000 token 上限约束。缓存指纹包含关键词，修改后下一轮即时可见；缓存只保存所属服务的清单，避免其他服务的旧工具/意图随缓存复活。

系统提示指导模型在有查证/处理需求时优先选择匹配的领域工具，内置技能作通用兜底。顺带提及无须调用；关键词不授予权限，仍先 `enable_tools`，调用继续由 `readOnlyHint + allow_readonly_auto`、`trusted` 和 pending-action 控制。确定性子串 nudge 保留为可选增量，本次不加入强制触发。

快照新字段带 serde 空串缺省，旧快照可直接接收，无须升级快照记录版本。既有本机行获胜；env/headers/信任仍保持原来的本机边界。本次增加的是配对快照字段，不新增 MCP 配置变更的实时增量同步协议。

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

## Original scenario

> 正常对话提到社媒或一些想法，比如「我想在小红书卖 xx」，如果用户配置了相关的 MCP/skill，系统会自动调用去搜索相关信息；或者系统可以问用户「你配置了 skill/mcp，是否需要我用这些工具去查查看」。

拆成两层：

1. **自动联想**：用户没点名工具，只表达了意图（卖货/种草/查资料），模型应主动把意图匹配到已配置的 MCP 工具并调用（读类），或提议调用（写类）。
2. **确认式兜底**：联想不确定时，系统提示用户「你配置了 X 工具，要不要我去查」。

## Discovery contract

现有链路（R2A 后）：

```
用户消息 → 模型看 enable_tools 花名册（MCP catalog 按 server 分组）
        → 模型语义匹配 → enable_tools 登记全名 → 同轮调用 mcp__<server>__<tool>
```

实施前模型「知道」的只有两样：**server 名**（如 `xx_xiaohongshu_xx`）与**工具描述首行**（`tools/list` 自报，catalog 里 120 字符截断，`src/ai/tool/mod.rs` 的 catalog_line）。本次增加用户显式声明的关联意图。

会漏触发的情形：

- 意图词与工具名/描述不重叠（server 叫 `social_media_kit`、描述是英文 `search notes on RED`，用户说「小红书」联想不到）
- 模型保守，倾向先凭记忆泛答
- catalog 2000 token 内联预算截断，工具根本不在花名册里
- 内置 skill（web-search 等）先被匹配，抢走了更具体的领域工具

核心缺口：**系统没有「用户为什么装这个工具」的显式关联定义**。子串肉眼可见（`xx_xiaohongshu_xx` 含「xiaohongshu」）不等于模型能可靠联想中文意图。

## Alternatives considered

### 位置一：改描述源头 —— skill 首选，MCP 常不可行

MCP 协议里 description 本就是给客户端模型的触发信号，自己写的 server 改首行（加「当用户提到小红书/种草时使用」）零客户端成本。但多数 server 是第三方（npx 拉取），改不了上游。

**skill 则没有这个限制**：SKILL.md 的 description 就在用户磁盘上，用户（或经 AI 引导）直接编辑 frontmatter 即可补触发词——这是 skill 侧的零机制成本方案，**优先级高于给 skill 也建 keywords 字段**。

### 位置二：客户端配置加「意图关键词」字段（已采用）

`mcp_servers` 表加一列：

```
migration 72: ALTER TABLE mcp_servers ADD COLUMN keywords TEXT NOT NULL DEFAULT ''
```

用户在设置 → 集成 → MCP 编辑对话框里填（选填，逗号分隔，如 `小红书,种草,卖货,笔记,博主`）。

目录消费已实现，确定性 nudge 仍是可选增强：

1. **catalog 渲染**（`mcp_catalog_text()`，src/ai/tool/mod.rs:446）：server 分组头从
   `- xx_xiaohongshu_xx（3 个工具）：`
   变为
   `- xx_xiaohongshu_xx（3 个工具）【关联意图：卖货/小红书/种草】：`
   花名册是模型每轮都看的，这一处即解决「系统知道相关性」。
2. **（尚未采用）确定性 nudge**：可在消息合成时用 keywords 对用户消息做包含匹配，双向命中（消息含词 + 用户声明过该词）才注入提示，仿 `PROTOCOL_REPAIR_NUDGE` 机制。先用目录与提示词观察真实模型效果，再评估是否需要；即使增加也只注入提示，不强制调用。

配套提示词（SYSTEM_PROMPT_BASE 加一条）：花名册中的「关联意图」是用户声明的触发信号，用户消息命中时应主动调用（读类）或提议调用（写类）；内置 skill 只做通用兜底，用户配置的领域工具优先。

改动清单：migration、`src/storage/mcp_servers.rs` 读写（含快照同步域字段进出，参照 platform/trusted 先例）、`ui/lib/screens/settings_parts/integrations.dart` 编辑对话框加输入框、`mcp_catalog_text()` 拼接、既有快照测试同步。

### 位置三：per-tool 级关键词（❌ 过度设计）

粒度更细但 catalog 行会挤爆 2000 token 预算；用户也分不清 server 级/tool 级该填什么。server 级够用。

### 「问用户要不要查」形态（❌ 不做通用机制）

读类搜索加确认步骤反而打断对话流：用户说「想在小红书卖手工皂」，期待的是带情报的回答，不是先被反问。项目里确认卡同构物（habit 问候、chat proposal）都是写操作/定时动作，确认闸合理。统一规则：

> **读 = 直接调（readOnlyHint + allow_readonly_auto 已有审批框架），写 = 确认（pending-action 审批卡已有）。**

不需要「问一下」这个中间态。

## Verification

- Rust 回归覆盖旧数据库升级保留配置、关键词读写/格式与长度边界、stdio/HTTP 清单透传、服务关键词修改/停用的缓存行为、目录按组去重与总预算，以及审批开关组合。
- 快照回归覆盖新关键词往返、本机行保留与旧快照无关键词缺省，同时继续验证环境变量与鉴权头不外发。
- Flutter 使用 `createIsolatedBridge` 的临时数据库经真实桥接验证设置框回填、保存规范化与列表刷新；只读审批设置保留原回归。
- `cargo test --all-targets`：库 1054 passed、20 ignored，集成 2 passed，配置环境检查 5 passed，0 failed。首次沙箱运行因本地端口权限和迁移旧版本断言失败，修正断言并以获准权限重跑后全绿。
- `fvm flutter test --no-pub test/mcp_intent_keywords_test.dart test/mcp_readonly_gate_test.dart`：2 passed。首版测试把数据库提交误当作 UI 异步刷新完成，现等到列表可见再断言；真实桥接往返和卡片显示均通过。
- 本次四个 Dart 文件的定向 `fvm dart analyze`：No issues found；全项目 Flutter analyze 无 error，仍有 130 条其他文件和第三方代码的既有 warning/info。
- `scripts/regen.sh` 完成桥接生成及 release 库编译；Rust/Dart 内容 hash 均为 `-1468026998`。`git diff --check` 通过。
- 自动测试验证字段与调用链路，不代表真实模型的漏触发率已经验收；实际选用工具仍受问题语义和模型影响。

## Consequences

- **误触发**：聊天顺口提到「小红书」也可能触发查询。提示词写明「顺带提及且无查证必要时不必调用」，权限仍由既有审批判断。
- **catalog 预算**：关键词略增分组头体积，仍小于 per-tool 注入；大量服务下仍可能省略工具，本次将分组头与省略提示一起纳入上限。关键词修改会重取该服务的清单，换取下一轮立即生效。
- **快照同步**：keywords 属「定义随人走」域（同 platform），进快照、不含敏感信息，无 trusted/headers 那样的本机隔离问题。
