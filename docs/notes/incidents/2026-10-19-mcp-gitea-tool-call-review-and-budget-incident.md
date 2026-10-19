# 事故档案：Gitea MCP 工具调用失效与上下文预算熔断（「你看看gitea项目anvil有哪些bug」测试分析）

**日期**：2026-10-19  
**测试场景**：对话输入「你看看gitea项目anvil有哪些bug」，尝试通过 Gitea MCP 查询私有 Git 仓库 anvil 的未解决 Issue  
**涉及模块**：
- `src/ai/tool/mod.rs`（MCP 未信任拦截与暂存机制）
- `src/ai/approval.rs` / `ui/lib/widgets/chat_proposal_review.dart`（审阅卡执行与回执展示）
- `src/ai/budget.rs`（非 GPT 模型 token 字节估算与上下文硬熔断）
- `src/ai/mcp.rs` / `gitea-mcp`（全量工具 Schema 注入）

---

## 一、现象还原与用户测试过程

用户在桌面端 Elsewhen 中接入本地私有 Gitea 的 MCP 服务（`gitea-mcp`），在会话中发起提问：
> **用户**：「你看看gitea项目anvil有哪些bug」

整个测试经历了两次截然不同的失败体验：

### 阶段一：未设全局可信（`trusted=0`）时的测试过程
1. **AI 回复异常**：
   - AI（GLM-5.3-Flash）并未直接给出 bug 列表，而是回复了一句带有“打太极”性质的话：
     > *“已经发起了在 Gitea 上搜 anvil 仓库的请求，不过 MCP 调用需要你在审阅卡里点一下接受才会执行。确认后我就去把它的 bug（open 状态的 issue）列给你 🔍”*
2. **用户点击审阅确认**：
   - 用户打开对话右上角的「审阅提议」弹窗，找到了该项 MCP 调用（`mcp__gitea-mcp__search_repos`），点击「接受这项」。
3. **用户体验与实际表现严重脱节（两大痛点）**：
   - **痛点 A（输出位置严重错位）**：接受后，并没有在主对话流中出现 AI 的回答，而是**直接在审阅弹窗的标题正下方，突兀地刷出了一大段未经排版的原始 JSON 字符串**。
   - **痛点 B（输出内容并非用户所要，工具调用未闭环）**：弹窗里刷出来的 JSON 文本是：
     ```json
     [{"archived":false,"clone_url":"http://192.168.101.116:30000/ai/anvil.git","default_branch":"main","full_name":"ai/anvil","open_issues_count":10,...},
      {"archived":false,"clone_url":"http://192.168.101.116:30000/ai/anvil-deploy.git","full_name":"ai/anvil-deploy","open_issues_count":0,...}]
     ```
     **里面仅仅是 Gitea 仓库的元数据（搜索结果），根本没有包含任何具体 bug / issue 的标题、内容和排查信息！**
   - **痛点 C（承诺动作蒸发，对话直接死亡）**：关闭弹窗回到主聊天界面，聊天界面毫无动静，模型承诺的“确认后我就去把它的 bug 列给你”彻底消失，AI 永远不会再次开口。

### 阶段二：用户设为全局可信（`trusted=1`）后的二次测试
1. 用户在「设置 → MCP」中将 `gitea-mcp` 更改为全局可信（允许直接执行）。
2. 用户再次在同一对话中发送：
   > **用户**：「你看看gitea项目anvil有哪些bug」
3. **直接崩溃熔断**：
   - 界面未输出任何回答，直接报错提示：
     > **`生成失败，上下文超限`**
     > （底层错误：`本轮必要上下文超过预算；输入已保存，请缩小任务范围`，error_kind: `budget`）

### 对照组：OpenCode 的正常行为
在配置相同 Gitea MCP 的 OpenCode 工具中，发送完全相同的指令：
- OpenCode 在后台自动执行两步工具调用：先 `search_repos(query="anvil")` 命中 `ai/anvil`，接着自动调用 `list_issues(owner="ai", repo="anvil", state="open")`。
- 在主界面直接清晰地以 Markdown 列表格式输出了 10 个具体 Issue 的标题、编号与直达链接（如 `#11 人物资产候选`、`#10 【web-portal】【今日交付】改为交付历史`、`#6 BullMQ 监控整合问题` 等）。

---

## 二、代码级深度根因分析

通过审查本地数据库 `~/.local/share/elsewhen/elsewhen.db` 与调试日志 `agent-debug.log`，还原底层每一环节的具体物证：

### 1. 阶段一根因：伪完成误导、多轮链路断裂与回执渲染错位

#### 根因 1.1：审批拦截未挂起对话，而是伪造“工具成功”诱骗模型输出承诺
- 代码路径：`src/ai/tool/mod.rs:724-742`
  ```rust
  let action_args = serde_json::json!({
      "full_name": call.name,
      "arguments": call.arguments,
  });
  match store_create_pending(store, conversation_id, "mcp_tool_call", &action_args) {
      Ok(_) => ToolResultMsg::ok(
          call,
          format!(
              "工具 {} 已暂存待用户确认（MCP 服务器为第三方代码，执行前需要逐次审批）。在审阅卡点击接受后执行。",
              call.name
          ),
      ),
      Err(e) => ToolResultMsg::err(call, e),
  }
  ```
