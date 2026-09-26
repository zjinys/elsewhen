# Agent Note: 深度代码审查 P1 修复批（数据正确性 × 崩溃 × 资源泄漏）

Status: implemented

## Problem

深度代码审查（3 并行子代理 + 主代理抽验）标注 10 个 P1 问题，分两类核心伤害：

1. **数据正确性 / 丢失**：`record_event` 全表扫描回读且相同文本会取错行；`list_events` 伪造 UUID + 硬编码 source/status；pending action 执行失败被直接删除（动作内容永久丢失）；failover 重放下 WriteDirect 事件重复入库；config 数据库迁移吞错（legacy 数据滞留、启动后建空库）。
2. **崩溃 / 泄漏**：`strip_blocks` 用 `to_lowercase()` 的索引切原串（İ→i̇ 变长即 panic）；settings 的「最大 Token 数」输入框每次 build 新建 `TextEditingController` 且从不 dispose、值也不回写；`upsert_wiki_page` 页面写入与 revision 审计非原子。

## Decision

- **record_event**：直接用插入前的 `NewEvent` 构造 DTO（move 前先摘字段），删掉扫描 + `find(raw_text)`。
- **list_events**：`EventSummary` 补 `id/source/status` 三字段，三个查询如实读库；伪造 UUID 与 "unknown"/"completed" 硬编码删除。
- **pending action**：仅执行成功才 `delete_pending_action`；失败保留排队，可下次确认重试。
- **failover 幂等**：`run_agent_loop` 拆出 `run_agent_loop_inner`（带 attempt 级事件收集器）；WriteDirect 唯一的工具 `record_event` 返回格式固定（`已保存事件（{uuid}）`），解析收集；尝试 Err 时回滚本尝试写入的事件（`Store::delete_event`），重放从无副作用库开始。自动记录输入的事件不在收集器内（走 `input_already_recorded` 守卫），不会误删。
- **strip_blocks**：新增 `find_case_insensitive`，按 `char_indices` 字符边界定位，杜绝小写化索引越界。
- **config 迁移**：`migrate_db_file` 返回 `Result`，主文件与 -wal/-shm 副产物迁移失败全部上抛——不静默建空库、不丢 WAL 已提交事务。
- **upsert_wiki_page**：`record_wiki_revision` 共享化（`record_wiki_revision_on(&Connection)`，事务经 Deref 传入）；更新/建页 + revision 同事务原子提交。merge/undo 核查后本身已事务完整，无需改动。
- **settings maxToken**：内联控制器提升为 `_maxTokensController` 字段（initState 初始化、dispose 释放），并在保存块回写 `maxTokens`——控件从死变活。

## Consequences

- Rust 测试 **172 passed / 0 failed**（新增 `list_events_carries_real_identity_and_meta`、`strip_blocks_survives_unicode_prefix_and_case_variant` 两条回归）。
- Dart analyze 0 error；`flutter test` **+159 -11 与基线逐题一致**，无新增破坏。
- 桥接口 DTO 形状未变，无需 `./regen.sh`。
- 兼容并行会话：保留其已改的 `record_event` DTO `source: "flutter_gui"` 字面量；遗留觉察——DB 行实际 source 是 `"capture"`（`NewEvent::now`），GUI 录入路径的 source 语义待并行侧对齐。
- 改动未提交，随并行批次落库。

## 收尾（source 语义遗留项，2026-09-25）

遗留觉察已解决：`record_event` 不再借用 `NewEvent::now` 的 `"capture"`，改为显式构造 `NewEvent { source: "flutter_gui" }` 落库——DTO 与 DB source 对齐。`list_events` 如实读库后，GUI 快录事件在 `event_card` 的 source badge 正确显示 GUI（而非 device_unknown）；`NewEvent::now` 的 `"capture"` 保留给 capture/hotkey 窗口路径，AI 工具路径仍为 `"tool"`。新增回归 `record_event_persists_dto_source_so_badge_renders_consistently`（`ELSEWHEN_DATA_DIR` 隔离 + Drop guard 恢复，防并行测试污染）。Rust 全量 173 passed / 0 failed；flutter analyze 0 error。

## P0 修复批次（2026-09-25，review 原始输出补齐）

复盘三路 review 子代理原始报告（opencode 会话库）发现：deep review 实际产出 **P0 8 / P1 20 / P2 26**，笔记此前只记录了 10 项 P1，8 项 P0 与 26 项 P2 未记录。遂补齐 P0：

