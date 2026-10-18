# FR-PES-ARCH-003: 外部工具执行与信任模型技术设计

**版本**: v0.3

**最后更新**: 2026-10-18

**状态**: Implemented（实现约束与验证见 §8）
**关联需求**: [FR-PES-006](../product/FR-PES-006-外部工具与技能执行.md)

## 1. 基线和边界

实现分五个提交落地：MCP 接入（`6502717`）、pending 审批（`c4780e1`）、streamable-http（`c26c750`）、MCP trusted（`e58fb0f`，迁移 47）、skills trusted 与审计对齐（`eeeb0e6`、`fc54c2d`）。本文记录最终形态与不变量。

代码位：

| 层 | 文件 | 职责 |
|---|---|---|
| 存储 | `src/storage/mcp_servers.rs` | server CRUD、enabled/trusted 切换（`set_mcp_server_trusted` 失活连接缓存） |
| 执行 | `src/ai/mcp.rs` | stdio / streamable-http 连接、`connect`、`call_tool`（内部独立重查 enabled）、sidecar 注册、审批与 trusted 的 fixture 测试 |
| 调度 | `src/ai/tool/mod.rs` | dispatch 的 MCP 分支（可达性→trusted 快照→pending 暂存三段）、`execute_pending_action` 的 `mcp_tool_call` / `run_skill_script` 路由 |
| 脚本 | `src/ai/tool/skill_script.rs` | `resolve_script`（启用校验 + 目录穿越拒绝 + shebang 解析）、`execute_confirmed`（唯一进程起跑点）、`run_with_data_dir`（data_dir 注入内层） |
| 技能 | `src/ai/skills.rs` | `skills_state.json` 状态读写、`is_trusted`/`set_trusted`、`record_audit`（信任墙 #4） |
| FFI/UI | `src/api/mcp.rs`、`src/api/user_skills.rs`、`settings_screen.dart` | Dto（trusted 字段）、生命周期审计落点、信任开关 UI 与风险明示 |
| 迁移 | `migrations/44-mcp-servers`、`45-mcp-streamable-http`、`47-mcp-trusted`、`48-mcp-platform` | 表结构演进（48：platform 标签 + (name,platform) 唯一） |

**不变量（任何改动不得破坏）**：

1. 免审批与确认后执行共享同一执行函数（`call_tool` / `execute_confirmed`）。禁止为豁免路径另开执行通道。
2. `call_tool` 对 enabled 的检查是执行时独立的，不信任 dispatch 时的快照；停用/删除即拒绝，与审批状态无关。
3. pending-action 的 `args_json` 是执行内容的唯一权威来源；确认路径除说明文案外不做任何参数加工。
4. trusted 是本机事实：`skills_state.json` 不进同步域；`mcp_servers` 行的 trusted 值**不得随快照传播**（apply 强制置 0）。迁移 48 起定义本身进同步域（平台标签方案），豁免不搭车。
5. 新增「免审批」类开关必须同步补审计动作与双向测试（豁免路径 + 未豁免路径），否则不得合并。

## 2. 数据模型

```sql
-- 迁移 44/45/47 逐步演进
CREATE TABLE mcp_servers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  transport TEXT NOT NULL CHECK (transport IN ('stdio','streamable-http')),
  command TEXT, args_json TEXT, env_json TEXT,   -- stdio
  url TEXT, headers_json TEXT,                    -- streamable-http；headers_json 承载鉴权
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),
  trusted INTEGER NOT NULL DEFAULT 0 CHECK (trusted IN (0,1)),
  created_at TEXT, updated_at TEXT
);
```

