# 结构性重构排期：storage.rs 与 wiki_page_detail_view.dart 拆分

Status: partially done（storage.rs 拆分已完成 2026-09-26；wiki_page_detail_view 待排期）

来源：deep review P1-1 / C9。两项均为「单文件巨型化」的结构性问题，不影响正确性，
但抬高后续改动的心智负担与冲突面（本仓库有并行会话，大文件合并冲突风险高）。

> 约束：拆分属纯结构搬运，**不改变任何行为与 SQL**。每一步独立可验证，
> 跑 `cargo test --all-targets`（Rust 180 项）/ `flutter test`（170 项）+ `analyze` 保持绿。
> 避开并行活跃区，开工前对照 `git diff <file> | grep '^@@'`。

---

## 一、`src/storage.rs`（6147 行，113 个 Store 公开方法）

### 现状结构（实测行号）
- 1~467：域类型（`RuleStatus`/`TodoStatus` 等枚举与 impl）、`map_*` 行映射、常量
- 462~467：`impl Clone for Store`（`expect("reopen database")`，另见 P1-2）
- 468~712：`Store::open` —— PRAGMA + 基础 DDL + **30 个版本的 schema 迁移**
  （`PRAGMA table_info` 探测 → 条件 `ALTER`/`CREATE`，幂等防中途崩溃）
- 713~4225：113 个业务查询方法（事件/分析任务/会话/wiki/关系/待办/规则/provider…）
- 4226~4300：`impl StorageAdapter for Store`（FRB 桥接契约）
- 4301~4311：`impl Drop`
- 4312~6147：`mod tests`（约 1800 行）

### 拆分方案
`storage.rs` 已是 `mod adapter;` 的父模块（`src/storage/adapter.rs` 存在），
顺势把 `storage.rs` 转为 `src/storage/mod.rs`，按域切子模块：

```
src/storage/
├── mod.rs            # Store 结构体 + open + Clone + Drop + 域类型 re-export
├── adapter.rs        # （现有）StorageAdapter trait
├── migrations.rs     # Store::open 内的 30 版本迁移 → ensure_schema(conn) 函数
├── events.rs         # impl Store：事件 + analysis_jobs + event_analyses
├── conversations.rs  # impl Store：conversations + messages + input_records
├── wiki.rs           # impl Store：wiki_pages + revisions + log + relations + aliases
├── entities.rs       # impl Store：entity_facts + merges + snapshots
├── todos.rs          # impl Store：todos + rules + recordability
└── provider.rs       # impl Store：ai_provider_configs + token_usage
```

Rust 允许同一类型跨文件多个 `impl Store` 块（同 crate 内），无需改类型定义。

### 执行顺序（每步独立 commit + 全量测试）
1. **先抽 migrations.rs**（风险最低、收益最大）：把 `Store::open` 里 469~712 的
   DDL/迁移整块抽成 `pub(crate) fn ensure_schema(conn: &Connection) -> Result<()>`，
   `open` 改为建连接后调一次。**这是最大单一收益项**——open 从 244 行缩到 ~20 行，
   迁移逻辑自成一体、可单独加单元测试。
2. 再按域抽 events/conversations/wiki/entities/todos/provider（机械搬运，可分批）。
3. `mod tests` 最后处理：可整体留 `mod.rs`，或按域拆 `*_tests.rs`。

### ✅ 实际落地记录（2026-09-26，7 个 commit，逐步全绿）
采用「`src/storage.rs` 保留为模块根 + `src/storage/` 子模块」结构，最后 `git mv`
为 `mod.rs` 规范化。每步独立 commit + `cargo build` + `cargo test --all-targets`
（180/180 全绿）验证，零行为/SQL 变化：

| commit | 子模块 | 行数 | storage 主文件 |
|---|---|---|---|
| `e588cee` | `migrations.rs`（ensure_schema，30 版本迁移） | 706 | 6147 → 5462 |
| `f503a74` | `provider.rs` + `entities.rs` | 232 + 356 | → 4904 |
| `a8b6c06` | `wiki.rs`（wiki_pages/revisions/log + record_wiki_revision） | 786 | → 4136 |
| `b83f285` | `conversations.rs`（会话/消息/token_usage） | 357 | → 3790 |
| `3076503` | `records.rs`（rules/todos/relations/pending_actions/meta） | 523 | → 3285 |
| `0b24d71` | `events.rs`（insert_event/input_records/analysis_jobs/insights） | 827 | → 2479 |
| `46bba0e` | `storage.rs` → `src/storage/mod.rs` | — | — |

