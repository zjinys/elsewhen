# Agent Note: deep review 剩余待改清单（backlog）

Status: done

> **2026-09-26 收口**：C 组（Dart）经逐项核实，绝大多数已由并行会话在
> `0636dd1` 落库时修复，本轮补齐 C1 并修复 7 个失败测试。详见文末「收口记录」。
> 仅存的 C9（wiki_page_detail_view 3854 行拆分）与 storage.rs（6147 行）拆分
> 属结构性重构，转为独立排期项，不再是 bug-fix backlog。
>
> **2026-09-27 收口**：C10 三项死代码全部处理（legacy screen 之外的 mock repo 迁出
> `lib/`、hotkey stub 删除），C 组仅剩已转排期的 C9。期间修复了一个**阻塞全仓库构建**
> 的桥接损坏（并行会话拆 `src/api/` 后未重跑 codegen，叠加 frb 2.14.0-beta.2 的
> barrel 缺陷），详见文末「收口记录（2026-09-27）」。

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

### C9. P20 `wiki_page_detail_view.dart`（3,807 行）— 单文件巨型化 — ⏸ 转排期
- 一个文件四个 feature、~24 个 widget 类
- **转排期理由**：该文件是并行会话的活跃区且体量已涨到 3.8k 行，拆分属于纯结构性
  重构（移动 widget、调整共享的 `_editTags` 等私有方法），不修任何缺陷、却极易与
  后续功能改动冲突。收益（可读性）不足以承担这个冲突风险。
- 该文件现存 6 条 analyzer warning（3 × unused_element + 3 × unused_local_variable），
  属既有状态，未处理。

### C10. P21 死代码 — ✅ 已处理（2026-09-27）
- **legacy screen**：`main_timeline_example.dart` / `conversation_timeline_screen.dart` /
  `conversation_detail_screen.dart` 共 813 行，与 C8 一并删除（见上）。
- **mock repo**：`lib/data/mock_storage_repository.dart`（76 行）迁至
  `test/support/mock_storage_repository.dart`。全项目只有 `test/mock_storage_test.dart`
  用它，留在 `lib/` 等于让发布产物带一份假实现、也让死代码扫描多一个目录要过滤。
  改用 `package:elsewhen_ui/...` 绝对导入（与既有的 `test/support/isolated_bridge.dart`
  同一约定），`mock_storage_test.dart` 改为相对导入 `support/...`。
- **hotkey stub**：`lib/utils/hotkey_service.dart` 删除，接线一并拆除。它是永久 stub
  （`isSupported => false`、`registerCaptureHotkey()` 恒返回 false），而全项目真正用到的
  只有 `initialize()` / `sessionType` / `getSetupInstructions()` 三个成员，且**只在
  `app_provider.dart` 的启动流程里**——每次启动 `debugPrint` 一整段教程，打包后用户
  根本看不到。`registerCaptureHotkey` 与 `getHotkeyDescription` 从未被调用。
  - 文案不能直接丢：`--mode=capture` 确实存在（`app_config.dart:14` +
    `capture_screen.dart`），`elsewhen-capture.sh` 也在，那段说明是准确的。已迁到
    `ui/README.md`「Capture 模式」小节（系统快捷键绑定 / 常驻脚本两种做法），
    待办清单里的 `hotkey_manager` 条目改为指向该小节。
  - 顺带修掉 `ui/README.md` 一条**错命令**：`--dart-define=mode=capture` 不生效
    （`AppConfig.fromArgs` 只看 `args` 里有没有 `--mode=capture`），且与紧邻的下一行
    自相矛盾，已删除该行。

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

---

## 收口记录（2026-09-27，C10 + 桥接修复）

本轮 C 组只剩 C10 三项死代码。期间撞上一个**阻塞全仓库构建**的桥接损坏，先修它。

### 桥接损坏：frb codegen 的 CPATH 陷阱 + beta 版 barrel 缺陷

并行会话把 `src/api.rs` 拆成 `src/api/` 多模块目录（`58d9a43`…`8ce167d`），但**没有重跑
codegen**，两侧同时失配：

| 侧 | 症状 |
| --- | --- |
| Rust | `src/frb_generated.rs` 仍按旧字段名解构 `KnowledgeDigestTickResult::Processed { created, updated, protected }`，而枚举已改成 `created_slugs / updated_slugs / protected_slugs`（改名是为避开 C 保留字），`cargo check` 10 个 `E0559/E0026/E0027`。 |
| Dart | `generated.dart/api.dart` 被删掉 1154 行、只新增一行 `import 'api/todos.dart';`，其余 13 个模块既没被 barrel 引用，函数与 DTO 也没落到任何地方 → `flutter analyze` 156 个 `undefined_function/undefined_class`。 |

重跑 codegen 时踩到两个坑，都写进 `flutter_rust_bridge.yaml` 注释了：

1. **CPATH 不能只给隔离的 `stdbool.h`**。之前记的「隔离目录」做法会让 clang 找不到自己的
   `stddef.h`，ffigen 报 `SEVERE` 后**半途产出残缺 Dart**——但 Rust 侧照样编译通过，
   极易误判成成功。必须给整个 `/usr/lib/clang/22/include`（仓库配置里原本就写着这条）。
2. **frb 2.14.0-beta.2 的 barrel 缺陷**：`crate::api` 下有多个子模块时，codegen 把 API 拆到
   `api/<module>.dart`（14 个文件，19 个 DTO 类型 + 72 个函数齐全），但生成的 barrel
   `api.dart` 只 import 其中一个模块。不加干预，analyzer 仍是 81 个 error。

   根因是 beta 的 barrel 生成不完整，**没有官方开关可关**（`generate --help` 无相关选项）。
   临时解法：在生成的 `api.dart` 顶部手工补 13 行 `export`（已在文件内注释标明）。
   **注意与并行会话的方案重叠**：它已新建手写 barrel `ui/lib/bridge/api.dart`
   （同样 export 这 14 个模块，但**不在生成目录里，能扛住 codegen 覆盖**，是更正的
   做法），最新提交 `f5bf272` 的标题就是「api.rs 拆分后的 Dart 生成物迁移方案」。
   本轮**没有**去改那 15 个文件的 import（避免与并行会话抢同一批文件）；等 import 迁到
   `bridge/api.dart` 后，生成文件里那段 export 就成了冗余，可直接删掉。

