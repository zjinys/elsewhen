# wiki 页面人类编辑（设计说明 v2）

> 状态：设计稿（v2，方向修订）· 日期：2026-09-21
> 前置文档：[`llm-wiki.md`](llm-wiki.md) —— 已知边界「应用内编辑未做」的闭环
> **v1 → v2 变更**：用户目标明确为「更好展示 + 直接编辑 + AI 对话嵌入编辑器」，主线从 v1 的「markdown 分屏预览」**升级为 AppFlowy Editor**。核心保护机制（§4）不变且并行推进；先做可行性 spike（§5）锁住 md⇄JSON 往返保真，再全面集成（§6）。

## 1. 背景与目标

知识库当前语义是「AI 维护，人类只负责记录与提问」：`wiki_pages.content_md` 为 markdown 事实来源，由 `wiki digest` 整篇写回，留 `wiki_revisions` + `wiki_log` 审计；人类在应用内只能改标签。

本次要满足的三件事：

1. **更好的展示**：现有自研 `MarkdownView`（`ui/lib/widgets/markdown_view.dart`）渲染质量有限（无嵌套列表/表格/图片等），知识库浏览观感差；
2. **直接编辑**：人类在 GUI 内 WYSIWYG 编辑知识页正文；
3. **AI 对话嵌入编辑器**：页内 AI 对话不另开 tab，直接存在于编辑体验中（机器可改、人可改、边改边聊）。

## 2. 现状梳理（为什么不能只加个编辑器）

### 2.1 根问题：digest 整篇覆盖正文

`Store::upsert_wiki_page`（`src/storage.rs:2574`）的「确定性合并」**只保护数字**：事件 id 并集 + `evidence_count` 由核心重算；但 `content_md` 是 **AI 提议的整篇替换**。不加保护，人类编辑会被下一次 `wiki digest` 冲掉。这是与编辑器无关的机制问题，§4 解决。

### 2.2 现有可复用资产

| 资产 | 位置 | 角色 |
|---|---|---|
| `wiki_revisions` + `record_wiki_revision` | `src/storage.rs:2788` | 任意写回留审计 revision |
| `wiki_log` + `append_wiki_log` | `src/storage.rs:2825` | 追加式操作日志 |
| `update_wiki_tags` API 模式 | `src/api.rs:1680` | 人类写入标准姿势（bridge 由 `./regen.sh` 生成） |
| 页内 AI 对话 | `ensure_wiki_page_chat` / `archive_wiki_page_chat`（`src/api.rs`）+ Flutter 聊天 UI | AI 对话嵌入编辑器的现成后端 |

## 3. 为什么 AppFlowy Editor 成为主线（v1 结论修订）

### 3.1 已验证能力（源码证据，`appflowy_editor` 6.2.0）

| 需求 | 能力 | 证据 |
|---|---|---|
| 双向 markdown | `markdownToDocument()` 解码 + `documentToMarkdown()` 编码，均支持自定义 parser | `lib/src/plugins/markdown/document_markdown.dart` |
| `[[wikilink]]` | 解码支持 `md.InlineSyntax` 自定义行内语法；编码支持自定义 `NodeParser` | `document_markdown.dart` / `custom_markdown_node_parser.dart` |
| 只读展示 | `editable: false`，只读仍可选词、可自定义快捷键 | changelog 3.1.0 / 3.2.0 |
| AI 对话嵌入 | 自定义 block 组件可嵌任意 Flutter widget（页内聊天 UI 包一层即可） | 官方文档 customizing.md |
| 版本兼容 | `sdk >=3.6.0`、`flutter >=3.32.0` | `pubspec.yaml`（本机 Flutter 3.47.4 满足） |
| 渲染质量 | WYSIWYG 块渲染（嵌套列表/表格/代码块/引用/checklist/图片） | 编辑器本体 |

### 3.2 v1 反对理由的再评估

