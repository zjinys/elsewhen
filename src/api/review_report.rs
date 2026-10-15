//! 周期报告管道 + 偏差检测（R3，FR-PES-005-04）。
//!
//! 分层：报告数字来自本地 SQL 实时聚合（R2 聚合层同一实现），AI 只做
//! 「统计与目标之间有没有值得说的偏差」的语义判定——统计本地算，偏差
//! 才用 AI（FR-PES-005 不变式 1）。
//!
//! 触达语义（对 Q4 裁决与验收场景「进程一周未开 → 启动后补做本周报告，
//! 历史空窗不补」的调和）：每个 tick 保证**最近一个完整周期**有报告
//! （若非空且缺失则补做），生成时刻的下限是周期结束后的第一个 08:00；
//! 只补这一个，更早的空窗不追补。UNIQUE(period, period_type) 让补做可以
//! 在任意 tick 安全重试。
//!
//! 生成与检测独立状态：报告先落库（可读），检测失败或 Provider 未配置
//! 都只是把 `deviations_json` 留在 NULL（「尚未检测」，合法状态），退避
//! 重试由同一条 5s timer worker 承担——与知识消化同模式。

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use uuid::Uuid;

use super::*;

/// 报告结构版本：统计字段演进时递增，消费方（R4 呈现）按版本解析。
pub(crate) const REPORT_SCHEMA_VERSION: &str = "deviation-report-v1";

/// 偏差类型枚举（closed set，逐字校验；R4 呈现做中文化映射）。
pub(crate) const DEVIATION_TYPES: [&str; 4] = ["no_progress", "stalled", "conflict", "scope_drift"];
/// 严重度枚举。
pub(crate) const SEVERITIES: [&str; 3] = ["low", "medium", "high"];
/// 检测退避基数与上限（连续失败时指数退避，防每 5s 打一次 provider）。
const DETECT_BACKOFF_BASE_MS: i64 = 5 * 60 * 1000;
const DETECT_BACKOFF_MAX_MS: i64 = 60 * 60 * 1000;

// ── 窗口计算 ─────────────────────────────────────────────────────────

/// 报告窗口：period = 周期末日（周=周日、月=月末），本地日界含两端。
pub(crate) fn report_window(
    period_type: &str,
    period: NaiveDate,
) -> Result<(NaiveDate, NaiveDate)> {
    match period_type {
        "week" => Ok((period - Duration::days(6), period)),
        "month" => {
            let start =
                NaiveDate::from_ymd_opt(period.year(), period.month(), 1).context("月窗口起点")?;
            Ok((start, period))
        }
        other => anyhow::bail!("未知 period_type：{other}"),
    }
}

/// 最近一个完整周期（period 锚 = 周期末日）。
/// 今天周一 → 昨天是上周日；其他日子 → 同理取最近一个已过去的周日/上月末。
fn latest_complete_week(today: NaiveDate) -> NaiveDate {
    today - Duration::days((today.weekday().num_days_from_monday() as i64) + 1)
}

fn latest_complete_month(today: NaiveDate) -> NaiveDate {
    // 上个月末 = 本月 1 日的昨天。
    let first = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).expect("本月 1 日");
    first - Duration::days(1)
}

fn gate_is_passed(now: DateTime<Local>, day: NaiveDate) -> bool {
    // 周期结束次日的 08:00（本地）是生成时刻下限；时区跳跃日取不到该时刻
    // 时保守跳过本 tick，下个 tick 自然重试。
    match day
        .and_hms_opt(8, 0, 0)
        .map(|naive| Local.from_local_datetime(&naive))
    {
        Some(chrono::LocalResult::Single(gate)) => now >= gate,
        _ => false,
    }
}

/// 当前时刻已应生成、且仅限「最新完整周期」的待补报告清单。
/// `now` 为本地时刻（生成时刻下限：周期结束次日 08:00 后）。
pub(crate) fn due_reports(now: DateTime<Local>) -> Vec<(&'static str, NaiveDate)> {
    let today = now.date_naive();
    let mut due = Vec::new();
    let week = latest_complete_week(today);
    if gate_is_passed(now, week + Duration::days(1)) {
        due.push(("week", week));
    }
    let month = latest_complete_month(today);
    if gate_is_passed(now, month + Duration::days(1)) {
        due.push(("month", month));
    }
    due
}

