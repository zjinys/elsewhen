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
- 最后更新：2026-10-14（多设备同步收口；已实现提案物理归档）

## 待实现（11 项）

| Proposal | 缺失内容 | 优先级 |
|---|---|---|
| [对话框 → 抽屉改造](notes/proposed/product/2026-10-04-chat-dialog-to-drawer.md) | 方案已定，未动手 | 高 |
| [决策辅助闭环](notes/proposed/architecture/2026-09-19-decision-support-loop.md) | 自动提醒 + 完整行动度量（Phase 4C 收尾） | 高 |
| [目标偏差检测](notes/proposed/architecture/2026-09-28-goal-schema-and-deviation.md) + [周期统计报告](notes/proposed/architecture/2026-09-30-periodic-deviation-report.md) | 周期统计 + 偏差检测（`goals` 表已建） | 中 |
| [单流主题后续演进](notes/proposed/product/2026-10-05-single-stream-topic-follow-ups.md) | 主题生命周期与状态排序、分析队列摘要、同名候选确认、AI 建议归拢 | 中 |
| [工作项一等页面](notes/proposed/architecture/2026-09-22-work-items-as-first-class-pages.md) | 未做 | 中 |
| [今日记忆检索](notes/proposed/2026-09-15-daily-memory-retrieval.md) | 待实现 | 低 |
| [save_knowledge_draft 来源发现死循环](notes/proposed/bug-fix/2026-10-08-save-draft-source-discovery-loop.md) | 错误消息指路 + list_wiki_pages 输出带 slug | 中 |
| [Linux/Wayland 窗口位置记忆失效](notes/proposed/bug-fix/2026-10-10-window-geometry-wayland.md) | 关闭时补存已落地但仍不恢复；疑似 Wayland 下 setBounds position 被忽略，需补恢复侧诊断 | 中 |
| [快速捕获切不到 CaptureScreen UI](notes/proposed/bug-fix/2026-10-10-quick-capture-no-ui-switch.md) | tray/单实例转发只改窗口外观不改 UI（渲染只认启动 config.mode）；需运行期 mode provider，全局快捷键 + tray 项存留待定 | 中 |
| [Today 每日回顾工作台](notes/proposed/product/2026-09-20-today-daily-review-workbench.md) | 暂缓（后置） | 后置 |

## 进行中（0 项）

| Proposal | 现状 |
|---|---|
| [多设备同步](notes/proposed/architecture/2026-09-30-multi-device-sync.md) | **开发关闭（2026-10-14，`e9254d1`）**：S00–S05 全部交付；基线 `f7f6e74`（迁移 34、快照 v3，Rust 711 / Flutter 372 / analyze 0 error）。唯一阻塞项为 S06 真人双设备门禁——按[人工测试路线 M0–M7](roadmap/2026-10-14-sync-manual-test-roadmap.md) 执行后回填 [S06 清单](roadmap/2026-10-14-s06-human-gate-checklist.md)。历史证据见 [Handoff §1–§31b](roadmap/2026-10-07-multi-device-sync-handoff.md) |

## 收尾（2 项）

| Proposal | 现状 |
|---|---|
| [多设备同步](notes/proposed/architecture/2026-09-30-multi-device-sync.md) | 开发关闭（2026-10-14，`e9254d1`）：S00–S05 交付，基线 `f7f6e74`（迁移 34、快照 v3，Rust **711** / Flutter **372** / analyze 0 error）。唯一阻塞项为 S06 真人门禁——按[人工测试路线 M0–M7](roadmap/2026-10-14-sync-manual-test-roadmap.md) 执行后回填 [S06 清单](roadmap/2026-10-14-s06-human-gate-checklist.md)；证据见 [Handoff §1–§31b](roadmap/2026-10-07-multi-device-sync-handoff.md) |
| [AI 对话体验](notes/proposed/architecture/2026-09-30-ai-chat-experience.md) | C00–C09 工程 + 合成场景完成；真人 GUI 签收移交[人工验收 roadmap](roadmap/2026-10-04-ai-chat-manual-acceptance-roadmap.md)（S1–S6） |

## 分析/方向（1 项，不产生直接待办）

| Proposal | 说明 |
|---|---|
| [Elsewhen 作为 Agent Harness](notes/proposed/2026-10-02-elsewhen-as-agent-harness.md) | 现状对照与缺口分析，产出方向清单 |

## 已完成 / 已归档（不在待办内）

以下 note 已标为非 `proposed` 状态，不作为待办（状态以各 note 头部 `Status` 为准）；2026-10-14 起已实现类物理移入 `docs/notes/implemented/{architecture,feature,simplification}/`，superseded/archived 类移入 `docs/notes/archived/`（文件名未变）：

- `implemented`：`event-recordability-filter`、`archived-conversation-deletion`、`single-stream-topic-implementation`（`d6ca18d`）、`wiki-workflows-and-scale`、`wiki-tabs-tweet-import-refactor`、`wiki-metadata-and-reading`、`conversation-role-separation`、`editor-reading-settings-layer`、`2026-09-27-web-search-tool`（外网搜索，migration v25：多引擎 Tavily/Brave/SearxNG + DoH+ECH `src/net_doh/`，实测开 x.com）
- `superseded`：`external-directory-project-import-boundary`、`personal-event-system-boundaries`
- `archived`：`rikkahub-chat-context-comparison`
- `adopted`：`personal-cognition-main-loop`
- `done`：`refactor/api-dart-generation-migration`、`refactor/storage-and-detail-view-split`
- 已修复（历史记录）：`refactor/input-budget-token-accounting`