| v1 顾虑 | 现状 | 结论 |
|---|---|---|
| 数据模型错配 | 有官方 md 解码/编码器 + 双向自定义 hook | 往返链路存在，但**保真度必须 spike 实测**（§5.1），不打包票 |
| `[[wikilink]]` 成本 | 行内语法 + 自定义 NodeParser 两条 hook | 可控，spike 一并验证 |
| 复杂度/重量 | 只读模式可作展示层，且本就要编辑+AI 嵌入 | 需求本身就是编辑器场景，复杂度是需求的一部分 |
| 许可 AGPL/MPL | 本地分发可接受 | 记录在案，不阻塞 |

### 3.3 仍成立的前提

- **往返保真优先**：markdown 是 digest 管线的母语，`md → JSON → md` 若漂移（wikilink、表格、嵌套列表、checklist、引用、代码块、行内粗斜体），AI 后续合并会拿"走样"的 markdown 当基座。**spike 用真实 wiki 页做快照 diff 锁行为**，补丁用自定义 parser，禁用不了的语法列入白名单并在编辑器中降级展示。
- **核心保护（§4）与编辑器无关**，先做不亏。

## 4. 核心设计（Rust，并行推进）

### 4.0 kind 拆分（前置修正）

`kind=topic` 现行身兼两职（用户粘贴笔记 `save_text_page` 与 AI 主题页共用），按"页面所有权"拆开：

- 用户粘贴笔记（`note-` 前缀 slug）→ 新 kind `note`；
- AI 提炼的主题页（`topic/` 前缀）→ 保持 `topic`。

迁移 v29 backfill：`UPDATE wiki_pages SET kind='note' WHERE kind='topic' AND slug LIKE 'note-%'`。
权限判定一律用 `(kind, slug 前缀)` 组合（与 `derive_wiki_area` 同键），不再单看 kind 字段。

### 4.1 页面类型 × 能力矩阵（编辑权分层）

两正交维度：**kind 决定默认持有权**（谁写、能否编辑），**`human_edited_at` 是"曾被人动过"的页级事实开关**。一次人工编辑 = 永久把该页从 AI 持有切到人工持有。

| kind | 内容可改 | 评价（观点表态） | digest 默认行为 | 人工编辑一次后 |
|---|---|---|---|---|
| `source` / `note`（采集素材） | ❌ 只读 | ✅ `endorse`/`reject`（缺省认可） | 永不整篇覆盖（人工持有） | 不适用 |
| `person`/`project`/`capability`/`recurring_cost`/`topic`（AI 档案页） | ✅ 修改完善 | ❌ | 整篇覆盖（AI 持有） | 保护：正文不动，证据照累 |
| `derivative`（派生加工） | ✅ | ❌ | 覆盖、可再生（预期） | 保护：人接管，不再被重生成覆盖 |

两类对所有 kind 一刀切、不可人工修改：
- **证据/机制字段**：`source_event_ids`、`evidence_count`、`first/last_seen_at`、`created/updated_at` —— 系统持有；
- **结构字段**：`area`/`based_on`/`content_type`/`source_url`/`kind`/`id`/`slug` —— 只能由产生它的流程改动。
- 素材 `source`/`note` 的正文对**人和 AI 都只读**；评价是唯一入口。

> **开放决策（待使用反馈）**：`note-` 用户笔记当前按素材档**只读 + 可评价**先运行（M1
> 已落地）；是否放开为「可人工编辑」（等同 AI 档案页）取决于实际使用体验，改判定仅需
> 调整 `save_wiki_page_content` 的拒绝范围，保护机制本身不用动。

### 4.2 迁移 v29

```sql
-- 1) 拆 kind 双语义（存量数据）
UPDATE wiki_pages SET kind='note' WHERE kind='topic' AND slug LIKE 'note-%';
-- 2) 人工编辑保护位（AI 档案页：非空 ⇔ digest 不得整篇覆盖正文）
ALTER TABLE wiki_pages ADD COLUMN human_edited_at TEXT;  -- 可空 RFC3339
-- 3) 素材页观点评价（NULL ⇔ 未表态，渲染/读取按缺省认可 'endorse' 处理）
ALTER TABLE wiki_pages ADD COLUMN opinion TEXT;  -- 'endorse' | 'reject' | NULL
```

既有数据：`note-` 存量改 1 条 kind；其余全 NULL，digest 行为与升级前一致。

### 4.3 写入 API（人类路径）