// ── 报告结构（report_json 载荷）──────────────────────────────────────

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ReportGoalSnapshot {
    pub id: String,
    pub phase: String,
    pub content: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ReportStats {
    pub total: i64,
    pub todo_done_count: i64,
    pub daily: Vec<(String, i64)>,
    pub by_event_type: Vec<(String, i64)>,
    pub by_person: Vec<(String, i64)>,
    pub by_project: Vec<(String, i64)>,
    pub by_activity: Vec<(String, i64)>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct PeriodReport {
    pub schema_version: String,
    pub period_type: String,
    pub period: String,
    pub start_date: String,
    pub end_date: String,
    pub stats: ReportStats,
    pub goals: Vec<ReportGoalSnapshot>,
}

fn fmt_date(d: NaiveDate) -> String {
    d.format("%F").to_string()
}

/// 报告是给人看与给 AI 看的概貌：每组截 top 12（与 R1 拼装同款截断）。
fn top_groups(groups: Vec<(String, i64)>) -> Vec<(String, i64)> {
    groups.into_iter().take(12).collect()
}

fn active_goal_snapshots(store: &Store) -> Result<Vec<ReportGoalSnapshot>> {
    let mut statement = store.connection.prepare(
        "SELECT id, phase, content FROM goals WHERE status='active'
         ORDER BY created_at ASC, id ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(ReportGoalSnapshot {
            id: row.get(0)?,
            phase: row.get(1)?,
            content: row.get(2)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

/// 生成报告（不落库）；空报告（0 可记录事件）返回 None——Q4 裁决：
/// 空窗做不出偏差判定，也不该在回顾页堆一个空心报告，跳过不建。
fn build_period_report(
    store: &Store,
    period_type: &str,
    period: NaiveDate,
) -> Result<Option<PeriodReport>> {
    let (start, end) = report_window(period_type, period)?;
    let no_filter = super::event_stats::EventStatsQuery::default();
    let overall = super::event_stats::query_event_stats(store, start, end, &no_filter, "none")?;
    if overall.total == 0 {
        return Ok(None);
    }
    let daily = super::event_stats::query_event_stats(store, start, end, &no_filter, "day")?;
    let by_type =
        super::event_stats::query_event_stats(store, start, end, &no_filter, "event_type")?;
    let by_person = super::event_stats::query_event_stats(store, start, end, &no_filter, "person")?;
    let by_project =
        super::event_stats::query_event_stats(store, start, end, &no_filter, "project")?;
    let by_activity =
        super::event_stats::query_event_stats(store, start, end, &no_filter, "activity")?;

    // 生成时刻的活跃目标快照（phase + content），偏差逐字校验锚定在它上面。
    let goals = active_goal_snapshots(store)?;
    Ok(Some(PeriodReport {
        schema_version: REPORT_SCHEMA_VERSION.to_string(),
        period_type: period_type.to_string(),
        period: fmt_date(period),
        start_date: fmt_date(start),
        end_date: fmt_date(end),
        stats: ReportStats {
            total: overall.total,
            todo_done_count: store.todos_done_between(start, end)?,
            daily: daily.grouped,
            by_event_type: top_groups(by_type.grouped),
            by_person: top_groups(by_person.grouped),
            by_project: top_groups(by_project.grouped),
            by_activity: top_groups(by_activity.grouped),
        },
        goals,
    }))
}

// ── 检测：prompt、校验、去重 ──────────────────────────────────────────

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub(crate) struct DeviationCandidate {
    pub goal_content: String,
    pub deviation_type: String,
    pub evidence: String,
    pub severity: String,
}

fn build_deviation_prompt(report: &PeriodReport, seen: &[(String, String)]) -> String {
    let mut prompt = String::new();
    prompt.push_str("你是周期回顾助手。对照用户的活跃目标与下面的行为统计，判断是否存在真正值得说的行为偏差。输出 JSON 数组（禁止 Markdown 代码块围栏），每项：{\"goal_content\":\"…\",\"deviation_type\":\"…\",\"evidence\":\"…\",\"severity\":\"…\"}。无偏差输出 []。\n");
    prompt.push_str("红线：\n");
    prompt.push_str("1. 宁缺毋滥：没有值得说的偏差就输出空数组；不为凑数报偏差；「用户没有目标相关记录」只有在该目标明确要求推进时才算偏差。\n");
    prompt.push_str("2. 不评判、不指路：只陈述事实与目标的差距，不给建议、不给评分、不展望。\n");
    prompt.push_str("3. 证据必须引用统计里的具体数字或条目，不得编造。\n");
    prompt.push_str("4. goal_content 必须逐字取自下方目标快照。\n");
    prompt.push_str(&format!(
        "5. deviation_type 枚举：{}。severity 枚举：{}。\n",
        DEVIATION_TYPES.join(" / "),
        SEVERITIES.join(" / ")
    ));
    if !seen.is_empty() {
        prompt.push_str(
            "此前已报告过的偏差（同目标同类型不得重复输出，即使有新证据——重复一律省略）：\n",
        );
        for (goal, kind) in seen.iter().take(20) {
            prompt.push_str(&format!("- {goal}：{kind}\n"));
        }
    }
    prompt.push_str(&format!(
        "\n统计（{} {} ~ {}）：\n",
        report.period_type, report.start_date, report.end_date
    ));
    prompt.push_str(&format!(
        "- 可记录事件 {} 条；完成待办 {} 条。\n",
        report.stats.total, report.stats.todo_done_count
    ));
    for (groups, title) in [
        (&report.stats.by_event_type, "事类"),
        (&report.stats.by_person, "人物"),
        (&report.stats.by_project, "项目"),
        (&report.stats.by_activity, "活动"),
    ] {
        if groups.is_empty() {
            continue;
        }
        let items = groups
            .iter()
            .map(|(k, v)| format!("{k}×{v}"))
            .collect::<Vec<_>>()
            .join("、");
        prompt.push_str(&format!("- {title}: {items}\n"));
    }
    prompt.push_str("\n活跃目标快照（生成报告时点）：\n");
    if report.goals.is_empty() {
        prompt.push_str("（无活跃目标。没有目标就没有「偏差」，直接输出 []。）\n");
    } else {
        for goal in &report.goals {
            prompt.push_str(&format!("- [{}] {}\n", goal.phase, goal.content));
        }
    }
    prompt
}

fn fold_eq(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

fn strip_code_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    let without_open = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    without_open
        .strip_suffix("```")
        .unwrap_or(without_open)
        .trim()
}

fn preview(raw: &str) -> String {
    raw.chars().take(80).collect()
}

/// 结构化输出逐字校验 + 确定性去重（不变式 3：不用 AI 判断重复）。
/// 校验失败 = 整批拒绝（roadmap：校验失败整批不入、不确认、按退避重试）。
pub(crate) fn validate_deviation_candidates(
    raw: &str,
    goals: &[ReportGoalSnapshot],
    seen: &[(String, String)],
) -> Result<Vec<DeviationCandidate>> {
    let stripped = strip_code_fence(raw);
    let list: Vec<serde_json::Value> = serde_json::from_str(stripped)
        .with_context(|| format!("偏差输出不是 JSON 数组：{}", preview(stripped)))?;
    let mut valid = Vec::new();
    for (index, item) in list.iter().enumerate() {
        let field = |name: &str| -> Result<String> {
            item.get(name)
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .with_context(|| format!("第 {index} 项缺少有效字段 {name}"))
        };
        let goal = field("goal_content")?;
        let kind = field("deviation_type")?;
        let evidence = field("evidence")?;
        let severity = field("severity")?;
        if !DEVIATION_TYPES.contains(&kind.as_str()) {
            anyhow::bail!("第 {index} 项 deviation_type 不在枚举内：{kind}");
        }
        if !SEVERITIES.contains(&severity.as_str()) {
            anyhow::bail!("第 {index} 项 severity 不在枚举内：{severity}");
        }
        // goal_content 逐字（trim 后）必须命中生成时刻的活跃目标快照，防编造。
        if !goals.iter().any(|g| g.content.trim() == goal) {
            anyhow::bail!("第 {index} 项 goal_content 不在活跃目标快照内（逐字要求）：{goal}");
        }
        if evidence.chars().count() > 400 {
            anyhow::bail!("第 {index} 项 evidence 超长（>400 字符）");
        }
        valid.push(DeviationCandidate {
            goal_content: goal,
            deviation_type: kind,
            evidence,
            severity,
        });
    }
    // 确定性去重：同 (goal_content, deviation_type) 不得与历史或本批重复。
    let mut out = Vec::new();
    for candidate in valid {
        let goal = candidate.goal_content.as_str();
        let kind = candidate.deviation_type.as_str();
        if seen
            .iter()
            .any(|s| fold_eq(&s.0, goal) && fold_eq(&s.1, kind))
        {
            continue;
        }
        if out.iter().any(|c: &DeviationCandidate| {
            fold_eq(&c.goal_content, goal) && fold_eq(&c.deviation_type, kind)
        }) {
            continue;
        }
        out.push(candidate);
    }
    Ok(out)
}

/// 对单份报告跑一次偏差检测。返回去重后的偏差数。
/// AI/解析失败原样上抛（tick 记账后退避重试），报告本体不受影响。
fn detect_deviation(
    store: &Store,
    report_id: &str,
    provider: &dyn crate::ai::provider::AiProvider,
) -> Result<usize> {
    let Some((period_type, report_json, existing)) = store.deviation_report_row(report_id)? else {
        anyhow::bail!("报告不存在：{report_id}");
    };
    if existing.is_some() {
        return Ok(0); // 已检测过（并发/重复调用），不重复打 provider。
    }
    let report: PeriodReport =
        serde_json::from_str(&report_json).context("report_json 解析失败")?;
    let seen = store.reported_deviation_keys(&period_type)?;
    let prompt = build_deviation_prompt(&report, &seen);
    let reply = provider.generate_reply_with_tools(
        vec![crate::ai::memory::ContextMessage::new("user", prompt)],
        None,
    )?;
    if !reply.tool_calls.is_empty() {
        anyhow::bail!("偏差检测不应触发工具调用");
    }
    let candidates = validate_deviation_candidates(&reply.content, &report.goals, &seen)?;
    let json = serde_json::to_string(&candidates)?;
    store.record_detected_deviations(report_id, &json, Utc::now())?;
    Ok(candidates.len())
}

// ── 运行账本（knowledge_background_runs 同款，task=period-report-*）──

fn start_run(store: &Store, task: &str, input_key: &str) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    store.connection.execute(
        "INSERT INTO knowledge_background_runs(id,task,input_key,status,started_at)
         VALUES (?1,?2,?3,'running',?4)",
        rusqlite::params![id, task, input_key, Utc::now().to_rfc3339()],
    )?;
    Ok(id)
}

fn finish_run(
    store: &Store,
    run_id: &str,
    status: &str,
    error: Option<&str>,
    result_count: Option<i64>,
) -> Result<()> {
    store.connection.execute(
        "UPDATE knowledge_background_runs
         SET status=?2, finished_at=?3, error=?4, result_count=?5
         WHERE id=?1",
        rusqlite::params![
            run_id,
            status,
            Utc::now().to_rfc3339(),
            error,
            result_count.unwrap_or(0)
        ],
    )?;
    Ok(())
}

// ── FRB 门面（tick；Dart worker 在知识消化之后调用）──────────────────

/// 一次后台 tick 的结果（Dart worker 只记日志/刷新，不做业务分支）。
#[derive(Clone, Debug)]
pub enum PeriodReportTickResult {
    /// 未配置 AI provider：报告仍会生成，检测等待（静默重试）
    NoProvider,
    /// 无事可做（报告齐、无待检测，或检测退避未到期）
    Idle,
    /// 新建了报告（检测由同 tick 或后续 tick 接续）
    Generated { period: String, period_type: String },
    /// 完成一次检测：deviations 为最终（去重后）条数
    Detected {
        report_id: String,
        deviations: usize,
    },
    /// 检测失败（已记账，退避后重试）
    DetectFailed { report_id: String, error: String },
}

/// 检测退避：进程内状态（不落库——重启后从头重试一次没有伤害）。
struct DetectBackoff {
    next_at_ms: std::sync::atomic::AtomicI64,
    failures: std::sync::atomic::AtomicI64,
}

fn detect_backoff() -> &'static DetectBackoff {
    use std::sync::OnceLock;
    use std::sync::atomic::AtomicI64;
    static BACKOFF: OnceLock<DetectBackoff> = OnceLock::new();
    BACKOFF.get_or_init(|| DetectBackoff {
        next_at_ms: AtomicI64::new(0),
        failures: AtomicI64::new(0),
    })
}

impl DetectBackoff {
    fn ready(&self, now_ms: i64) -> bool {
        now_ms >= self.next_at_ms.load(std::sync::atomic::Ordering::Relaxed)
    }
    fn reset(&self) {
        use std::sync::atomic::Ordering;
        self.failures.store(0, Ordering::Relaxed);
        self.next_at_ms.store(0, Ordering::Relaxed);
    }
    /// 指数退避：5min → 10 → 20 → 40 → … → 60min 封顶。
    fn bump(&self, now_ms: i64) {
        use std::sync::atomic::Ordering;
        let failures = self.failures.fetch_add(1, Ordering::Relaxed);
        let delay = (DETECT_BACKOFF_BASE_MS << failures.min(7)).min(DETECT_BACKOFF_MAX_MS);
        self.next_at_ms.store(now_ms + delay, Ordering::Relaxed);
    }
}

/// 后台 worker 调用。错误语义：
/// - 存储/guard 失败：上抛（与知识消化 tick 相同，调用方记日志）。
/// - AI/校验失败：不抛——返回 `DetectFailed`，报告仍可读，退避后重试。
pub fn tick_periodic_reports() -> Result<PeriodReportTickResult> {
    let config = crate::config::AppConfig::load()?;
    // 同分析 worker：后台路径不做启动写入，库待升级时本轮跳过。
    let Some(store) = crate::storage::Store::open_background(&config.database_path)? else {
        return Ok(PeriodReportTickResult::Idle);
    };
    let Some(_sync_guard) = crate::sync::runtime::background_guard(&config, &store)? else {
        return Ok(PeriodReportTickResult::Idle);
    };
    if !store.background_processing_allowed()? {
        return Ok(PeriodReportTickResult::Idle);
    }

    // 1) 生成：每个 tick 最多补一份（最新完整周期，幂等）。
    for (period_type, period) in due_reports(Local::now()) {
        let Some(report) = build_period_report(&store, period_type, period)? else {
            continue; // 已存在（幂等）或空窗跳过。
        };
        let json = serde_json::to_string(&report).context("报告序列化")?;
        let id = Uuid::new_v4().to_string();
        if store.insert_deviation_report(
            &id,
            NaiveDate::parse_from_str(&report.period, "%F")?,
            period_type,
            &json,
            Utc::now(),
        )? {
            return Ok(PeriodReportTickResult::Generated {
                period: report.period.clone(),
                period_type: period_type.to_string(),
            });
        }
        // insert 返回 false：并发 tick 抢先建好；下个 tick 检测接续。
    }

    // 2) 检测：最旧的一份「尚未检测」；退避未到期或无 provider 则等待。
    let now_ms = Utc::now().timestamp_millis();
    if !detect_backoff().ready(now_ms) {
        return Ok(PeriodReportTickResult::Idle);
    }
    let Some((report_id, _)) = store.pending_deviation_report_ids()?.into_iter().next() else {
        return Ok(PeriodReportTickResult::Idle);
    };
    let Some((provider, _model)) = crate::wiki::digest_provider(&store)? else {
        return Ok(PeriodReportTickResult::NoProvider);
    };
    let run_id = start_run(&store, "period-report-detect", &report_id)?;
    match detect_deviation(&store, &report_id, &provider) {
        Ok(count) => {
            finish_run(&store, &run_id, "succeeded", None, Some(count as i64))?;
            detect_backoff().reset();
            Ok(PeriodReportTickResult::Detected {
                report_id,
                deviations: count,
            })
        }
        Err(error) => {
            let message = format!("{error:#}");
            finish_run(&store, &run_id, "failed", Some(message.as_str()), None)?;
            detect_backoff().bump(now_ms);
            Ok(PeriodReportTickResult::DetectFailed {
                report_id,
                error: message,
            })
        }
    }
}


// ── R4 报告可观察性（FRB 读面）────────────────────────────────────────
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct StoredDeviation {
    goal_content: String,
    deviation_type: String,
    evidence: String,
    severity: String,
    #[serde(default)]
    dismissed: bool,
}

#[derive(Clone, Debug)]
pub struct DeviationItemDto {
    pub goal_content: String,
    pub deviation_type: String,
    pub evidence: String,
    pub severity: String,
    pub dismissed: bool,
}

#[derive(Clone, Debug)]
pub struct DeviationReportDto {
    pub id: String,
    pub period: String,
    pub period_type: String,
    pub start_date: String,
    pub end_date: String,
    pub generated_at: String,
    pub detected_at: Option<String>,
    pub read_at: Option<String>,
    pub total: i64,
    pub todo_done_count: i64,
    pub daily: Vec<DayCountDto>,
    pub by_event_type: Vec<NamedCountDto>,
    pub by_person: Vec<NamedCountDto>,
    pub by_project: Vec<NamedCountDto>,
    pub by_activity: Vec<NamedCountDto>,
    pub deviations: Vec<DeviationItemDto>,
}

fn report_dto(row: crate::storage::DeviationReportRow) -> Result<DeviationReportDto> {
    let report: PeriodReport = serde_json::from_str(&row.report_json)
        .with_context(|| format!("报告 {} 的 report_json 解析失败", row.id))?;
    let deviations = row
        .deviations_json
        .map(|raw| serde_json::from_str::<Vec<StoredDeviation>>(&raw))
        .transpose()
        .context("deviations_json 解析失败")?
        .unwrap_or_default();
    Ok(DeviationReportDto {
        id: row.id,
        period: row.period,
        period_type: row.period_type,
        start_date: report.start_date,
        end_date: report.end_date,
        generated_at: row.generated_at,
        detected_at: row.detected_at,
        read_at: row.read_at,
        total: report.stats.total,
        todo_done_count: report.stats.todo_done_count,
        daily: report
            .stats
            .daily
            .into_iter()
            .map(|(date, count)| DayCountDto { date, count })
            .collect(),
        by_event_type: into_named(report.stats.by_event_type),
        by_person: into_named(report.stats.by_person),
        by_project: into_named(report.stats.by_project),
        by_activity: into_named(report.stats.by_activity),
        deviations: deviations
            .into_iter()
            .map(|d| DeviationItemDto {
                goal_content: d.goal_content,
                deviation_type: d.deviation_type,
                evidence: d.evidence,
                severity: d.severity,
                dismissed: d.dismissed,
            })
            .collect(),
    })
}

fn into_named(groups: Vec<(String, i64)>) -> Vec<NamedCountDto> {
    groups
        .into_iter()
        .map(|(label, count)| NamedCountDto { label, count })
        .collect()
}

/// 报告列表（最新在前，支持历史回看）。`period_type` 空 = 全部；
/// `limit <= 0` 时按 20 封顶。损坏的 JSON fail-closed 报错（不静默丢）。
pub fn list_deviation_reports(period_type: String, limit: i32) -> Result<Vec<DeviationReportDto>> {
    let period_type = period_type.trim().to_string();
    anyhow::ensure!(
        period_type.is_empty() || period_type == "week" || period_type == "month",
        "不支持的时间粒度：仅 week / month（空 = 全部）"
    );
    let limit = if limit <= 0 { 20 } else { limit as i64 };
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store
        .list_deviation_reports(&period_type, limit)?
        .into_iter()
        .map(report_dto)
        .collect()
}

/// 标记已读（幂等：首次的 read_at 保留，重复标记不覆盖）。
pub fn mark_deviation_report_read(id: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.set_report_read(id.trim(), Utc::now())
}

/// 忽略单条偏差：置 dismissed=true（审计轨迹，不删除条目），并把报告
/// 置为已读（消除未读提示；裁决 Q1：忽略后不再提醒，但记录保留）。
pub fn dismiss_deviation(report_id: String, index: i32) -> Result<()> {
    let report_id = report_id.trim();
    anyhow::ensure!(index >= 0, "偏差序号无效：{index}");
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let Some((_, _, existing)) = store.deviation_report_row(report_id)? else {
        anyhow::bail!("报告不存在：{report_id}");
    };
    let raw = existing.context("报告尚未检测，无可忽略的偏差")?;
    let mut list: Vec<StoredDeviation> =
        serde_json::from_str(&raw).context("deviations_json 解析失败")?;
    anyhow::ensure!(
        (index as usize) < list.len(),
        "偏差序号越界：{} / {}",
        index,
        list.len()
    );
    list[index as usize].dismissed = true;
    store.replace_deviations_json(report_id, &serde_json::to_string(&list)?, Utc::now())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::{AiProvider, AiReply};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct ScriptedProvider(Vec<Result<String>>);
    impl AiProvider for ScriptedProvider {
        fn generate_reply_with_tools(
            &self,
            _messages: Vec<crate::ai::memory::ContextMessage>,
            _tools: Option<&[crate::ai::tool::ToolSpec]>,
        ) -> Result<AiReply> {
            self.0
                .iter()
                .find_map(|r| r.as_ref().ok().cloned())
                .map(AiReply::text)
                .with_context(|| "脚本耗尽".to_string())
        }
    }

    struct DataDirGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }
    impl DataDirGuard {
        fn set(dir: &std::path::Path) -> Self {
            let lock = crate::config::test_env_guard();
            crate::config::unpin();
            crate::config::AppConfig::pin(Some(dir.to_str().unwrap())).unwrap();
            Self { _lock: lock }
        }
    }
    impl Drop for DataDirGuard {
        fn drop(&mut self) {
            crate::config::unpin();
        }
    }

    fn open_pinned_store() -> (DataDirGuard, Store) {
        let dir = std::env::temp_dir().join(format!(
            "elsewhen-review-report-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let guard = DataDirGuard::set(&dir);
        let config = crate::config::AppConfig::load().unwrap();
        (guard, Store::open(&config.database_path).unwrap())
    }

    /// 本地某日某时（归属唯一、避开午夜 DST 转换窗）。
    fn local_at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
        let naive = NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
            .unwrap();
        Local
            .from_local_datetime(&naive)
            .single()
            .expect("测试时刻无 DST 歧义")
    }

    fn analysis_json(activities: &[&str], recordable: bool) -> String {
        let kind = if recordable { "event" } else { "chitchat" };
        format!(
            r#"{{"schema_version":"{}","event_type":"运动",
             "confidence":0.9,"summary":"s","clarifications":[],
             "people":[],"projects":[],"activities":{activities},
             "follow_ups":[],"recordable":{recordable},"kind":"{kind}"}}"#,
            super::super::EVENT_ANALYSIS_VERSION,
            activities = serde_json::to_string(activities).unwrap(),
        )
    }

    fn seed_event(
        store: &Store,
        text: &str,
        recorded_at: chrono::DateTime<chrono::Utc>,
        result_json: &str,
    ) -> String {
        let id = store
            .insert_event(crate::event::NewEvent {
                raw_text: text,
                occurred_at: recorded_at,
                recorded_at,
                source: "capture",
            })
            .unwrap();
        store
            .connection
            .execute(
                "INSERT INTO event_analyses (id, event_id, prompt_version, result_json, created_at)
                 VALUES (?1, ?2, 'p', ?3, ?4)",
                rusqlite::params![
                    format!("analysis-{id}"),
                    id,
                    result_json,
                    recorded_at.to_rfc3339()
                ],
            )
            .unwrap();
        id
    }

    fn noon(date: NaiveDate) -> chrono::DateTime<chrono::Utc> {
        use chrono::TimeZone;
        let naive = date.and_hms_opt(12, 0, 0).unwrap();
        Local
            .from_local_datetime(&naive)
            .single()
            .expect("本地正午无 DST 歧义")
            .with_timezone(&chrono::Utc)
    }

    fn seed_goal(store: &Store, content: &str) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        store
            .connection
            .execute(
                "INSERT INTO goals (id, content, phase, status, created_at, updated_at)
                 VALUES (?1, ?2, 'near', 'active', ?3, ?3)",
                rusqlite::params![id, content, now],
            )
            .unwrap();
        id
    }

    #[test]
    fn windows_and_due_gate_follow_period_end_anchor() {
        // 周窗口：周日锚 → 上周一..周日。
        let sunday = NaiveDate::from_ymd_opt(2026, 3, 8).unwrap();
        let (start, end) = report_window("week", sunday).unwrap();
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 3, 2).unwrap());
        assert_eq!(end, sunday);
        // 月窗口：月末锚 → 自然月。
        let (ms, me) =
            report_window("month", NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()).unwrap();
        assert_eq!(
            (ms, me),
            (
                NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()
            )
        );
        assert!(report_window("quarter", sunday).is_err());

        // 周一 07:59：上周报告未到生成时刻。
        let monday_early = local_at(2026, 3, 9, 7, 59);
        let due = due_reports(monday_early);
        assert!(
            !due.iter().any(|(t, _)| *t == "week"),
            "周一 07:59 不应触发"
        );
        // 周一 08:00 整：触发，锚 = 昨天（上周日 3-08）。
        let monday = local_at(2026, 3, 9, 8, 0);
        let due = due_reports(monday);
        assert!(
            due.iter().any(|(t, p)| *t == "week" && *p == sunday),
            "周一 08:00 应触发上周报告：{due:?}"
        );
        // 周五（非 08:00 时刻约束，gate 已过）：仍补最近完整周期（验收场景
        // 「进程一周未开 → 启动后补做本周报告，历史空窗不补」）。
        let friday = local_at(2026, 3, 13, 10, 0);
        let due = due_reports(friday);
        assert!(
            due.iter().any(|(t, p)| *t == "week" && *p == sunday),
            "周五应补最近完整周：{due:?}"
        );
        // 2 月 28 日 10:00：最新完整月 = 1 月（gate 2/1 08:00 早已过）。
        assert!(
            due_reports(local_at(2026, 2, 28, 10, 0))
                .iter()
                .any(|(t, p)| *t == "month" && *p == NaiveDate::from_ymd_opt(2026, 1, 31).unwrap())
        );
        // 月 gate 未到：1 月 31 日（月末）次日才是 2/1——1/31 当天 07:59，
        // 上一完整月（12 月）的 gate（1/1 08:00）自然已过，这里取
        // 「12 月报告已存在」不可测（无库）；所以考察 gate 未到需看锚日当天：
        // 3/1 07:59 → 2 月末（2/28）的 gate（3/1 08:00）未到 → 无 2 月锚。
        assert!(
            !due_reports(local_at(2026, 3, 1, 7, 59))
                .iter()
                .any(|(t, p)| *t == "month" && *p == NaiveDate::from_ymd_opt(2026, 2, 28).unwrap())
        );
        // 3/1 08:01 → 2 月报告触发，锚 = 2 月末。
        assert!(
            due_reports(local_at(2026, 3, 1, 8, 1))
                .iter()
                .any(|(t, p)| *t == "month" && *p == NaiveDate::from_ymd_opt(2026, 2, 28).unwrap())
        );
    }

    #[test]
    fn generate_is_idempotent_and_skips_empty_windows() {
        let (_guard, store) = open_pinned_store();
        let period = latest_complete_week(NaiveDate::from_ymd_opt(2026, 3, 11).unwrap());
        // 空窗：不生成。
        assert!(
            build_period_report(&store, "week", period)
                .unwrap()
                .is_none()
        );

        // 有事件 + 活跃目标 → 生成，快照内联进 report_json。
        seed_goal(&store, "每周运动三次");
        let week_start = period - chrono::Duration::days(6);
        seed_event(
            &store,
            "在公园跑步 40 分钟",
            noon(week_start),
            &analysis_json(&["跑步"], true),
        );
        seed_event(
            &store,
            "晚上跑步",
            noon(period),
            &analysis_json(&["跑步"], true),
        );
        let report = build_period_report(&store, "week", period)
            .unwrap()
            .unwrap();
        assert_eq!(report.stats.total, 2);
        assert_eq!(report.goals.len(), 1);
        assert_eq!(report.goals[0].content, "每周运动三次");
        assert_eq!(report.start_date, week_start.format("%F").to_string());
        assert_eq!(report.stats.by_activity, vec![("跑步".to_string(), 2)]);

        // 表层幂等：同 (period, period_type) 二次写入被 UNIQUE 约束挡住。
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            store
                .insert_deviation_report("id-a", period, "week", &json, Utc::now())
                .unwrap()
        );
        assert!(
            !store
                .insert_deviation_report("id-b", period, "week", &json, Utc::now())
                .unwrap()
        );
    }

    #[test]
    fn validate_rejects_whole_batch_and_dedups_deterministically() {
        let goals = vec![ReportGoalSnapshot {
            id: "g1".into(),
            phase: "near".into(),
            content: "每周运动三次".into(),
        }];
        let seen = vec![("每周运动三次".to_string(), "no_progress".to_string())];

        // 历史同 (goal, type) → 去重丢弃。
        let dup = r#"[{"goal_content":"每周运动三次","deviation_type":"no_progress","evidence":"0 次","severity":"high"}]"#;
        assert!(
            validate_deviation_candidates(dup, &goals, &seen)
                .unwrap()
                .is_empty()
        );

        // 批内重复也只留一条。
        let inner = r#"[{"goal_content":"每周运动三次","deviation_type":"stalled","evidence":"1 次而后无","severity":"low"},{"goal_content":"每周运动三次 ","deviation_type":"stalled","evidence":"同上","severity":"low"}]"#;
        let out = validate_deviation_candidates(inner, &goals, &[]).unwrap();
        assert_eq!(out.len(), 1);

        // code fence 剥离。
        let fenced = "```json\n[]\n```";
        assert!(
            validate_deviation_candidates(fenced, &goals, &[])
                .unwrap()
                .is_empty()
        );

        // goal 不在快照（逐字要求）→ 整批拒绝。
        let forged = r#"[{"goal_content":"每天读一本书","deviation_type":"stalled","evidence":"无","severity":"low"}]"#;
        assert!(validate_deviation_candidates(forged, &goals, &[]).is_err());
        // 枚举外 deviation_type → 整批拒绝。
        let bad_kind = r#"[{"goal_content":"每周运动三次","deviation_type":"变差了","evidence":"无","severity":"low"}]"#;
        assert!(validate_deviation_candidates(bad_kind, &goals, &[]).is_err());
        // 非 JSON → 整批拒绝。
        assert!(validate_deviation_candidates("没什么问题", &goals, &[]).is_err());
        // 空数组合法。
        assert!(
            validate_deviation_candidates("[]", &goals, &[])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn detect_is_isolated_from_ai_failures_and_dedups_against_history() {
        let (_guard, store) = open_pinned_store();
        seed_goal(&store, "每周运动三次");
        let period = NaiveDate::from_ymd_opt(2026, 3, 8).unwrap();
        seed_event(
            &store,
            "在公园跑步",
            noon(period - chrono::Duration::days(2)),
            &analysis_json(&["跑步"], true),
        );
        let report = build_period_report(&store, "week", period)
            .unwrap()
            .unwrap();
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            store
                .insert_deviation_report("r-1", period, "week", &json, Utc::now())
                .unwrap()
        );

        // AI 失败：报告可读性不受影响，deviations_json 保持 NULL。
        let broken = ScriptedProvider(vec![Ok("模型胡言乱语".to_string())]);
        assert!(detect_deviation(&store, "r-1", &broken).is_err());
        let (_, _, deviations) = store.deviation_report_row("r-1").unwrap().unwrap();
        assert!(deviations.is_none(), "AI 失败不应写入任何检测结论");

        // 合法输出：逐条校验后落库；重复调用不再打 provider。
        let ok = ScriptedProvider(vec![Ok(
            r#"[{"goal_content":"每周运动三次","deviation_type":"no_progress","evidence":"可记录事件 1 条，无运动相关记录","severity":"medium"}]"#.to_string(),
        )]);
        let count = detect_deviation(&store, "r-1", &ok).unwrap();
        assert_eq!(count, 1);
        let (_, _, deviations) = store.deviation_report_row("r-1").unwrap().unwrap();
        assert!(deviations.is_some());
        // 已检测过的报告再跑 = 幂等空操作。
        assert_eq!(detect_deviation(&store, "r-1", &ok).unwrap(), 0);

        // 历史去重：第二份同类型（week）报告 + 同偏差 → 最终 []。
        let period2 = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        let json2 = {
            let mut r2 = report.clone();
            r2.period = period2.format("%F").to_string();
            r2.start_date = (period2 - chrono::Duration::days(6))
                .format("%F")
                .to_string();
            serde_json::to_string(&r2).unwrap()
        };
        assert!(
            store
                .insert_deviation_report("r-2", period2, "week", &json2, Utc::now())
                .unwrap()
        );
        let same = ScriptedProvider(vec![Ok(
            r#"[{"goal_content":"每周运动三次","deviation_type":"no_progress","evidence":"又没有运动记录","severity":"high"}]"#.to_string(),
        )]);
        let count = detect_deviation(&store, "r-2", &same).unwrap();
        assert_eq!(count, 0, "同 (goal, deviation_type) 不得重复输出");

        // 管道产物纪律：报告排除于快照同步。
        assert!(
            crate::storage::sync::snapshot::SNAPSHOT_EXCLUDED_TABLES
                .iter()
                .any(|(table, _)| *table == "deviation_reports")
        );
    }
    #[test]
    fn read_dismiss_and_list_ordering() {
        let (_guard, store) = open_pinned_store();
        seed_goal(&store, "每周运动三次");
        seed_event(
            &store,
            "在公园跑步",
            noon(NaiveDate::from_ymd_opt(2026, 3, 6).unwrap()),
            &analysis_json(&["跑步"], true),
        );
        seed_event(
            &store,
            "整理了读书笔记",
            noon(NaiveDate::from_ymd_opt(2026, 2, 26).unwrap()),
            &analysis_json(&["阅读"], true),
        );
        let make = |id: &str, period: NaiveDate| {
            let report = build_period_report(&store, "week", period)
                .unwrap()
                .unwrap();
            let json = serde_json::to_string(&report).unwrap();
            store
                .insert_deviation_report(id, period, "week", &json, Utc::now())
                .unwrap();
        };
        let older = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        let newer = NaiveDate::from_ymd_opt(2026, 3, 8).unwrap();
        make("r-old", older);
        make("r-new", newer);

        // 检测两份报告（各落一条偏差）。
        let detect_ok = ScriptedProvider(vec![Ok(
            r#"[{"goal_content":"每周运动三次","deviation_type":"no_progress","evidence":"无相关记录","severity":"medium"},
                {"goal_content":"每周运动三次","deviation_type":"stalled","evidence":"进度未动","severity":"low"}]"#
                .to_string(),
        )]);
        // 检测 r-old：两条合法（历史为空）。r-new 再检测：同 (goal, type)
        // 跨期去重 → 0 条（「unwoke 但不重复提醒」语义，R3 裁决）。
        assert_eq!(detect_deviation(&store, "r-old", &detect_ok).unwrap(), 2);
        assert_eq!(detect_deviation(&store, "r-new", &detect_ok).unwrap(), 0);

        // 列表：最新创建在前（历史回看顺序）；period_type 过滤与空串全量。
        let all = list_deviation_reports("week".into(), 10).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].period, "2026-03-08");
        assert_eq!(all[1].period, "2026-03-01");
        assert!(
            list_deviation_reports("month".into(), 10)
                .unwrap()
                .is_empty()
        );
        assert_eq!(list_deviation_reports("".into(), 10).unwrap().len(), 2);
        // 报告体投影：统计与偏差完整落 DTO；未检测的 r-new deviations 为空。
        assert!(all[0].total >= 1);
        assert_eq!(all[1].deviations.len(), 2);
        assert!(all[0].deviations.is_empty());
        assert!(all[1].read_at.is_none());

        // 已读幂等：首次 mark 保留，重复 mark 不覆盖时间戳。
        let (r_old_id, r_new_id) = ("r-old".to_string(), "r-new".to_string());
        mark_deviation_report_read(r_old_id.clone()).unwrap();
        let first_read = list_deviation_reports("week".into(), 10).unwrap()[0 + 1]
            .read_at
            .clone()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        mark_deviation_report_read(r_old_id.clone()).unwrap();
        assert_eq!(
            list_deviation_reports("week".into(), 10).unwrap()[1]
                .read_at
                .as_deref(),
            Some(first_read.as_str())
        );

        // 忽略单条偏差：dismissed 置位、条目保留（审计轨迹，不删除）、
        // 报告兜底置已读；同报告另一条不受影响。
        dismiss_deviation(r_old_id.clone(), 0).unwrap();
        let dto = &list_deviation_reports("week".into(), 10).unwrap()[1];
        assert!(dto.deviations[0].dismissed);
        assert!(!dto.deviations[1].dismissed);
        assert_eq!(dto.deviations.len(), 2, "忽略是置位不是删除");
        assert!(dto.read_at.is_some());

        // fail-closed：未检测报告无可忽略偏差；越界序号拒绝；报告不存在拒绝。
        assert!(dismiss_deviation(r_new_id.clone(), 0).is_err());
        assert!(dismiss_deviation(r_old_id.clone(), 99).is_err());
        assert!(dismiss_deviation("no-such".into(), 0).is_err());
        assert!(mark_deviation_report_read("no-such".into()).is_err());
    }

    #[test]
    fn report_dto_rejects_corrupt_json() {
        let (_guard, store) = open_pinned_store();
        let period = NaiveDate::from_ymd_opt(2026, 3, 8).unwrap();
        seed_event(
            &store,
            "在公园跑步",
            noon(period - chrono::Duration::days(2)),
            &analysis_json(&["跑步"], true),
        );
        let report = build_period_report(&store, "week", period)
            .unwrap()
            .unwrap();
        let json = serde_json::to_string(&report).unwrap();
        store
            .insert_deviation_report("r-1", period, "week", &json, Utc::now())
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE deviation_reports SET report_json = 'not-json' WHERE id = 'r-1'",
                [],
            )
            .unwrap();
        assert!(
            list_deviation_reports("week".into(), 10).is_err(),
            "损坏的 report_json 必须 fail-closed 而非静默丢报告"
        );
    }
}