### 测试修复（1 个真断言 bug）

- **`settings_screen_test.dart:99` 断言过窄**：「数据」tab 现在有两个队列区块
  （`事件分析队列` / `知识消化`，后者是知识消化功能新增的），两者都有「待处理」
  「已完成」「失败」三枚**同名**指标，而测试用 `findsOneWidget` 假设只有一个队列。
  两个区块各有独立 section 标题，UI 本身不歧义，所以改的是测试而不是文案：
  三个撞名指标按 `findsNWidgets(2)` 断言（顺带把知识消化区块的存在与状态读取纳入覆盖），
  事件分析队列独有的「处理中」「等待重试」仍要求恰好一枚。

### 新发现：真 FFI 测试随机 BUSY —— 根因已定位（`init_bridge` 用进程级 env 当隔离边界）

**症状**：`flutter test` 下真 FFI 测试（`test/support/isolated_bridge.dart` 建真库）随机
失败，每次失败用例与调用点都不同。实测异常：

```
AnyhowException(database is locked
  Caused by: Error code 5: The database file is locked)
  bridge_integration_test.dart:38 → repo.listEvents()
```

> ⚠️ **本节根因在 2026-09-27 被推翻并改写。** 下面是最初的推断，仅保留作为「为什么
> 会猜错」的记录。**不要**再拿「`flutter test` 单进程 / env var 串库」当结论。

**曾经的错误根因**（三步验证，实为错误）：

1. `flutter test` 把所有测试文件作为 isolate 跑在**同一个 `flutter_tester` 进程**里。
   实测：并发跑 4 个测试文件期间 `ps` 只出现 **1 个** `flutter_tester` 进程。
2. `init_bridge` 把库路径写进**进程级环境变量** `ELSEWHEN_DATA_DIR`。
3. **每个** API 函数都重新 `AppConfig::load()` → `Store::open()`，即每次调用都重读全局 env。

于是「并发 suite 互相覆盖 `ELSEWHEN_DATA_DIR`，打开同一个库」→ `SQLITE_BUSY`。

**错在哪**：第 1 步的 `ps` 采样是**抽样误判**（flutter_tester 会被复用 / 采样时刻只启动了
一个），并没有真的证明单进程。决定性反证是：`AppConfig::pin` 的**冲突分支从未触发过**
（多轮全量跑下来「显式冲突 = 0」）——如果 suite 真的共享进程、且各自 `init_bridge` 不同
目录，第二个 suite 必定撞上 `pin` 的 `bail!` 而让该文件的 `setUpAll` 失败。实际没有任何
文件这样失败，说明各 FFI suite 并不共享进程，也就谈不上跨 suite 串库。**顺序反了：不是
跨 suite 冲突，而是单 suite 内部竞态。**

**真正的根因：Dart 侧周期性后台 worker 与测试自己的 FFI 调用抢锁。**

`RustBridgeRepository.initialize()`（`ui/lib/bridge/rust_bridge_repository.dart`）在
init 成功后起一个 `Timer.periodic(5s)`，调 `_wakeAnalysisWorker()`；而
`recordEvent()` / `submitInput()` 也会**立刻**唤醒它。`_wakeAnalysisWorker` 里的排空循环
做的是**写**操作：

```dart
while (!_disposed) {
  final result = await triggerAnalysis();      // 写：标记 running/succeeded
  final stats = await getAnalysisJobStats();    // 读
  if (stats.pending == 0) break;
}
await _drainKnowledgeDigest();                  // 写
```

Dart 是单 isolate 单线程，但这些都是异步链：worker 的 `await` 挂起时，事件循环会去跑测试
自己发起的 `await repo.listEvents()`。**每个 API 调用都各自 `Store::open` 一个新连接**，
于是同一 SQLite 文件上出现「一个连接在写、另一个在读」，撞 `SQLITE_BUSY`。交错顺序取决于
事件循环时序 → 每次失败的用例和调用点都不同，表现为「随机」。

三处放大它的因素：

- `_wakeAnalysisWorker` 的 `catch (_) {}` 把 worker 内的异常**全静默**掉了。
- `settings_screen.dart` 的 `initState` postFrame 里 6 个加载并发发出，再叠加那个 5s
  timer，写入窗口被显著拉宽。
- `dispose()` 只能阻止**后续**循环，**在途**的 Rust 调用无法取消——teardown 期间仍可能写。

**已排除的其他猜测**（这些结论仍然有效）：
- *busy_timeout 太短*——`Store::open` 已设 `busy_timeout(5s)`（`src/storage/mod.rs:485`）
  且开了 WAL（`migrations.rs:19`），仍 BUSY，且日志时间戳显示是**快速失败**而非等满 5s。
- *Rust 侧有常驻线程抢锁*——`grep 'thread::spawn'` 在整个 `src/` **零命中**；但这不代表
  「没有后台写者」——写者在 **Dart 侧**（上面的 worker），这也正是上一条误判翻车的地方。
- *CPU 抢占 / 固定延时不够*——`--concurrency=1`（无并发）同样失败，因为竞态发生在
  **单个 suite 内部**。
- *跨 suite 共享数据目录*——`createIsolatedBridge()` 每 suite 建独立临时目录，且实测
  `pin` 冲突从未触发，各 suite 的库确实是分开的。

**为什么一直没被发现**：CI（`.github/workflows/ci.yml:20`）只跑 `cargo test --all-targets`，
**不跑 `flutter test`**；而 `test_bridge.sh` 是手工环境检查脚本（查 `.so`/表/进程），
不是按文件隔离的 runner。

### 产品侧已修：库路径在进程内钉死（`config::AppConfig::pin`）

`src/config.rs` 新增进程级钉死值 `PINNED: RwLock<Option<AppConfig>>`，`init_bridge`
改调 `AppConfig::pin(database_path.as_deref())`，**去掉了 `std::env::set_var`**：

