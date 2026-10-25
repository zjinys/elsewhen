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
- 最后更新：2026-10-25（relay v3 roadmap **R0 ~ R6 全部完成**、review 修复 **R7-P0 ~ R7-P6 全部完成（含二次复核残留）**：数据面/信令面分离落地，验收 6 项指标一次性复核通过；`docs/review_logs/2026-10-25/review_20261025_155152.md` 的 6 条发现与 12 条小刺全部有结论）
- 口径变更 2026-10-24：relay 只保留 standalone 单实现（Cloudflare Worker 下线），功能面收窄为「信箱 + blob」，不再承担 APK 分发。

## 待实现（17 项）

| Proposal | 缺失内容 | 优先级 |
|---|---|---|
| [relay 存储放大与配额失效修复](notes/proposed/architecture/2026-10-24-relay-storage-amplification-and-quota.md) | 方案已定（含逐文件改动范围）：已升为 v3 架构方案（数据面/信令面分离，含 P0 止血附录）；P0 止血=`seq_hint` 幂等（533MB→122MB）·`507` 语义化停重试·`relay_id` COALESCE·sweep+VACUUM+句柄 LRU；P1–P4=blob 端点·客户端窗口化·`drop` 动词·状态机。实测依据：1.7GB 归档（重复投递 4.4×、joiner 信箱打满 512MiB、死信 9996/10000）。**已转实施并全部完成：[v3 数据面重构 roadmap](roadmap/2026-10-24-relay-v3-dataplane-roadmap.md)**（2026-10-24 起，2026-10-25 收尾）。R0 前置清理（relay 单实现、删应用内升级链）·R1 两处生产 bug（配对帧被当损坏信封冒泡、`owner_confirmed` 早退吞回执）·R2 P0 止血（`seq_hint` 幂等 4.41×→1.00×、`507` 退避、`relay_id` COALESCE、sweeper+VACUUM 把 0 封的 66.6MB 空壳缩到 0.1MB）·R3 relay 侧 blob 数据面（六端点 + 两张表 + `/status` 两本账）·R4 客户端窗口化快照（174 片/121.7MB 实测峰值 6.67 MiB、`close` 后归零、outbox 只剩 2 条信令）·R5 `drop` 动词与 `ack` **签名域分离** + 「3 次拒绝必表态」计数落表 + 三条容量不变式进 CI·R6 `deliver` 状态机（**推导不存储**，唯一新增列 `last_error`）+ 结构化日志 + 只读 UI·**R7 review 修复 P0~P5**（`snapshot-start` 收口 + 幂等重投、abort 续传、能力协商（缺 `capabilities` 即无数据面）、`blocked` 真落地、拒收计数按信箱分维度（迁移 63）、blob 回收失败可重试、relay 侧 m10 七个小刺（`schema` 去重只认 UNIQUE、`SQLITE_BUSY`→503、六签名域交叉重放、PUT 走 `validateEnvelope`、UI 未知状态码原样显示））。**R7-P6**（[二次复核](../review_logs/2026-10-25/review_20261025_193514.md)残留）把同步链路 8 处裸 `eprintln!` 全部归一到 `wire_log` + 源码 lint 用例，并把汇总行的 `bytes` 从「封数」改成真实密文字节。两处**主动不做**并记录理由：`DeliveryFailure::BlobGone`（无消费者）、`SnapshotTransport::from_env` 语义变更。**验收 6 项一次性复核通过**（写入 1.00×、峰值 6.67 MiB、空信箱 VACUUM 后 0.07 MiB、死信 0、客户端分片级、残留 0）。门禁（R7 收口后）：`cargo test --all-targets` **960 passed / 0 failed / 19 ignored**、relay `node --test` **49/49**、`fvm flutter analyze` **0 error**、`scripts/regen.sh` 桥 hash 一致 | 高（已完成） |
| [对话框 → 抽屉改造](notes/proposed/product/2026-10-04-chat-dialog-to-drawer.md) | 方案已定，未动手 | 高 |
| [决策辅助闭环](notes/proposed/architecture/2026-09-19-decision-support-loop.md) | 已转实施路线：[D0–D6](roadmap/2026-10-14-decision-support-loop-roadmap.md)（2026-10-14）；规则触发引擎 / 用户模型增量 / 分析指导 / 情绪陪伴 / 闭环度量 | 高 |
| [目标偏差检测](notes/proposed/architecture/2026-09-28-goal-schema-and-deviation.md) + [周期统计报告](notes/proposed/architecture/2026-09-30-periodic-deviation-report.md) | 周期统计 + 偏差检测（`goals` 表已建） | 中 |
| [单流主题后续演进](notes/proposed/product/2026-10-05-single-stream-topic-follow-ups.md) | 主题生命周期与状态排序、分析队列摘要、同名候选确认、AI 建议归拢 | 中 |
| [工作项一等页面](notes/proposed/architecture/2026-09-22-work-items-as-first-class-pages.md) | 未做 | 中 |
| [今日记忆检索](notes/proposed/2026-09-15-daily-memory-retrieval.md) | 并入[决策辅助闭环 D6](roadmap/2026-10-14-decision-support-loop-roadmap.md)（对话出口） | 低 |
| [save_knowledge_draft 来源发现死循环](notes/proposed/bug-fix/2026-10-08-save-draft-source-discovery-loop.md) | 错误消息指路 + list_wiki_pages 输出带 slug | 中 |
| [Linux/Wayland 窗口位置记忆失效](notes/proposed/bug-fix/2026-10-10-window-geometry-wayland.md) | 关闭时补存已落地但仍不恢复；疑似 Wayland 下 setBounds position 被忽略，需补恢复侧诊断 | 中 |
| [快速捕获切不到 CaptureScreen UI](notes/proposed/bug-fix/2026-10-10-quick-capture-no-ui-switch.md) | tray/单实例转发只改窗口外观不改 UI（渲染只认启动 config.mode）；需运行期 mode provider，全局快捷键 + tray 项存留待定 | 中 |
| [主对话流摘要管道永久失败 → AI 忘记刚说的话](notes/incidents/2026-10-19-summary-pipeline-permanent-failure.md) | 每轮摘要失败（summary_ready 0 次）+ 降级砍掉最近轮原文 + 错误被 Err(_) 吞掉；P0：失败日志 + 最近 N 轮不可裁剪 + 诚实降级文案；P1：归并根节点失败点；P2：工具 schema 40K 瘦身 | 高 |
| [上下文预算与工具注入修复路线图](roadmap/2026-10-19-context-budget-and-tool-injection-roadmap.md) | 统一修复计划：R1 测量校准 → R2 工具瘦身 → R3 审批续跑闭环 → R4 超限兜底；含顺序理由与红线 | 高 |
| [Gitea MCP 工具调用失效与上下文超限](notes/incidents/2026-10-19-mcp-gitea-tool-call-review-and-budget-incident.md) | 现象：未受信拦截伪成功致 AI 承诺、确认后无恢复续跑闭环、回执 raw JSON 糊在弹窗、受信后 53 个工具 Schema 40KB+字节退化估算打爆 64K 上下文。待修复 P0/P1/P2 | 高 |
| [MCP 工具接入延迟加载与按需暴露](notes/proposed/architecture/2026-10-19-mcp-tools-deferred-injection.md) | 方案已定：L1 花名册常驻 enable_tools 描述 + L2 按工具全名激活注入；复用 defer/enabled_tools 框架；tools/list 结果缓存；事故场景验收指标明确 | 高 |
| [非 GPT 模型 token 估算校准](notes/proposed/architecture/2026-10-19-token-estimation-calibration.md) | 方案已定：L0 族分词器 + L1 校准字节比率（JSON 3.5/中文 1.2-1.5）+ L2 保守兑底 + provider 级 ratio 覆盖逃生门；保守方向不破 | 高 |
| [摘录孵化闭环（讨论 → 纳入知识 / 丢弃为负知识）](notes/proposed/product/2026-10-19-clip-incubation-and-negative-knowledge.md) | 方案已定待实施：孵化闸门（`opinion IS NULL`=孵化中，仅对 source/note 收紧缺省语义，不参与编译/召回）+ AI 只在派生稿上改 + 纳入前须先采用一版派生稿 + 丢弃写 `knowledge_sources.reject_reason` 并 archived 留痕（不做撤销否定、素材库独立分组）；需统一参与谓词、回填存量、把 `knowledge_sources` 移入快照 carried | 中 |
| [知识页持续更新与关系网](notes/proposed/architecture/2026-10-19-knowledge-page-reconsolidation-and-relations.md) | 方案待评审：消化选页改「相关性优先」+ 提示词/slug 纪律 + 页级再汇总 job + `knowledge_relations` 语义关系网（关系只扩上下文 / 排队复核，不自动改写邻居） | 中 |
| [Today 每日回顾工作台](notes/proposed/product/2026-09-20-today-daily-review-workbench.md) | 暂缓（后置） | 后置 |

