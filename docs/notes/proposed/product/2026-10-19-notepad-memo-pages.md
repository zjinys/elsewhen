# 记事本页（随手记想法，不经 AI）

- 日期：2026-10-19
- 状态：proposed（方案待评审；2026-10-19 存储设计评审已通过，评审结论见 §8）
- 范围：新增用户自有 wiki 页类型 `memo` + 记事本入口与快记追加；**不**新增事件 / 分析 / 知识消化管线
- 性质：wiki 页类型扩展 + 交互入口，不涉及同步协议演进（复用 `wiki_pages` 快照）
- 关联：[FR-PES-004 LLM-Wiki 知识库闭环](../../../requirements/product/FR-PES-004-LLM-Wiki知识库闭环.md)、[docs/llm-wiki.md](../../../llm-wiki.md)、[LLM Wiki 维护（schema v7/v8/v9）](../../../CLAUDE.md)

## 1. 需求（用户原话归纳）

1. 一个**记事本**：一些想法可以随手记一下；
2. **AI 不用处理**这些内容；
3. 落点是一个**专门记录的知识页**（用户明确倾向「记录到一个特定知识页」，认为知识页灵活性更高——可多页、可标签、可检索、可导出）；
4. 期望的灵活性：能自己挑一个页面写、能持续追加、未来能有多本记事本。

## 2. 现状盘点（为什么不能直接复用现有 `note`）

- **`note` kind 已存在，但语义是「导入素材」**：`WIKI_KINDS` 注释写明「用户粘贴笔记（M1 kind 拆分：从 topic 独立为素材档 note）」（`src/wiki.rs:37`）。`source/note` 被当作**采集素材**：`save_wiki_page_content` 直接拒绝人工编辑正文（`src/storage/wiki.rs:736`），AI 侧也只累加证据、不改内容（`src/storage/wiki.rs:357`、`src/storage/mod.rs:389`）。它是「我贴进来一份材料」，不是「我持续往里写」。
- **可编辑页面已经有人工持有保护**：`save_wiki_page_content`（`src/api/wiki.rs:208`）保存后置 `human_edited_at`，此后 AI digest 把该页视为人工持有、不再整篇覆盖正文（`src/storage/wiki.rs:353-359`）。这正好是记事本需要的「AI 不碰我的正文」。
- **现有编辑器可直接复用**：`ui/lib/wiki/wiki_content_editor.dart` + `ui/lib/widgets/wiki_page_detail_view.dart` 已有编辑态、保存链路（`documentToMarkdown → saveWikiPageContent`，乐观锁）、未保存保护。UI 侧还显式区分「素材页 source/note 只读」（`wiki_page_detail_view.dart:636`）。
- **事件路径默认喂 AI**：`capture → recordUnifiedInput → Store::insert_event` 同事务建 `analysis_jobs` + `knowledge_digest_jobs`（`src/storage/events.rs:88-93`）。虽有「可记录性」可跳过消化（`src/storage/digest.rs:132`），但随手记不该走事件管线、也不该污染时间线/回顾。
- **没有「手动新建 wiki 页」的公开 API**：现有页面来自导入（`src/api/import.rs:91`）、来源确认（`src/api/knowledge.rs:385`）、AI 消化/主题整合（`src/knowledge.rs:827`）或人/项目关系建档。记事本需要补一个小的建页入口。

## 3. 方案 A（推荐）：新增 wiki kind `memo`（记事本）

**页面语义**

