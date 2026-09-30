# Agent Note: 周期统计报告与目标偏差检测的实现路径

Status: proposed

关联需求：[FR-PES-005 目标与偏差检测](../../../requirements/product/FR-PES-005-目标与偏差检测.md)（FR-PES-005-04 偏差检测、FR-PES-005-05 评估可观察性）

## Problem

FR-PES-005-04 要求系统周期性评估「用户近期实际在做的事」与活跃目标之间的偏差。当前状态：

- ✅ `goals` 表与 `list_active_goals()` 已实现（迁移 v2）
- ✅ 事件分析管线已提取 `event_type`、`people`、`projects`、`activities` 存入 `event_analyses.result_json`
- ✅ Flutter 5s timer 驱动 `triggerAnalysis()` + `triggerKnowledgeDigest()` 的后台模式已验证
- ❌ 没有「周期性统计」机制——用户问「这个月散步了几次」时 AI 无法精确回答
- ❌ 没有「偏差检测」——统计结果不会自动与目标对照

用户场景举例：记录「今天吃完饭出去走了一圈」→ AI 应识别为运动习惯；用户问「这个月散步了几次」→ 系统应给出精确统计而非 AI 估算；更进一步，系统应定期（每周/每月）生成统计报告，并检测「行为与目标的偏差」（比如目标是「每周运动 3 次」但实际 0 次）。

## 核心判断

**统计与偏差检测是两个问题，不该混为一谈。**

| | 统计（本地） | 偏差检测（AI） |
|---|---|---|
| 输入 | `event_analyses.result_json` 的结构化字段 | 统计结果 + 活跃目标 |
| 输出 | 精确数字（次数、分布、频率） | 可溯源的判断（「0 次运动 vs 目标 3 次」） |
| 成本 | 零 token，SQL 聚合 | 一次 AI 调用（有界上下文） |
| 失败影响 | 不可能失败（本地查询） | 失败不影响统计与对话 |
| 可重复性 | 确定性 | 可错建议，允许用户认为它判断错了 |

这个分离直接对应 FR-PES-005 的两条纪律：「宁缺毋滥」（没有真正值得说的偏差时返回空结果）与「不重复已给过的判断」（同一偏差已提过且情况未变时不重复输出）——前者要求 AI 判断，后者要求本地去重。

## Proposal

### 1. 通用统计工具：`query_event_stats`

不穷举统计类型（「散步几次」「熬夜多吗」「和老张聊了几次」无法枚举），而是给 AI **一个工具，参数就是查询条件**。AI 把用户意图翻译成结构化参数，本地 SQL 精确执行。

```json
{
  "tool": "query_event_stats",
  "arguments": {
    "date_range": "this_month",
    "event_type": "运动",
    "activity_contains": "散步",
    "group_by": "day"
  }
}
```

参数设计（可组合）：

| 参数 | 类型 | 说明 |
|---|---|---|
| `date_range` | string | `"this_week"`, `"this_month"`, `"last_7_days"`, `"last_30_days"`, `"2026-09"` |
| `event_type` | string? | 事件类型过滤（`result_json.event_type`） |
| `activity_contains` | string? | 活动关键词（`result_json.activities` 数组内匹配） |
| `person` | string? | 涉及的人（`result_json.people` 数组内匹配） |
| `project` | string? | 涉及的项目（`result_json.projects` 数组内匹配） |
| `text_contains` | string? | 原文关键词（`events.raw_text` 模糊匹配） |
| `group_by` | string | `"day"`, `"week"`, `"event_type"`, `"person"`, `"project"`, `"activity"` |

SQL 直接从 `event_analyses.result_json` 用 `json_extract` 聚合，不需要改 schema。

返回格式（AI 拿到后包装成自然语言）：

```json
{
  "total": 12,
  "period": "2026-09-01 ~ 2026-09-30",
  "grouped": [
    {"key": "09-01", "count": 1},
    {"key": "09-03", "count": 2}
  ],
  "samples": [
    {"date": "09-28", "text": "今天吃完饭出去走了一圈"}
  ]
}
```