## 进行中（0 项）

| Proposal | 现状 |
|---|---|
| [多设备同步](notes/proposed/architecture/2026-09-30-multi-device-sync.md) | **开发关闭（2026-10-14，`e9254d1`）**：S00–S05 全部交付；基线 `f7f6e74`（迁移 34、快照 v3，Rust 711 / Flutter 372 / analyze 0 error）。唯一阻塞项为 S06 真人双设备门禁——按[人工测试路线 M0–M7](roadmap/2026-10-14-sync-manual-test-roadmap.md) 执行后回填 [S06 清单](roadmap/2026-10-14-s06-human-gate-checklist.md)。历史证据见 [Handoff §1–§31b](roadmap/2026-10-07-multi-device-sync-handoff.md) |

## 收尾（5 项）

| Proposal | 现状 |
|---|---|
| [记事本页（随手记，不经 AI）](notes/proposed/product/2026-10-19-notepad-memo-pages.md) | 已实现（2026-10-19，`0665af1`）：`memo` kind + 三层白名单隔离（AI 可写排除 / digest 上下文排除 / 人类专属）+ `create_wiki_page` / 单事务 `append_wiki_page_content`；Flutter 记事本入口 + 快记框。基线 Rust **849** / Flutter **403** / analyze 0 error。剩余：真人 GUI 验收；系统通知通道与 Capture 落点为后续增量 |
| [定时提醒（"20 分钟后提醒我"）](notes/proposed/architecture/2026-10-19-timed-reminders.md) | 已实现（2026-10-19，`0665af1`）：migration 51 加 `todos.reminder` + 设备本地 `todo_reminder_fires`；pop 挂 5s tick 最前（故障隔离）；snooze = due_at 推后 + fire 删除（单事务）；本地解析（7 单测）+ `create_reminder` AI 工具（确认闸）。基线 Rust **849** / Flutter **403** / analyze 0 error。剩余：真人 GUI 验收；`flutter_local_notifications` 系统通知为后续增量 |
| [习惯主动问候](notes/proposed/product/2026-10-22-proactive-habit-checkins.md) | 已实现（2026-10-22，`36a88fc` + A 方案）：migration 57 + 计划 CRUD/确定性推断 + 5s tick + 主对话 `habit_nudge`/`habit_checkin` + 定制按钮/未读点 + 后台 failover + 设置页「习惯」集中列表（暂停/恢复/调整）；**多设备只有 owner 主设备触发**，其他设备用同步 checkin 判断已回答、不显示无本机 fire 的「晚点再问」。验证：Rust **914** / Flutter **429** / analyze 0 error（既有 133 条 info/warning 不计）。已修 digest 漏写 kind 前缀导致的重复 habit 页，并清理真实库中的 `after-lunch-walk` 重复。2026-10-23 补齐：确认卡「推断依据」展示、问候卡「聊聊/聊聊原因」预填输入框（不自动发送）、全局限流每日 2 条（补问不占额度）、连续 3 天忽略后确定性建议卡（改时间走确认对话框、暂停经用户点击才落库）。验证：Rust **916** / Flutter **429** / analyze 0 error。剩余：真人 GUI 多设备验收 |
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