```rust
/// 人类编辑保存。仅允许可编辑 kind（person/project/capability/recurring_cost/topic/derivative）；
/// 素材 kind（source/note）直接拒绝。非空、长度上限（64k）。写 revision（reason 前缀 "[human]"）
/// + wiki_log；置 human_edited_at=now。返回更新后的页面。
pub fn save_wiki_page_content(slug: String, content_md: String, reason: String) -> Result<WikiPageDto>

/// 素材页观点评价。仅允许素材 kind（source/note）。不改变正文、不置位 human_edited_at，
/// 只写 wiki_log 审计。
pub fn set_wiki_opinion(slug: String, opinion: Option<String>) -> Result<WikiPageDto>
// opinion: None=清空回未表态；Some("endorse"|"reject")
```

均仿 `update_wiki_tags`（`src/api.rs:1680`），`./regen.sh` 自动生成绑定。评价字段为后续「AI 提炼引用人工认可素材」的权重信号预留（消费逻辑不在 M1）。

### 4.4 digest 保护

`upsert_wiki_page` 增加策略参数，并按 kind 决定默认：

```rust
enum ContentPolicy { Always, PreserveHumanEdits }
```

- 素材页（`source`/`note`）：digest 调用即传 `PreserveHumanEdits`（内容对 AI 也只读，仅素材流程可写）；
- 档案页/派生页：AI **静默生成路径**（digest 消化、洞察归档、关系建档）传 `PreserveHumanEdits`——见人工编辑（`human_edited_at` 非空）即转保护；
- `PreserveHumanEdits` 且存在既有行且 `existing.content_md != draft.content_md`：
  - 不动 `content_md/title/summary/tags`；
  - 照做 `source_event_ids` 并集 + `evidence_count` 重算 + `last_seen_at` 刷新；
  - 结果记入 `DigestResult.skipped`，reason `"human-edited, 仅累加证据"`。

两档语义（实现即此，`Always` 触发条件为空集——它只出现在「有显式授权」的写回，不产生静默覆盖）：

```rust
enum ContentPolicy { Always, PreserveHumanEdits }
// Always             → 不拦内容列：素材导入流程（素材页唯一合法写入方，可刷新采集快照）、
//                      AI 草拟 → 用户确认制（save_knowledge_draft 建档 / save_wiki_revision 修订，
//                      确认即显式授权）。人工编辑页也允许被确认制修订覆盖——这正是下面这句话的意义。
// PreserveHumanEdits → human_edited_at 非空 或 kind∈{source,note} 时内容列只读。
```

AI 对人工编辑页仍可走既有 `save_wiki_revision` 草拟确认制修订（显式确认，非静默覆盖），保持「AI 只提议、核心决定」纪律。

### 4.5 派生页

`derivative` 不设保护（AI 生成物，覆盖是预期）；人类要改某条派生页，等同人工编辑置位即受保护（升级为人工持有）。

## 5. 可行性 spike（本轮第一步，先于全面集成）

### 5.1 md ⇄ JSON 往返保真

- 取**现网全部 wiki 页** `content_md`（含 wikilink、表格、嵌套列表、checklist、引用、代码块、行内粗斜体/链接/代码）批量跑 `markdownToDocument → documentToMarkdown`；
- 逐页 diff，产出一张**漂移清单**（丢失/变形/额外转义）；
- 按清单逐个补自定义 `inlineSyntaxes` / `markdownParsers` / `customParsers`，直到 diff 收敛；
- 项目内落一个 `round_trip_test.dart`（git 化样例 → 快照），防止后续升级回归。

