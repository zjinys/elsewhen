# 推文预览「AI 整理为 Markdown」（保存前美化）

**状态**：已实现（代码在工作区，随并行会话的重构提交一起落库）
**日期**：2026-09-23

## 需求与取舍

- 诉求：Twitter 等采集来的是纯文本，入库前做一次「分析 → 转 Markdown」的美化。
- 明确排除：**不动 AI 回复链路**（对话、流式、工具调用等一律不碰），只做内容美化本身。
- 拍板：AI 一次性整理（非本地规则）；挂在**保存前预览 tab**（推文预览 `_TweetTabBody`）。

## 实现（全部在 `ui/lib/widgets/wiki_page_detail_view.dart` 推文预览区，外加一个测试文件）

- **零 Rust 改动**：复用现有一次性内容对话入口 `generateContentChat(content, messages)`——
  系统提示 = 抓取内容，用户消息 = 固定整理指令 `_kBeautifyInstruction`
  （保留事实/分段/列表化/提炼 `##` 小标题/链接 `[文字](url)`/话题提及原样/只输出正文）。
  不进聊天历史、不写库，与「与 AI 讨论这篇推文」对话互不干扰。
- **围栏剥离**：`_unwrapCodeFence` 去掉模型偶发的整段 ```markdown 包裹。
- **UI**：推文原文卡片下方新增美化区块——「AI 整理为 Markdown」按钮（busy 态转圈、
  已整理时变「重新整理」）+「放弃整理」+「保存时将使用整理版」提示；
  结果卡用 `MarkdownView` 渲染预览（accent 描边区分于原文卡）。
- **保存语义**：`_save()` 入库 `_beautified ?? fetch.text`——有整理版时入库整理版，
  原文卡片保持原样可对照。

## 测试

- `tweet_tab_widget_test.dart` 新增用例：触发整理 → 预览卡出现 → 保存入库的是整理后
  Markdown（`_FakeBeautifyRepo extends RustBridgeRepository`，记录入参，不触 FFI）。
- 顺带修正该文件既有 stale 断言：tab 标签「导入」→「首页」（并行会话把 ImportTabEntry
  重命名为首页所致，与本次功能无关）。
- 本文件 3 项全绿；全量 130 过 / 6 失败——失败全部在 message_copy/generating/retry/window
  （message_area 属并行 WIP 区，本次改动未触碰）。

## 协作交接

- 改动文件：`wiki_page_detail_view.dart`（推文预览区 + `markdown_view.dart` 导入）、
  `tweet_tab_widget_test.dart`。均未提交，随并行会话的重构提交一起落库。
- 已知边界：仅覆盖**推文预览 tab**；任意网址预览 tab（`_ImportFetchTabBody`）同构可复用
  同一模式，未做（按需再加）。

## 历史数据一次性修整（2026-09-23 完成）

用户拍板：不走应用层（M1 素材保护 `storage.rs:2819` 禁止改 source 页正文），直接修库。
执行方式：由 agent 本人逐篇阅读并整理为 Markdown（非调用外部 AI），共 8 篇 `tweet-*` 素材页。

- 流程：导出 8 篇 `.orig.txt` → agent 逐篇整理出 `.new.md`（仅加标题/列表/引用/表格结构，
  保留全部事实与观点，字数变化 ±5% 以内）→ 备份 DB → Python 脚本校验快照一致后单事务写回。
- 写库镜像应用格式：`UPDATE content_md + updated_at`（**刻意不置 `human_edited_at`**，
  素材页仍归机器管线）+ `wiki_revisions`（reason=`[beautify] AI 整理为 Markdown`）
  + `wiki_log`（`AI 整理正文为 Markdown：{slug}`）。
- 备份位置：`~/.local/share/elsewhen/backup-beautify-20260923-144124/`（回滚 = 停应用后覆盖 db）。
- 验证：无结构候选页归零；8 条修订与日志齐全。
- 工作区产物（一次性，未入库）：`/tmp/opencode/beautify/`（orig/new 快照 + apply.py）。
