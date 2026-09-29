# 下一步工作

项目当前实施顺序以 [个人认知主循环 Roadmap](docs/roadmap/2026-09-17-personal-cognition-main-loop-roadmap.md) 为准。本文件只提供开发入口，不再维护另一套任务清单。

## 当前阶段

Phase 0：安全基线与可观测性。

已经完成：

- Flutter/Rust Bridge 集成测试使用独立临时数据库，不再读取或写入个人数据；
- 分析队列 pending / running / retry / succeeded / failed 状态已通过 Bridge 暴露，并在设置页“数据”标签展示。

下一步：

- 完成 Phase 0 文档与验证门禁；
- 进入 Phase 1，设计统一输入关联模型及 `submit_input` / `list_daily_entries` 业务 API；
- 保持现有 `record_event`、`send_message` 和 Capture 行为兼容。

## 开发验证

```bash
# Rust
cargo test

# Flutter（项目固定使用 FVM）
cd ui
fvm flutter analyze
fvm flutter test

# Rust API 变化后，在项目根目录重新生成 Bridge 并构建 release 动态库
scripts/regen.sh
```

真实 Bridge 测试必须通过 `ui/test/support/isolated_bridge.dart` 创建临时数据库。不得依赖默认平台数据目录中的已有记录。

## 长期约束

- 原始事件不可变；
- 保存不等待 AI 或网络；
- AI 结果可追溯、可重算，不直接成为不可审计的业务事实；
- 高影响写操作继续使用草拟、确定性校验和必要时确认；
- 每完成一个 Roadmap 步骤，立即更新对应勾选项和“完成记录”。
