# 调研：多设备同步作为可选功能组件的组织方式

Status: research

关联：[多设备同步实施路线](../roadmap/2026-10-03-multi-device-sync-roadmap.md)、[同步设计 Note](../notes/proposed/architecture/2026-09-30-multi-device-sync.md)。本文是 2026-10-07 的一次方案调研，记录"把同步拆成可选组件"的可行性结论，不代表已开始实施。

## 问题

把多设备同步从主应用中拆出来，作为一个**可选的功能组件**——不启用同步的用户/构建不受其复杂度、依赖与二进制体积影响。需要回答：当前代码结构是否支持，以及按哪种方式组织项目。

## 结论

**可行。** 作为"运行时/编译期可选的功能组件"完全可行，成本低；作为"可物理分离的独立 crate"也可行，但有一定改造成本，建议推迟到首版真实设备验收之后。不推荐拆成独立仓库/跨进程插件。

关键依据：当前耦合面比预想的小，且是当初"复制底座"设计做对了的结果。

## 当前耦合的实际情况（2026-10-07 代码核查）

### 核心 → sync 的调用点只有 3 类，全部是窄接口旁路写入

| 位置 | 调用 | 性质 |
|---|---|---|
| `src/storage/events.rs` | `sync_record_capture` × 3、`sync_record_analysis`（第 96/212/288/455 行） | 事务内向 outbox 投递 |
| `src/storage/conversations.rs`、`provider.rs` | `sync_record_*` / `sync_mark_provider_dirty` | 同上 |
| `src/api/mod.rs` | `initialization_guard`（启动）、`background_guard`（后台加工前）、`has_sync_authority`（第 358/903 行附近） | 守卫式检查 |

业务主流程**不读** sync 的任何状态——所有接触点都是"如果启用了就往 outbox 投一份"的旁路。

### sync → 核心是单向依赖

sync 只读取核心的领域类型、`Store`、迁移机制，无循环依赖。

### schema 已天然分离

`migrations/` 下 v16–v24 共 9 个迁移目录全部是 sync / provider-key 专用（`16-sync-core` … `24-provider-edit-basis`），v1–v15 是业务核心。sync 的表没有打散在核心迁移里。

### Flutter 侧已有门控先例

`ELSEWHEN_SYNC_PREVIEW` 环境变量已控制设置页同步入口的显隐（`ui/lib/sync/sync_setup_controller.dart`），UI 层无需为"可选"再做结构改动。

## 三种组织方式（按改造成本排序）

### 方案 A：Cargo feature 门控（推荐，立即做）

保持单 crate，加一个 default feature：

```toml
[features]
default = ["sync"]
sync = ["dep:snow"]
```

- `src/lib.rs`：`#[cfg(feature = "sync")] pub mod sync;`
- 4 个 `sync_record_*` 方法内部改为 cfg 空实现旁路；或更干净地抽一个 `SyncOutbox` trait，no-sync 构建给 no-op 实现
- `src/api/mod.rs` 3 处 guard 调用同样 cfg 化
- Flutter 侧沿用 `ELSEWHEN_SYNC_PREVIEW`；无 sync 的构建里设置页不渲染同步节

**优点**：一天内可完成；schema 不用动（不编译 sync 的库照样能开 v24 的库，sync 表闲置）；CI 加一条 `cargo build --no-default-features` 把"核心可独立编译"变成门禁，防止耦合悄悄长回去。
**缺点**：代码仍在仓库/二进制里，只是关掉了。

### 方案 B：拆成 workspace 多 crate（中期目标，S06 之后）

```
elsewhen/                 # workspace 根
├── core/                 # 现 src/ 业务 + storage 主体，迁移 v1–v15
├── sync/                 # 现 src/sync + src/storage/sync + 迁移 v16–v24
└── bridge/               # flutter_rust_bridge 门面，feature 组合 core/sync
```

- core 定义 `SyncSink` trait（投递领域操作、查询授权），sync crate 实现
- **`provider_keys.rs` 的归属需单独决策**：它是"Provider 静态加密"，单机也有价值（锁定时不让 Key 明文落盘），严格说不属于 sync——建议留在 core 或拆成第三个 crate
- 迁移归属：sync crate 自管 v16–v24，core 只到 v15；打开数据库时按 feature 决定迁移目标版本

**优点**：物理分离，sync 可独立演进/测试/未来独立发布。
**缺点**：`src/api/` 要拆成"纯业务 API"与"sync API"两层门面，frb 代码生成复杂度上升；目测 2–3 天工作量加回归风险。等 sync 的 API 面稳定后再拆代价最小。

### 方案 C：完全独立仓库 / 跨进程插件（不推荐）

sync 深度依赖**同一事务边界**（"事件提交和 outbox 投递必须原子"，是路线文档的不变式 1/3）。跨进程后无法保持，只能退化为最终一致的旁路抓取——这是架构倒退，不是组件化。

## 必须正视的约束：schema 演进是单向的

无论选哪个方案：

- 一旦用户开过 sync（库到了 v24），关掉 sync 后那些表仍在；no-sync 构建的 App 打开 v24 的库必须能正常工作（只读写 v1–v15 的表，sync 表原样保留）。
- 按 rusqlite_migration 的行为这天然成立，但必须写成测试：**no-sync 构建打开含 sync 数据的库，业务功能完整、sync 数据不丢**。
- **不要承诺"去掉 sync 数据库就降回 v15"**——向下迁移是另一个大坑，不在"可选组件"范围内。

## 建议路径

1. **现在做方案 A**：趁 sync 代码尚未提交进 git（2026-10-07 时 `src/sync/`、`src/storage/sync/` 等全部未跟踪），先加 feature 门控并落 CI 门禁。
2. **方案 B 记为 S06 之后的技术债**：首版真实设备验收通过、API 面稳定后再拆。
3. **文档一致性**：这个定位与路线文档既有边界一致（`ELSEWHEN_SYNC_PREVIEW` 开发预览、"未启用完整加工归属前只作为测试能力，不开放真实多设备运行"），是把已在走的路线正式化，不是架构转向。

## 验证记录

本调研基于 2026-10-07 的工作区代码核查：`grep` 统计核心对 sync 的调用点、`migrations/` 目录清单、`Cargo.toml` 依赖与 lib crate 类型。未改动任何文件。
