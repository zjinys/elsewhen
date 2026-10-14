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
- 最后更新：2026-10-10

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

## 进行中（1 项）

| Proposal | 现状 |
|---|---|
| [多设备同步](notes/proposed/architecture/2026-09-30-multi-device-sync.md) | 进行中（2026-10-14）：P4d S04/S06 正常加工移交完成——迁移 v31（`sync_processor_handoffs` 凭据记录 + `sync_processor_state` 单例状态机 + `sync_background_permission` 视图硬阻断）、`ProcessingYieldProof` Ed25519 移交凭据与名册继承校验、`yield_processor_role`（quiescent 收尾 + 签名 proof + relinquished）、`apply_signed_roster` handoff 处理（目标端 pending_takeover 观察端直接切换）、`confirm_processor_takeover`（frontier 追平门禁 + active）、延迟分析放行前任加工端 `<= final_sequence`；Rust **675 passed / 17 ignored**、Flutter **349 passed**、桥接重生成/release 通过。 Provider 冲突掩码审阅/显式裁决/可达性提示工程与自动化验收完成——迁移 v28（`sync_provider_versions.state` 增 `'dismissed'`）、冲突审阅/裁决存储层（双版本绑定失败关闭、绝不字段拼接）、UTF-8 安全掩码、纯静态可达性提示码、FRB API 完成；Rust 全量 **660 passed / 17 ignored**（含 6 个新端到端用例 + v28 迁移测试）、relay **16 passed**、Flutter 全量 **349 passed**、桥接重生成/release 通过；审阅对话框 UI 接线完成。P4c S03 生命周期 GC/水印/补快照完成（2026-10-13）：迁移 v29 + 每成员应用水印 + catch-up 窗口 + needs_resnapshot + 单调 GC 游标，Rust **667 passed / 17 ignored**（7 个新测试），落后成员失败关闭不静默清历史。P4a 完成（2026-10-12）：迁移 v27 + 7 个同步测试，Rust 648 全量通过。P3 数据集选择/修复与 App 自动同步调度工程与自动化验收完成——全量 Rust **640 passed / 17 ignored**、实际 Worker **16 passed**（含新 P3 稳态增量用例）、Flutter 全量 **345 passed**、桥接重生成/release 通过；全仓 Rust 格式通过，Flutter analyze 72 项例外单列。P2 自动交接验收完成（2026-10-11）：P2 Rust 定向 **16 passed**、全量 Rust **640 passed / 16 ignored**（另配置环境 **5 passed**）、实际 Worker **15 passed**（含生产 P1→P2 故障代理流程）、桥接重生成/release（hash **233902080**）；Flutter 同步相关定向 **48 passed**、全量 **345 passed**、P2 定向 analyze 无问题、P2 格式通过。同日经用户明确要求修复全仓 Rust 格式（16 个文件，纯格式化）并复验全量/release 通过。下一步为 P4e（撤销 UX/诊断/升级）与发布门禁，见 [Handoff §19](roadmap/2026-10-07-multi-device-sync-handoff.md)；原生密钥库签收、S05 与真人设备门禁仍未完成，加工移交的真人双端验收属 S06 门禁（[同步 roadmap](roadmap/2026-10-03-multi-device-sync-roadmap.md)） |

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

- `implemented`：`event-recordability-filter`、`archived-conversation-deletion`、`single-stream-topic-implementation`（`d6ca18d`）、`wiki-workflows-and-scale`、`wiki-tabs-tweet-import-refactor`、`wiki-metadata-and-reading`、`conversation-role-separation`、`editor-reading-settings-layer`、`2026-09-27-web-search-tool`（外网搜索，migration v25：多引擎 Tavily/Brave/SearxNG + DoH+ECH `src/net_doh/`，实测开 x.com）
- `superseded`：`external-directory-project-import-boundary`、`personal-event-system-boundaries`
- `archived`：`rikkahub-chat-context-comparison`
- `adopted`：`personal-cognition-main-loop`
- `done`：`refactor/api-dart-generation-migration`、`refactor/storage-and-detail-view-split`
- 已修复（历史记录）：`refactor/input-budget-token-accounting`
