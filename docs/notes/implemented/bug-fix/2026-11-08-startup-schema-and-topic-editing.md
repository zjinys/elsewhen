# Agent Note: 后台开库版本检查、缺表升级与手工主题编辑

Status: implemented

## Problem

用户报告debug-run启动变慢、分析worker重复报缺少`knowledge_artifact_languages`，以及“创建主题”保存后不能编辑。只读核查实际库为user_version=75，但缺少该表；此前迁移75只修复单列漂移，范围不足。`Store::open_background`每次调用latest_version时还会建内存库重放全部迁移。“创建主题”调用saveTextPage而不是saveTopicPage，落库kind=note，命中素材只读保护。

## Decision

新增迁移76，通过CREATE TABLE IF NOT EXISTS补齐缺少的产物语言元数据表，不重置现有分析结果或伪造历史语言。回归模拟实际75缺表，前台升级后worker可claim并complete，代数/原文保持。迁移版本检查改为编译内嵌目录的结构校验和数量，不运行SQL；后台仍不执行迁移。Rust启动新增storage_ready_ms与bridge_ready_ms日志，以区分实际库初始化与恢复步骤。

创建主题表单改用已有saveTopicPage接口；虚分派委托移入仓储实例，以保留测试注入。新主题创建时即有人工正文保护，可直接编辑，后台不得整篇覆盖。素材导入仍走原note/source路径并保持只读。

迁移77恢复旧手工创建页面：仅kind=note、note-*地址、无来源URL、非派生页、有“用户粘贴文本导入”修订记录、且关联来源无locator时，改为topic/insight并补人工保护标记。保留ID/slug/正文/标签/修订和来源快照；不能区分同一旧文本入口的“粘贴自写笔记”与“创建主题”，两者都归手工创建页面。缺少可核实创建记录的旧页不变。

## Alternatives considered

- 将全部note/source放开编辑：会破坏导入原文与来源快照保护。
- 给旧页重新生成slug：会打断既有链接与引用；类型与地址不要求一一对应。
- 后台遇到缺表临时建表：结构升级必须由前台统一执行。
- 修改启动脚本跳过数据库保护或关闭后台：掩盖根因，无法修复重复的全迁移开销。

## Verification

- 可复测工具`cargo run --release --example profile_storage_startup -- /tmp/disposable.db`仅用于可丢弃库。修复前空库20次后台打开1869ms，修复后1ms；最终版本首次前台新建约100ms、20次后台1ms。单机计时不代表所有硬件，首次真实库回填仍取决于数据量。
- Flutter仅构建不启动：`fvm flutter build linux --debug --no-pub --target=lib/main_desktop.dart`约21.944秒。它是进入窗口前的编译阶段，与后台版本检查是不同开销；debug-run本身只设置日志/代理并转发flutter run。
- 迁移专项32项通过（`/tmp/startup-migrations-test.log`）；全量库1141+集成2，即1143 passed / 20 ignored（`/tmp/startup-topic-all.log`）。后续新主题人工保护增量见`/tmp/topic-create-protection.log`，原note保存见`/tmp/topic-create-rust.log`。
- 表单/编辑器21项通过（`/tmp/topic-editor-tests.log`），相关静态检查无问题。真实Bridge创建topic、编辑保存并拒绝覆盖导入源的回归见`/tmp/topic-editor-bridge.log`。release重新编译，FRB签名不变。
- 个人库只读核查结构与日志，未在后台或外部脚本里执行修复；新构建前台启动自动执行76/77。计时工具没有指向个人库。

## Consequences

版本检查不再反复运行全迁移，已知75缺表有明确升级路径。手工主题与原始材料的编辑边界恢复一致，代价是旧手工入口的来源只能按现有审计记录识别。运行中的旧进程不会热更新Rust库，需启动新构建；Flutter编译/插件初始化仍有独立成本，不宣称整个启动缩短1800倍。
