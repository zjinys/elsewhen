# 事故档案：AI 回复被任务输出顶包 / 承诺动作蒸发（ESP32-糖尿病 对话事故）

**日期**：2026-10-18　**状态**：根因已定位至代码级并已实施 P1-A/P1-B 修复（分支 `fix/progress-fallback-honest-failure-20261018`；Linux 复现实锤仍待跑）
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

## 五、P1-A / P1-B 修复进展（2026-10-18，分支 `fix/progress-fallback-honest-failure-20261018`）

### 已落地（全量 lib 测试 830 过）
- **P1-A 门控**（`continuity.rs`）：`progress_fallback` 增加 `failure` 参数。
  - 仅当最新用户消息命中 `FollowUp`（显式追问进展）时才允许"已取得的结果"式汇报；
  - 自动触发（模型没给出答复）一律走新 `failure_reply`：点名原请求 + 给出原因 + 有承诺则承认"同样没有做到"，**工具结果永不进消息流**（留在 run 记录，待 reports 侧栏承接）；
  - 用户已确认且已执行的写操作（内部记录 summary）仍如实汇报——那是承诺的兑现，不算顶包；
  - 失败消息固定带"这一轮还没有完成 / 还没有交付可用结果"指纹，并已加入 `interrupted_reply` 识别列表：失败说明之后追问进展会正确衔接原任务。
- **P1-B 运行失败可见**（`generation.rs`）：run 落终态失败后，追加一条诚实失败消息到对话流、挂在本轮输入上（budget→"必要上下文超过预算…"；生成错误→对应文案）。独立于终态写入：中途崩溃退化为现状（只有 failed run 记录），不会卡死状态；cancelled 不写（用户自己停的）。
- **测试**：continuity_tests 补齐事故对应三例——`automatic_failure_fallback_never_passes_tool_results_off_as_an_answer`（失败路由只产失败消息，检索结果不出现）、`only_explicit_progress_questions_may_report_known_results`（显式追问才产结果报告）、`failure_notice_quotes_request_and_admits_unfulfilled_promise`（确认后的失败点名未兑现承诺并引用原请求）；原 `fallback_renders_found_knowledge_without_exposing_protocol_json` 改写为断言不顶包。

### 预算前移（原预案第 3 条，已落地）
- **挂载预检上限**（`supplements.rs`）：单条补充材料在生成挂载视图时截到 24K 字符以内，超限部分用诚实声明收尾（「已按挂载上限截断：以上只是部分内容，不得当作全文使用；完整原文保存在输入队列记录中」）。针对手机 v48 一次挂 27 页全文的形态：原文不丢，挂载不再顶爆请求。
- **fit 内截断挂载取代拒绝整轮**（`budget.rs`）：请求超预算时，降级顺序变为「可选背景移除 → 早期工具结果占位 → **补充材料砰半截断** → 砍历史轮 → 才报错」。补充材料（TaskSupplement）是唯一可以无损重取的必要段（原文在输入队列记录），因此在砰历史轮之前先截；截断带固定声明「因超过请求预算被截断…不得当作全文使用」。只有当前问题本身超限时才仍然报错。
- **测试**：`oversized_supplement_is_truncated_instead_of_rejecting_the_turn`（fit 超限补充材料截断挂载、历史轮与当前问题完整保留、带如实声明）、`giant_supplement_is_clipped_with_honest_marker_at_mount`（挂载视图截到上限内且原文不变，常规尺寸不受影响）。全量 lib 测试 832 过。