- `memo` 是**用户自有页**：只能由用户创建、由用户编辑；正文永远是用户写的。
- AI 侧**永不提议、不合并、不改写** `memo`：digest 的校验与提示词显式排除该 kind（不能只加进 `WIKI_KINDS` 就算完，因为 AI 提议也用 `validate_kind` 做闸门，需要区分「AI 可提议的 kind」与「仅人工的 kind」）。
- **kind 拆三张白名单（评审钉死，2026-10-19）**：现有 `validate_kind` 只有一个布尔，挡不住隐式集合。实现时拆为：
  - **AI 可写**（digest 提议、`save_knowledge_draft` 等工具、`knowledge/organization.rs` merge/split、`knowledge/maintenance.rs` 跨页解决）——`memo` 不在内；
  - **AI 可读**（`search_knowledge_base`、`@wiki(slug)` 引用式补充）——`memo` 在内，但见下条挂载边界；
  - **人类专属**——`memo` 只在这里。
  - **显式排除点**：`INSIGHT_KINDS`（认知推微取材，`src/wiki.rs:42`）与 `select_context_pages(pages, WIKI_KINDS, …)`（`src/wiki.rs:1360`，按全 kind 选上下文页）都**不得**包含 `memo`，否则随手记会被拿去推用户画像/默认挂载。
  - grep 所有 `validate_kind` / `WIKI_KINDS` 使用点，逐点归类，不允许漏点。
- **AI 读取的挂载边界（评审钉死）**：`memo` 进 AI 路径**只走引用式补充材料**（标题 + 摘要 + 按需取回），**永不整页挂载**——memo 持续追加后迟早超过 24K 挂载截断线（`supplements.rs`），整页挂载必然半截进上下文（10-18 事故的教训）。

**入口与交互**

- 知识域新增「记事本」入口（列表 + 详情），复用现有 wiki 编辑器（富文本 + 保存 + 乐观锁 + 未保存保护）。
- 详情页顶部一个**「追加一条」快记框**：输入想法 → 追加为带时间戳的一个段落/块 → 自动保存。围绕「随手记」优化，而不是每次进完整编辑态。
- **快记必须走 append API，不走乐观锁（评审钉死，2026-10-19）**：`save_wiki_page_content` 的乐观锁（`expected_updated_at`）为编辑器场景设计；快记框「打开→输入→追加→自动保存」在双设备同时追加时会让后保存方失败——对随手记，失败 = 丢一条想法，比 wiki 编辑严重。`append_wiki_page_content(slug, block)` 在 Rust 侧单事务内读-改-写，不经 Dart 往返的版本号。完整编辑态仍走乐观锁不变。
- 新建/多本：默认一本「我的记事本」，允许再建更多 `memo` 页（标题可改），满足用户说的「灵活性」。

**零 AI 保证**

- 写 `memo` 页 = 直接 `save_wiki_page_content`（或新增的追加 API），**不建 event / analysis_job / digest_job**。所以 AI 从头到尾不会看到或处理这些内容。
- AI 读取侧（对话挂载、知识引用补充）默认也可以读到（用户若需要），但**只读**：`memo` 页不进 AI 提议的写入目标集合。

**同步与检索（白拿）**

- `wiki_pages` / `wiki_revisions` / `wiki_log` 已在快照 carried 集（`src/storage/sync/snapshot.rs:45-55`），多设备自动同步。
- 标签、搜索、Markdown 导出等既有能力全部复用。

**改动清单（预估）**

- Rust：`WIKI_KINDS` 增 `memo`；新增「仅人工 kind」集合并让 digest 校验/提示词排除；新增 `create_wiki_page(kind, title)` 与（**必选**，非可选）`append_wiki_page_content(slug, block)`（Rust 单事务读-改-写，见 §3 快记条）；`WikiPageDraft` 复用。
- Flutter：`kindLabel` 增「记事本」（`ui/lib/models/wiki_page.dart:185`）；`SidebarTab` 或知识域内新增入口；复用 `wiki_content_editor.dart`；新增快记追加框；`list_wiki_page_metas`/侧栏可见性过滤确认包含 `memo`。
- 测试：Rust 建页/追加/人工持有不被 AI 覆盖；Flutter 桥接往返 + 追加保存。

## 4. 方案 B：改造现有 `note` kind 为可编辑