> **✅ 已收敛（实证，2026-09）**：`ui/test/round_trip_test.dart` + `ui/test/fixtures/wiki_md/`（7 真实素材 + 16 合成特征）24/24 通过。
> 三层门：`bytes`（逐字节，12 个合成特征全覆盖）/ `spaced`（空白规范但 AST 语义相等，真实 7 页 + 嵌套缩进 2）/ `knownDrift`（已知语义漂移，快照锁定）。
> vendor 补丁（`ui/third_party/appflowy_editor/lib/.../markdown/`）：① 块间空行连接（段落+`---` 会被重解析成 setext 二级标题的语义破坏）；② 有序列表逐条赋号 `start+i`（原所有条目共用 start 号 → 编号塌缩）；③ 新增 `<pre><code>` 解码 parser + 围栏尾换行裁剪（原代码块整体丢失）；④ 引用块逐行打 `>`（续行滑出引用块）；⑤ 斜体用 `*` 不用 `_`（中文 `_斜体_` 是 intraword 下划线，CommonMark 不当斜体解析）；⑥ 表格/列表样式归一（`| a | b |` 间距、`- ` 列表符、tab 嵌套缩进——tab=4 列对 `1. ` 无歧义，2 空格会滑出列表）。
> **白名单（不可消除，已记录）**：斜体标记归一（`*`→保留，语义等价）；紧邻同型列表的边界（节点模型无此信息，两独立列表会合并，快照锁定）；尾随空格 hard-break 与全角空格行的归一。
> **开放项（留给 §5.2/M3）**：`code` 节点在编辑器无对应 block component（上游 01eccc6 无代码块组件），管线已保真，展示降级待 M3 处理。

### 5.2 `[[wikilink]]`

- 解码：`md.InlineSyntax` 匹配 `[[slug]]` → 行内 span（样式同 `MarkdownView` 现有 wikilink 视觉）→ 点击 `ref.read` 跳转目标页；
- 编码：自定义 `NodeParser` 输出 `[[slug]]`；
- 兜底：打开无 wikilink 解析的旧样例必须仍然保真（spike 用例里覆盖）。

> **✅ 已收敛（实证，2026-09）**：
>
> **解码（vendor 补丁 + 应用注入）**
> - `ui/lib/wiki/wikilink_syntax.dart`：`md.InlineSyntax` 匹配 `[[target]]` / `[[target|alias]]`，产出 `<wikilink>` 元素（`attributes['wikilink']` 存 target，子文本为 alias）；
> - `ui/lib/wiki/wiki_markdown_codec.dart`：`wikiMarkdownToDocument` 在 `markdownToDocument(inlineSyntaxes: [WikilinkInlineSyntax()])` 注册（块级解析即产出元素；各块 parser 的 `DeltaMarkdownDecoder` 已按 tag 映射为 delta 行内 `wikilink` 属性）；
> - ⚠️ **踩坑（已根治）**：markdown 包中正则一旦匹配、`onMatch` 返回 false，`tryMatch` 仍返回 true，parse 循环不复位位置 → 死循环。`[[|x]]` / `[[a|]]` 这类空 target/alias 会触发。修复：pattern 收紧为 `\[\[([^\[\]|]+(?:\|[^\[\]|]+)?)\]\]`（target/alias 均非空、至多一个 `|`），且 `onMatch` 永不返回 false（防御分支按原样文本消费）→ 非法形态直接按字面保留。
>
> **编码（vendor 补丁）**
> - `DeltaMarkdownEncoder.convert` 处理 `wikilink` 属性：`[[target|alias]]`（alias == target 时省略为 `[[target]]`）——无需自定义 NodeParser（该属性本就是行内级，走现成行内编码路径即可）。`BuiltInAttributeKey.wikilink` 常量补进 vendored 常量类，vendor 与 app 共用。
>
> **渲染（应用侧注入，无 vendor 渲染补丁）**
> - `ui/lib/wiki/wiki_text_span_decorator.dart`：`wikiTextSpanDecorator({onTapWikiLink})` 经 `EditorStyle.copyWith(textSpanDecorator:)` 注入；wikilink 属性 → MarkdownView 同款视觉（accentPrimary + 下划线 + w600）+ `TapGestureRecognizer` 点击回调 slug；非 wikilink 委托 `defaultTextSpanDecoratorForAttribute`（href 等原生行为保留）。
>
> **测试**
> - `ui/test/round_trip_test.dart` 切换为生产管线（`roundTrip` 走 wiki codec）→ 25 个 fixture（7 真实 + 18 合成特征）26 测试全绿：s02（既有 wikilink 样例，alias==target 省写仍逐字节收敛）+ 新增 `s17_wikilink_alias`（alias/列表项内 wikilink）+ `s18_wikilink_fallback`（未闭合 `[[`、代码 span 内 `[[x]]`、空 target/alias 均按字面保真）；
> - `ui/test/wiki_wikilink_spike_test.dart`：编解码属性断言 + 真编辑器 widget spike（span 视觉断言 + 点击回调 slug = `topic/投资`）。
> - IME 测试页同步切到 wiki codec，作为手动快速入口。

