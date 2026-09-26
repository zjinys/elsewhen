# Agent Note: deep review 剩余待改清单（backlog）

Status: done

> **2026-09-26 收口**：C 组（Dart）经逐项核实，绝大多数已由并行会话在
> `0636dd1` 落库时修复，本轮补齐 C1 并修复 7 个失败测试。详见文末「收口记录」。
> 仅存的 C9（wiki_page_detail_view 3854 行拆分）与 storage.rs（6147 行）拆分
> 属结构性重构，转为独立排期项，不再是 bug-fix backlog。

来源：三路深度 review 原始报告（Rust storage/core、Rust AI、Dart），主代理复核分级
P0 8 / P1 20 / P2 26。本文件只记录**尚未收口**的项，作为后续批次的执行依据。
已完成项详见 `2026-09-25-deep-review-p1-fixes.md`（P1 + P0 + P2 批次一/二/三）。

> 重要约束：本仓库有并行会话在同一工作区继续开发（未提交）。**改动前必须对照
> `git diff` 的 hunk 分布**，避开并行正在改的区域。`src/ai/*` 与多数 Dart 文件均为
> 并行活跃区。任何改动留在工作区，不代用户提交。

---

## A. Rust AI 区（src/ai/*）— ✅ 已全部处理（2026-09-26）

> 并行会话已收工，本组六项已在此后逐个处理完成：
> - **A1 CJK token 低估**：`estimate_tokens` 改为 ASCII 4 字符/token + CJK 2 字符/token
>   （2× 加权，压缩预算下可行的折中），`compress_context` 三处 `*4` 截断换算同步改 `*2`
>   保持一致。并行新增的 compress_context 测试全部保持绿。新增回归
>   `estimate_tokens_weights_cjk_more_than_ascii`。
> - **A2 eprintln 隐私日志**：conversation.rs 顶部新增 `debug_eprintln!` 宏
>   （`ELSEWHEN_DEBUG` gate，与 insight.rs:212 模式一致），11 处 agent 日志全部套用。
> - **A3 compress_context 截断**：并行会话已在 memory.rs 重写为保留 system+最新 user、
>   旧轮次压摘要，双测试覆盖，无需再动。
> - **A4 insight byte/char**：`insight.rs` wiki 页内容截断由 `content_md.len()`（字节）
>   改为 `chars().take(3000).count()`，统一字符口径（中文页面不再截到约 3 倍长）。
> - **A5 retry backoff**：conversation.rs 加 `PROTOCOL_FALLBACK_SLEEP_MS=300` 协议回退
>   前小退避 + `jitter_backoff_ms`（400·n ms ±30% 抖动）provider failover 轮换退避。
> - **A6 direct_query 吞错**：`direct_query` 签名改 `Result<Option<_>>`，内部 `.ok()?`
>   改 `?` 上抛；调用处与三个测试同步适配。

### B. Rust storage/core 区 — ✅ 已全部处理（2026-09-26）