- 只需去掉 `source/note` 的只读限制，避免新增 kind。
- **不推荐**：会改变导入素材的语义与保护策略（`save_wiki_page_content` 拒绝、AI 只累加证据等一串判断都以 `source/note` 为支点），影响面大且容易在导入流上引入回归。

## 5. 方案 C：复用可编辑的 `topic` kind

- 不新增 kind，直接用 `topic` 页当记事本，靠 `human_edited_at` 保证内容不被覆盖。
- **不推荐**：`topic` 参与 AI 主题发现/整合/组织（`docs/CLAUDE.md` schema v9），会把「我的记事本」卷进 AI 主题治理；侧栏也会把它当主题展示，语义混。

## 6. 待评审确定的问题

1. **单页 vs 多页**：默认一本 + 允许新建多本（推荐），还是固定单页？
2. **快记入口落点**：是否把 Capture 浮窗也接进来（写着写着选「存到记事本」），还是只做页面内快记框？
3. **AI 读取权限**：~~对话/知识引用是否允许读 `memo`~~ → **已定（2026-10-19 评审）**：读允许、写禁止；读取只走引用式（标题+摘要+按需取回），不整页挂载；AI 引用 memo 内容时须标注「用户私人记事，不是已核验知识」。
4. **是否随同步**：默认随同步；是否需要「仅本机」选项（敏感随手记）？→ **评审建议**：首版不做仅本机，产品说明写清「memo 会同步到所有设备」。
5. **时间线归属**：记事本**不**进回顾/日视图（与事件解耦）——确认这是期望行为。✅ 评审同意。

## 7. 非目标（首版）

- 不做 markdown 差异/版本对比 UI（`wiki_revisions` 仍照写，但暂不专门做记事本的历史界面）。
- 不做记事本 → 知识页/事件的「升级沉淀」动作（未来可让 AI 从 memo 提炼，但那是另一提案，且必须是显式用户动作）。
- 不改 Capture 的默认落点（默认仍走事件管线），除非 §6.2 决定纳入。
- **删除语义**：首版只做**归档**（`status='archived'`），不做物理删除；`wiki_revisions` 兜底恢复。
- **Capture 浮窗多落点**（事件/记事本/提醒）：与定时提醒提案的二期交集，另立提案统一做，两份提案各自不动 Capture（见 [2026-10-19-timed-reminders §7](../architecture/2026-10-19-timed-reminders.md)）。

## 8. 存储设计评审结论（2026-10-19）

**判定：合理，可以按方案 A 落地。** 依据（均已对代码核实）：

1. **人工持有保护现成**：`storage/wiki.rs:353` `human_held` 判定（`PreserveHumanEdits` 下 `human_edited_at.is_some()` 即拦截，正文不动、只并集证据）——memo 每次保存置 `human_edited_at`，从第一秒即人工持有态，无需新保护逻辑。
2. **同步零成本**：`wiki_pages` 在快照 carried 集（`sync/snapshot.rs:45`），按 rowid 整表扫不带 kind 过滤，新 kind 不动快照代码，白拿同步/revisions/导出/搜索/标签。
3. **规模无隐患**：现有 102 页、平均 1.1K、最大 9.3K 字符；memo 记到 50K 也在 SQLite TEXT 与 24K 挂载线的安全距离内。

**评审钉死的三条修正**（已回写正文对应位置）：

1. kind 拆三张白名单（AI 写 / AI 读 / 人类专属），`memo` 显式排除出 `INSIGHT_KINDS` 与 `select_context_pages` 的取材集——见 §3 页面语义。
2. 快记走 `append_wiki_page_content` 单事务追加（必选），不走乐观锁——见 §3 入口与交互。
3. AI 读取只走引用式、不整页挂载，防 24K 截断线——见 §3 页面语义与 §6.3。

**临时绕过无关**：本提案与 [2026-10-19 摘要管道事故](../../incidents/2026-10-19-summary-pipeline-permanent-failure.md) 无耦合，可独立落地。