### 5.3 AI 对话块 ✅

- 把页内聊天 UI 抽成共享面板 `ui/lib/widgets/wiki_ai_chat_panel.dart`（`WikiAiChatPanel` / `WikiChatBubble`，行为与详情页底部原面板一致），并包成自定义 block `ui/lib/wiki/wiki_chat_block.dart`（`wiki_chat` 节点 + `WikiChatBlockComponentBuilder` + `wikiChatNode({slug})`）；
- 验证结论（`ui/test/wiki_chat_block_spike_test.dart`，9 项全绿）：
  - 编辑 / 只读模式均可交互：输入→发送→回复走通，AI 回复气泡渲染；
  - 不干扰选区/光标：聊天输入只进会话、不写正文文档；聊天交互后点击正文段落，选区正确落回正文（折叠光标）；
  - 聊天块不进 markdown 往返（无 NodeParser，编码器静默跳过，正文往返不受影响）；
  - 块节点携带 `slug` 属性，会话按页走 `ensure/archive_wiki_page_chat` 独立持久化。
- 回复「插入到正文」落点：v1 **不做自动插入**——沿用现有 `save_wiki_revision` 确认门，见 §11 Q2 拍板。

### 5.4 只读展示 + 版本 ✅

- `editable: false` 走查（`ui/test/wiki_readonly_walkthrough_test.dart`，4 项全绿）：嵌套列表 / 引用 / 待办（选中+未选）/ 分割线 / 表格单元格 / 标题 / wikilink 均正常渲染；wikilink 沿用 MarkdownView 同款视觉（accentPrimary + 下划线）；只读态仍可点选正文（复制场景不阻塞）；
- 依赖树：vendored `appflowy_editor`（01eccc6）以 path 依赖随主 pubspec 解析；自定义 block 运行时需 `provider`/`collection`，已加为直接依赖（版本对齐 vendor pubspec）；
- ⚠️ 代码块降级实证：vendor 无 `code` 块组件 → `code` 节点渲染为 30px "placeholder" 占位框，见 §11 Q4。

**完成标准**：往返 diff 收敛 + wikilink 可解析可回填 + 聊天块可交互。✅ 全部达标 —— M2 关闭，无需回退 v1 方案（markdown 分屏预览）。

## 6. Flutter 集成设计（spike 通过后）

### 6.1 浏览 / 编辑双模式 ✅（M3 落地, 2026-09）

- 详情页（`wiki_page_detail_view.dart` 内容 tab）渲染从 `MarkdownView` 换为 `AppFlowyEditor`（`editable: false` 浏览态）；
- 工具栏「编辑」→ 同一实例切 `editable: true`；离开编辑态时若有改动：`documentToMarkdown` → `saveWikiPageContent` → `ref.invalidate` 刷新。

**落地细节（实现即此，与初稿差异已注明）**：

- 封装 `ui/lib/wiki/wiki_content_editor.dart`（`WikiContentEditor`）：`editable` 双模式、`wikiMarkdownToDocument` 解码 + 尾部 `wikiChatNode`、`wikiDocumentToMarkdown` 保存、`editorStyle` 注入 `wikiTextSpanDecorator(onTapWikiLink:)`、注册 `wiki_chat` 自定义块；**同一 `EditorState` 实例切 editable，进出编辑态不重建文档**；
- 脏判定以 **markdown 编码串** 为基准（`save()` 前 `wikiDocumentToMarkdown(doc) != 加载快照`）。⚠️ 不要用 `Document.toJson() == …` 做快照比较——vendor 的 `toJson()` 两次调用返回的 Map 即使内容一致也 `==` false（内部 HashMap 迭代序不确定），会导致「无改动却判脏」（集成测试实证）；
- 编辑器首次挂载可能对文档做规范化，`_initEditor` 在首帧渲染完成后（postFrame）再定格一次快照，避免脏标记误报；
- **编辑器自持滚动，不进外层 `SingleChildScrollView`**：vendor 的 overlay（`_Theatre`）断言有限约束，放进无界高滚动父级会直接抛 `constraints.biggest.isFinite`（widget 测试实证）。布局改为：header → 编辑工具栏 → `Expanded(编辑器，含页尾对话块滚动流)` → 下方「卡座」条（派生产物 / 事实 / 相关待办，高度上限约 42% 内自滚动）；
- 编辑入口「编辑正文」仅对**可编辑 kind** 显示（`!kind ∈ {source, note}`），与 Rust 侧 `save_wiki_page_content` 守卫一致（素材采集页全链路只读）；
- 编辑工具栏：浏览态「编辑正文」；编辑态「取消 · 完成（保存）+ 提示 Ctrl/⌘+S」；保存失败展示错误条并留在编辑态。