- FFI 侧动态 JSON 全部字符串化（`args_json`/`env_json`/`headers_json`）；`serde_json::Value` 不过 frb 界面。
- `platform` 列（迁移 48）：`all/linux/macos/windows`。**定义随人走，激活看平台**——`list_active_mcp_servers` 只放行 enabled 且 platform 匹配当前 OS（或 all）的行，是工具注册与调用的统一口子；同一名 server 可有跨平台变体（npx vs npx.cmd），唯一约束 `(name, platform)`。
- 双语义列 `enabled`（可调用性）与 `trusted`（审批豁免）正交：停用的 server 即使 trusted 也拒绝（`call_tool` 只查 enabled + platform 适用性）。
- 技能侧信任存 `<data_dir>/skills_state.json`（`trusted: Vec<String>`），非数据库——随体系既有约定，且天然本机化。

## 3. 执行三态与信任判定

工具调度的判定顺序（MCP 分支）：

1. **可达性**：server 不存在/未启用 → 拒绝文案；stdio 在移动端 → 诚实降级文案。模型收到的是明确失败，不是静默。
2. **信任判定**：dispatch 时读 server.trusted 快照——trusted=1 → 直接执行（仍走含 enabled 复查的 `call_tool`）；untrusted → 落 pending-action。
3. **审批闭环**：`store_create_pending`（action=`mcp_tool_call`，args 冻结 `{full_name, arguments}`）→ 工具结果告知「已暂存待用户确认」→ 审阅卡接受 → `execute_pending_action` 按封存 args 原样调 `call_tool`，不再二次批准。

技能侧同构：`run` 取全局 data_dir 后进入 `run_with_data_dir`（data_dir 显式化，测试注入点）；trusted → `execute_confirmed`（resolve_script 全套校验重做），untrusted → pending（args 冻结 `{skill, script, args}`）。移动端在 `cfg(target_os)` 下诚实降级。

关键命名约定：MCP 工具名 `mcp__<server>__<tool>`，registry 挂 sidecar（动态名不符 `Tool::name()->&'static str`，见 §1 提交 `6502717` 的取舍）。

## 4. 信任豁免的工程契约

- **同函数原则**：豁免路径与确认路径在执行层无分叉（`call_tool`、`execute_confirmed`）。审批豁免只发生在「是否等用户点头」这一步。
- **快照与复验分离**：dispatch 的 trusted/enabled 快照只用于当轮决策，执行函数内部各自复查权威状态。
- **风险明示**：设置页在信任开关入口（列表 tooltip/snack + 表单文案）与技能侧（trusted 警告）两处层级提示第三方代码风险。
- **不引入混合态粒度**：信任对象是 server/技能整体。若未来需要工具级信任，先修本节契约与测试矩阵，而不是在 dispatch 加一列。

## 5. 审计（信任墙 #4）

- skills：`src/ai/skills.rs::record_audit` 追加写 `skills/.audit.log`（JSONL：`{ts, action, detail}`），动作含 install/enable/disable/trust/untrust。审计失败静默降级。
- MCP：`src/api/mcp.rs::record_mcp_audit` 写 `<data_dir>/mcp/.audit.log`，同格式；save（区分 create/update）、delete、enable/disable、trust/untrust 全部打点，trust/untrust 是必留痕项。
- 审计不进用户事件流、不进同步域：它是本机系统操作记录。
- 有意选择在 FFI 层落点而非 Store 方法：审计需要 data_dir，且生命周期动作的用户意图（create vs update）只在 FFI 参数里可得。

## 6. streamable-http 传输

- 逐消息 POST、拉模式；不开常驻 SSE 流（无后台连接、无心跳任务，进程内连接缓存 `mcp-session-id` 复用会话）。
- 会话头：`MCP-Protocol-Version: 2025-06-18`；首次 initialize 换取 session-id 后续携带；自定义鉴权头经 `headers_json` 注入（取值原样发往 server，文档/UI 明示不要存超权凭证）。
- 传输不可达回归为「工具结果错误文本」，与决策链第 1 段一致。

## 6b. 同步语义（迁移 48 平台标签方案）

`mcp_servers` 整表进快照域（v1 曾整表排除，理由是 stdio 命令的本机绑定；平台标签把「本机绑定」变成行级元数据后，整表分发 + 激活过滤更统一）。行的同步面：