**关键操作点**：
- `Store` 字段 `connection`/`path` 提为 `pub(crate)`，使同 crate 拆分 impl 块可访问。
- 共享行映射 `map_wiki_page`/`map_input_record` 与常量 `WIKI_PAGE_COLS` 提为 `pub(crate)`。
- 跨域自调用（如 `merge_entity` 调 `self.get_wiki_page`、事件域调 `self.get_conversation`）
  在同 crate 的多 `impl Store` 块间仍有效，无需改签名。
- 仅测试用的 `OptionalExtension`/`Transaction`/`Uuid` import 移入 `mod tests`，lib 零警告。

**最终结构**：`mod.rs` 2479 行（其中 ~1835 行为 `mod tests`，业务代码仅 ~640 行：
Store 核心 open/search_knowledge_base/StorageAdapter/Drop + 类型定义）。
113 个公开方法全部归入 7 个领域子模块。

**未做（按计划保留）**：`mod tests`（1835 行）整体留 `mod.rs`，可按域再拆 `*_tests.rs`；
`impl Clone` 的 `expect`（P1-2）单独评估。

### 验证门
- 每步后 `cargo test --all-targets` 全绿（180 项含迁移幂等/并发/不可变 trigger 测试）。
- **关键**：迁移抽取后必须确认旧库升级路径不变——已有测试覆盖（schema 迁移
  幂等 + 防中途崩溃），若拆动 open 顺序会破坏。
- `cargo build --release` + `scripts/regen.sh`（若动了 FRB 暴露的方法签名——本拆分不动签名，应无需 regen）。

### 不做
- 不改 `impl Clone` 的 `expect`（P1-2 单独评估：启动期 reopen 失败即崩，非数据损坏）。
- 不动 `StorageAdapter`/`Drop`（桥接边界，稳定）。

---

## 二、`ui/lib/widgets/wiki_page_detail_view.dart`（3854 行，~24 个 widget 类，4 个 feature）

### 现状
单文件承载：tab 条管理 / 页面详情正文（AppFlowy 编辑器集成）/ 派生产物区块 /
人物关系区块 / AI 对话面板 / 导入入口（`_urlController` 等 4 个 controller）。
142 处 `matches`/`=>`，C9 标记为巨型化。

### 拆分方案
```
ui/lib/widgets/wiki_detail/
├── wiki_page_detail_view.dart   # 主 view + tab 条（对外入口，保持原 import 路径可用）
├── wiki_content_editor.dart     # 正文 AppFlowy 编辑器封装（已被 integration test 引用）
├── wiki_ai_chat_panel.dart      # AI 对话面板（测试已按类型 find）
├── wiki_relations_section.dart  # 人物关系区块
├── wiki_derivatives_section.dart# 派生产物区块
└── wiki_import_section.dart     # 导入入口 + 4 个 controller 表单
```

### 注意
- `wiki_editor_integration_test` 已 `find.byType(WikiContentEditor)` /
  `find.byType(WikiAiChatPanel)`——**类型名与公开 API 不能变**，否则 10 个集成测试红。
- 主文件保持 `WikiPageDetailView` 为对外入口，内部转发，外部 import 不受影响。
- 编辑器/对话面板已隐约有独立边界（测试按类型定位），拆分成本低于 storage.rs。

### 验证门
- 每步 `flutter test`（170 项，含 10 个 wiki_editor_integration）+ `flutter analyze` 无新增告警。
- 保持公开类型名稳定；仅移动实现。

---

## 建议排期
- 优先级：**storage.rs migrations 抽取 > wiki 详情拆分 > storage.rs 业务域拆分**。
- storage migrations 抽取是性价比最高的第一步（单点收益大、风险低、有测试兜底）。
- 两者都建议在无并行活跃改动时各安排一个独立批次，避免与 feature 开发交错。