- 语义是**先到先得 + 冲突即报错**。已钉死且显式路径不同 → `bail!`，报错文案点名两个目录。
  不静默共库，也不随机 BUSY。
- 未钉死时 `load()` 仍按环境变量重新解析且**不**写全局——保持旧的探测性行为，避免
  init 之前的一次探索性调用把后续 `init_bridge` 的显式路径顶掉。
- 顺带消掉每调用一次的 `create_dir_all` + `chmod 0o700` + 旧库迁移探测（现在只跑一次）。

测试：`config::tests` 5 个用例覆盖「显式路径优先于 env」「钉死后改 env 无效」
「同路径幂等」「异路径报错且不改动已钉值」「未钉死时跟随 env」。因为钉死值与环境变量
都是进程级状态而 Rust 测试并行跑在同一进程，新增 `config::test_env_guard()` 串行化
所有碰这两者的用例，`DataDirGuard`（`api/mod.rs`）也一并持这把锁——否则先跑的用例
会把目录钉死，后跑的静默读到它的库。

**但这没有解决测试 flake**（当时我对原因的判断也是错的，见下）。它消除的是**每调用一次
重新解析路径**这件事：init 之后 `AppConfig::load()` 直接返回钉死值，不再 `create_dir_all`
+ `chmod 0o700` + 旧库迁移探测，也不再有「两个 suite 可能指向同一文件」的可能。BUSY 的
真正来源是 Dart 侧后台 worker，与路径解析无关。

### 已修 2：设置页 6 处吞异常 —— 真问题是「失败被伪装成空状态」

`settings_screen.dart` 的 `initState` postFrame 里并发发 6 个加载，每个
`catch (e) { setState(loading = false); debugPrint('xxx failed: $e'); }`。实测日志里
`loadTokenUsage failed` / `loadRules failed` 就是这么消失的，而 `flutter test` 仍报
`All tests passed`——**失败显不暴露，取决于「被吞掉的那个调用恰好是不是该测试断言的
那个」**，这是随机性的第二个来源。

查渲染侧时发现比「吞异常」更严重：**失败被渲染成了正常的空状态**，用户完全看不出出了
故障：

| 区块 | 改前读取失败时用户看到 |
|---|---|
| 事件分析队列 / 知识消化 | 「…状态读取失败」（有兜底、无原因）|
| 规则库 | 「**还没有规则。**在对话里分享踩坑或心得时…」← 把故障说成没沉淀过 |
| Provider 列表 | 「**未配置**」← 同样伪装成没配 |
| 每日 Token 用量 | 「**暂无 AI 调用记录**」← 伪装成从没调用过 |
| 推文抓取服务 | **静默显示默认值 `fxtwitter`**（无 loading 态，失败 100% 不可见）|

Provider 那条尤其糟：项目铁律里「没配 provider」本身有首次运行引导横幅
（`ai_provider_setup_hint.dart`），把「读不出来」显示成「未配置」会与之矛盾。

改法（加法式，不动写路径）：

- 新增 `Map<String, String> _loadErrors`（区块名 → 异常文本）+ 6 个区块名常量。
- 6 个 catch 各自记录原因。`_loadTweetService` 的 catch 顺带补上 `mounted` 守卫——
  它是 6 处里唯一漏掉的，是个真实的潜在 setState-after-dispose。
- 新增 `_loadErrorOf()`（只取异常**首行**：`anyhow` 的 `toString()` 带多行
  `Caused by:`，整段塞单行 Text 没法读）与 `_buildLoadErrorHint()`。
- 6 个区块的「空状态」分支**前面**插一层错误判断，顺序是 loading → 错误 → 内容
  （加载中时旧错误已过期，所以错误检查必须在 loading 之后）。

`flutter test test/settings_screen_test.dart` 新增用例
「读取失败时展示失败原因，而不是伪装成空状态」：用一个 `noSuchMethod` 全抛的假仓库，
断言 4 条——两个 `find.textContaining('…读取失败：')` 命中，且
「暂无 AI 调用记录」/「还没有规则」**必须** `findsNothing`。用例注释里写明了该假实现会
在 `as RustBridgeRepository` 硬转处抛 `TypeError`（这正是要验的那条 catch 路径）。
3/3 通过。

`settings_provider.dart:125 loadThemeFromBridge` 那处**未动**：它是主题加载，失败按设计
静默回退到默认主题，UI 上无对应区块可挂提示，且不在本轮范围。

### 未修（已定位、方案被否决）：`ensure_schema` 每次 open 都抢写锁

**这是残留 BUSY 的最可能来源，但本轮的修法不成立，已撤回。**

`Store::open` 是 spawn-per-call（每次 API 调用都新建连接），每次都调
`migrations::ensure_schema`，而它含 **30 余条 `INSERT OR IGNORE INTO
schema_migrations`** 和若干 `CREATE INDEX IF NOT EXISTS`。关键在于这些语句
**即使什么都不会改变也照样抢 SQLite 写锁**——这一点不是推测，是用独立 rusqlite
探针逐条实测的：

| 语句 | 抢写锁 |
|---|---|
| `SELECT` / `PRAGMA table_info` | 否 |
| `PRAGMA journal_mode` / `PRAGMA synchronous` / `wal_checkpoint(PASSIVE)` | 否 |
| `CREATE TABLE IF NOT EXISTS`（表已存在） | 否 |
| **`INSERT OR IGNORE`（命中已有行）** | **是** |
| **`CREATE INDEX IF NOT EXISTS`（索引已存在）** | **是** |
| **无匹配行的 `UPDATE`** | **是** |

所以**纯读接口也在参与写锁争抢**——这解释了实测中 BUSY 为什么全部落在
`listEvents` / `getDailyTokenUsage` / `loadTokenUsage` 这些纯读调用上。也解释了为什么
`busy_timeout(5s)` 救不了：SQLite 在「deferred 事务读着 → 要升级成写」的冲突上
**不调用 busy handler**，直接返回 `SQLITE_BUSY`。`settings_screen` 一次 postFrame
并发 6 个加载 = 6×30 次争抢。