> - **B1（P2-2 / P2-3 事务竞态）**：复核后确认问题真实存在（主代理旧论证只覆盖
>   `set_active_ai_provider_config`，未覆盖 `save_ai_provider_config` 新建路径与三处
>   idempotency check-then-insert）。
>   - P2-2：`save_ai_provider_config` 新建路径的 count+insert 包进 IMMEDIATE 事务
>     （与既有 `set_active_ai_provider_config` 风格一致），并对部分唯一索引冲突给出
>     友好错误映射。
>   - P2-3：`create_input_record` 改为 `INSERT OR IGNORE` + 按幂等键回读（单语句原子，
>     无需事务）；`submit_input_as_event` / `submit_conversation_input` 保留快速预检，
>     事务内 input_records 插入对本轮添加的 catch-rollback-refetch 兜底。
>   - 实测发现 SQLite 对纯列部分唯一索引报「UNIQUE constraint failed:
>     **table.column**」（不是索引名），两个 canary 测试锁定了正确匹配串
>     （`input_records.idempotency_key` / `ai_provider_configs.is_active`）。
> - **B2（P2-9 bridge string-contract）**：桥函数类型化 + 重生成桥代码 + Dart 消费端同步。
>   - Rust：新增 `AnalysisTriggerResult` / `DailyReviewResult` 枚举；`init_bridge` 改
>     `Result<String>`（失败上抛取代 `"Error "` 前缀）；`trigger_analysis` 返回
>     `AnalysisTriggerResult`；`generate_daily_review` 返回 `DailyReviewResult`；
>     `process_analysis_queue` / `generate_daily_review_with_provider` 同步类型化。
>   - 已运行 `flutter_rust_bridge_codegen generate`（带 stdbool.h shim）重生成
>     `src/frb_generated.rs` + `ui/lib/bridge/generated.dart/*`，并 `cargo build --release`
>     对齐 content hash。
>   - Dart：`initialize()` 改 try/catch（替换 `startsWith('Error ')`）；worker 循环改
>     `is api.AnalysisTriggerResult_Processed` 判定；抽象接口/mock/两个测试同步更新。
>   - 验证：Rust 180 passed；桥接集成测试（真 FFI）+ mock 测试全绿；analyze 无新增告警。
>   - 注：`generate_daily_review` 的 Dart 消费端仍无屏幕接入（review 已注明契约未被
>     消费），现返回类型化为 `DailyReviewResult`，将来接入方不会再踩字符串坑。

## C. Dart 区（多数为并行活跃文件，等并行落库后逐项做）

### C1. P9 `message_area.dart:293` — retry 仅对最后一条可见
- `identical(message, messages.last)` 作重试 key，其他失败消息无重试入口
- 修复方向：改为按消息自身 state（含失败标记）判定，不依赖集合位置

### C2. P12 `markdown_view.dart:452-454 + 88-176` — recognizer 泄漏 + 每 build 全量重排
- TapGestureRecognizer 不 dispose；正文每次 build 全量重新解析
- 修复方向：dispose recognizer；解析结果缓存（按 source 记忆化）

### C3. P13 `message_area.dart:144-157` — 七个 broad provider watch 重建整个聊天
- `.value` 监听无关状态全量重绘
- 修复方向：拆 select（`select((s) => s.xxx)`）或下沉到最小 widget

### C4. P16 `wiki_page_detail_view.dart:1799` / `settings_screen.dart:739-749` — dialog-scope 控制器泄漏
- 对话框局部 `TextEditingController` 从未 dispose
- 修复方向：dialog 关闭 / state 卸载时 dispose

### C5. P17 `settings_provider.dart:84-122` — 三次 state 写入无 mounted 守卫
- `loadThemeFromBridge` 三个 `await` 后写 state，无 `ref.mounted` 检查
- 修复方向：每次写前 `if (ref.mounted)`

### C6. P18 `conversation_provider.dart:150` — FutureProvider 内做副作用写
- 修复方向：副作用移出 provider 主体（改为事件/命令）

### C7. P19 `message_area.dart:824-867` — `_resolveAmbiguity`：裸 jsonDecode(829) + context 跨 async gap(845/860) + 部分解析
- 修复方向：jsonDecode 包 try/catch；async gap 后用 `context.mounted`；残余格式容错

### C8. P10/P11 legacy screen — future-in-build + setState 无 mounted
- `conversation_timeline_screen.dart:71-145`（无限重建循环）、`conversation_detail_screen.dart:201-208`
- 这两个 legacy screen 与 C10 死代码联审：若删除则 P10/P11 一并消失

### C9. P20 `wiki_page_detail_view.dart`（3,807 行）— 单文件巨型化
- 一个文件四个 feature、~24 个 widget 类
- 修复方向：按 feature 拆文件（等并行落库后做）

### C10. P21 死代码 — legacy screen + mock repo + hotkey stub
- `lib/` 内已废弃代码删减（与 C8 联审）

### D. 已完成（核对用，勿重复做）
- P0 全部 8 项（4 项本轮修 / 3 项并行顺带解决 / 1 项货真价实）
- P2 批次一：P2-6 排序 tiebreak、P2-1 v29 迁移双列守卫、P2-7 wiki 导出文件名、
  P2-10 目录导入有界读、P2-11 capture 吞错、P2-12 字节切片、base_url 尾斜杠
