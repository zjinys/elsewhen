# 事故档案：「保存」确认后 AI 空回复，兜底模板误导用户 + 「存为主题」按钮报错

**日期**：2026-11-02　**状态**：两处根因均已修复并有回归测试
**涉及代码**：`src/ai/conversation/continuity.rs`（failure_reply 措辞）、`src/wiki/tweet_import.rs`（save_text_page_with_kind endorse）

## 一、现象时间线（会话 93196d4f）

| UTC | 谁 | 内容 |
|---|---|---|
| 01:36 | assistant | 目标细化答复，结尾「说『保存』我就会草拟页面供你确认」 |
| 02:53 | assistant | nvzhuang 市场评估，结尾同样引导「说『保存』」 |
| 03:01:48 | user | 「保存」 |
| 03:01+ | assistant | gpt-oss:120b@wududu 连续 3 轮空回复（content_len=0, tool_calls=0，completion 全烧在 reasoning 被网关吞掉）→ 兜底消息：「关于你刚才的要求『评估一下 nvzhuang…』，这一轮还没有完成……你的问题还挂着，说『继续』」 |
| 03:04 | user | 改点消息气泡「存为主题」按钮 → 弹窗报错「只有采集素材页（source/note）可以表态评价，当前 kind=topic」 |

两个独立缺陷在同一串交互里先后命中。

## 二、根因

### 根因 1：确认词失败后兜底模板张冠李戴
`failure_reply` → `pending_request()`：`"保存"` 命中 `is_confirmation` 词表 → 回溯到上一条"真请求"（nvzhuang 评估）→ 套「问题还挂着说继续」模板。用户刚才是**确认动作**，不是提问；「说继续」完全无法理解。数据库佐证：`pending_actions` 无新行，草拟从未发生——不是草拟错了，是根本没执行。

### 根因 2：topic 页保存误入 endorse 通道
`save_text_page_with_kind(kind="topic")` 无条件调用 `set_wiki_opinion(endorse)`，而该接口白名单只有 source/note，对 topic 直接 bail。后果：**页面已入库**（topic-15e2de08 于 03:04:24 成功创建），但 API 返回错误，UI 显示异常——保存实际成功用户却以为失败。

## 三、修复

1. `pending_request` 返回 `was_confirmation`；确认词失败且此前无预告时改说「你刚才确认的『保存』这一轮没有生效……再说一次『保存』即可重试」，不再追加「说继续」尾巴。此前有预告（promised）时保留原「承认没做到」模板（2026-10-18 事故语义不变）。
2. `save_text_page_with_kind`：endorse 仅对 note/source 执行；topic 页入库即视为认可，不额外表态。

## 四、回归测试

- `src/ai/conversation/continuity_tests/progress_recovery.rs::confirmation_failure_says_save_not_applied_not_question_pending`
- `tests/topic_endorse_test.rs`（topic 保存不 bail + note 仍 endorse）

## 五、附带观察

- wududu 网关 gpt-oss:120b 当天至少 3 次同款空回复（01:34/01:35/03:01），reasoning tokens 烧掉但内容不回传。空回复重试日志计数显示 retry 始终为 1（`empty reply; retry=1 finalize_next=false/true`），值得后续核对计数逻辑。
- 另一并行工作流（goals Phase G1）恰好把修复 1 的 tweet_import.rs 改动顺带提交进 b55dde4；continuity.rs 修复保留在工作区。

## 六、后续事故（同日 03:45-03:52）：派生物确认执行失败

**现象**：用户在《一个女装换装的应用》页面聊天请 AI 写抖音文案，AI 调 `save_wiki_revision(save_as=derivative)` 生成待审提案，但回复称「已生成派生产物，请在派生产物标签查看」；用户接受提案后执行失败：「上游知识已失效，请先修复其依据」，事务回滚。

**根因链**：
1. AI 话术：只登记了待确认草稿却说「已生成」→ 词表补充「已生成/生成好了/已创建出/已经生成」，核验回炉（85d7c23）。
2. 「派生产物」标签只显示已生成页；待审提案的入口是页面聊天面板的「审阅提议」按钮（带徽标）。
3. 结构性断链：`save_topic_page` 创建的页无事件/无快照/无 source_url，`citation_for_page`（select.rs 散文守卫）返回 None，`dependencies::capture` 引用核验拒收 → 派生物永远建不成。凡是「存为主题」的页全中。