- **已由并行批次顺带解决（3 项）**：P0-1 `runAiGeneration` catch 已有 `isMounted` 守卫；P0-3 wiki import tab 已统一 `wikiImportTabId()` getter；P0-7 `left_sidebar` 的 `.then` 异步链已随重构消失（现为 `pagesAsync.when` 同步渲染）。
- **本次修复（5 项）**：
  - **P0-2** `message_area.dart` `_submit` finally：`_focusNode.requestFocus()` 移入 `if (mounted)` 块——卸载后 requestFocus 抛 "A FocusNode was used after being disposed"。
  - **P0-4** `wiki_page_detail_view.dart` `_WorkItemPanel`：`updateFields`/`updateStatus` 包 try/catch（SnackBar 反馈）+ `context.mounted` 守卫；三处 async 调用点（状态/优先级/日期）统一走安全函数。
  - **P0-5** `message_area.dart` `_PendingRelationsBanner`：build 内 bare `jsonDecode` 改为 try/catch 保护，解析失败渲染"无法解析"占位而非崩掉整个消息列表。
  - **P0-6** `wiki_ai_chat_panel.dart`：`_saveReply` 与 `_reimport` 读页 await 移入 try，失败经 `_error`/SnackBar 反馈，页面缺失给出明确提示。
  - **P0-8** `todo_view.dart` `onOpenWorkItem` 包 try/catch + SnackBar 反馈，与 `_add`/`_toggle` 既有错误路径一致。

验证：`flutter test` 全量 **+170, All tests passed**；`flutter analyze` 全项目 **0 error**（剩余均为既有 third_party info 与并行批次死代码 warning）。

### 仍未处理的 P1/P2（后续批次候选）

- **Rust P1（6 项）**：merge_entity 非原子守卫、undo_entity_merge SQLITE_BUSY_SNAPSHOT、idempotency check-then-insert 竞态、ai_provider_config 竞态、recordability N+1、排序缺 id tiebreak——前 5 项经主代理核查论证"已有事务保护无需改动"，P2-6 排序不稳定是主代理标注"近期修"。
- **Rust P2（12 项）**：WAL 无 checkpoint（P2-8）、local_sources 整文件读内存（P2-10）、capture.rs 吞错（P2-11）、main.rs 字节切片（P2-12）、analyze-once 死命令、CJK token 低估、unconditional eprintln 隐私日志等。
- **Dart P1（11 项）**：retry 仅对最后一条可见、markdown TapGestureRecognizer 泄漏、七个 broad provider watch 重建整个消息列表、settings 滑窗 Token 输入死控件（已随 P1 修复）、settings_provider 三连 state 写入无 mounted 守卫等。
- **Dart P2（2 项）**：wiki_page_detail_view 3807 行单文件巨型化、死代码（两个 legacy screen + mock repo + hotkey stub）。

## P2 批次一（2026-09-26，安全区直修 7 项）

从三路 review 原始报告提取 P2 明细，按「避开并行会话改动区」原则直修 7 项，全部落在并行未触碰的安全区（`main.rs`/`capture.rs`/`local_sources.rs`/`wiki.rs`/`ai/provider.rs`）或存储层远离并行 hunk 的 SQL 行：

- **P2-6 排序 tiebreak（主代理标注"近期修"）**：`storage.rs` 六处补确定性排序——`recent_event_records` 补 `, id ASC`；三个 last_message 子查询（`list_conversations`/`list_archived_conversations`/`get_conversation`）补 `, m2.rowid DESC`；两个 conversations 外层补 `, c.id DESC`；`list_messages`/`get_child_messages` 补 `, id ASC`。（`list_events` 已由并行批次带上 `id DESC`，未重复）digest 事件映射与消息列表从此跨调用稳定。
- **P2-7 导出文件名碰撞**：`wiki.rs` `export_wiki` 文件名改取完整 slug（`/`→`-`），不再只取尾段——`person/a` 与 `misc/a` 不再落到 `a.md` 互相覆盖。
- **P2-10 目录导入无界读**：`local_sources.rs` `ingest_directory_files` 改为先 stat 字节数、超 `MAX_SCAN_FILE_CHARS*4+4`（≈48KB）的文件直接以明确原因进 `skipped`，其余走 `read_bounded_text`——不再对数百 MB 文本整文件读入内存。
- **P2-11 capture 吞错**：`capture.rs` `Message::Submit` 拆 `match`——写库失败在窗口内以红色错误行显示 `保存失败：{e}`（原静默留在窗口，事件丢失无反馈），成功才 `spawn_background_analysis` + 退出；窗口高度 82→108 容纳错误行。
- **P2-12 字节切片 panic**：`main.rs` wiki log 输出 `&ts[..19.min(ts.len())]` 改 `ts.chars().take(19)`——非 ASCII 时间戳不再 mid-char 边界 panic。
- **base_url 尾斜杠**：`ai/provider.rs` OpenAI 与 Ollama 端点拼接前 `trim_end_matches('/')`——配置带尾斜杠不再产生双斜杠 URL。
- **P2-1 migration 双列守卫**：`storage.rs` v29 由单守卫 + `execute_batch` 双 ALTER 拆为两列各自判存在再分别 ALTER——进程在列间中断不再留下"有 human_edited_at 无 opinion"的死状态、下次 open 永久跳过。

