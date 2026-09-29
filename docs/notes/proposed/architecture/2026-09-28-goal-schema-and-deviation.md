# Agent Note: 目标表设计与偏差检测的落地路径

Status: proposed

关联需求：[FR-PES-005 目标与偏差检测](../../../requirements/product/FR-PES-005-目标与偏差检测.md)

## Problem

目标要成为一级对象，先决条件是它有地方存。仓库当前没有 `goals` 表，本轮又刚把迁移体系从 v1–v31 版本链换成 `schema.sql` 加 `rusqlite_migration`（commit `6dbc582`），因此「目标表怎么加」这个问题同时受两个约束：

- 用户要求未来所有表结构变更通过迁移实现，不直接改 schema 基线；
- 目标有活跃上限 3 条与历史保留的双重要求，单纯 `CREATE TABLE` 不足以表达。

另外两个实施前的发现直接影响方案选择，见「实施前发现的既有事实」。

## Proposal

### 1. 迁移正文放 `migrations/` 目录；`01-baseline/up.sql` 冻结为基线

迁移正文走 `rusqlite_migration` **官方约定**：仓库根的 `migrations/` 目录，每个版本一个子目录 `{序号}-{名字}/up.sql`（需要回滚时同目录加 `down.sql`，本仓库不用）。`01-baseline/up.sql` 即原先的 `schema.sql`，仍是版本 1 基线。目录用 `include_dir!` 编译期嵌入，再交给 `Migrations::from_directory` 扫描——运行时不依赖磁盘上的 `migrations/`，六种安装包不用多带一份资源文件。

**这个约定有一个必须写下来的陷阱**：修改 `migrations/01-baseline/up.sql` 对已有数据库完全没有效果。`to_latest` 读到 `user_version = 1` 后判定已是最新，直接返回，不重跑版本 1。后果是后来者编辑基线文件增加表或列时，新库正常、老库静默缺字段，且不会有任何报错。

`migrations/01-baseline/up.sql` 现有 27 处裸 `CREATE TABLE`、零 `IF NOT EXISTS`，这在「只跑一次」的语义下是正确的，不要为了防御性而批量加 `IF NOT EXISTS`——那会掩盖上面那个陷阱，而不是解决它。

`from_directory` 消灭了另一方向的风险：**「新增了迁移却忘了登记」在官方方案里不可能发生**，因为根本没有手工登记表。代价是校验从编译期挪到了运行期，失误要等 app 启动才暴露。实测过它的两处校验都给出可操作的报错：序号跳号 → `Migration ids must be consecutive numbers`；子目录缺 `up.sql` → `Missing upward migration file for migration 03-y`。`migrations_validate()`（crate 官方的内置自检，把全部 up 迁移在内存库上从头跑一遍）保证这类错在 `cargo test` 阶段就炸。

### 2. 目标表作为版本 2 迁移追加

```
migrations/
├── 01-baseline/up.sql    冻结基线（原 schema.sql）
└── 02-goals/up.sql       目标表与两个上限触发器
```

```rust
static MIGRATION_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/migrations");

fn migrations() -> Result<Migrations<'static>> {
    Ok(Migrations::from_directory(&MIGRATION_DIR)?)
}
```

`goals` 表 SQL（`migrations/02-goals/up.sql`）：

```sql
CREATE TABLE goals (
  id TEXT PRIMARY KEY,
  content TEXT NOT NULL CHECK(length(trim(content)) > 0),
  phase TEXT NOT NULL CHECK(phase IN ('near','mid','long')),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','superseded')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  superseded_at TEXT
);
CREATE INDEX idx_goals_status ON goals(status);
```

**没有 `superseded_by`**（早期草稿里有，实施时删掉）：归档与新增是两个独立操作，中间不保证成对发生，指向新目标的链接经常悬空或干脆指不出来；且全仓没有任何消费方去读它。一个无人读取、还可能撒谎的列，比没有更糟。

`phase` 故意**不加唯一索引**。用户要的是「某个时间点最多 3 条」，总量封顶；「不要求 3 个时间点都有」意味着阶段可空缺。加了 `UNIQUE(phase)` 就变成每阶段各限 1 条，是另一种约束，需另行裁决。

`status` 只用 `active` 与 `superseded` 两值，沿用 `todos`（`open/done/archived`）与 `rules`（`active/pending`）既有的软状态风格，不引入删除态。

### 3. 活跃上限用触发器保证，不在 Rust 里数