- P2 批次二：P2-5 全表扫描改直查、P2-8 WAL checkpoint、provider 错误体隐私截断、
  analyze-once 死命令接通、`set_active_ai_provider_config` DEFERRED→IMMEDIATE
- P2 批次三：worker 限时（tick re-sweep + 120s 墙钟上限 + context 失败 fail_analysis）、
  client 复用（`shared_blocking_client` 按 timeout 档缓存）

## 执行策略
1. 每次开工先 `git diff <file> | grep '^@@'` 对照并行 hunk，避开并行活跃区。
2. 阻塞项（A1/A2）等并行提交后再动；A3 并行在做，先观察。
3. Dart 区（C 组）多数文件为并行活跃，逐项确认 hunk 后再做。
4. 验证基线：`cargo test --lib`（当前 177 passed / 0 failed）；`flutter test` + `flutter analyze`。
5. 改动留工作区，不代用户提交。

---

## 收口记录（2026-09-26）

并行会话已收工、工作区合并提交（`0636dd1`）后，主代理逐项核实 C 组并补齐测试：

### C 组最终状态
| 项 | 状态 | 说明 |
|---|---|---|
| C1 retry 逻辑 | ✅ 本轮修复 | 「重新生成」按钮条件 `id == latestUserMessageId` 在 user 消息已被 AI 回复后仍恒真，导致回复成功后按钮不消失。新增 `latestUserHasReply` 判断（`message_area.dart`），修复后 message_retry/enter 测试转绿。 |
| C2 recognizer dispose | ✅ 已修 | `markdown_view.dart` 识别器登记到列表、dispose 统一释放。 |
| C3 broad provider watch | ✅ 已修 | 头部统计下沉到 `_NowStatus` 自己订阅，不再重建整份消息列表。 |
| C4 dialog controller | ✅ 已修 | `wiki_page_detail_view.dart` dispose 统一释放。 |
| C5 mounted 守卫 | ✅ 已修 | `settings_provider.dart` 写 state 前查 `ref.mounted`。 |
| C6 provider 副作用 | ✅ 已修 | `mainConversationProvider` 改纯读，写操作只在 appInit。 |
| C7 jsonDecode/context | ✅ 已修 | `_resolveAmbiguity` jsonDecode 包 try/catch，失败渲染占位。 |
| C8/C10 legacy screen | ✅ 已删 | `conversation_timeline_screen.dart` / `conversation_detail_screen.dart` 已删除。 |
| C9 wiki_page_detail_view 拆分 | ⏸ 转排期 | 3854 行巨型文件，属结构性重构。 |

### 测试修复（7 个失败 → 170/170 全绿）
- **fake repo 未覆写 getMessages**（message_enter/message_retry/pending_knowledge_draft）：
  基类 `getMessages` 现做「主对话流合并」查询真实 bridge，单测中 frb 未初始化抛
  `StateError`。各 fake repo 补 `getMessages` 覆写。
- **文案漂移**（pending_knowledge_draft）：状态条入口文案 `N 份待入库` → `草稿N份`，断言同步。
- **wiki_ui_test fake-async/frb 死锁**：real-bridge 集成测试，主对话缺失使
  `mainConversationProvider` 落到 error 分支；且真实桥接的异步 provider 在
  fake-async 普通 pump 下不 resolve。修复：先建「主对话流」会话，再用
  `pump → runAsync(真实延时) → pump` 循环推进 provider 解析（与
  wiki_relations_ui_test 同模式）。

### 其他本轮处理
- **P0-1 worker CLI 命令**：查证为 commit `4d4f856` 有意移除（`ai::run_worker` 已删），
  后台持续分析由 Flutter bridge 内置周期 worker 承担。删除 `print_usage` 与 README
  中的 worker 残留引用，指向 `analyze-once`。
- **P0-3 c.sh**：内容为 `codex resume <id>` 个人命令，无 API key，已被 git 追踪，不处理。

### 最终验证
- Flutter 测试 **170/170 全部通过**（此前 163/170）。
- Rust `cargo build` 通过。
- 提交：`0636dd1`（基线）→ `fdb260a`（6 测试+C1）→ `7f5cede`（P0-1）→ `c29e26d`（wiki_ui_test）。