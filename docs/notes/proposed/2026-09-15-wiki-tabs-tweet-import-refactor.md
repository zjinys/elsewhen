# 知识库 tab 化 + 推文「抓取预览 → 确认入库」流程

日期：2026-09-15
状态：已实现（已 codegen + 重建 release；Rust 42+33 / Flutter 24 全绿）

## 背景

用户反馈原「粘贴链接 → 立即导入」流程不符合直觉：抓取结果是用户不可见的黑盒，且直接入库
无法先确认/与 AI 讨论。要求改为：

1. 知识库右侧面板 **tab 化**，缺省只有 「推文导入」（带文本框）这一个 tab；
2. 点「抓取」只调用 fxtwitter 解析 json，**新开一个 tab** 展示抓取内容；该 tab 带 AI 对话框
   （可针对内容对话）+「保存到知识库」按钮，**只有点保存才入库**；
3. 设置页增加「推文抓取 API 服务」配置项（当前仅 fxtwitter）。

## 关键决策

### 抓取与保存彻底拆开
- `fetch_tweet_text(url)`：只解析 fxtwitter JSON，返回 `TweetText`，**不写库**。
- `save_tweet_page(TweetText)`：唯一入库入口，`upsert_wiki_page` 幂等（重复保存=更新同页）。
- 原 `ingest_tweet`（抓取+保存一体）已删除，FRB 重新生成。

### 内容对话「不入库」：临时对话 API
- 新增 `generate_content_chat(content, messages)`：system 提示词 = 基础角色设定 +
  注入的抓取内容（`build_content_system_prompt`），messages 为临时历史（Flutter 侧只传最近 24 条）。
- **对话本身不写 conversations/messages 表** —— 严格满足「只有点保存才入库」；
  但**照常记录 token_usage**（conversation_id 为 NULL），保持每日用量统计不缺失。
- 否决了「建会话+seed 消息」方案：会在对话列表留垃圾会话，且与「不入库」冲突。

### 右侧面板多 tab 状态
- `wikiOpenTabsProvider`（List<WikiTabEntry>，上限 8）+ `wikiActiveTabIdProvider`。
- 三种 tab：`ImportTabEntry`（固定不可关）、`PageTabEntry`（点左侧列表开）、`TweetTabEntry`
  （抓取成功开，含 fetch 结果）。
- 点左侧知识库页 → `openWikiPageTab` 开页面 tab（并保持左侧高亮），替代原先的单页替换视图。

### 设置页服务配置
- `app_meta.tweet_fetch_service`（默认 `fxtwitter`），`get/update_tweet_fetch_service` 两个 API。
- UI 用下拉框（`DropdownButtonFormField`，非 TextField），避免破坏 settings_screen_test 的
  `fields.length == 6` 断言。

## 验证

- E2E 探针（真实 fxtwitter + 真实 gpt-4o，demo 副本）：抓取 `twitter.com/jack/status/20` →
  `tweet-20` / `jack 的推文` / 原文准确；内容对话返回基于原文的自然概括；保存后
  `kind=source`；token_usage 记录 conversation_id=NULL、544 tokens、model=gpt-4o。
- `wiki_ui_test` 需适配：tab 条引入横向 ListView 后，拖拽滚动必须用
  `scrollDirection == Axis.vertical` 谓词定位 sidebar 列表。
- flutter analyze 0 error/warning。

## 遗留

- 远期 sources 表（method/case/principle/series）落地时，`kind=source`（slug=`tweet-{id}`）
  页面迁移。
- tab 上限 8 个的踢出策略目前是「关闭最早的非固定 tab」，可按需优化。