**试过并否决的方案：给 `ensure_schema` 加「版本门」（`MAX(version) == 31` 就整段早退）。**

它 broke 了两个既有迁移测试，而且是**方案本身不成立**，不是测试写法问题：

- `migration_v29_recovers_when_only_second_column_is_missing` 模拟**迁移中断**——
  `opinion` 列被丢掉、v29 记录被删，但 `MAX(version)` 仍是 31。版本门看到「已最新」就
  整段跳过，于是**那列永远不会被补上**，而所有 wiki SELECT 会报 `no such column`。
- `migration_v29_splits_note_kind_and_adds_columns` 依赖 `ensure_schema` 尾部一段
  **故意每次 open 都跑**的数据 backfill（注释写明「恒执行」，抓「升级中途落库」的
  `note-` 前缀脏行）。

**教训**：`MAX(version)` 不是「schema 完好」的可靠判据。迁移系统的存在意义就是能从
**任意中断状态**恢复，而版本门把这个不变量换掉了。**任何以版本号推断 schema 状态的
优化都要先问：中断恢复怎么办。** 真要修，正确方向是让**版本记录本身**按需写
（`INSERT OR IGNORE` 前先 `SELECT 1 FROM schema_migrations WHERE version = ?` 探测），
而不是给整段加门——那样每个迁移的独立守卫和中断恢复能力都原样保留。

`migrations.rs` 末尾留了一条 `#[ignore]` 测试
`ensure_schema_takes_no_write_lock_once_current` 作为该缺陷的复现与验收标准：它在
另一个连接持写锁时调 `ensure_schema`，目前会失败，修好后去掉 ignore 即可。同时也钉住了
上面那张锁行为表。**本轮对 `migrations.rs` 是纯追加（`git diff` 零删除行），没有改动
并行会话的任何代码。**

#### 「按需写」方案的具体设计（已设计，**结论：不该做——层级搞错了**）

> **2026-09-27 复核更正**：下面这套设计是在**错误的层级**上打补丁。`ensure_schema` 是数据库
> 初始化（契约原文「幂等，可重复调用」），问题不在它内部 37 条语句各自低效，而在于
> **它被 per-call 调用**：API 层有 **88 个 `Store::open` 调用点且零缓存**（全仓无
> `OnceCell`/`OnceLock`/`static`），即每个接口都完整重跑一遍「开连接 → ensure_schema →
> chmod → backfill → wal_checkpoint」。所以「每次的写锁从 37 降到 2」是治症状，「初始化
> 只跑一次」才是治因。下面的设计保留作为**反面记录**：它能达成，但达成的不是该达成的目标。
>
> 治因的两条路：①88 个调用点改用共享 `Store`（大，且散在并行会话正在改的 `api/*.rs`）；
> ②`Store::open` 内加进程级「本路径已 ensure 过」跳过（~15 行，只动 `storage/mod.rs`，
> 但隐患真实——测试会在同进程内删库重建并对同一 path 再开 `Store`，按 path 缓存会导致
> 跳过初始化、拿到空 schema 库；要安全得把 key 换成 `(path, inode)` 或 `(path, size+mtime)`）。
>
> 另注：`Clone for Store` 实现是 `Self::open(&self.path)`，即克隆一次重跑全额初始化，
> 看着是放大器，但**产品代码 0 命中**（只有测试用），不构成生产问题。
>
> **共同点：两条都没证明能修掉任何用户可见症状。** 无争用时这 37 条都是微秒级 no-op，
> 性能大概率不痛；痛的是争用，而争用已由「关后台 worker」压下去。故**不做**。

以下为原始设计记录（层级已错，留作对照）：

范围实测清楚了：全文件 37 处锁敏感语句，分布在 22 个 `execute_batch` 块（19 个含记版本
语句）和 15 个单条 `execute`（6 个纯记版本）。把稳态必跑的首个 batch 拆成单条逐个喂给
持锁连接，得到稳态真值：**9 条抢锁 / 21 条不抢**（7 条记版本 + `CREATE TRIGGER` + 1 条
误切的 trigger 体 `END`）。`total_changes() == 0` **测不出**这个问题，已验证。

改动分三类：A 类纯记版本的单条 execute（6 处）直接换 helper；B 类 batch 里的记版本语句
（19 个块）需从 batch 里拆出来——**语义会从「ALTER 与记版本同事务」变成「ALTER 先提交、
记版本后提交」**，中间崩溃留下「列已加、版本未记」，而这正是 `migration_v29_recovers_*`
要恢复的场景、各迁移的 `if !has_column` 守卫也正是为它写的，故方向安全，但需跑那两个
测试验证；C 类 batch1 里的 `CREATE TRIGGER/INDEX IF NOT EXISTS`（即使已存在也抢锁）暂不
做——只做 A+B 预期已能降约 95%。

**为什么暂不落地**：我证明的是「这些语句确实抢写锁」，**没证明「剩余 BUSY 是它们造成的」**
——把前者当前者是跳跃。对唯一已知症状（测试 flake）的预期收益接近零：关 worker 后 28 轮里
测试从未红过，且 0.13/轮 vs 0.08/轮两档样本量统计上区分不开。反对现在做的还有两条：动的
是并行会话正在改的文件（33 处）；迁移代码是仓库里后果最重的代码（改错=数据损坏）。

#### ⚠️ 2026-09-28 产品侧复现：上面「没证明」的部分现在有证据了，且不是日志噪声

修 Linux 空窗口时顺带撞见。连续 3 轮真实启动 `elsewhen_ui`（release/debug 皆然），
**第 3 轮命中**：

```
loadThemeFromBridge skipped: AnyhowException(database is locked
Caused by: Error code 5: The database file is locked)
```

复现率约 **1/3 次启动**，且**当时无任何其他 elsewhen_ui 进程**（已 `ps` 确认），所以不是
残留进程占锁。

