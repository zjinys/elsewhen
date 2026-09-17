# 知识库元数据（标签）重组 + 阅读视觉优化

日期：2026-09-17
状态：已实现（Rust 82+69 / Flutter wiki 相关测试全绿；已 codegen + 重建 release）

## 背景

`thoughts.md` 两条遗留反馈：

- **#3 标签等元数据重新设计**：条目一多就没法组织、没法看。
- **#5 查看视觉效果太差**：正文/排版缺乏「读下去的欲望」。

两者都落在「知识库的消费与组织」这一段，一起处理。

## 关键决策

### #3：不新增 `applicable_when` / `strength` 列，先把已有 tags 变成一等公民

`docs/sources-and-deep-chat.md` 设计了 `applicable_when` / `strength` 等更结构化的元数据，
但它绑定尚未落地的「source 层」，现在实现会造出半截架构。用户的实际痛点是**组织**，
而 `wiki_pages.tags` 一直存在、只是「只能看不能改、也不能筛」。

因此本轮把 tags 做成一等元数据：

- 后端新增 `Store::update_wiki_tags(slug, tags)`（+ `api::update_wiki_tags`）：
  规范化（去 `#`、去空白、去重、保序、上限 24），更新 `updated_at`，
  并追加一条 `wiki_revision`（正文不变，只记「标签更新」）以便审计。
- 左侧栏从「只能按 kind 筛」升级为 **kind × 标签 × 搜索 三维过滤**，加 **排序**
  （最近更新 / 证据数 / 标题）与结果计数。
- 页面详情新增**可编辑标签**（弹窗，空格/逗号分隔），保存后刷新详情与列表。
- 导入时即可打标签：文本导入在保存后用 `update_wiki_tags` 补写；网址导入预览
  直接把标签传给 `save_imported_page`。

### #5：正文改成「居中阅读栏 + 分级排版」

- 详情页头部与正文统一限制在 `760px` 居中（`_kReadingMaxWidth`），长文不再横跨全宽。
- `MarkdownView` 重排：
  - 正文色由 `textSecondary` 提升到 `textPrimary`，`15 / 行高 1.8`；
  - 标题分级放大（24/19/16.5…），一二级标题加细分隔线帮助扫读；
  - 新增分隔线 `---`、复选列表 `- [ ]` / `- [x]`、斜体 `*x*`、markdown 链接 `[文字](url)`；
  - 行内代码加背景、wikilink/链接用强调色下划线；
  - 引用块改用强调色左边线，代码块加边框。

## 未做 / 取舍

- `applicable_when` / `strength` / collection / series 等 schema 级元数据**未做**，
  等 source 层落地后再评估；本轮不引入半成品字段。
- 推文预览 tab 暂不加标签输入（`save_tweet_page` 未加 tags 参数），推文仍自动带 `tweet` 标签。

## 验证

- Rust：`cargo test` → 82 + 69 全绿；新增 `wiki::tests::update_wiki_tags_normalizes_and_persists`
  （去 `#`/去重/忽略空/清空 + 持久化）。
- Flutter：`wiki_ui_test` / `wiki_bridge_test` / `wiki_ingest_placeholder_test` /
  `tweet_tab_widget_test` 全绿；新增 `markdown_view_test`（标题/加粗/行内码/wikilink/复选/有序列表）。
- `flutter analyze` 无 error/warning。
