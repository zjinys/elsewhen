# 同步功能检查（2026-11-19）

模式：PR Review，按当前同步功能限定范围检查。

基于 `f192526` 和当前工作区，检查 Rust 同步调度、配对交接、快照、投递与回执、Flutter 后台编排与设置状态、中继。结论是 **4 个已复现的问题，尚未修复**。这是本地代码和隔离测试检查，未操作个人数据库、手机或线上中继；此前未提交的其他工作保持原样。

本文前半段保留检查时的触发与复现事实。用户随后授权修复，当前实现和验证见文末[修复记录](#修复记录)，不再将这些问题视为未处理。

## P1：跨轮次的大快照在源数据变化后无法自动恢复

- **触发**：快照超过一个传输窗口，第一窗已被接收端持久化并确认；发送端完成一条后台分析，再续传。
- **原因**：[delivery.rs](../../src/storage/sync/delivery.rs) 的 `begin_snapshot_export` 比较 `blob_clock` 与当前逻辑时钟，变化时返回 `sync_snapshot_export_requires_new_request`。这道一致性检查应当保留。但 [engine/snapshot.rs](../../src/sync/engine/snapshot.rs) 在进入失败清理分支之前就传播了这个错误，导出仍为 `building`。[enrollment/flow.rs](../../src/sync/runtime/enrollment/flow.rs) 仅给 `aborted` 导出分配新 ID，对 `building` 总是续传旧 ID。
- **后果**：接收端停在未完成的初始同步，后续重试、重开 Store 都仍被拒绝。重快照的恢复还会复用 `aborted` 的旧 ID，不能越过相同的时钟检查。此复现没有发布混合数据，不应描述为已发生数据损坏。
- **复现结果**：实际调用生产导出、分片接收与 ack；正常 `complete_analysis` 后连续三次续传均报需要新请求，状态始终为 `building`；手动中止导出后复用旧 ID 仍失败。
- **建议**：优先为同一快照保存可跨轮次、跨进程使用的稳定导出内容，后续本地变更走增量。若采用重建方案，需要显式中止旧传输、生成新 ID 并协调接收端切换；只有改为 `aborted` 不足以修好重快照恢复。测试覆盖源端持续写入、重启、过期 blob 和新旧快照信令交错。

## P1：同步排在分析和知识消化之后，缺少独立的失败与调度边界

- **触发**：`triggerAnalysis` 或知识消化调用向外抛错，或分析任务不断到达、持续未排空。
- **原因**：[worker_reminders.dart](../../ui/lib/bridge/rust_bridge_parts/worker_reminders.dart) 中 `_wakeAnalysisWorker` 在同一个外层 `try` 内先排空分析，再处理消化和报告，最后才 `_runNetworkSyncTick`。前面抛错会直接跳到外层 `catch`；长队列则延迟同步。定时器同样唤醒这条 worker，不能独立补跑网络同步。
- **后果**：本地记录虽然保存成功，发送和接收却可能长时间停顿；同步自己的错误状态未必反映出“还没被调度”。普通 provider 单条失败已有部分捕获，这不等于每一次模型请求失败都会跳过同步；问题是编排层仍存在未隔离的异常路径和队列饥饿。
- **复现结果**：隔离桥接中先验证健康 worker 能到达网络 tick，再让分析调用抛出合成异常；下一次本地记录确实触发了分析，但未到达网络 tick。
- **建议**：独立管理同步的唤醒、错误与退避，给 AI 队列和同步分配有界执行预算。Rust 的 `background_guard` 也串行保护工作区，因此不能只加一个 Dart timer 就认为完成解耦；需要同时设计租约交接与让出时机。

## P1：加入第三台设备后，已有加入端可能整轮同步失败

- **触发**：owner 分别与手机和平板完成配对，手机收到包含平板的受信名册。手机只有与 owner 直接配对的密钥和路由。
- **原因**：[runtime/network.rs](../../src/sync/runtime/network.rs) 把所有活跃名册成员作为 peer，逐个无条件调用 `vault.peer_keys(...) ?`、`peer_route(...) ?`。名册成员身份与本机具备直接配对通道并不是同一事实。
- **后果**：遇到无直接配对密钥的成员，返回 `sync_vault_peer_missing`，整轮提前结束。若它排在 owner 前面，连 owner 都无法联系；若前面已有进展，整轮报错也会丢失成功部分的汇总反馈。
- **复现结果**：使用真实配对握手、签名名册与解锁会话构建三设备场景；确认 owner 的配对路由存在，第三成员排在 owner 前时，tick 稳定返回缺少 peer 密钥错误。
- **建议**：明确多设备采用星形转发还是完整互联。分别表示名册成员和已建立通道；错误应按 peer 隔离，同时显式暴露不可达状态。仅过滤无密钥成员不能保证第三台设备产生的操作最终送达其他设备，还需覆盖转发或通道建立、回执与撤销语义。

## P2：积压查询的迟到响应会覆盖更新后的锁定状态

- **触发**：`refreshBacklog()` 已发出状态查询，随后用户锁定同步；查询返回锁定前的 `local_ready`。
- **原因**：[sync_setup_controller.dart](../../ui/lib/sync/sync_setup_controller.dart) 只在 await 前判断 `busy` / `joiner.active`，返回后只判断是否关闭，随后用整份旧响应覆盖 `state`。
- **后果**：Rust/native 仍保持锁定，但界面倒退为已就绪，后续状态与操作入口失真。该问题是 UI 状态竞态，不是实际绕过密钥锁定。
- **复现结果**：用 Completer 控制响应时序，确认锁定已完成，再释放旧响应；期望 `locked`，实际变为 `local_ready`。
- **建议**：查询绑定操作代次、installation 与 vault revision；返回时再次校验，在状态已变化时丢弃并重排读取。积压刷新尽量只合并属于同一状态版本的积压字段。

## 优化点（与上述缺陷分开处理）

1. **有积压时连续推进短同步批次**：当前 `push_due` 每 peer 每次只发 3 封，上传/拉取有进展会重新唤醒整条 AI worker。可以在总耗时与字节预算内连续推进同步，再让出给 AI；空闲才降频。注意当前存在主动重唤醒，不能简单把 3 封 / 30 秒解释成固定的吞吐上限。纯空闲时等待远端新变化，仍可能要等约 30 秒的完整轮询间隔。
2. **复用连接并约束整轮耗时**：每 peer 每 tick 都新建 `RelayClient`，底层 HTTP 请求超时 30 秒；peer 数、页数有限不等于总墙钟耗时有限。按 relay / 代理配置复用客户端，并设整轮预算、超时后续跑，能减少握手成本和慢中继拖延。
3. **peer 公平调度**：当前固定 `.take(8)`，名册不变时始终选同一批成员。超过 8 个远端成员时需要持久或会话级轮转游标，否则后面的成员得不到轮次。该项为代码确认的扩展风险，本次未做超过 8 台设备的端到端复现。

## 验证与证据

现有测试均通过，但没有覆盖上述复现边界：

| 测试范围 | 结果 |
| --- | --- |
| `cargo test --lib sync::` | 337 通过，4 忽略 |
| `relay/` 下 `node --test standalone/*.test.mjs test/*.test.mjs` | 49 通过 |
| Flutter 同步设置、加入、交接、状态、会话刷新、数据集清理、积压刷新共 8 个测试文件 | 48 通过 |

Flutter 命令（在 `ui/`）：

```sh
fvm flutter test --no-pub test/sync_setup_controller_test.dart test/sync_join_controller_test.dart test/sync_enrollment_controller_test.dart test/sync_pairing_terminal_test.dart test/sync_tick_status_test.dart test/conversation_sync_refresh_test.dart test/sync_dataset_cleanup_test.dart test/sync_backlog_live_refresh_test.dart
```

额外使用两条 Rust 复现断言当前错误行为，均得到确认；两条 Flutter 回归断言期望的正确行为，均如预期失败。临时复现未纳入常规测试套件，生产源码已恢复，合成数据均位于临时目录。

本机 `/tmp` 保留本次证据（不作为长期 CI 产物）：

- 基线：`elsewhen-sync-audit-{rust,relay,flutter}.log`。
- 复现：`elsewhen-sync-audit-{snapshot-resume,three-device,repros}.log`。
- 复现源码：`elsewhen-sync-audit-engine-repro.rs`、`elsewhen-sync-audit-network-repro.rs`、`elsewhen-sync-audit-flutter-repros.dart`。

建议修复顺序：快照恢复 → 同步与 AI 调度隔离 → 多设备通道模型 → UI 异步状态保护；吞吐与连接优化随后处理。

## 修复记录

四项修复已落地，机制、取舍与运行条件见[Agent Note](../notes/implemented/bug-fix/2026-11-19-sync-resume-routing-and-scheduling.md)。包含 v90 迁移、完整加密导出缓存、owner 转发与逐跳应用回执、原生 AI 有界批次、Flutter 交替同步，以及 UI 查询代次校验。同时补齐 provider 全通道准备、mailbox 扫描识别全部通道与 peer 轮转。

回归覆盖：

- `src/sync/engine/tests/snapshot_resume.rs`：源端继续写入和分析、重启/中止后的相同快照续传、最终发布与回执后缓存清理、重快照无需等待卡死阈值即继续。
- `src/storage/sync/membership/tests/forwarding.rs`：三设备大操作转发、逐跳应用回执、重复投递、拒绝普通成员伪造来源及撤销来源、旧成员间发送队列升级清理。
- `src/sync/runtime/network/tests/three_devices.rs` 与 relay runner：真实 Noise 配对、签名名册、各端解锁和本机中继 HTTP 收发，三端各自写入并收敛、发件队列全部退休。
- `ui/test/sync_worker_scheduling_bridge_test.dart`：同步先于慢 AI、连续 AI 批次之间同步、异常后与 AI 退避期间继续同步。
- `ui/test/sync_backlog_live_refresh_test.dart`：迟到的 ready/locked 响应不能覆盖已经完成的锁定/解锁。
- `src/storage/new_migrations_tests.rs`：v89 → v90 保留原始事件、只中止旧的在途导出，收到的操作补齐上一跳字段；peer 轮转单测覆盖 19 个成员和重建游标。

已完成的定向门禁：中继 50 项、Flutter 同步 51 项、Dart 定向静态分析无问题；对话协议定向复跑 28 项及生成取消 1 项通过，数据库迁移套件 34 项通过。Rust `cargo test --all-targets -- --test-threads=4` 全量通过（1298 单元 + 7 集成，28 忽略）；其后补充的旧队列清理、名册先行与 relay 过期恢复通过定向重跑，最终同步专项 343 通过 / 5 忽略、release 构建通过。首次默认并发全量运行有 7 项 mock HTTP/生成检查超时，以及 2 项迁移版本断言未更新；版本断言已修正到 v90，超时项独立复跑通过，最终全量改为有限并发重新验证。

尚未执行手机/桌面真人多端验收，未部署中继、未安装 APK、未提交或推送 Git。三台设备需要使用更新后的客户端，owner 在线才能转发叶节点之间的增量。快照缓存额外占用本地密文空间；HTTP 跨轮次池复用与完整墙钟预算仍待后续优化。

本机最终证据：

| 命令或检查 | 日志 | 结果 |
| --- | --- | --- |
| `cargo test --all-targets -- --test-threads=4` | `/tmp/elsewhen-sync-fix-all-targets-bounded.log` | 1298 单元、7 集成通过，28 忽略；在最后三项边界补充前完成 |
| `cargo test --lib sync:: -- --test-threads=4` | `/tmp/elsewhen-sync-fix-final-sync.log` | 343 通过，5 忽略；含旧队列清理、轮转与快照 TTL 恢复 |
| `cargo build --release` | `/tmp/elsewhen-sync-fix-final-release.log` | 通过 |
| `relay/` 全套 Node tests | `/tmp/elsewhen-sync-fix-relay-final.log` | 50 通过；后续名册先行与 TTL 恢复改动另做定向 wire 重跑 |
| v90 迁移套件 | `/tmp/elsewhen-sync-fix-migrations-final.log` | 34 通过 |
| 最终 release 库上的 Flutter 同步回归（10 文件） | `/tmp/elsewhen-sync-fix-flutter-release.log` | 51 通过 |
| 最终配对交接、两端同步与旧名册三端收敛 wire 回归 | `/tmp/elsewhen-sync-fix-final-wire.log` | 3 通过 |
| Dart 定向 analyze | `/tmp/elsewhen-sync-fix-analyze-final.log` | 无问题 |

Agent Note 的结构、格式校验及新文档相对链接检查通过。新建迁移文件，未修改冻结的历史迁移；公开桥接 API 未变，无需重生成 FRB。