**这修正了本节此前的结论**：「剩余 BUSY 只是日志噪声、对用户不可见」**不成立**。它的用户
可见后果是**启动时静默回退到默认主题**——用户看到的是「app 有时自己换了主题」，而日志里
只有一行被吞掉的异常（`settings_provider.dart:125 loadThemeFromBridge` 是当初六处吞异常里
**唯一故意没改**的那处，因为「失败按设计静默回退默认主题」）。

连带影响：测试侧的 0.13/轮 vs 0.08/轮「统计上区分不开」这个论证**也随之失效**——产品侧
1/3 的复现率比测试侧 1/8 高一个量级，样本量小是因为产品侧本来就不该有这么多冲突。
**该修的判断需要重开**，不再是「不做」。

仍未证明的一环：锁的来源**是不是** `ensure_schema`。目前只有「启动期并发 `Store::open`」+
「探针已证实 `ensure_schema` 稳态每次 open 抢 9 次写锁」两条旁证，属推断。

**现在有了低成本、高灵敏度的判定实验**（比原先计划的 A/B 灵敏得多）——临时让
`ensure_schema` 稳态早退，跑 10 轮真实启动，数 `loadThemeFromBridge skipped` 次数：
0 → 确证是该修的；仍 >0 → 另有来源。约 10 分钟，可立刻做。

（2026-09-27 曾注入过该实验 hack 并被服务器重启打断，hack 已逐字移除、`.so` 已按无 hack
版本重建，`migrations.rs` 仍为纯追加 131 增 / 0 删。）

**判定该做不做的实验（10 分钟，被服务器重启打断未完成）**：临时让 `ensure_schema` 在
稳态早退（模拟 A+B+C 的终态），对同一批测试文件跑 A/B 数 `database is locked` 次数。
BUSY 归零 → 值得做；不归零 → 省下 33 处高风险改造。已注入的实验 hack 在重启后已逐字
移除并重建 `.so`，`migrations.rs` 仍为纯追加。

### 已修 3：测试侧关掉后台 worker

- `RustBridgeRepository` 新增 `runBackgroundWorker`（默认 `true`，生产行为不变）。
  `initialize()` 在它为 false 时不建 `Timer.periodic`、不初始唤醒；`_wakeAnalysisWorker()`
  开头加一条 `|| !runBackgroundWorker` 早退（这样 `recordEvent` / `submitInput` 里的
  唤醒调用也不会把它拉起来）。
- `createIsolatedBridge()` 传 `runBackgroundWorker: false`，并在文档注释里写明理由。

安全性已核对：**没有一个测试依赖后台 worker 推进队列**。测试都是显式调
`triggerAnalysis()` / `getAnalysisJobStats()`；`settings_bridge_test.dart:149` 甚至断言
`after.succeeded == 0`（明确要求队列**没**被处理），关掉 worker 反而让这条断言更稳。

顺带把 worker 的 `catch (_) {}` 改成 `catch (e)` + `debugPrint`。不上抛是有意的（队列状态
持久，下次唤醒会重试，不该让后台失败带崩保存路径），但**完全静默**不好：若每 tick 都因同一
原因失败（比如抢锁 BUSY），就会永远查不出来。只记不抛。

局限：`dispose()` 只能阻止**后续**循环，**在途**的 Rust 调用取消不了。测试里 worker 不启动，
这条路径基本消失，但生产环境依然存在。

### BUSY 频率实测（每轮 = 一次全量 `flutter test`）

| 阶段 | 轮数 | 总 BUSY 次数 | 每次全量 |
|---|---|---|---|
| 修前（基线） | 4 | 2 | 0.50 |
| 关 worker 后 | 16 | 2 | 0.13 |
| 关 worker + 版本门（**已撤回**） | 12 | 1 | 0.08 |

关掉后台 worker 把频率降了约 **4 倍**且 28 轮里再没出现「测试红」——**从用户可见的
故障变成纯日志噪声**，这是本轮的实质改善。但**没有归零**：版本门那 12 轮仍有 1 次
`loadTokenUsage` BUSY，也就是上面 `ensure_schema` 那条已知缺陷。

诚实说明：这三档样本量都偏小（4/16/12 轮），0.13 → 0.08 这一档**在统计上区分不开**。
能确定的只有两点：①关 worker 有效；②`ensure_schema` 的写锁争抢是独立于 worker 的
残留来源（已被探针实测确认，不是推测）。

### 本轮验证

- `cargo test --lib`：**198 passed / 0 failed / 1 ignored**（基线 180，+13 并行的知识
  消化测试，+5 本轮 `config::tests`；ignored 是上面那条 `ensure_schema` 写锁复现）。
- `cargo build --release` 重新编译 `libelsewhen.so`，对齐重新生成后的桥接（否则真 FFI
  测试会因 content hash 失配而失败）。
- `flutter analyze --no-pub lib test`：**0 error**，37 issues 与基线一致
  （`wiki_page_detail_view.dart` 6 条既有 + `test/settings_reading_layer_test.dart`
  1 条 unused_import，其余为 test 里的 `avoid_print` 等 info）。
- `flutter test`：全量多轮，**测试用例本身全绿**；日志里偶发 `database is locked`，
  现已由设置页 UI 显式展示（不再是静默）。频率见上表。

### 提交

本轮改动**全部留在工作区未提交**（含 C10 三项删除、`api.dart` export 补丁、
`settings_screen_test.dart` 断言、两处文档）。`HEAD` = `f5bf272`。

## 新开：外网搜索抽象层（不在本轮 deep review 范围）

需求与设计已落文档：**`docs/notes/proposed/2026-09-27-web-search-tool.md`**。

要点（细节看那份文档）：

- **只给 elsewhen 自用，不做 MCP server**；本轮**只做抽象层**，具体引擎后补。
- **做成 Tool 不做自动检索**——触发条件是「模型自认知识不足」，**只有模型自己知道它缺
  什么**，自动检索是在它察觉之前抢跑，正好把最需要判断的那步拿掉；且工具式能复用现成的
  `provider_specs_for` 门控。
