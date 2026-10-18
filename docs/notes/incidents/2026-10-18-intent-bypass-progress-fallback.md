# 事故档案：AI 回复被任务输出顶包 / 承诺动作蒸发（ESP32-糖尿病 对话事故）

**日期**：2026-10-18　**状态**：根因已定位至代码级，待复现实锤（用户计划次日 Linux 下复现）
**涉及代码**：`src/ai/conversation/continuity.rs`（progress_fallback）、budget 闸门链路

## 一、现象时间线（手机端会话 93196d4f，已同步回桌面）

| UTC | 谁 | 内容 |
|---|---|---|
| 10:31 | user | ESP32+糖尿病患者能做哪些有潜力的事情？ |
| 10:32 | assistant | **正常回答**（789 字，质量好：糖尿病足预警最值/Siren Care 先例/避医疗器械注册/**结尾承诺**「要我把这块整理成一条产品灵感记进『产品灵感与赛道记录』那页吗？」） |
| 10:34 | user | 「好」（=input 51ee3409）→ **该回合失败（上下文超限），承诺的整理从未执行** |
| 10:37 | user | 「糖尿病足预警这个具体怎么做呢」（=input a7ef1f55/message 8ae8491e）→ 也失败 |
| 12:33 | assistant | **3343 字知识库 dump 冒充回复**（「已取得的结果：Elsewhen（个人工具）[[kb-…]]」× 多段：27 页知识库/产品灵感页等，与提问零相关） |

**物证**：
1. 灵感页 `wiki_pages.updated_at` 停在 2026-09-30 —— 「好」触发的整理从未发生
2. `message_response_links`：12:33 的 assistant 消息挂在输入 8ae8491e（「糖尿病足预警…」）上 —— 顶包实锤
3. 用户口述：期间有多次「本轮必要上下文超过预算」报错

## 二、代码级根因（已定位，待复现确认触发路径）

### 根因 A：run 失败后 progress_fallback 用工具结果拼文冒充 AI 回复
`src/ai/conversation/continuity.rs:164` `progress_fallback()`：
- run 失败（budget 闸门拒绝）后，代码**不跑模型**，把 context 里 system「本机数据直查」、pending 草稿、以及 `results` 里最近的 ToolResultMsg 拼成文本（:235 `已取得的结果：\n{readable_result(result)}`）直接作为 assistant 回复写库。
- 注释自称 "a report of known state, never a fabricated completed answer"，但实际后果：**知识检索工具返回的整页 dump 被当成回答**，用户完全无法分辨「这轮根本没回答」。失败真相（上下文超限）被淹没。
- 12:33 的消息内容与 readable_result 输出格式逐字吻合。

### 根因 B：承诺动作失败后蒸发
- 10:32 AI 承诺「整理成灵感」；10:34 用户确认；该回合 budget 超限失败后：
  - 无 UI 失败态/重试入口（用户只看到沉默或一句报错）
  - pending_actions（continuity 里有「N 项操作等待确认」提示）未把「用户已确认要整理」保留下来
- 预算参数背景：deepseek-flash 配 `context_window=65536`；知识库快照（手机 v48 同步后 27+ 页全文挂载）+ 长对话 + 工具定义叠加 → 必要上下文超预算。

### 附带：chat_runs 不同步导致取证受限
chat_runs/run_requests/** 是 `local-chat-execution-state` 数据集（snapshot.rs:150），**设计上不跨设备**。手机的 run 细节永远到不了桌面——这就是取证要靠 Linux 复现的原因。

## 三、Linux 复现测试方案（次日执行）

### 准备
1. 桌面正常启动 `./scripts/elsewhen.sh`（release so 已含 wire 修复 f534b4f）
2. **强制低成本触发超限**：设置里把 provider 的 `context_window` 调小（如 4096，保留正常对话发起），比自然堆爆快得多；记下原值，测完恢复
3. 知识库放 2-3 页长文（或直接用现有页）；确认桌面 MCP 工具注册正常（触发 tool results）

### 复现脚本（对话序列）
1. 问一个开放题，诱导 AI 回答并**主动提出整理进知识页**（如「这个思路值得记下来吗」→ 等 AI 问「要我记进 XX 页吗」）
2. 回「好」→ 观察下一轮（预期：budget 失败 → **观察点 2**）
3. 再追问一个具体问题（模拟 10:37）→ 观察回复内容（预期：**观察点 1**）

### 观察点
1. **顶包复现**：失败后是否出现「已取得的结果：…」消息（grep messages 最新 assistant 行）；它的 input_message_id 是否指向失败那轮的输入
2. **失败可见性**：budget 拒绝时 UI 表现（toast？聊天流里的报错文本？还是纯沉默）；有没有重试按钮
3. **run 取证**（桌面上直接查本地库，手机拿不到的那些）：
   ```bash
   sqlite3 ~/.local/share/elsewhen/elsewhen.db \
     "SELECT substr(id,1,8),status,error_kind,substr(error_text,1,80),request_count,substr(created_at,12,8) FROM chat_runs ORDER BY created_at DESC LIMIT 6;"
   sqlite3 ~/.local/share/elsewhen/elsewhen.db \
     "SELECT length(prompt), substr(model,1,30) FROM chat_run_requests ORDER BY rowid DESC LIMIT 3;"
   ```
4. **progress_fallback 调用路径**：谁在 run 失败后调它（grep 调用点，expected conversation.rs 调度链）——确认是「失败自动 fallback」还是「用户追问进度才触发」；若是前者，这就是静默顶包的机制本体
5. wiki_pages.updated_at 是否变化

### 修复方向预案（复现确认后按序评估）
1. **P1-A（continuity 顶包）**：progress_fallback 只允许在「用户显式问进度」时触发；自动触发时改为一条**诚实失败消息**（模板含 error_kind：如「上一轮回答因上下文超限没有完成，你的问题还挂着，可以缩小范围重试」）+ tool 结果永不进入用户消息流（进 reports 侧栏或折叠卡片）
2. **P1-B（承诺蒸发）**：run 失败时若上下文里有 AI 承诺+用户确认（pending 意图），失败消息必须点名「答应你的 XX 还没做」；重试应保留原任务（入 pending_actions 队列）
3. **预算前移**：组装请求前预检必要上下文体积；知识库挂载按相关度限量（Top-K 而非全量倾向），超限**截断挂载**而非拒绝整轮
4. 测试：continuity_tests 补两例（失败路由只产失败消息；显式追问才产结果报告）

## 四、本轮已完成的相关修复（已推送 origin/main f534b4f）
- `sync_wire_identity_content_conflict` 死结：acknowledged 同 id 重封（nonce 随机→密文必变）按幂等跳过 + 审计日志；provider dirty 清除接受已确认信封为送达。事故后果：手机消息上行被堵约 4 小时（provider:b149240c 重发循环）。
- 桌面对话回流已恢复（287 封确认、pending 0）。
- 手机 APK 需重打（含 wire 修复），时间允许时配合复现一起做。
