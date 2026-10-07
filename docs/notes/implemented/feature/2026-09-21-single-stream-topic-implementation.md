# Agent Note: 单流对话与显式主题入口实现

Status: implemented

## Problem

Flutter 对话入口曾把 conversations 展示为可新建、改名、归档和切换的列表。这个模型把聊天过程当成长期对象管理，不符合 Elsewhen 以事件、实体、待办和知识页承载记忆的设计。

## Decision

对话入口使用唯一主对话流。底层 conversations 保留，但 UI 主路径不再提供会话列表管理。数据库中持久化的保留标题 `主对话流` 与 `diary` 标签标识主对话；旧普通会话在首次加载时收敛到该入口，知识页聊天通过 `wiki_page_slug` 保持隔离。

主对话按日期分隔并提供分页、日期定位、用户原话搜索和“回到现在”。显式输入 `#主题名`、`/topic 主题名` 或“进入主题：XXX”会查找或创建 topic 知识页，并切换到页内聊天。topic 聊天不进入主事件流，讨论结果通过 `save_wiki_revision` 确认门沉淀；topic 页可返回主对话。

AI 可以建议把讨论沉淀到已有或新知识页，但不能因为普通讨论就静默创建 topic，也不能绕过确认门修改页面。topic 仍是内部存储类型，不新增独立“议题库”入口。

## Alternatives considered

- 保留最近对话列表作为过渡：不采用。它会继续强化把聊天过程当成长期对象管理的信息架构。
- 完全依赖 AI 自动判断 topic：不采用。边界错误时用户无法预期内容归属，因此保留明确、可纠正的显式入口。
- 删除底层 conversations 数据模型：不采用。旧消息和知识页聊天仍需可读，存储模型与 UI 信息架构保持分离。

## Verification

- 提交 `d6ca18d` 同时落地主对话、时间线定位、显式 topic 入口、topic 页返回和沉淀入口。
- `ui/test/topic_marker_test.dart` 覆盖 `#主题`、`/topic`、自然语言入口和普通文本误触发排除。
- 实现时 `cargo test --lib` 129 项通过，`flutter analyze` 无 error，`git diff --check` 通过。
- 当前实现仍保留 `主对话流`/`diary` 身份、分页与定位、显式 topic 语法和 `wiki_page_slug` 隔离。

## Consequences

用户打开对话入口后直接进入持续主流，不再管理多个普通会话；需要长期承接的讨论可以进入知识页聊天，并通过确认门形成可复用内容。底层旧会话和消息没有被删除，历史仍可回读。

主对话无限增长仍需要分页。主题生命周期、按状态排序的“现在”区、分析队列可见性、同名候选确认和 AI 建议归拢没有作为本次基础实现的完成条件，后续范围见[单流主题后续演进](../../proposed/product/2026-10-05-single-stream-topic-follow-ups.md)。
