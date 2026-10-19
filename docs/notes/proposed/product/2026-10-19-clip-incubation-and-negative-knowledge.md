# 摘录孵化闭环（讨论 → 纳入知识 / 丢弃为负知识）

- 日期：2026-10-19
- 状态：accepted→首版已实施（2026-10-20，Rust 侧全链 + 同步面；Flutter 分组/按钮后置）。评审澄清两项：①孵化闸门只作用于导入流，`save_text_page` 随手记生而入册（缺省 endorse）；②闸门只拦「自动投递」（编译/召回/引用挂载/依据校验），「按需读取」豁免（get_wiki_page 工具、`citation_for_page_explicit` 通道、派生稿采用核对——否则「先采用后纳入」死锁）
- 范围：把「导入摘录 → 页内 AI 讨论 → 定论」串成显式生命周期；补两个缺口——**孵化闸门**与**负知识**；AI 改动只落派生稿
- 性质：复用现有 `opinion` / 派生稿 / 版本采用 / 阅读状态，收紧一处缺省语义 + 扩展 `knowledge_sources`
- 关联：[FR-PES-004 LLM-Wiki 知识库闭环](../../../requirements/product/FR-PES-004-LLM-Wiki知识库闭环.md)、[docs/llm-wiki.md](../../../llm-wiki.md)、[CLAUDE.md LLM Wiki 维护](../../../CLAUDE.md)

## 1. 需求（用户原话归纳）

1. 从网上摘取内容，感兴趣 → **作为知识页加入 elsewhen**；
2. **和 AI 针对这个知识页讨论**，讨论期间 **AI 可以对该页做修改**；
3. 讨论足够后，用户决定：**① 作为知识加入系统**，或 **② 丢弃**；
4. 丢弃的话，**可作为"负知识"加入系统**（记住"我否定过"，而不是简单删掉）。

## 2. 现状映射（骨架已有七成）

| 步骤 | 现有机制 | 位置 |
|---|---|---|
| 摘取 → 知识页 | 导入流 `preview_knowledge_source` / `confirm_knowledge_source` → `kind=source` 页 | `src/api/knowledge.rs:326,340` |
| 页内 AI 讨论 | 「AI 处理本页」面板 + 页面对话 | `ui/lib/widgets/wiki_ai_chat_panel.dart` |
| AI 改动落「派生稿」 | 从 AI 回复保存产物 → `create_derivative_with_generation`（`area='derivative'`, `based_on`） | `src/api/knowledge_workflows.rs:68`、`src/storage/wiki.rs:200` |
| 多版迭代 + 采用某版 | 产物版本 + `adopt_artifact_version`（每 (parent,type) 采用一个精确 revision） | `src/knowledge/workflows.rs:105` |
| 认可 / 否定 | `set_wiki_opinion`（仅 `source`/`note`）：`endorse`/`reject`/`None` | `src/storage/wiki.rs:812` |
| 阅读/使用状态 | `knowledge_reading_states`：unread/read/valuable/adopted/archived（独立于认可） | `migrations/09-wiki-workflows/up.sql:61` |

**映射结论**：管道几乎都在，缺的是把管道接成闭环的两块 + 一处缺省语义。

## 3. 缺口与决策（已定）

### 3.1 缺口 1：没有「孵化中」——导入即自动编译

`confirm_knowledge_source` 落地为 `status='active'`、`opinion=NULL`；后台素材编译资格是 `COALESCE(origin.opinion,'')<>'reject'`（`src/knowledge_background/sources.rs:55`）——**NULL 被当"缺省认可"，未表态内容已在自动消化**。`reject` 是终审否定，不是"暂缓等我定论"。

**决策：复用 `opinion`，收紧缺省语义（仅 `source`/`note`）。**

- `opinion IS NULL` = **孵化中** → 不参与素材编译、不进知识召回/引用；
- `opinion='endorse'` = **已纳入** → 进入知识管线；
- `opinion='reject'` = **已否定** → 排除 + 记负知识；
- 其余 kind（insight/topic/person/…）保持 `NULL`=纳入不变，避免冻结 AI 产物/人物页。
- 抽取统一谓词 `knowledge_participating(page) = status!='archived' AND (kind NOT IN ('source','note') OR opinion='endorse')`，替换各调用点（§5）。
- **数据迁移回填**：存量 `source`/`note` 页 `opinion` 回填 `endorse`，保住现有行为；新导入从孵化开始。

### 3.2 缺口 2：AI 不能改 `source/note` 正文

`save_wiki_page_content` 对 `source/note` 直接拒绝（`src/storage/wiki.rs:736`）。

**决策：AI 只在派生稿上改，源页正文保持只读。**

- 讨论 → 从 AI 回复保存**派生稿**（`create_artifact_from_message`）；可多版迭代；`adopt_artifact_version` 采用其中一版；
- 源页（`source`）作为**来源/真源始终只读**，AI 的加工只落派生稿——符合「AI 只提议、核心决定」纪律。

### 3.3 缺口 3：负知识不是一等公民

`opinion='reject'` 在全管线约 20 处当过滤器（`opinion!='reject'`），只意味着"别用"，不会被记住/展示/召回。