## 本轮新增完成（2026-10-25，R4 + R5 + R6 收尾）

- **relay v3 roadmap R4 ~ R6 全部完成，验收 6 项指标一次性复核通过**：[roadmap](roadmap/2026-10-24-relay-v3-dataplane-roadmap.md)。
  - **R4 窗口化快照**：分片走 `/blob/*` 数据面、接收端边拉边删、逐窗 `ack`，relay 峰值与快照总量解耦。三条落地修订：窗口按 `WINDOW_CHUNKS * CHUNK_BYTES_MAX ≤ RESPONSE_BYTES` 定（32 片 = 21 MiB 会撞穿 relay 16 MB 响应上限，表现为 `relay blob response too large` 死锁）；等 ack 预算改成调用方属性 `BlobUpload.window_wait_ms`；峰值采样必须紧跟 `push_due`（接收端收一窗 ack 一窗，import 之后再读 relay 账恒为 0）。R4-5 口径纠正：回执对象仍是 `snapshot-end:{id}` manifest 信封，变的是时机。
  - **R5 `drop` 与 `ack` 分家**：relay 上同为删除，但**签名域不同**（拿 ack 的签名去 drop 结构性 403），「已消费」与「已丢弃」不再共用一个词；「必表态」把死信治理从「等 7 天 TTL」变成「3 次拒绝后必须表态」，计数落表（放内存等于每次重启重数、阈值永远到不了），且**只对不可恢复错误计数**（依赖未到的 `waiting` 不计、部署错位的 `sync_protocol_unsupported` 只上报不 drop）；三条容量不变式进 CI。
  - **R6 `deliver` 状态机**：状态**推导不存储**（`relay_id`/`available_at_ms`/`blocked_reason`/`last_error` 已是真相，再存一列必然自相矛盾），唯一新增列是迁移 62 的 `last_error`。落地抓到两处语义混淆并拆开：「已送出的最小重发间隔」不是退避（backoff 判定加 `last_error <> ''`，成功路径改名 `resend_spacing_ms`）；`blocked` 只负责让人看见、不负责刹车（507 保留 15 s 再试）。结构化日志唯一入口 `wire_log`，行格式拆成 `wire_line` 以便断言「只有四个字段」。
  证据：事故同规模（`ELSEWHEN_BLOB_SNAPSHOT_EVENTS=3 ELSEWHEN_BLOB_SNAPSHOT_UNITS=400000`，174 片/密文 121.7 MB）跑 `smoke::end_to_end_send_fetch_ack_against_real_worker` **18 拍 29.51 s 绿**，`[blob] acceptance ... peak=6995630 baseline=0`（≈6.67 MiB = 一窗 10 片，与快照总量无关）；`close` 后 `blob_bytes`/`blobs` 归零；三个跑过数据的信箱 0 封/0 blob/0 片，`wal_checkpoint(TRUNCATE)+VACUUM` 后 8,486,912 → **77,824 字节**。门禁：`cargo test --all-targets` **932 passed / 0 failed / 19 ignored**（200 s）、relay `node --test` **41/41**（含两个原先 `--ignored` 的 Rust e2e 包裹用例）、`fvm flutter analyze` **0 error**、`sync_deliver_status_test` 4 例绿、`scripts/regen.sh` 桥 hash 一致。