**前置条件**：分析 prompt 需加一句引导——`activities` 提取具体活动名称，尽量用常见动词（散步、跑步、游泳、阅读），不要用描述性短语。这样 `activity_contains: "散步"` 才能匹配到。这是 prompt 微调，不是架构改动。

### 2. 周期统计报告：`generate_periodic_report`

新增 Rust 函数，本地聚合生成结构化报告，**不需要 AI**：

```rust
pub struct PeriodicReport {
    pub period: String,                        // "2026-W40" / "2026-09"
    pub period_type: PeriodType,               // Weekly / Monthly
    pub generated_at: DateTime<Utc>,
    pub event_count: i64,
    pub by_event_type: Vec<(String, i64)>,     // [("运动", 5), ("饮食", 12)]
    pub by_activity: Vec<(String, i64)>,       // [("散步", 3), ("跑步", 2)]
    pub by_person: Vec<(String, i64)>,         // [("老张", 4)]
    pub by_project: Vec<(String, i64)>,        // [("elsewhen", 8)]
    pub daily_distribution: Vec<(String, i64)>, // [("09-28", 2), ...]
    pub active_goals: Vec<GoalSnapshot>,       // 当时的活跃目标快照
}

pub struct GoalSnapshot {
    pub phase: String,    // "near" / "mid" / "long"
    pub content: String,  // 目标原文
}
```

`GoalSnapshot` 内联保存目标文本快照（FR-PES-005-03 要求：评估记录必须能说明「当时是对着哪个目标判的」，不依赖后续可能变化的表状态回溯）。

### 3. 触发机制：复用 Flutter timer

Flutter 的 `_analysisTimer` 每 5 秒跑一次 `_wakeAnalysisWorker`，在其中加：

```dart
// 检查是否需要生成本周/本月报告
if (_shouldGenerateReport()) {
  final report = await api.generatePeriodicReport(period: 'this_week');
  await _storeReport(report); // 存到 reports 表或注入对话
}
```

`_shouldGenerateReport()` 判断逻辑：
- 每周一早 8 点后，且本周还没生成过周报
- 每月 1 号早 8 点后，且本月还没生成过月报
- 用 `last_weekly_report_at` / `last_monthly_report_at` 存 shared_preferences（或 `settings` 表）

**复用现有 timer 而非新增调度器**，与 knowledge digest 的模式一致（FR-PES-005-05：周期性自动执行，无手动触发入口）。

### 4. 偏差检测：`detect_deviation`

报告生成后，**不自动推送**，而是存到 `deviation_reports` 表。用户下次打开对话时，如果有未读报告，在「今天」状态栏显示提醒（与 FR-PES-005-01 的目标入口同处）。

偏差检测本身是一次 AI 调用，输入：
- 本周期的 `PeriodicReport`（结构化统计）
- 当时的活跃目标快照（已内联在报告里）
- 历史偏差记录（用于去重：同一偏差已提过且情况未变时不重复）

AI prompt 核心约束：

```
你是目标偏差检测器。输入是用户本周/本月的实际行为统计与活跃目标。

纪律：
1. 宁缺毋滥：没有真正值得说的偏差时返回空数组，不为凑数产出判断。
2. 可溯源：每条偏差必须指向具体事件类型/活动/人/项目，不得输出无证据的断言。
3. 不重复：如果同一偏差已在历史记录中提过且情况未变，不重复输出。
4. 不评判：只陈述事实与目标的差距，不给出「应该怎么做」的建议。

返回 JSON 数组，每项：
{
  "goal_content": "目标原文",
  "deviation_type": "no_progress" | "insufficient_frequency" | "direction_mismatch",
  "evidence": "具体证据（引用统计数据）",
  "severity": "low" | "medium" | "high"
}
```

### 5. 存储：`deviation_reports` 表

```sql
CREATE TABLE deviation_reports (
  id TEXT PRIMARY KEY,
  period TEXT NOT NULL,              -- "2026-W40" / "2026-09"
  period_type TEXT NOT NULL,         -- "weekly" / "monthly"
  generated_at TEXT NOT NULL,
  report_json TEXT NOT NULL,         -- PeriodicReport 序列化
  deviations_json TEXT,              -- AI 输出的偏差数组，NULL 表示尚未检测
  detected_at TEXT,                  -- 偏差检测时间
  read_at TEXT,                      -- 用户查看时间
  created_at TEXT NOT NULL
);
```