- **问题**：系统把拦截创建的 `pending_action` 伪装成一个正常的 `ToolResultMsg::ok` 返回给了模型。模型以为这个工具已经“执行完毕且返回了这段文字”，所以根据模型的自然语言心智，它顺理成章地向用户回复：“我已发起申请，等你在审阅卡确认后我再去查”。

#### 根因 1.2：用户接受审阅提议后，无 Agent 唤醒闭环（多轮调用断裂）
- 代码路径：`src/ai/approval.rs:25-50`
  ```rust
  pub(crate) fn execute_claimed_proposal(store: &Store, proposal: &ChatProposal) -> Result<ChatDecision> {
      ...
      proposal.pending_action().and_then(|action| super::tool::execute_pending_action(store, &action))
  }
  ```
- **问题**：当用户在 UI 点击「接受这项」时，Rust 仅仅在后台执行了 `execute_pending_action`（调用了 Gitea 的 `search_repos`），并将结果作为 receipt 写入 `chat_proposal_decisions` 表。
- **系统没有编写任何唤醒对话 Agent 的逻辑**！Agent 没有拿到 `search_repos` 搜出来的仓库名字 `ai/anvil`，因此根本不会发起第二步的 `list_issues`。整个交互链路在第 1 步搜仓库时就被切断了。

#### 根因 1.3：前端审阅弹窗直接把工具 Raw JSON 回执当成 UI 最终结果显示
- 代码路径：`ui/lib/widgets/chat_proposal_review.dart:516-525`
  ```dart
  if (_receipt != null) ...[
    Text(
      _statusLabel(_receipt!.status),
      style: Theme.of(context).textTheme.titleMedium,
    ),
    SelectableText(_receipt!.result), // ⚠️ 直接把原始执行结果 JSON 倾倒在弹窗正文中
    const Divider(),
  ],
  ```
- **问题**：审阅弹窗（`ChatProposalDetailDialog`）原本的设计场景是“修改知识库页面”、“创建待办事项”、“保存规则”，这类操作的回执是一两句状态提示。但对于 `mcp_tool_call`，`receipt.result` 是完整的 API 返回 JSON。用户在弹窗点击确定后，弹窗直接把 `search_repos` 查到的几十行原始代码仓 JSON 贴在用户眼前，造成强烈的“输出位置不对、内容不是我想要的”体验崩塌。

---

### 2. 阶段二根因：全量工具 Schema 注入 + 字节降级估算 + 上下文预算硬熔断

当用户把 `gitea-mcp` 改为全局受信任后，模型跳过了审批拦截，直接进入真实执行。但立刻撞上了 Elsewhen 的**双重预算机制瓶颈**：

#### 根因 2.1：Gitea MCP 暴露了 53 个工具，未做过滤或延迟加载
- 物证：通过向 `gitea-mcp` 发起 `tools/list`，该服务一次性导出了 **53 个工具**（包含大量用户根本用不到的 Actions、Secrets、Release、Tag、Org 等庞大接口）。
- 这 53 个工具的 JSON Schema 序列化后长达 **34,204 字节**。

#### 根因 2.2：`src/ai/budget.rs` 对非 GPT 模型使用字节数计算 token（致命放大）
- 代码路径：`src/ai/budget.rs:468-482`
  ```rust
  pub fn text_tokens(model: &str, text: &str) -> usize {
      if model.starts_with("gpt-4o") || model.starts_with("gpt-5") ... {
          tiktoken_rs::o200k_base_singleton().encode_ordinary(text).len()
      } else if model.starts_with("gpt-4") {
          tiktoken_rs::cl100k_base_singleton().encode_ordinary(text).len()
      } else {
          text.len() // ⚠️ 对 deepseek / glm / qwen 等模型，退化为 UTF-8 字节长度！
      }
  }
  ```
- 当前会话配置的模型是 `deepseek-flash`。
- 因为不以 `gpt-` 开头，`text_tokens` 将整个请求中包含的所有字符串按字节数 1:1 统计为 token 数量！
- 在 `agent-debug.log`（Run ID `e13dfa7e...`）第 359 和 365 条事件中，清晰记录了：
  **`tools_tokens: 40032`**！
  （实际 Gitea 34,204 字节 + Elsewhen 原生内置工具 5,828 字节，正好被算成了 40,032 个 token）。

#### 根因 2.3：不可裁剪必要上下文击穿了模型预算上限
- 数据库中 `deepseek-flash` 的 `context_window` 配置为 **65,536**。
- 系统根据安全原则计算：
  - `output_reserve` = 4,096
  - `safety_margin` = 3,276
  - `input_limit` = 65,536 - 4,096 - 3,276 = **58,164**。