## 本轮新增完成（2026-10-25，R3 blob 数据面）

- **relay v3 数据面（P1）落地**：[roadmap R3](roadmap/2026-10-24-relay-v3-dataplane-roadmap.md)。`relay/standalone/schema.mjs` 加 `relay_v3_blob` / `relay_v3_blob_chunk`（主键 `(blob_id,idx)` ⇒ 重复 PUT 结构性去重）并把 blob 的 30 分钟无进展 TTL 并进 `purgeExpired`；新增 `relay/standalone/blob.mjs` 实现 `open`/`chunk(PUT)`/`chunk(GET)`/`state`/`ack`/`close` 六端点，全部复用现有 `stamp()`/签名域/`nonce`/`withTx`，没有一行自创鉴权；`PUT` body 是裸二进制（新 `boundedBinary`），`GET` 返回 `[4 字节长度][密文]` 连续帧裸流（省 base64 的 1.37×）；`/status` 增 `capabilities:["blob"]` 与 `signal_bytes`/`blob_bytes` 两本账（老字段语义不变）。
  证据：新增 `relay/standalone/blob.test.mjs` 7 用例 —— 重投同片 `blob_bytes` 不变、`ack upto` 后 `10240→2048`、`close` 后 `0`、7 种非法 `blob_id`→400、`version:1`→426、未注册 sender/owner 冒签/越界索引全拒、`1MiB+1`→413、超配额→507、TTL 到期自动回收。门禁：`node --test relay/standalone/*.test.mjs relay/test/*.test.mjs` **39/39 pass**（32 基线 + 7 新增）、`cargo test --all-targets` **917 passed / 0 failed / 19 ignored**（Rust 未动）。
  两条与方案原文的偏离（已在 roadmap 与方案 note 里写明理由）：读侧多出 `blob-state`/`blob-get` 两个签名域（否则 `state` 的签名可重放 `get`）；上传侧用 sender 凭据而非 owner（owner 凭据能读能删整个信箱，不能给上传方）。