### 挂载摘要层（预算前移第二步，已落地）
事故根因之一：`wiki_pages.summary` 当时只是正文前 120 字符的机械预览，挂载时没有「更短的独立版」可用，客户端只能整包携带 `content_md`。本轮把摘要变成真产物：
- **AI 生成摘要**（`knowledge.rs`）：编译契约新增 `summary` 字段（一到两句独立概括，最多 200 字，不得照拷正文开头）；落库优先用 AI 摘要，旧契约/空值退回正文预览。字数超限按既有模式触发一次修正重试。
- **建议表随行携带**（迁移 49）：`knowledge_proposals` 新增 `summary` 列（存量行为空，应用时退回预览），编译建议与后台自动建议两条路径都把摘要带进 `wiki_pages`。frb 绑定已重新生成（Dart 全量 403 测试过，`flutter analyze` 无新问题）。
- **引用式补充材料**：新增 FRB API `compose_page_reference_supplement(slugs, request)`（校验页面存在 + 标识安全），客户端把生成文本作为用户消息提交；挂载时（`supplements.rs::context`）消息开头的 `@wiki(slug)` 行展开为「标题 + 摘要 + get_wiki_page 按需取回路径」，引用不存在的页面就地如实声明（「不得假设看到过它」），纯文本补充不受影响，24K 挂载上限与 fit 截断仍作为最后防线。从根上结束「挂知识库=整包全文」的形态：全文回到按需检索（工具）路径。
- **测试**：`compilation_carries_ai_mount_summary_into_proposal_and_page`、`compilation_without_summary_falls_back_to_content_preview`、`v49_proposals_gain_empty_summary_column_without_touching_rows`、`page_reference_supplement_expands_summaries_and_discloses_missing`。全量 lib 836 过。

### 投递侧不设防的统一结论与确定性摘要挂载（关联 [2026-10-19 事故](2026-10-19-summary-pipeline-permanent-failure.md)，已落地）
复盘两次事故 + 本会话对话发现：**知识的编译侧一直在建设，投递侧从 2026-10 起是不设防的**——2026-10 的速度优化硬掉了每轮知识预载（固定 3.2k tokens），把「用不用知识」完全赌在模型的 search_knowledge_base 工具调用上。三次落败：①顶包轮检索成功了但结果被 progress_fallback 顶包，用户没看到知识；②手机端为保知识在场整包挂 27 页全文（用脚投票）；③主对话流出事轮模型忙于应对被砍历史，一次知识检索都没有。工具 schema 40K 占窗口 2/3，为 wiki 付的窗口租金换回 0 投递。
落地（折中重建，不是回到 3.2k 整包预载）：
- `knowledge.rs::summary_mount`：对当前用户问题 `select_knowledge`，最多 3 页、总字符封顶 3000，excerpt 换成 AI 摘要（`mount_summary`）；沿用「知识选材：」JSON 包装，`candidates_in_context` 引用校验与 usage 记录零改动（`current_candidates` 只比对身份字段，excerpt 可安全替换——已测验证）。
- `conversation.rs::relevant_knowledge_mount`：挂载段标 `SelectedEvidence / KnowledgeSelection / OptionalBackground`——预算紧张时最先让路，且不像工具结果那样成为顶包素材。
- 测试：`summary_mount_carries_ai_summary_with_verified_identity`、`summary_mount_stays_silent_when_nothing_relevant`、`knowledge_mount_segment_is_sacrificial_background`。全量 lib 839 过。
- 排队：工具 schema 瘦身（40K→<10K，关联 10-19 档案 P2）；reports 侧栏（让「搜过什么、引用了什么」可见）。

### 待办
1. Linux 复现实锤（第三节脚本不变；顶包路径应已无法复现，改为验证失败消息出现且检索内容只在 run 记录里；超长补充材料现走截断挂载，验证不再出现「必要上下文超过预算」拒绝整轮）
2. reports 侧栏/折叠卡片（Flutter UI 层）：给失败轮的工具结果一个非聊天流出口
3. 手机 APK 重打（含 wire 修复 f534b4f 与本次修复）
4. 补一个 run 级 E2E 集成测试（execute 失败→消息落库）——本次手工推演，由复现脚本覆盖

## 四、本轮已完成的相关修复（已推送 origin/main f534b4f）
- `sync_wire_identity_content_conflict` 死结：acknowledged 同 id 重封（nonce 随机→密文必变）按幂等跳过 + 审计日志；provider dirty 清除接受已确认信封为送达。事故后果：手机消息上行被堵约 4 小时（provider:b149240c 重发循环）。
- 桌面对话回流已恢复（287 封确认、pending 0）。
- 手机 APK 需重打（含 wire 修复），时间允许时配合复现一起做。