- **带**：id/name/platform/transport/command/args_json/env_json/url/enabled/时间戳——定义随人走，换设备不再重配远程端点。
- **不带**：`headers_json`（凭据不离本机，与 provider key 同一先例；接收端调用 401 是显式失败，UI 提示补头）；`trusted`（信任是本机事实，apply 强制置 0——URL 可以搭车，豁免不搭车）。
- **本机获胜**：接收端已有同 id 或同 `(name, platform)` 行则远端行跳过（INSERT OR IGNORE 对两个唯一目标同效），与技能「目录获胜」同构——本机是用户此刻的意图。
- **协议代价**：`Record::McpServer` 是新 serde 变体，跨版本两端必须同升（旧端整份拒收快照，显式隔离；对应 SNAPSHOT_VERSION v4→v5）。
- 后续方向：移动端执行委托（云端/本地执行选择），见 `docs/notes/proposed/architecture/2026-10-18-mobile-execution-delegation.md`。

## 7. 测试矩阵（现状）

| 场景 | 位置 | 断言要点 |
|---|---|---|
| MCP untrusted 审批闭环 | `src/ai/mcp.rs` tests `dispatch_creates_pending_action_and_execute_confirmed_runs_it`（真 python echo fixture） | dispatch 回「待用户确认」、pending 参数封存等于调用参数、确认后输出 `echo: hi` |
| MCP trusted 直执行 | 同上 `trusted_server_dispatch_executes_directly_without_pending_action` | 直接输出、无 pending 产生 |
| 审批卡标签/路由 | `chat_proposal_review.dart` 等 | `run_skill_script`/`mcp_tool_call` 动作可读、可接受 |
| skill untrusted 封存链 | `src/ai/tool/skill_script.rs` trust_tests | pending 封存 `{skill,script,args}`、`execute_confirmed` 输出格式与完成态一致 |
| skill trusted 直执行 | 同上 | 输出与确认后同格式、无 pending、`.audit.log` 有 trust 记录 |
| trust 状态轮转 | 同上 | set true/false 幂等、状态可逆 |
| platform 校验/变体唯一/激活过滤 | `storage/mcp_servers.rs` tests | 值域拒绝、(name,platform) 冲突拒绝、同名变体允许、停用或不适用平台不激活 |
| 快照三铁律 | `sync/snapshot/tests.rs` `mcp_server_snapshot_strips_credentials_and_trust_and_local_rows_win` | 行到达但 headers='{}'、trusted=0、本机同 (name,platform) 行原样保留 |
| 迁移 47 | `src/storage/new_migrations.rs`（47 条跑完断言）+ `mcp_servers.rs` CRUD 测试 | user_version==47、trusted 列默认 0、CRUD 带 trusted 字段往返 |

 Flutter 侧 UI 测试不含信任开关的交互回归（SnackBar/tooltip 属展示层），是已知覆盖空白；修改设置页信任 UI 时需手测两处文案。

## 8. 全量实施记录

- 迁移 44–47 与字段命名同 schema 基线规则演进；迁移失败即抛错，没有静默降级路径。
- 已知取舍（v1 范围外，需求侧披露）：无 tools 元数据缓存失效策略（每次 dispatch 重读）、无 per-server 工具子集、无空闲回收/健康巡检、`test_mcp_server` 是同步阻塞调用（UI 层自行限时）。
- 曾出现的坑（后人避让）：frb codegen 对新增 Dto 字段的 impl 补齐两次失败（`McpServerDto`、`UserSkillDto`、`McpServerDto` trusted 均需手工 patch `src/frb_generated.rs` 后再 regen；barrel `ui/lib/bridge/api.dart` 需手工补 export）。
- `trusted` 相关信任豁免的**完整回归**随 `fc54c2d` 落地；后续触碰 dispatch 分支、`call_tool`、`execute_confirmed` 或审计落点的改动，必须以其为基线对照。