- **「补充知识」的定位（初稿定位错，已更正）**：触发不是「问题属于外部事实/时效信息」
  那套题材分类，而是**自认知识不足**。于是问题变成「不足时去哪找」，去处有三是固定
  顺序：参数里的知识 → 本地真源（`search_knowledge_base`/`get_wiki_page`）→ **外网
  （最后手段）**。反例：「谁写的哈姆雷特」是外部事实题但模型记得，不该搜；「我上周答应过
  什么」不涉外部事实但模型不知道，且这个缺口**绝不该去 internet 找**。
  `web_search` 因此不是本地检索的平级替代，而是它的**兜底**。
- **闭环已存在，只缺第三条腿**：`ImportUrlToWikiTool`（`tool/mod.rs:1270`，
  `WriteConfirm`「导入一个网址的内容到知识库…调用后进入待确认状态」）就是为这个位置
  准备的。链路 = 发现缺口 → 搜到 URL → `import_url_to_wiki` 抓全文 → 用户确认 → 入库
  → 下次本地能答。①②（本地检索）与落库都已具备。
- **本设计最大的软肋（如实记）**：模型对自身知识缺口**没有校准**——察觉不到就自信地编
  （更危险），过度察觉就逢问必搜（更浪费）。「要求先说清缺什么」等只是缓解，不是根治。
- 硬约束：不新增依赖（复用 `shared_blocking_client(5)`，`Cargo.toml` 无哈希库故缓存
  键用 query 原文）；配置**必须存表**不能存文件，因为 `ToolContext`（`tool/mod.rs:96`）
  没有 config 访问途径；迁移版本 **32**（当前 31）。
- 已知残余风险：**提示注入挡不干净**。截断 + 显式标记 + 强制 `fetch_page` 核实能大幅
  降低，但真正的兜底要靠 provider 侧消息隔离，超出本设计范围——不假装解决。
- 落地前须确认 `src/storage/migrations.rs` 的并行会话已收工（该文件 30 处 fmt diff）。

## Linux 桌面端空窗口（已修复；根因经二次更正）

**症状**：`fvm flutter run -d linux` 窗口只有边框无内容，日志报
`Could not determine GL version`（`impeller/renderer/backend/gles/description_gles.cc:92`）
+ `Failed to create platform view rendering surface` +
`FlutterEngineRunTask returned kInvalidArguments`。

### ⚠️ 根因曾被错误归因两次，第二次是真因

**第一版归因（错）**：`window_service_desktop.dart:116/139` 的
`w.backgroundColor = _transparent`（`a: 0`）。据以改了 `_applyWindowBackground`
（Linux 跳过透明）并声称 3/3 验证通过。

**第二版归因（对）**：`w.titleBarStyle = na.TitleBarStyle.hidden;`。
在正确入口下逐项二分，只有跳过它能归零：

| 跳过的 chrome 项 | GL 错误 |
|---|---|
| **titleBarStyle** | **0** |
| backgroundColor / minimumSize / isVisibleInTaskbar / isClosable / title / 尺寸落位 | 1 |

机制：nativeapi 在 Flutter **建好 GL 上下文之后**才改窗口属性，GTK 为此重建窗口的
GdkVisual，与已建好的上下文不匹配，首帧即查不到 GL 版本。与透明是同一类问题
（事后改 visual），但触发项不同。`w.backgroundColor` 被证无罪——Linux 上设透明背景
实测 GL 错误 0，故第一版那个 Linux 跳过已撤销，圆角效果保留。

**第一版为什么会错 —— 方法论教训（比修复本身更重要）**：
二分用的构建命令是 `fvm flutter build linux --debug`，**没带 `--target`**，
而 Flutter 默认构建 `lib/main.dart`。桌面入口是 `lib/main_desktop.dart`
（它才初始化 nativeapi 窗口 chrome），`main.dart` 走 `window_service_stub.dart`，
**根本不执行被测代码**。所以每一次「GL 错误 0」都是「没执行到」的空结论，
而不是「跳过后就正常」。真凶在错误的构建产物下必然测不出来。

判别方法（当时没做）：验证日志里有没有入口点独有的标记行
（`Applied main window chrome`）。它的缺席才是「0 错误」的真实解释。
**教训：二分/验证的第一件事是确认被测路径真的被执行，而不是先看指标。**

### 修法（已落地）

无边框改由 `linux/runner/my_application.cc` 在 `fl_view_new` **之前**用
`gtk_window_set_decorated(window, FALSE)` 完成——窗口创建时设置 visual 还没被
Flutter 用上，时序安全。相应地 `window_service_desktop.dart` 在 Linux 上不再设
`titleBarStyle`（新增 `_applyTitleBarHidden`，macOS/Windows 不变），
`applyMainChrome` / `applyCaptureChrome` 两处调用点共用。捕获模式与主窗口是同一
进程同一窗口，故一并生效。

**验证**（正确入口 `--target=lib/main_desktop.dart`）：
主模式 4/4 与捕获模式各 1 次 `GL错误=0  surface错误=0  kInvalidArguments=0`，
且 `App initialized` 恒为 1（此前失败时为 2 = app 自重启）；`./elsewhen.sh` 实跑同样 0 错误。

排除项（都实测过）：**不是环境问题**（同机器全新 `flutter create` 最小项目同后端正常）、
**不是 Wayland/X11 之别**（`GDK_BACKEND=x11` 仍复现）、**不是 Impeller 本身**、
**也不是 Skia**（Skia 在本机同样失败于 `gpu_surface_gl_skia.cc` "Could not make the
context current"）、`lib/` 内无 PlatformView（`platform_view.cc` 那条是后果非原因）。

**顺带更正**：`ELSEWHEN_OPAQUE_BACKGROUND=1` **是有读取点的**，
在 `ui/linux/runner/my_application.cc:107`（原生侧）。此前称「全代码库无读取点」
是错的——那次只 grep 了 `lib/` 下的 Dart 文件。同文件同时读取
`ELSEWHEN_DISABLE_IMPELLER`，两者都是 09:05 那次排障加的（未提交）。
`elsewhen.sh` 里基于错误根因设的「关 Impeller + 强制软件渲染」两个规避已删除：
它们会切到 Skia，而 Skia 在本机同样失败，等于主动绕开唯一能工作的那条路。

