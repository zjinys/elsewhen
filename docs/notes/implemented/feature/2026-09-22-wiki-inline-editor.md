# Agent Note: 知识页正文内联编辑器（M3 双模式集成）

Status: implemented

## Problem

知识页正文此前完全由 `wiki digest` 整篇维护：内容以 markdown 存 `wiki_pages.content_md`，人类在应用内只能改标签。详情页浏览用自研 `MarkdownView` 渲染，无嵌套列表/表格等；AI 对话独立在页内底部面板，与正文编辑割裂。用户需要「更好展示 + 直接编辑 + 边改边聊」三者合一。

## Decision

详情页内容 tab 由 `MarkdownView` 换为 `AppFlowyEditor`，封装为 `ui/lib/wiki/wiki_content_editor.dart` 的 `WikiContentEditor`：

- **双模式**：同一个 `EditorState` 实例切换 `editable`（false=只读浏览，true=编辑）；进出编辑态不重建文档，选区/光标自然保留。
- **正文管线**：`wikiMarkdownToDocument` 解码 + 尾部插入 `wikiChatNode(slug)` 对话块（§7 Form A，聊天内容不进 markdown，按 slug 走 `ensure/archive_wiki_page_chat` 独立持久化）；保存用 `wikiDocumentToMarkdown` → `saveWikiPageContent`（bridge wrapper，走 Rust 侧 `record_wiki_revision` 确认门，`human_edited_at` 置位后 digest 不再整篇覆盖）。
- **脏检查以 markdown 编码串为基准**：`isDirty = wikiDocumentToMarkdown(doc) != 加载快照`。⚠️ 不用 `Document.toJson()` 比较——实测同一文档两次 `toJson()` 返回的 Map 即使逐字节打印相同也 `==` false（内部 HashMap 迭代序不确定），会导致「无改动却判脏」。
- **编辑器自持滚动，不进无界滚动父级**：vendor overlay（`_Theatre`）断言 `constraints.biggest.isFinite`，放进无界高 `SingleChildScrollView` 直接抛异常（widget 测试实证）。布局为 header → 编辑工具栏 → `Expanded(编辑器 + 页尾对话块滚动流)` → 底部卡座条（派生产物/事实/相关待办，高度上限约 42% 内自滚动）。
- **未保存保护**：tab 容器改 `IndexedStack` 保活（切 tab 不丢编辑）；编辑器经 `onDirtyChanged` 上报脏 slug 到 `wikiDirtyTabsProvider`；关闭脏 tab 弹「取消 / 放弃修改并关闭」确认；「完成」有改动才保存、「取消」`discard()` 从加载快照重建。
- **保存快捷键**：`HardwareKeyboard` 全局监听 `Ctrl/Cmd+S`（排除 `KeyRepeatEvent`），编辑态内任意焦点可用，与「完成」同一持久化路径。
- **素材只读**：编辑入口「编辑正文」仅 `kind ∉ {source, note}` 显示，与 Rust 侧 `save_wiki_page_content` 守卫一致（素材采集页只允许表态评价）。

首次挂载可能对文档做规范化，`_initEditor` 在首帧渲染完成后（postFrame）再定格一次快照，避免脏标记误报。

## Alternatives considered

- 保留 `MarkdownView` + 输入框分屏预览（v1 原方案）：编辑与展示分离、无 WYSIWYG，弃。
- 用 `Document.toJson()` 与加载快照做 diff：实测两次调用 Map 不相等、必然误判脏，弃（本 note 最重要的实测教训）。
- 编辑器放进外层 `SingleChildScrollView`（整页滚动）：vendor overlay 需有限约束，运行即崩，弃；改为编辑器自持滚动 + 下方卡座独立滚动。
- §7 Form B（右侧悬浮聊天面板）：不侵入文档模型但「对话与段落」关联弱，弃；对话块与正文同滚动流，编辑与对话同一上下文。

## Consequences

页面正文可被人类直接编辑，改动经 revision 审计且 digest 不再覆盖人工持有页；素材采集页保持全链路只读。编辑器以其自持滚动段承载对话块，超长页的页脚卡座独立滚动、行宽受阅读栏约束。`code` 节点仍渲染为 placeholder 占位框（§5.4 降级 code block 留 M4）。9 项画布集成测试覆盖浏览态/保存/无改动不持久化/取消丢弃/Ctrl+S/保存失败错误条/wikilink 跳页/脏 tab 关闭确认；全量 89 项测试绿，改动文件 analyzer 零告警。