### 6.2 保存链路

```
editorState.document → documentToMarkdown() → saveWikiPageContent(slug, md, "[human] GUI 编辑")
→ bridge → Rust: record_wiki_revision + append_wiki_log + human_edited_at=now
```

本页保存回调 `_persistEdit` 即上述链路：`RustBridgeRepository.saveWikiPageContent`（M3 新增 wrapper）→ `ref.invalidate(wikiPageProvider / wikiPagesProvider)` → snackbar「已保存到知识库」→ 退出编辑态。手动路径之外还有 `Ctrl/Cmd+S`（`HardwareKeyboard` 全局监听，编辑态任意焦点可用；Linux/Win 用 Ctrl、macOS 用 ⌘，`KeyRepeatEvent` 排除按键连发）。

### 6.3 未保存保护 ✅（M3 落地, 2026-09）

- **切 tab 不丢编辑**：`WikiPageDetailView` 的 tab 容器改 `IndexedStack`（保活所有 tab 子树），编辑中的页面切走再切回编辑态与改动原样保留（初稿未规定，实现补足）；
- **关闭 tab 确认**：编辑器经 `onDirtyChanged` 把未保存 slug 上报 `wikiDirtyTabsProvider`；关闭该 tab 时弹「关闭前确认」——v1 只做「取消 / 放弃修改并关闭」两档；**「关前先保存」留 v1.5**（对话框加保存按钮）；
- **显式离开编辑态**：`「完成」`（有改动才保存，无改动直接退出）与 `「取消」`（`discard()` 从加载快照重建文档，丢弃未保存改动）双入口，不需要额外确认弹窗；
- `Ctrl/Cmd+S` 保存成功即退出编辑态（与「完成」同一持久化路径）。

### 6.4 边界

- v1 只编辑正文；title/summary 人工编辑留 v2；
- `wiki export` 仍为只读快照，应用内编辑写真源，快照由真源重生成；
- 详情页不再引用 `MarkdownView`（`wiki_page_detail_view.dart` 已移除 import）；文件本体保留未删，未来对话消息等场景的高保真 markdown 展示可复用。

## 7. AI 对话嵌入（两种形态，v1 取其一）✅ A 已落地

| 形态 | 做法 | 取舍 |
|---|---|---|
| **A. 页尾对话块** | 自定义 block「AI 对话」，复用现有聊天 UI 与 `ensure/archive_wiki_page_chat` | 与正文同滚动流，编辑与对话同一上下文（推荐先做） |
| B. 右侧悬浮面板 | 详情页横向布局加聊天面板 | 不侵入文档模型，但"对话与段落"的关联弱 |

v1 建议 A；**spike §5.3 已验证聊天交互与编辑器焦点/选区互不干扰 → 拍板取 A（页尾对话块）**；B（右侧悬浮面板）保留为移动端 / 超长页备选。

**落地说明（M3）**：对话块随编辑器文档渲染（正文解码后尾部插 `wikiChatNode(slug)`，聊天内容不进 markdown、按页走 `ensure/archive_wiki_page_chat` 独立持久化）；详情页 footer 不再独立挂 `WikiAiChatPanel`（页内全页仅一个聊天面板，避免双 UI）。

## 8. 兼容与迁移