`report_json` 内联目标快照，`deviations_json` 存 AI 输出。`read_at` 用于「今天」状态栏的未读提醒。

### 6. 呈现：复用「今天」状态栏

FR-PES-005-01 已定目标管理入口挂在对话区顶部「今天」状态栏（`_NowStatus`），与「草稿」「待办」并列。偏差报告的未读提醒复用同一处：

```
[今天 09-30] [草稿 2] [待办 3] [目标 2] [📊 周报]
```

点击「📊 周报」展开报告详情，标记 `read_at`。

## 为什么这样设计

| 决策 | 理由 |
|---|---|
| 统计本地算，不用 AI | 精确、零成本、可复现；AI 数不准、会编 |
| 偏差检测用 AI | 「每周运动 3 次」vs「实际 0 次」的比较需要理解语义 |
| 报告存表，不自动推送 | 用户主动查看，不打扰；FR-PES-005「不做系统推送」 |
| 复用现有 timer | 不需要新的调度机制；与 knowledge digest 模式一致 |
| 目标快照内联 | FR-PES-005-03 要求可溯源 |
| AI 输出去重 | FR-PES-005-04「不重复已给过的判断」 |
| 通用统计工具 | 不穷举统计类型，AI 翻译意图为参数 |

## 待决问题

| 编号 | 问题 | 候选 |
|---|---|---|
| Q1 | 报告呈现方式 | A. 存表 + 「今天」状态栏提醒（本方案）；B. 自动创建对话；C. 写入待办。倾向 A，不打扰且可溯源 |
| Q2 | 偏差检测触发时机 | A. 报告生成后立即检测（本方案）；B. 用户查看报告时才检测。倾向 A，用户看到的是完整结果 |
| Q3 | 历史偏差去重的「情况未变」如何判断 | A. 简单字符串比较（goal_content + deviation_type）；B. AI 判断。倾向 A，确定性 |
| Q4 | 周报/月报的生成时间点 | 周一早 8 点 / 每月 1 号早 8 点。进程未运行时延迟到下次启动后第一次 timer tick |

## 实现量估算

| 文件 | 改动 |
|---|---|
| `src/ai/tool/mod.rs` | 加 `QueryEventStatsTool`（~120 行） |
| `src/ai/report.rs`（新） | `generate_periodic_report()` SQL 聚合（~150 行） |
| `src/ai/deviation.rs`（新） | `detect_deviation()` AI 调用 + prompt（~100 行） |
| `src/storage/report.rs`（新） | `deviation_reports` 表 CRUD（~80 行） |
| `migrations/04-deviation-reports/up.sql` | 建表（~20 行） |
| `src/api/mod.rs` | 暴露 `generate_periodic_report`、`list_deviation_reports`、`mark_report_read` 到 bridge（~50 行） |
| `ui/lib/bridge/rust_bridge_repository.dart` | `_wakeAnalysisWorker` 里加报告触发（~30 行） |
| `ui/lib/widgets/message_area.dart` | 「今天」状态栏加「📊 周报」入口（~80 行） |

总计 ~630 行，不改现有 schema（只加新表），不改分析管线（只调 prompt）。

## 验收场景

| 场景 | 输入/条件 | 预期 |
|---|---|---|
| 用户问「这个月散步了几次」 | AI 调用 `query_event_stats` | 返回精确次数与日期分布 |
| 周一早 8 点后首次打开 app | 上周有事件记录 | 自动生成周报，「今天」状态栏显示「📊 周报」 |
| 目标是「每周运动 3 次」但实际 0 次 | 周报生成后 | 偏差检测输出「运动频率不足」，指向 `by_event_type["运动"] = 0` |
| 同一偏差连续两周 | 第二周周报 | 不重复输出该偏差 |
| 进程一周未运行 | 下次启动 | 延迟生成本周周报，不遗漏 |
| 用户查看报告 | 点击「📊 周报」 | 展开详情，标记已读 |