```sql
CREATE TRIGGER goals_cap_active_insert
BEFORE INSERT ON goals WHEN NEW.status = 'active'
BEGIN
  SELECT CASE WHEN (SELECT COUNT(*) FROM goals WHERE status='active') >= 3
    THEN RAISE(ABORT, 'active_goal_limit_reached') END;
END;

CREATE TRIGGER goals_cap_active_update
BEFORE UPDATE OF status ON goals
WHEN NEW.status='active' AND OLD.status <> 'active'
BEGIN
  SELECT CASE WHEN (SELECT COUNT(*) FROM goals WHERE status='active') >= 3
    THEN RAISE(ABORT, 'active_goal_limit_reached') END;
END;
```

理由：应用层 `count + insert` 是 check-then-insert 竞态，而仓库已有两处同类竞态的修复记录（事务竞态批次）。触发器在 SQLite 内与写入原子。写法沿用 `migrations/01-baseline/up.sql` 中既有的 `prevent_raw_event_mutation`（`RAISE(ABORT, ...)`）。

需要两个触发器而不是一个：新增走 INSERT 路径，但复活历史目标走 UPDATE 路径，只挡 INSERT 会漏掉后者。

`UPDATE` 触发器的 `WHEN` 必须含 `OLD.status <> 'active'`，否则编辑一条已活跃目标的正文（状态不变）也会被计入并误判为第四条。

### 4. 目标进对话 Prompt，但必须带反向护栏

目标注入 `build_system_prompt`（`src/ai/memory.rs`），放在个人规则段之前。注入本身是显然的，风险全在措辞。

`rules` 段的既有措辞是「回复时对照检查」——那是行为约束，要求 AI 逐条比对。目标若沿用同一措辞，对话里每轮都会拿用户随口说的话去比目标，偏差检测就退化成实时审计：用户吐槽一个技术细节，AI 判定「这与你三个月内发 1.0 的目标无关」。

所以目标段的措辞必须反向：仅作理解意图的背景，相关才用；不评判是否偏离；不把对话引向目标。偏差检测是后台周期任务的职责，它带「不重复 / 宁缺毋滥」纪律；对话里这遍更频繁，若两处都做，同一件事被做两遍且更吵的那遍赢。

成本约 198 token/轮，可忽略。

### 5. 偏差检测复用 `src/ai/insight.rs`，不新写分析器

`insight.rs` 已是偏差检测的近完整原型：输入最近 N 天事件 + wiki 页面 + 已输出过的洞察，输出结构化 JSON 且每条带 `source_slugs` 与 `related_events` 溯源。其提示词里已写死两条对本需求至关重要的纪律——「结合已输出过的洞察递进：不重复已给过的认知」与「宁缺毋滥，最多输出 3 条」。

这两条正是「周期性偏差提醒」这类系统最容易死掉的地方：若每天重复同一句「你的行为与目标有偏差」，用户两周内就会屏蔽。复用而非重写，是因为纪律写在提示词里，重写等于重新学一遍。

实施时新增 `goal_assessments` 表（对应既有 `knowledge_digest_runs` 的记录角色），逐条评估内联保存当时的活跃目标快照，满足 FR-PES-005-03 的可解释要求。

## 验证证据

以下为实测结果，非推断。

**迁移只跑一次且原子**（`rusqlite_migration` 2.5.0 `src/lib.rs:713` `goto` 与 `:618` `goto_up`）：`to_latest` 读 `user_version`，`goto_up` 以 `for v in current_version..target_version` 推进，全部迁移包在单个 `conn.transaction()` 中提交。结论：v2 无需 `IF NOT EXISTS`，且任一迁移失败则整体回滚。

**触发器行为**（`sqlite3` 逐条实测）：

| 场景 | 结果 |
|---|---|
| 第 4 条活跃目标 INSERT | `active_goal_limit_reached` 拒绝 |
| 3 条满时复活历史目标（UPDATE 路径） | 拒绝，历史行保持 `superseded` |
| 编辑活跃目标正文 | 不误触发，改写成功 |
| 归档一条后新增 | 成功，活跃集换血 |
| 历史保留 | `superseded` 行内容与阶段完整 |
| 同阶段多条 | 允许（`near:near:long` 实测通过） |
| **对照：删除 `goals_cap_active_update`** | **活跃数变为 4，上限被突破** |

最后一行是变异测试，证明两个触发器都承重，不是冗余保险。前两轮测试设计有误（误选已活跃行、激活前只计到 2 条），均未真正触达 UPDATE 路径，重做后才测到。