回归测试（`storage.rs` tests，新增 2 条）：`message_ordering_is_deterministic_on_same_timestamp`（同毫秒消息 list_messages 按 id 稳定排序）、`migration_v29_recovers_when_only_second_column_is_missing`（DROP opinion + 删 v29 记录后重开，opinion 被单独补回）。

验证：Rust **175 passed / 0 failed**（173→175）；本轮改动文件 `cargo fmt --check` 干净；clippy 无本轮新增；未触碰并行 hunk（`storage.rs` 的 2281-2399/2712-2799/3143-3168 与 `src/ai/*` 全保留）。

### 仍未处理的 P2（后续批次候选）

- **Rust storage/core**：P2-8 WAL checkpoint、P2-5 全表扫描（`open_todo_work_item`/`get_todo`）、P2-2/P2-3 事务竞态（待主代理复核后再动）、merge/undo 相关已论证无需改动。
- **Rust AI**：P2 analyze-once 死命令（`capture.rs` spawn 的 `analyze-once` 在 `main.rs` 无对应 arm——实际分析由 Dart timer 驱动，需先确认协议）、CJK token 低估、unconditional eprintln 隐私日志、expand 后 client 复用——全在并行会话活跃的 `src/ai/*` 区内。
- **Dart**：P1/P2 列表见上（`message_area`/`wiki_page_detail_view` 等均为并行批次活跃文件，等并行落库后再动）。

## P2 批次二（2026-09-26，安全区直修 5 项）

- **P2-5 全表扫描改直查**：`storage.rs` `get_todo` 由两遍 `list_todos`（archived + 全量）内存 find 改为单行 `SELECT … WHERE id=?1`（列映射与 `list_todos` 逐字段对齐，archived 语义由既有测试 `get_todo_includes_archived_history_for_on_demand_migration` 覆盖）；新增 `find_wiki_page_by_tag`（`instr(tags, ?1)>0` 限定 JSON 数组成员 + `ORDER BY updated_at DESC LIMIT 1`）；`api.rs` `open_todo_work_item` 从 `list_wiki_pages(None,None)` 全表线性 find 改用 `find_wiki_page_by_tag(&marker)`。
- **P2-8 WAL checkpoint**：`storage.rs` `Store::open` 尾部（迁移+回填完成后）执行 `PRAGMA wal_checkpoint(PASSIVE)`（PASSIVE 不阻塞并发连接、不因 busy 报错）；新增 `impl Drop for Store` 在连接销毁前再 checkpoint 一次——长驻 GUI + spawn-per-call 写入不再让 -wal 无限增长。
- **provider 错误体隐私截断**：`ai/provider.rs` 新增 `provider_error_body(status, body)`（超过 300 字符截断并标注 `…（已截断）`），OpenAI（289）与 Ollama（511）两处 `bail!` 透传前截断——完整服务端 body（可能含对话内容/报错堆栈）不再直接进用户可见错误与落库。
- **analyze-once 死命令接通**：`main.rs` 补 `Some("analyze-once")` arm 调 `crate::api::trigger_analysis()`（无 provider 时打印提示、正常打印处理结果）——`capture.rs` `spawn_background_analysis` 后台拉起却落到 `print_usage()` 直接退出的空转进程从此真正排空分析队列。
- **`set_active_ai_provider_config` DEFERRED→IMMEDIATE**：`storage.rs:2229` 由 `unchecked_transaction` 改 `Transaction::new_unchecked(_, TransactionBehavior::Immediate)`（与 `claim_analysis_job` 一致）——先全零后点亮的两次 UPDATE 在拿写锁后一次性完成，避免 WAL 并发下升级锁时撞 worker 写事务报 `database is locked`。（conversation.rs:234 调用侧"回复落库后才 `?`"属并行活跃区，未动。）

回归测试（新增 2 条）：`find_wiki_page_by_tag_exact_matches_json_array_member`（精确命中 JSON 成员 / 不存在 tag 返回 None）、`provider_error_body_truncates_long_bodies`（短 body 原样、长 body 截断标注、空白 body 无样截断）。