- 系统固定不可裁剪的基底开销（System Prompt、个人规则、知识库快照索引）约占 **16,000**。
- **固定基底（16,000）+ 工具 Schema（40,032）= 56,032 tokens**，距离 58,164 的熔断红线只剩下 **2,132 tokens**。
- **Round 0 运行情况**：
  模型决定调用 `mcp__gitea-mcp__list_issues`。此时尚未返回结果，上下文总量为 57,350，以 814 tokens 的微弱优势惊险通过。
- **Round 1 熔断崩溃**：
  Gitea MCP 成功返回了 10 条 issue 的 JSON 数据（长度 2,283 字节，折合 2,835 tokens）。
  模型试图将这 10 条 issue 喂回给模型以生成最终回答。
  此时系统执行预算裁剪：**即便将过去所有的聊天历史记录全部裁剪为 0（`tokens_after: 0`）**，当轮必要上下文（系统提示 + 全量工具定义 + 本轮 Tool 结果 + 协议帧）仍然达到了：
  **`60,356 tokens`**！
- **60,356 > 58,164**！
- 预算管理器判定“必要上下文超过窗口限制，且无法再通过裁剪历史满足预算”，于是直接触发本地强行拒绝（`local_rejection`），并向用户抛出：
  > **`本轮必要上下文超过预算；输入已保存，请缩小任务范围`**。

---

## 三、对比与架构心智差距

| 维度 | OpenCode / 现代 Agent 架构 | Elsewhen 当前实现 |
|---|---|---|
| **MCP 工具交互心智** | 工具是模型大脑的自然延伸，工具执行结果只作为中间观察值（Observation）反馈给 Agent 继续思考 | 把只读查询的 MCP 也强行塞进“写入型审阅卡”，将中间步骤结果直接作为最终交付给用户 |
| **审批流连续性** | 若需确认，确认后自动恢复 Agent Loop（唤醒模型并将 Observation 喂回） | 审阅确认后只跑一次单个动作并记入 DB 回执，执行即终点，对话流永久断裂 |
| **回执 UI 交付** | 弹窗仅反馈操作状态（如“已执行”），真正的数据由模型在聊天界面中用 Markdown 总结回答 | 弹窗直接把工具返回的几千字节 Raw JSON 糊在弹窗正文中，聊天界面毫无响应 |
| **多工具加载策略** | 按需激活、动态工具选择或延迟加载（Deferred Tools） | 一次性将三方 MCP 的全部 50+ 个工具定义全量塞进每一次模型请求中 |
| **Token 预算评估** | 采用 BPE 分词器或针对中文/JSON 的合理压缩估算比率（如 1 token ≈ 3~4 bytes） | 非 GPT 模型简单粗暴退化为 `text.len()` 字节数，将 34KB 的 JSON 夸大为 34,000 tokens，严重挤占上下文 |

---

## 四、改进与治理方案

为了彻底解决上述问题，建议分三层实施改造：

### P0：修复审批流恢复闭环与回执展示错位
1. **区分读写工具策略**：
   - MCP 工具的注解中包含 `readOnlyHint: true`。对于只读查询类工具（如 `search_repos`、`list_issues`、`get_file_contents`），即使在非全局可信模式下，也应当允许自动执行，或至少提供“本会话允许只读查询”选项，无需逐次审批。
2. **构建审批后的续跑回路（Resume Agent Loop）**：
   - 当用户在审阅卡接受 `mcp_tool_call` 后，不能简单停在 `execute_claimed_proposal`。应当触发该会话的唤醒流程，将该工具的实际返回结果作为 `ToolResultMsg` 注入消息流，自动发起下一轮生成，让模型完成多轮调用（例如从 `search_repos` 到 `list_issues` 并输出最终总结）。
3. **审阅卡 UI 降噪**：
   - 审阅弹窗内对于结构化 JSON 数据应折叠展示或仅显示摘要，明确告知用户“已执行，正在等待 AI 回复”，而不是直接贴大段原始 JSON。

### P1：MCP 工具延迟加载与按需暴露（Schema 瘦身）
1. **利用现有的 `defer_tools` 机制**：
   - Elsewhen 已经有 `registry.defer_tools()` 和 `enable_tools` 的框架。第三方 MCP 服务（尤其是工具数 > 5 的）不应在开局就注入全量 schema，而应先暴露服务器级概览或按需声明，大幅削减固定上下文占用。
2. **Server 级工具过滤配置**：
   - 在 MCP 服务器配置中增加工具白名单或标签（例如 Gitea 只勾选 Issue 和 Repo 查询），屏蔽掉不必要的 Actions/Secrets 等重型工具。

### P2：校准非 GPT 模型的 Token 估算算法
1. **替换 `src/ai/budget.rs` 中的纯字节退化**：
   - 引入针对 DeepSeek / Qwen 等开源主流模型的合理分词器（或基于常见 UTF-8 / JSON 的保守压缩比，如 `text.len() / 2` 或接入通用 BPE 分词库），避免将 34KB 工具 schema 误算为 40,000 tokens。
2. **配置推荐**：
   - 在使用大型 MCP 时，如果使用的模型上下文较小（如 64K），建议在设置中改用 128K+ 窗口的模型（如 `glm-5.3-flash`），或者调大对应 Provider 的 `context_window` 配置。
