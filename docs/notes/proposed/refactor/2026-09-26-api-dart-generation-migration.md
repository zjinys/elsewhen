# api.rs 拆分后的 Dart 生成物迁移方案

- Date: 2026-09-26
- Status: proposed（Rust 侧已拆完并提交，Dart 生成物迁移待 Dart agent 协调）
- Area: ui
- Related: docs/notes/proposed/refactor/2026-09-26-storage-and-detail-view-split.md

## 背景

`src/api.rs`（2630 行 FRB 门面）已拆为 `src/api/` 目录：`mod.rs`（1272 行共享内核）+
12 个领域子模块（fonts/theme/todos/wiki/relations/tweet/import/wiki_chat/
provider_config/entities/rules/conversations）。详见 git log `58d9a43`..`8ce167d`。

**Rust 侧关键机制**：各子模块 `pub fn`，`mod.rs` 用 `pub use <域>::*` 扁平化重导出，
因此 `crate::api::xxx` 对外路径不变。已验证：旧 `frb_generated.rs`（用 `crate::api::xxx`
路径）仍编译通过、CONTENT_HASH 与现有 Dart 生成物一致——**Rust 拆分不破坏现有桥**。

## 待办：Dart 生成物迁移

FRB 按 Rust 函数的**物理定义文件**生成 Dart 模块（不认 `pub use`）。一旦跑 `./regen.sh`：

1. 生成 `ui/lib/bridge/generated.dart/api/<域>.dart` 共 12 个新文件
2. 聚合文件 `api.dart` 只 `import` 这些子文件、**不 `export`**
3. `frb_generated.rs` 的函数路径变为 `crate::api::<域>::xxx`，CONTENT_HASH 改变
4. **后果**：UI 侧 `import 'generated.dart/api.dart' as api; api.createTodo(...)`
   将找不到 `createTodo`（它搬到了 `api/todos.dart`）

## Dart 调用点现状（实测）

- `api.xxx` 调用共 **129 处**
- 集中在 4 个手写文件：
  - `ui/lib/bridge/rust_bridge_repository.dart`（102 处，唯一聚合点）
  - `ui/lib/screens/settings_screen.dart`
  - `ui/lib/utils/system_fonts.dart`
  - `ui/lib/models/settings.dart`
- import 统一为 `import 'generated.dart/api.dart' as api;`

## 可选迁移方案

### 方案 A（推荐）：聚合 export 层
在 `generated.dart/api.dart` 之外建一个**手写**聚合文件（如 `ui/lib/bridge/api.dart`）：
```dart
export 'generated.dart/api.dart';
export 'generated.dart/api/todos.dart';
export 'generated.dart/api/wiki.dart';
// ... 其余 10 个域
```
4 个调用文件把 import 从 `generated.dart/api.dart` 改为这个手写聚合文件即可，
129 处 `api.xxx` 调用**一行不用改**。
- 优点：regen 不覆盖手写文件，稳定；改动集中在 4 个 import 行
- 缺点：新增一个聚合文件

### 方案 B：repository 内部消化
`rust_bridge_repository.dart` 已承载 102/129 调用。让它内部按域 import
（`import '.../api/todos.dart' as todo_api;`），对外暴露的 repository 接口不变；
其余 3 个文件改为只依赖 repository，不直接碰 frb api。
- 优点：UI 与 FRB 生成物彻底解耦，长期最干净
- 缺点：repository 需为 settings/fonts/models 三个文件补少量转发方法

### 方案 C（不可行）
两个 `import ... as api` 会命名空间冲突，排除。

## 执行步骤（建议 A）

1. Dart agent 确认当前工作区改动已提交（避免与 `main.dart`/`settings.dart`/
   `app_theme.dart`/`content_font.dart` 的活跃改动冲突）
2. 仓库根跑 `./regen.sh`（生成 12 个 `api/*.dart` + 更新 `frb_generated.rs` hash）
3. 新建 `ui/lib/bridge/api.dart` 聚合 export（方案 A）
4. 改 4 个调用文件的 import 指向聚合文件
5. `cd ui && fvm flutter analyze && fvm flutter test`（170 项须全绿）
6. 一起提交：Rust 生成物 + Dart 生成物 + 聚合文件 + 4 个 import 改动

## 验证门

- `cargo build --release`（hash 与 Dart 侧一致）
- `fvm flutter analyze` 无 error
- `fvm flutter test` 170/170
- 手动冒烟：App 启动不报 `rustContentHash mismatch`