**决策：丢弃默认记负知识（带原因），扩展 `knowledge_sources` 承载，不用新表。**

- `ALTER TABLE knowledge_sources ADD COLUMN reject_reason TEXT`；
- 复用 `knowledge_sources.identity`（URL/tweet id/内容指纹，`src/storage/knowledge.rs:57`）做同源识别；
- 「已否定」列表 = `knowledge_sources WHERE opinion='reject'` JOIN 其来源页；
- 再导入同源时按 `identity` 提示"你否定过这条来源"；
- AI 读面可召回（只读工具/上下文注入），避免重复推荐同类内容。

> ⚠️ **同步暗礁（必须在实施时处理）**：`knowledge_sources` 当前是**快照排除表**（`src/storage/sync/snapshot.rs:181`，理由 `local-knowledge-pipeline`），即**不同步**。而"负知识要记住"隐含跨设备可见。二选一：
> - **(a) 把 `knowledge_sources` 移入 `SNAPSHOT_CARRIED_TABLES`**（+ QUERY + apply Record 变体）——表很小（identity/kind/locator/opinion/created_at + reject_reason），是干净做法；
> - **(b) 接受负知识原因仅本机**（判定 `wiki_pages.opinion` 已同步，只有"原因"不同步）。
> **推荐 (a)**：负知识不跨设备等于半残。实施时按 (a) 落，并在门禁测试登记。

### 3.4 决策汇总（用户已定）

1. 「纳入知识」**要求先采用一版派生稿** → 前置校验 `knowledge_artifact_adoptions` 存在该来源的采用记录；
2. 负知识承载 = **扩展 `knowledge_sources.reject_reason`**（不新建表）；
3. 丢弃 = **`status='archived'` 保留痕**（不物理删）；
4. **不做**撤销否定（`reject → endorse` 不支持）；
5. 孵化页在素材库**独立分组**。

## 4. 闭环流程（定稿）

1. 摘取导入 → `source` 页，`opinion=NULL`（**孵化中**，不进管线）。
2. 页内与 AI 讨论；AI 修改保存为**派生稿**（多版、可对比）；用户 **`adopt` 采用一版**；源页只读。
3. 定论：
   - **纳入知识** → 前置：已采用一版派生稿。动作：`opinion='endorse'` + `reading_state='adopted'` → 源页进入编译管线；采用的派生稿作为该来源的加工产物保留。
   - **丢弃** → `opinion='reject'` + `status='archived'`（留痕）+ 写 `knowledge_sources.reject_reason`。
4. 不做撤销否定；要重新纳入需重新导入/新建。

新 API（建议）：`decide_source(slug, decision: endorse|reject, reason?)` —— 一个事务内校验并写 `wiki_pages.opinion/status`、`knowledge_sources.opinion/reject_reason`、`reading_state`。

## 5. 改动清单（预估）

**Rust**
- 谓词统一：`knowledge_participating`，替换——`knowledge_background/sources.rs:55`（编译资格改 `opinion='endorse'`）、`knowledge.rs:163`（召回/引用）、`knowledge/reading.rs:105`、`knowledge_background/refresh.rs:16,25,28,39,43`、`knowledge/dependencies.rs:53`、`knowledge_background/integration.rs:78,117`、`knowledge/organization.rs:126`、`storage/wiki.rs:1073`、`storage/knowledge.rs:660,809,953,1047,1076`、`knowledge.rs:233,886`。
- migration `50-...`：①`ALTER TABLE knowledge_sources ADD COLUMN reject_reason TEXT`；②回填存量 `source`/`note` 的 `opinion='endorse'`。
- `snapshot.rs`：把 `knowledge_sources` 从 EXCLUDED 移入 CARRIED（+ QUERY + apply Record）；更新门禁登记。
- 新 API：`decide_source(slug, decision, reason?)`；`list_rejected_sources()`；负知识只读工具（AI 读面）。
- 复用现有：`set_knowledge_reading_state` / `create_artifact_from_message` / `adopt_artifact_version`。

**Flutter**
- 素材库**独立分组**「孵化中」（`opinion=NULL` 的 `source/note`）。
- 页内动作：「纳入知识」（未采用派生稿时禁用并提示）、「丢弃」（原因输入 + 确认）。
- 派生稿：从回复保存产物、版本列表、采用（复用 `ui/lib/widgets/artifact_versions.dart`）。
- 「负知识 / 已否定」列表视图；导入时同源命中提示。

**测试**：Rust——NULL 不编译/不召回、endorse 编译、回填、`decide_source` 前置校验、reject_reason 写读、快照门禁；Flutter——桥接往返 + 孵化分组 + 纳入/丢弃动作。

## 6. 非目标（首版）

- 不放宽「AI 无确认改正文」——源页永远只读，AI 只在派生稿上写。
- 不给派生稿开 `opinion`（`set_wiki_opinion` 仍仅限 `source/note`）。
- 不做撤销否定。
- 负知识首版不做相似/向量检索，用 `source_identity` + 标题精确匹配。
- 不重做知识管线；只加一道前置闸门 + 一个负知识读面。
