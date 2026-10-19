//! 偏差报告持久化：DTO、落库、已读/忽略与查询端点。

use super::*;

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
