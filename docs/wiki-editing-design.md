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

### 4.1 迁移 v29：`human_edited_at`

```sql
ALTER TABLE wiki_pages ADD COLUMN human_edited_at TEXT;  -- 可空 RFC3339
```

- `NULL` ⇔ 从未人工编辑（既有数据全 NULL，digest 行为不变）；
- 非空 ⇔ digest 不得整篇覆盖正文。

### 4.2 写入 API（人类路径）

```rust
/// 人类编辑保存。非空、长度上限（64k）。写 revision（reason 前缀 "[human]"）+ wiki_log；
/// 置 human_edited_at=now。返回更新后的页面。
pub fn save_wiki_page_content(slug: String, content_md: String, reason: String) -> Result<WikiPageDto>
```

仿 `update_wiki_tags`（`src/api.rs:1680`），`./regen.sh` 自动生成绑定。素材原文（`area=imported`）同样允许人工编辑。

### 4.3 digest 保护

`upsert_wiki_page` 增加写回策略参数：

```rust
enum ContentPolicy { Always, PreserveHumanEdits }

// PreserveHumanEdits 且 existing.human_edited_at 非空 且 existing.content_md != draft.content_md：
//   → 不动 content_md/title/summary/tags；只做 source_event_ids 并集 + evidence_count 重算 + last_seen_at 刷新
//   → 结果记入 DigestResult.skipped，reason "human-edited, 仅累加证据"
```

AI 对人工编辑页仍可走既有 `save_wiki_revision` 草拟确认制修订（显式确认，非静默覆盖），保持「AI 只提议、核心决定」纪律。

### 4.4 派生页

`derivative` 不设保护（AI 生成物，覆盖是预期）；人类要改某条派生页，等同人工编辑置位即受保护。

## 5. 可行性 spike（本轮第一步，先于全面集成）

### 5.1 md ⇄ JSON 往返保真

- 取**现网全部 wiki 页** `content_md`（含 wikilink、表格、嵌套列表、checklist、引用、代码块、行内粗斜体/链接/代码）批量跑 `markdownToDocument → documentToMarkdown`；
- 逐页 diff，产出一张**漂移清单**（丢失/变形/额外转义）；
- 按清单逐个补自定义 `inlineSyntaxes` / `markdownParsers` / `customParsers`，直到 diff 收敛；
- 项目内落一个 `round_trip_test.dart`（git 化样例 → 快照），防止后续升级回归。

### 5.2 `[[wikilink]]`

- 解码：`md.InlineSyntax` 匹配 `[[slug]]` → 行内 span（样式同 `MarkdownView` 现有 wikilink 视觉）→ 点击 `ref.read` 跳转目标页；
- 编码：自定义 `NodeParser` 输出 `[[slug]]`；
- 兜底：打开无 wikilink 解析的旧样例必须仍然保真（spike 用例里覆盖）。

### 5.3 AI 对话块

- 把现有页内聊天 UI（`ui/lib/widgets/` 下聊天视图）包成自定义 block component，验证：聊天在编辑/只读模式下可交互、不干扰选区/光标、回复"插入到正文"的落点（可选 v1）。

### 5.4 只读展示 + 版本

- `editable: false` 走查展示观感（嵌套列表/表格/代码块样式，主题对齐 `AppTheme`）；
- `flutter pub add appflowy_editor` 实际解析一次依赖树，记录体积与覆盖范围。

**完成标准**：往返 diff 收敛 + wikilink 可解析可回填 + 聊天块可交互。任一不达标 → 回退 v1 方案（markdown 分屏预览，§设计稿 v1），决策点在文档留痕。

## 6. Flutter 集成设计（spike 通过后）

### 6.1 浏览 / 编辑双模式

- 详情页（`wiki_page_detail_view.dart` 内容 tab）渲染从 `MarkdownView` 换为 `AppFlowyEditor`（`editable: false` 浏览态）；
- 工具栏「编辑」→ 同一实例切 `editable: true`；离开编辑态时若有改动：`documentToMarkdown` → `saveWikiPageContent` → `ref.invalidate` 刷新。

### 6.2 保存链路

```
editorState.document → documentToMarkdown() → saveWikiPageContent(slug, md, "[human] GUI 编辑")
→ bridge → Rust: record_wiki_revision + append_wiki_log + human_edited_at=now
```

### 6.3 未保存保护

- 编辑态切换/离开时 diff `document.toJson()` 与加载快照；有改动弹确认；
- `Ctrl/Cmd+S` 保存；保存成功 toast + 退出编辑态。

### 6.4 边界

- v1 只编辑正文；title/summary 人工编辑留 v2；
- `wiki export` 仍为只读快照，应用内编辑写真源，快照由真源重生成。

## 7. AI 对话嵌入（两种形态，v1 取其一）

| 形态 | 做法 | 取舍 |
|---|---|---|
| **A. 页尾对话块** | 自定义 block「AI 对话」，复用现有聊天 UI 与 `ensure/archive_wiki_page_chat` | 与正文同滚动流，编辑与对话同一上下文（推荐先做） |
| B. 右侧悬浮面板 | 详情页横向布局加聊天面板 | 不侵入文档模型，但"对话与段落"的关联弱 |

v1 建议 A；若聊天交互（键盘焦点/光标）与编辑器冲突严重，降级 B——spike §5.3 专门验证这一点。

## 8. 兼容与迁移

- 迁移 v29 补列，既有行全 NULL，digest 行为与升级前一致；
- CLI `wiki digest / insight / export` 不受影响（保护是内部行为变化）；
- 所有写回仍留 `wiki_revisions`，可回滚；
- `MarkdownView` 保留（对话消息等场景仍在用），不删除。

## 9. 测试计划

### Rust

- `save_wiki_page_content`：置位、revision 追加、日志、空/超长拒绝；
- digest 保护：人工编辑页 → 正文不变、事件并集与证据数正确累加、进 `skipped`；未人工编辑页 → 与现网一致（回归）。

### Flutter

- `round_trip_test.dart`：真实样例 md → doc → md 快照 diff（§5.1 产物，常驻）；
- 编辑 → 保存 → bridge 调用与刷新；未保存离开确认；wikilink 点击跳页；只读模式可选词。

## 10. 里程碑

1. **M1（并行）**：核心保护 —— 迁移 v29 + `save_wiki_page_content` + digest 保护 + Rust 测试；
2. **M2（spike）**：`flutter pub add appflowy_editor` + §5 四项验证，产出来回 diff 清单与决策记录；
3. **M3（集成）**：双模式编辑器替换 + 保存链路 + AI 对话块（§6/§7）；
4. **M4**：`round_trip` 常驻测试 + 主题打磨 + 移动端走查。

## 11. 开放问题（实施前拍板）

1. **往返保真不收敛时**：回退 markdown 分屏预览，还是接受白名单降级（如表格只读不可建）？
2. **AI 对话落点**：回复默认插在光标处（生成类）还是仅展示（确认后手动插入）？——影响 §5.3 与权限模型；
3. **乐观锁**：本地单写者，v1 不做版本冲突检测；是否接受编辑期间 digest 并发导致"保存覆盖 digest"的极端情况（缓解：保存时校验 `updated_at`）。