诊断代码已全部撤销（`lib/` 与 `my_application.cc` 均无残留，`flutter analyze` 干净）。

## 迁移重写后的失败测试处置（2026-09-28）

### 背景

`migrations.rs`（v1–v31 版本链）已废弃，改为 `schema.sql` 全量建库 +
`rusqlite_migration` 管理版本；`schema.sql` 由 `include_str!` 引入，是**构建必需
文件**（此前一直未跟踪，等于构建依赖一个不在库里的文件）。`Store::open` 改为
按库路径进程级只初始化一次（`INITIALIZED_DATABASES`）。

> 2026-09-29 更新：`schema.sql` 已移入仓库根的 `migrations/`，成为
> `migrations/0001_schema.sql`。下文提到的 `schema.sql` 均指它。
> 该次搬家顺带查出了上面「旧库开不起来」那个未修问题。

废弃后 `cargo test` 194 passed / 4 failed。这 4 个失败全部是**测已删除的迁移过程**，
不是测最终结构——`schema.sql` 已正确固化最终形态（`entity_merges` 无 UNIQUE、
`wiki_pages` 含 `human_edited_at` / `opinion`）。

### 处置原则

不直接删测试。先判断每条断言的不变量**今天是否还成立**：成立就改写为对当前
写入路径/当前 schema 的断言（保住回归网），不成立才删。

| 测试 | 断言的不变量 | 处置 |
|---|---|---|
| `migration_v29_splits_note_kind_and_adds_columns` | `note-` 页 kind 应为 note | 改写 → `imported_note_page_gets_note_prefix_kind_and_imported_area`（改测 `save_text_page` 写入侧） |
| `legacy_entity_merge_unique_constraint_is_migrated_without_losing_audit` | `entity_merges` 不得有 UNIQUE | 改写 → `entity_merges_allows_repeated_merge_of_same_source_and_keeps_audit` |
| `migration_v29_recovers_when_only_second_column_is_missing` | 迁移中断后重开补齐第二列 | 删除（无版本链即无「中断」可言），仅保留列存在性 → `wiki_pages_schema_exposes_human_edited_at_and_opinion` |
| `digest migration_backfills_existing_events_once` | 存量事件开库时全量入队 | 改写 → `every_event_is_enqueued_once_at_insert_and_open_never_duplicates` |

两删两改写，净 194 → 198，全绿。

### 改写过程中被测试纠正的三处认知

1. `note-` 前缀 + `kind='note'` 现在由 `save_text_page`（`src/wiki.rs:572`）写入时
   直接指定，不再依赖迁移回填。测试跟着移到写入侧才是真的。
2. `digest` 队列表**有沉淀窗**：事件落库即入队，但 `available_at` 在未来，不立即
   可领取。原测试那句「立即可领取」是针对 v31 回填路径的，抄到新路径上就错。
3. `TempStore` 实现了 `Drop`（连带删文件），不能 `drop(t.store)` 再重开；直接并存
   两个连接即可验证「重复开库不重复入队」。

### 回归网有效性：变异测试

改写后的测试必须能抓回归，否则只是把断言搬了个地方。做了两次变异确认：
- `schema.sql` 把 `UNIQUE(entity_kind, source_slug)` 加回 → 新测试 **FAILED**
- `save_text_page` 的 `kind` 改成 `"topic"` → 新测试 **FAILED**

两次变异均已还原（`schema.sql` 匹配数 0、`wiki.rs:572` 恢复为 `"note"`）。

### 提交范围：用 worktree 验证闭包，而不是猜

本次提交要含「迁移重写 + digest 功能 + 测试修复」，但工作区有 99 个文件被改
（含并行会话的 api 拆分、Flutter 生成物、脚本等）。**按文件列表猜闭包会错**——
第一版按 digest 符号引用猜，漏了 `src/config.rs`（`pin`/`unpin`/`test_env_guard`），
`cargo check` 报 3 个 `E0425`。

改用实测：`git worktree add --detach` 到 HEAD，逐批叠加候选文件并跑
`cargo check --all-targets`，直到零错误。最终闭包 = **22 个路径**
（13 改 / 5 删 / 4 新 + `schema.sql`），`cargo test --all-targets` 198/198。

**留在工作区未提交**（闭包外）：`src/ai/*`、`src/api/theme.rs`、`src/fonts.rs`、
`src/local_sources.rs`、`src/storage/{conversations,entities,provider,records}.rs`、
全部 `ui/**`、`docs/**`、`scripts/**`、`Cargo.toml` 之外的构建脚本改动。

### 遗留

- `schema.sql` 现已入库（它此前未跟踪却是构建必需——本 commit 修掉了这个隐患）。
- 旧库（`user_version=0` 且已有表）在新 bootstrap 下会因 `schema.sql` 裸 `CREATE`
  报错。**用户明确表示不考虑旧库**（计划走导出/导入重建）。若日后要支持，
  正确做法是让 `schema.sql` 自身幂等（加 `IF NOT EXISTS`），而**不是**在 Rust 侧
  自建「这是新库还是旧库」的判断去绕开 `rusqlite_migration`——后者是本轮走过的弯路。

### 未定位即搁置：「对话无法加载」

用户曾报应用内对话列表/内容加载不出来，**全程未定位**——未抓到实际报错，也未确认
症状形态（列表空？点开白屏？报错文案？），期间先后猜过两个方向且都被自己推翻。

2026-09-28 用户决定不再追。若日后复现，先做两件事再动手：①要具体症状描述；
②抓完整 stdout（此前几次排查都因为缺实际报错而只能靠推断，两次推错）。



## 旧库开不起来：user_version=0 的活库（2026-09-29，**未修，等裁决**）

### 症状与根因

应用启动报 `初始化存储失败: rusqlite_migration error while executing query
'CREATE TABLE schema_migrations ...'`，实际是 52 处 `table already exists`。