`goals` 经实测可直接作表名，非 SQLite 保留字。

## 实施前发现的既有事实

两条与本需求相邻但独立的事实，建议单独处理：

1. **`src/ai/insight.rs::generate_insights` 是死代码**。全仓零调用点——无 Rust 调用、无 FRB 导出、Dart 侧亦无。它 `pub mod` 声明、编译进产物，但没有任何入口能触发。这意味着 FR-PES-004 所称「阶段 1 起事件消化由应用后台自动运行」对 digest 成立（`trigger_knowledge_digest` 已接线），对洞察不成立。复用它做偏差检测顺带解决死代码，但要意识到那是接线而非从零实现。

2. **`new_migrations.rs` 第 2 行注释已失实**：「Historical databases continue through `migrations.rs`」，而 `migrations.rs` 已被同一个 commit `6dbc582` 删除。属该次重构留下的自相矛盾注释，一行可修，但需用户确认是否计入代码改动。

## Alternatives considered

- **复用 `rules` 表存目标**：否决。`rules` 是行为约束语义，`list_active_rules()` 会把目标当作「回复时对照检查」的行为准则；且 `rules` 无任何时间字段，无法表达阶段。
- **目标表硬性只留 3 行，改目标即覆盖**：否决。违反 ARCH-002 §3「不原地覆盖不可变旧快照」，且三个月后无法解释一条历史偏差当初对照的是哪句目标表述。
- **上限满时自动顶掉同阶段目标**：否决。用户写好的目标无声消失，与人工编辑保护、来源快照不可变同属一条原则。
- **新增 `vector` / 引入向量检索做目标匹配**：否决，非目标已排除强制向量数据库；且目标上限 3 条，有界匹配足够。
- **偏差结论写入 `todos`**：暂不采纳，语义不同（用户承诺 vs AI 判断），混入用户自己的待办列表易被忽略。待 FR-PES-005 Q1 裁决。

## Risks

- **基线文件冻结约定靠人遵守**。技术手段无法阻止后来者编辑它，且失败模式是静默的。缓解只能靠本文与 `new_migrations.rs` 注释显式声明，实测无自动检测手段。
- **偏差评估的误报**是最大风险。用户正在推进目标但近期没聊到它，是最可能的假阳性，也是最招人烦的一种。FR-PES-005-06 验收场景已列该场景，实现时不得以「时间久没提及」替代「有无推进证据」。
- **目标表与 FR-PES-005 Q3（是否成为一级导航）耦合**。Q3 已裁决：不新增一级导航，管理入口挂在对话区「今天」状态栏。代价是该入口是唯一入口，**0 目标时也必须可见**（显示为 accent 色的「目标」，不显示计数），否则功能不可发现。窄屏（实测 360px）第一行放不下三个带文案的按钮，窄屏下入口改落第二行。
- **评估记录内联目标快照**会使记录随目标改写而重复占用空间；目标最多 3 条、单条为短文本，量级可接受，但若将来放宽上限需重新评估。
- 现有数据库无法打开（基线文件无 `IF NOT EXISTS`，而历史库 `user_version=0`），按既定决策走导出/导入重建，不在本轮处理。

## 实施进展（本轮已落地）

FR-PES-005-01 与 -02 已实现，-03（偏差检测）未动。

- 迁移层 `src/storage/new_migrations.rs`：v1 冻结基线 + v2 `GOALS_V2`（`goals` 表、`idx_goals_status`、两个上限触发器），6 个测试。
- 存储层 `src/storage/goals.rs`：CRUD 与错误转译（`map_write_error` 把 `active_goal_limit_reached` 翻成中文，避免下划线标识冒到界面），9 个测试。
- API 层 `src/api/goals.rs`：`GoalDto` + 6 个 FRB 函数。
- Prompt 注入 `src/ai/memory.rs::build_system_prompt`，5 个测试（含反向护栏存在性、已归档不注入、与规则段措辞分离）。
- Dart 侧 `models/goal.dart`、`providers/goal_provider.dart`、`widgets/goal_view.dart`、入口接线在 `message_area.dart` 的 `_NowStatus`，18 个测试。

界面侧对上限的处理是**两道防线而非一道**：UI 在满 3 条时预先禁用新增并说明原因（提前告知），数据库触发器才是最终权威。测试两处都覆盖——删掉触发器会失败 1 个 Rust 测试，把 `atCap` 写成常量 `false` 会失败 1 个 widget 测试。

