//! 报告构建：goal 快照、周期报告 JSON 载荷与提示词。

use super::*;

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

pub(crate) fn fmt_date(d: NaiveDate) -> String {
    d.format("%F").to_string()
}

/// 报告是给人看与给 AI 看的概貌：每组截 top 12（与 R1 拼装同款截断）。
pub(crate) fn top_groups(groups: Vec<(String, i64)>) -> Vec<(String, i64)> {
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
pub(crate) fn build_period_report(
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

pub(crate) fn build_deviation_prompt(report: &PeriodReport, seen: &[(String, String)]) -> String {
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