活库是**旧迁移链（v1–v31）**建的，版本记在 `schema_migrations` **表**里
（31 行），而 `PRAGMA user_version = 0`。新引导（commit `6dbc582`）改用
`PRAGMA user_version` 记版本，读到 0 就判定「这是空库」，去执行
`migrations/0001_schema.sql` —— 那里是 27 处裸 `CREATE TABLE`、零
`IF NOT EXISTS`，对着已有的 28 张表直接撞墙。

**不是本轮引入的**：`db3c84b`（本轮之前）的迁移列表只有 v1，同样会失败。
本轮的 v2 根本没机会执行。

### 「走导出/导入重建」这条既定方案其实走不通

两个独立的阻断，都实测过：

1. **够不着**：导出必须应用能启动，而它启动不了。
2. **会丢数据**：`export_wiki`（`src/wiki.rs:1225`）只覆盖 `list_wiki_pages`
   与 `list_wiki_log` 两张表。对话、事件、待办、规则**都没有导出**。
   活库实测 `events 90 / conversations 67 / wiki_pages 36 / todos 2 / rules 3`
   —— 重建只能回来 36 个 wiki 页。

### 反转：这个库其实已经是 v1 基线了

把活库副本与 `migrations/0001_schema.sql` 新建库逐项比对：

| 比对项 | 结果 |
|---|---|
| 表集合 | 28 张全等，无缺无多 |
| 逐表列结构 | 全等 |
| 索引 | 全等 |
| 触发器 | 全等 |
| 基线文件冻结后是否被改过 | 没改过（冻结于 `6dbc582`） |

旧链跑完 31 个版本后的最终形态**就是**冻结的 v1 基线。`user_version = 0`
是个**谎报**，不是真的落后。

端到端验证（副本上走真 `Store::open`）：只执行 `PRAGMA user_version = 1`，
不动任何数据 → 开库成功，`user_version` 变 2（只补 v2 goals），
`events 90 / conversations 67 / wiki_pages 36` 一条没丢。

### 未决

用户未选方案，此项**悬空**。三个现有备份（09-28 两份 events=86、09-23 一份
events=66）的 `user_version` 也都是 0，**不能当「重建后的干净起点」**。

候选：
- A. 钉 `user_version = 1`，原地保留。改代码为零，但需要一条显式、可审计的
  命令（或 `scripts/` 下的一次性脚本），且**不能**放进应用启动路径。
- B. 重建，接受丢 154 条记录。
- 自动检测「有表但 user_version=0 就自动补版本」**否决**：这正是本仓已走过并
  记录的弯路——在 Rust 侧自建版本判断去绕开 `rusqlite_migration`。它会静默
  改写版本元数据，把「谎报」变成「自动生效的谎报」。

## 目标与偏差检测（FR-PES-005）落地后的待办（2026-09-29）

本轮只做了 FR-PES-005-01（目标管理）与 -02（目标进对话记忆），-03 的评估快照、
-04/-05 全部未动。以下为因此新增/确认的待办。

### 待裁决（不做实现决策，先等用户）

- **Q1 偏差结论往哪里呈现**：独立列表页 / 混入对话上下文 / 写入待办。倾向独立
  列表加对话注入，**不写入待办**——AI 判断的偏差与用户自己的承诺语义不同，混在
  用户亲笔的待办列表里容易被当成用户自己的话。
- **Q2 检查节奏是否由 phase 决定**：候选近期每周 / 中期每月 / 长远每季。若不由
  phase 决定，则要另找节奏来源（上一轮评估时间 / 上次结论时间）。
- **Q4「用户在干什么」以何为主**：事件流有结构化分类与原文，对话只有自由文本。
  是否额外抽一层对话主题，成本显著更高，是 -04 最大的工作量不确定项。

### 实现层待办（不依赖裁决）

- **`generate_insights` 仍是死代码**（`src/ai/insight.rs`，全仓零调用点）。它是 -04
  的分析器原型，接线即可用，但**接线**这一步需要：新增 `goal_assessments` 表（又一条
  迁移，须走 `new_migrations.rs`）、后台周期任务（照 `trigger_knowledge_digest` 的
  重试/冷却/重启恢复模式）、以及失败可见性。
- **目标上限在 UI 侧是硬编码的 `3`（`ui/lib/widgets/goal_view.dart`）**。Rust 侧
  已有 `trigger_cap_literal_matches_the_error_message_constant` 守住「触发器字面量 vs
  报错文案常量」，Dart 侧只有一条「等于 3」的断言，**跨层仍无自动比对**：
  `MAX_ACTIVE_GOALS` 没被 frb codegen 导出到 Dart（只有函数与 DTO 生成出来），
  `ui/test/goal_model_test.dart` 断言的是一个自己写死的 3。放宽上限需手改三处：
  `src/storage/goals.rs` 常量、`new_migrations.rs` 触发器字面量、Dart 侧常量。
  UI 那处只是「提前告知」，漂移的最坏后果是提示文案与实际不符，不是约束失效。

- **`ui/lib/bridge/generated.dart/api.dart` 的手工 export 段**：frb 2.14.0-beta.2
  每次重跑 codegen 都会覆盖掉它，靠注释提醒人工补回。根治要换 frb 版本或改上游；
  在此之前每次重跑都要复查，陷阱已写进 `flutter_rust_bridge.yaml`。
- **Linux 上 codegen 需要 `CPATH` + 剥掉 `CPATH` 的 cc wrapper 双管齐下**，否则
  `cargo expand returned empty output`。同样已写进 `flutter_rust_bridge.yaml`。
  建议写成 `scripts/` 下的一键脚本，当前只以注释形式留在 yaml 里。
- **`settings_screen_test.dart: prefills real provider from bridge` 仍 flaky**（走真
  FFI，共享数据目录）。非本轮引入，但每轮 `flutter test` 都会随机翻车，值得隔离。
- **commit `ba797e3` 不是自洽的 Flutter 提交**：`ui/lib/widgets/custom_title_bar.dart:36`
  引用了只在 `f73e827` 出现的 `knowledgeDigestBusyProvider`。单独 checkout 该 commit
  编译不过。处理需要 rebase 改写历史，**用户已知悉但尚未决定**。