## 本轮新增完成（2026-10-25，R1+R2）

- **relay v3 roadmap R1 + R2（P0 止血）全部完成**：[roadmap](roadmap/2026-10-24-relay-v3-dataplane-roadmap.md)。R1 修掉两处生产 bug（信箱混入配对帧被当损坏信封冒泡、`owner_confirmed` 早退吞掉 completion 回执）；R2 六项：`seq_hint` 作投递幂等键（**原地替换、保住原 id** —— `DELETE`+重插会让按 `id>after` 翻页的接收方永远卡在最新一批，`roundtrip.rs:341` 实测稳定红）、客户端 `sync_wire_seq` 单行发号器（migration 59）、传真值到两条 push 路径、`507` 语义化（`RelayQuotaExceeded` + 15s 退避 + 面板提示）、`relay_id` COALESCE、relay 卫生（`schema.mjs` 共用 DDL + `sweeper.mjs` 扫目录的 `worker_threads` 回收器 + 句柄 LRU + `VACUUM`/`wal_checkpoint`）。
  证据：真实 1.7GB 归档量出**同一信箱 801 封 / 534.8 MB 只有 175 个不同载荷 = 4.41×**，同形态重放 `seq=0` → 801 封 / 510.0MB、固定 `seq_hint` → **175 封 / 115.5MB**；`d6157af7` **0 封邮件占 66.6MB** → sweeper 后 **0.1MB**。门禁：relay `node --test` **32/32 pass**、`cargo test --all-targets` **917 passed / 0 failed / 19 ignored**、`fvm flutter analyze` 零 error。

## 本轮新增完成（2026-10-24）

- **relay 单实现化 + 移除应用内自动升级（APK 分发）**：[roadmap R0 全部完成](roadmap/2026-10-24-relay-v3-dataplane-roadmap.md)。relay 只剩 `relay/standalone/server.mjs`（零第三方依赖，`worker.ts`/`wrangler.toml`/`deploy-relay.sh`/`miniflare` 全删）；`/release/latest`、`/release/apk`、`RELEASES_DIR`、R2 桶、Dockerfile releases 挂载删除；升级链从 Android 权限（`REQUEST_INSTALL_PACKAGES`）、FileProvider、平台通道、`app_update.dart`、设置页入口，到 Rust 的 `check_app_update`/`download_update_apk`/`AppUpdateDto`/`ReleaseManifest` 与 `scripts/publish-release.sh` 整条删除。证据：`cargo test --all-targets` **917 passed**、relay `npm test` **21 passed**、`fvm flutter analyze` 零 error、bridge hash 两侧 `916603462`。附带收获：删掉 Worker 版后 standalone 的 `base64Size` 缺校验（空 payload 入库、非字符串 500）立即被契约测试打红 -> 已修。