**修复**（派生侧降级，引用契约不动）：
- 曾尝试在 select.rs 豁免 kind=topic，但破坏既有契约 `keeps_free_notes_unattributed`（无归因自由笔记不可作引用/依据），已回滚。
- `dependencies::capture_for_derivative`：上游纯无来源 → 降级为空依赖（靠 `based_on` 保留追溯，派生物信任来自 generation provenance + 用户审阅）；上游有来源但不可引用（快照过期/事件失效）或归档/否定/待复核 → 仍然拒绝。
- `workflows::adopt`：无来源派生物跳过引用核对（失效由 stale/归档检查覆盖）。
- 回归测试 `derivative_of_unattributed_topic_degrades_to_no_dependency_but_fails_when_stale`。

**遗留**：既有失败的那份提案（0b81d790）已回滚，用户重新审阅即可成功；如果 UI 允许重新执行失败提案更好。

**补充（12:30 二次失败原因）**：用户 12:30 重新审阅仍失败，原因是运行中的应用（12:27 启动）加载的 `target/release/libelsewhen.so` 是 12:21 编译的，当时实际未包含 12:09-12:10 的修复源码（修复只跑过 debug profile 的测试）。重新 `cargo build --release` 并重启应用后生效。端到端回放测试 `save_wiki_revision_derivative_executes_on_unattributed_topic` 验证完整 `execute_pending_action` 链路通过。

**补充二（12:42 三次失败，真失败点）**：12:36 重编 release 并重启后仍失败。追查发现执行链有两道 capture：
`create_derivative_with_generation`（已降级）之后还有 `bind_proposal_materials`（`storage/chat_page_tasks.rs`），
它对提案的 `page_task` 材料页调用**严格** `capture`，同样被无来源 topic 页拒收——这才是真正炸点。
已改为逐页走 `capture_for_derivative` 降级通道。端到端测试带 `page_task`，并通过 stash 回退法
验证了测试确实覆盖该路径（回退时复现「上游知识已失效」，恢复后通过）。

## 七、后续发现（13:49 保存成功但两页关系不可见）

用户最终通过页面聊天的 `save_knowledge_draft`（a0800d13）成功保存《nvzhuang 抖音宣传系列·文案与素材计划》，
但产生三个观察：

1. **AI 未带证据**：页面聊天里模型直接草拟，未先 `get_wiki_page`/`search_knowledge_base`，
   `verified_evidence=[]` → 页面落库无来源、无依赖边，结果文案自称「尚无可核验来源」。
   `candidates_in_context`（knowledge/bind.rs）只认「知识选材：」块和工具结果前缀，
   页面任务的 CurrentPage 材料（page_tasks.rs）不算候选——当前页明明就在上下文里却不能作为来源被引用。
2. **save_knowledge_draft 不写 based_on / artifact_versions**：派生关系只存在于
   `create_derivative_with_generation`（save_wiki_revision 路径）里；草稿路径生成的
   method/insight 页即使内容源自当前页，也无任何关系记录。
3. **数据修复**：给 kb-nvzhuang 页补 `based_on='topic-15e2de08'` + `knowledge_artifact_versions`
   记录（与 create_derivative 相同的两个写入），父页详情的产物版本区即可显示。

遗留改进方向：页面聊天的草拟把当前页自动登记为来源候选（或 save_knowledge_draft 支持显式 parent 参数）。

**修复（同日 §7）**：`save_knowledge_draft` 新增可选 `parent_slug` 参数：
- 草拟侧（write_tools.rs）：与 `source_slugs` 互斥；带 parent_slug 时不走证据通道（派生信任来自父页+用户审阅）。
- 执行侧（authoring.rs save_draft）：校验父页存在 → 写 `based_on` + `knowledge_artifact_versions`（与
  `create_derivative_with_generation` 同一写法）+ `capture_for_derivative` 降级依赖捕获。
- 回归测试：`knowledge_draft_with_parent_slug_attaches_as_derivative`（save_draft 层）、
  `knowledge_draft_parent_slug_end_to_end_via_pending_action`（执行层 + 父页缺失拒绝）。
- 工具描述引导：页面聊天里草拟时把当前页填为 parent_slug。