验证：Rust **177 passed / 0 failed**（175→177）；本轮改动文件（storage/api/main/ai-provider）`cargo fmt --check` 干净（全库仅并行 `conversation.rs` 有 fmt 差异，未格式化）；`cargo build` 0 warning；clippy 无指向本轮新增行的告警；未触碰并行 hunk。

### 仍未处理的 P2（后续批次候选）

- **Rust storage/core**：剩余仅 P2-2/P2-3 事务竞态（主代理复核后再动）、P2-9 bridge string-contract（`api.rs` + `rust_bridge_repository.dart`，均并行活跃区）。
- **Rust AI**：CJK token 低估、unconditional eprintln 隐私日志、`compress_context` 截断、insight byte/char 单位、client 复用、worker 取消/限时——全在并行会话活跃的 `src/ai/*` + `api.rs` 部分区。
- **Dart**：P1/P2 列表见上（`message_area`/`wiki_page_detail_view` 等均为并行批次活跃文件，等并行落库后再动）。

## P2 批次三（2026-09-26，`src/ai/*` 组直修 2 项 / 阻塞 2 项）

- **P2 worker 限时（完成）**：`api.rs` `trigger_analysis` 每次 tick 前先 `recover_interrupted_analysis_jobs()`（启动处 api.rs:307 之外补上队列内 re-sweep，mid-loop 崩溃的 running job 不再孤悬到下次启动）；`process_analysis_queue` 加 `ANALYSIS_BATCH_MAX_SECS=120` 总墙钟上限（此前 50×60s 超时可占住 worker ~50 分钟）；`decision_support_context` 构建失败改为 `fail_analysis` 记 `last_error` 后 `continue`，不再 `?`-abort 导致已 claim 的 job 永久 running。
- **P2 client 复用（完成）**：`ai/provider.rs` 新增 `shared_blocking_client(timeout_secs)`（`OnceLock<Mutex<Vec<(u64, Client)>>>` 按超时档缓存全局实例）；OpenAI new（60s）、Ollama new（120s）改用缓存；`tool/mod.rs` `fetch_page_plain_text`（20s）改用 `super::provider::shared_blocking_client(20)`。api/insight/conversation 的调用点经 provider new 间接复用，无需逐点改。
- **P2 CJK token 低估（阻塞）**：`estimate_tokens` 的 `chars()/4` 低估 CJK（1 中文字 ≈ 1 token，4 字符估成 1）。但并行会话刚在 `memory.rs` 新增 `compress_context` 且其内部用 `dynamic_budget * 4` 字符截断（硬编码 1 token = 4 char 假设）、新测试断言 `sum <= budget` 且正文必须保留——给 CJK 加权会同时破坏截断逻辑与测试断言。**需与并行 `compress_context` 联动改动，等并行落库后再动。**
- **P2 eprintln 隐私日志（阻塞）**：conversation.rs 的 12 处 `eprintln!` 需套 `ELSEWHEN_DEBUG` guard（insight.rs:212 已有此模式）。但 `run_agent_loop` 正被并行新增的 failover 事件收集代码（`tool_created_event_id`）逐行改写，eprintln 落点与并行 hunk 紧邻。**等并行落库后再套 guard。**

验证：Rust **177 passed / 0 failed**（与批次二末一致，第 3 项 client 复用为纯性能重构、第 4 项由既有 process_analysis_queue 测试覆盖）；本轮文件 fmt 干净（全库剩余 fmt 差异均为并行会话代码或历史遗留：conversation.rs、memory.rs 并行区、storage.rs 3718/3728/5846/5880、api.rs 2569、tool/mod.rs 738 等）；clippy 无新增；未触碰并行 hunk。

### 仍未处理的 P2（批次三之后）

- **已消除**：worker 限时（api.rs 部分）、client 复用、provider 错误体泄漏、analyze-once 死命令、WAL checkpoint、全表扫描、排序 tiebreak、migration 双列守卫、目录导入有界读、capture 吞错、main.rs 字节切片、wiki 导出同名覆盖。
- **等并行落库**：CJK token 低估（需联动 compress_context）、eprintln 隐私日志（run_agent_loop 并行改写中）、`compress_context` 截断（并行会话正在做）、insight byte/char、retry backoff、direct_query 吞错。
- **Rust storage/core**：P2-2/P2-3 事务竞态（主代理复核后再动）、P2-9 bridge string-contract（并行活跃区）。
- **Dart**：P1/P2 列表见上（并行批次活跃文件）。