- 迁移 v29：拆 kind（`note-`→`note`）+ 补 `human_edited_at`/`opinion` 两列，既有数据 backfill 后行为与升级前一致；
- CLI `wiki digest / insight / export` 不受影响（保护是内部行为变化）；
- 所有写回仍留 `wiki_revisions`，可回滚；
- `MarkdownView` 文件保留（未删），但由于详情页正文已切换 AppFlowyEditor，lib 内已无使用方；对话消息等场景如需高保真 markdown 展示可复用。

## 9. 测试计划

### Rust

- `save_wiki_page_content`：置位、revision 追加、日志、空/超长拒绝、**素材 kind 拒绝**；
- `set_wiki_opinion`：校准/清空/审计，素材 kind 专用、非素材 kind 拒绝；
- digest 保护：人工编辑页 → 正文不变、事件并集与证据数正确累加、进 `skipped`；未人工编辑页 → 与现网一致（回归）。

### Flutter

- `round_trip_test.dart`：真实样例 md → doc → md 快照 diff（§5.1 产物，常驻）；
- 编辑 → 保存 → bridge 调用与刷新；未保存离开确认；wikilink 点击跳页；只读模式可选词。—— ✅ M3 落地为 `ui/test/wiki_editor_integration_test.dart`（9 项：浏览态/素材页无编辑入口/编辑保存/无改动不保存/取消丢弃/Ctrl+S/保存失败错误条/wikilink 跳页/脏 tab 关闭确认）。

## 10. 里程碑

1. **M1（并行）**：核心保护 —— 迁移 v29（kind 拆分 + `human_edited_at` + `opinion`）+ `save_wiki_page_content` + `set_wiki_opinion` + digest 按 kind 保护 + Rust 测试 —— ✅（`feat/wiki-m1-protection` 已合 main）；
2. **M2（spike）**：`flutter pub add appflowy_editor` + §5 四项验证，产出来回 diff 清单与决策记录 —— ✅ 全部完成（依赖/IME/测试页 ✅；§5.1 往返保真 ✅；§5.2 wikilink ✅；§5.3 AI 对话块 ✅ 拍板 Form A；§5.4 只读展示 ✅ + code 块降级实证）；
3. **M3（集成）**：双模式编辑器替换 + 保存链路 + AI 对话块（§6/§7）—— ✅（编辑器 `WikiContentEditor` + 详情页双模式/保存/未保存保护 + 页尾对话块 + 9 项集成测试；UI 侧全程无 Rust 改动）；
4. **M4**：`round_trip` 常驻测试 + 主题打磨 + 移动端走查。—— ✅（`round_trip_test.dart` 常驻；code 块降级渲染 ✅ §11 Q4；移动端走查 ✅ 窄视口 390×844 专项测试 + 修复 vendor 默认 block padding 左右各 100 导致的窄屏溢出 —— 编辑器显式收窄为 24；主题打磨 = code 块配色随 `AppTheme` 深浅主题）；

## 11. 开放问题（实施前拍板）

1. **往返保真不收敛时**：回退 markdown 分屏预览，还是接受白名单降级（如表格只读不可建）？——**§5.1 已收敛**（上表），白名单条目已固化进常驻测试；
2. **AI 对话落点**：v1 拍板 —— 回复**仅入会话展示**，改页必须过 `save_wiki_revision` 确认门（模型提议 → 用户确认 → 写库），不做"生成即插光标"；理由：改动可审计、避免 AI 半成品直进正文；§5.3 已证聊天焦点与编辑器选区隔离，不构成自动插入的技术障碍，仍按确认门推进。
3. **乐观锁**：本地单写者，v1 不做版本冲突检测；是否接受编辑期间 digest 并发导致"保存覆盖 digest"的极端情况（缓解：保存时校验 `updated_at`）。
4. **代码块展示降级**（§5.4 实证）：vendor 01eccc6 的编辑器**无 `code` 块组件**（`code` 节点渲染为 30px placeholder 占位框；管线上已保真）。**M4 已落地**：注册降级 code block `WikiCodeBlockComponent`（`ui/lib/wiki/wiki_code_block.dart`，等宽字体只读块 + 语言角标 + 复制按钮），**不升级 vendor**（避免引入代码块组件的体积与行为漂移）；复制走 `Clipboard.setData`（集成测试断言内容去围栏/语言行）。