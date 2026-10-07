# Proposal Backlog（未完成提案总索引）

> 集中列出 `docs/notes/proposed/` 下尚未完成的功能提案。本文件是「还有哪些没做」的唯一权威总览。
>
> **维护规则（必须遵守）：**
> 1. 新增提案时，在对应分类下加一行（链接到 note 文件）。
> 2. 提案完成 / 推进时，立即更新其「状态」列，并同步该 note 头部的 `Status` 字段。
> 3. 发现「标记滞后」（note 仍写 `proposed` 但实际已完成）时，先修正 note 状态，再更新本表。
> 4. 每次状态变更附日期与一行证据（commit / 测试结果），不写「空头完成」。
> 5. 已完成的提案不留在「待实现」表里，移到「已完成（归档）」或直接删除行；状态以各 note 头部 `Status` 为准。

- 建立日期：2026-10-04
- 最后更新：2026-10-07

## 待实现（8 项）

| Proposal | 缺失内容 | 优先级 |
|---|---|---|
| [对话框 → 抽屉改造](notes/proposed/product/2026-10-04-chat-dialog-to-drawer.md) | 方案已定，未动手 | 高 |
| [决策辅助闭环](notes/proposed/architecture/2026-09-19-decision-support-loop.md) | 自动提醒 + 完整行动度量（Phase 4C 收尾） | 高 |
| [目标偏差检测](notes/proposed/architecture/2026-09-28-goal-schema-and-deviation.md) + [周期统计报告](notes/proposed/architecture/2026-09-30-periodic-deviation-report.md) | 周期统计 + 偏差检测（`goals` 表已建） | 中 |
| [单流主题后续演进](notes/proposed/product/2026-10-05-single-stream-topic-follow-ups.md) | 主题生命周期与状态排序、分析队列摘要、同名候选确认、AI 建议归拢 | 中 |
| [工作项一等页面](notes/proposed/architecture/2026-09-22-work-items-as-first-class-pages.md) | 未做 | 中 |
| [外网搜索工具](notes/proposed/2026-09-27-web-search-tool.md) | 设计已定，未实现 | 低 |
| [今日记忆检索](notes/proposed/2026-09-15-daily-memory-retrieval.md) | 待实现 | 低 |
| [Today 每日回顾工作台](notes/proposed/product/2026-09-20-today-daily-review-workbench.md) | 暂缓（后置） | 后置 |

## 进行中（1 项）

| Proposal | 现状 |
|---|---|
| [多设备同步](notes/proposed/architecture/2026-09-30-multi-device-sync.md) | 进行中（2026-10-07）：owner 开发预览已接实际中继邀请、指纹/原 token 明确审批及原生修订确认后投递；新增Rust5项、Flutter6项（定向共15项）通过。最新Rust588 passed / 11 ignored（另5项配置）、实际Worker14 passed，桥接重生成/release通过；analyze两条测试风格提示已修，复验和Flutter全量待工具恢复（前一全量306 passed）。S00 已完成。App 邀请传输/relay 授权、joiner 原生握手、选择/修复 UI、自动同步、原生签收、加工移交、完整知识对象、TTL/GC/恢复及真实设备门禁仍未完成（见 [同步 roadmap](roadmap/2026-10-03-multi-device-sync-roadmap.md)） |

## 收尾（1 项）

| Proposal | 现状 |
|---|---|
| [AI 对话体验](notes/proposed/architecture/2026-09-30-ai-chat-experience.md) | C00–C09 工程 + 合成场景完成；真人 GUI 签收移交[人工验收 roadmap](roadmap/2026-10-04-ai-chat-manual-acceptance-roadmap.md)（S1–S6） |

## 分析/方向（1 项，不产生直接待办）

| Proposal | 说明 |
|---|---|
| [Elsewhen 作为 Agent Harness](notes/proposed/2026-10-02-elsewhen-as-agent-harness.md) | 现状对照与缺口分析，产出方向清单 |

## 已完成 / 已归档（不在待办内）

以下 note 已标为非 `proposed` 状态，不作为待办（状态以各 note 头部 `Status` 为准）：

- `implemented`：`event-recordability-filter`、`archived-conversation-deletion`、`single-stream-topic-implementation`（`d6ca18d`）、`wiki-workflows-and-scale`、`wiki-tabs-tweet-import-refactor`、`wiki-metadata-and-reading`、`conversation-role-separation`、`editor-reading-settings-layer`
- `superseded`：`external-directory-project-import-boundary`、`personal-event-system-boundaries`
- `archived`：`rikkahub-chat-context-comparison`
- `adopted`：`personal-cognition-main-loop`
- `done`：`refactor/api-dart-generation-migration`、`refactor/storage-and-detail-view-split`
- 已修复（历史记录）：`refactor/input-budget-token-accounting`
