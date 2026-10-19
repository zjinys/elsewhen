//! 事件统计查询（R2）：对话工具 `query_event_stats` 与 R3 周期报告管道的
//! 共享聚合层。口径与 daily_overview / 周·月拼装（get_period_stats）一致：
//! manual 决策覆盖 → 分析 recordability → 默认 的判定链、本地日界、
//! 库内 recorded_at 统一 UTC rfc3339。本层只读，不写任何表。

use anyhow::Result;
use chrono::{Datelike, NaiveDate};

use crate::storage::Store;

/// 过滤条件：全部可组合，None / 空串 = 该维度不过滤。
/// 匹配语义：event_type 精确匹配（大小写折叠）；person/project/activity
/// 与数组任一元素「包含」匹配（大小写折叠，同 D1 预筛手写扫描纪律，无 regex）；
/// text_contains 对原文包含匹配（大小写折叠）。
#[derive(Clone, Debug, Default)]
pub struct EventStatsQuery {
    pub event_type: Option<String>,
    pub person: Option<String>,
    pub project: Option<String>,
    pub activity: Option<String>,
    pub text_contains: Option<String>,
}

#[derive(Clone, Debug)]
pub struct EventStatsSample {
    /// 本地时刻，格式 `MM-DD HH:MM`。
    pub when: String,
    /// 原文片段（超长截断）。
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct EventStatsOutcome {
    /// 窗口内可记录事件总数（过滤前）。空结果时调用方也能回答
    /// 「这期间有 N 条记录但没有命中你的条件」。
    pub total: i64,
    /// 过滤后命中数。
    pub matched: i64,
    /// group_by 分组计数。确定性排序：次数降序、同次数 label 字典序；
    /// 截断到 [`GROUP_TOP_N`]。
    pub grouped: Vec<(String, i64)>,
    /// 最新在前的事件样本（原文片段）。
    pub samples: Vec<EventStatsSample>,
}

/// 分组输出上限：给 AI 的结果应是完整概貌，超出的长尾不再枚举。
pub const GROUP_TOP_N: usize = 50;
/// 样本原文截断长度（与工具导语/目录导入同款纪律）。
pub const SAMPLE_TEXT_CHARS: usize = 240;
/// 样本条数上限。
pub const SAMPLE_MAX: usize = 12;

/// 窗口聚合上限：工具可查全年（366 天），更长的口径性问题应让用户收窄，
/// 而不是悄悄给一个跨数年的全量答案。
const MAX_WINDOW_DAYS: i64 = 366;

pub const GROUP_BY_CHOICES: [&str; 6] =
    ["none", "day", "event_type", "person", "project", "activity"];

fn contains_fold(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn fold_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// 把对话用语解析成本地日界窗口（fail-closed：未知值报错不猜）。
/// 复杂/自定义区间不在本工具的语境内解决——用户给明确日期时用
/// `list_events_by_date` 或直接回答。
#[flutter_rust_bridge::frb(ignore)]
pub fn resolve_date_range(spec: &str, today: NaiveDate) -> Result<(NaiveDate, NaiveDate)> {
    let weekday = today.weekday().num_days_from_monday() as i64;
    let week_start = today - chrono::Duration::days(weekday);
    let month_start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
    let next_month = if today.month() == 12 {
        NaiveDate::from_ymd_opt(today.year() + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(today.year(), today.month() + 1, 1).unwrap()
    };
    let month_end = next_month.pred_opt().unwrap();
    let last_month_start = if today.month() == 1 {
        NaiveDate::from_ymd_opt(today.year() - 1, 12, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(today.year(), today.month() - 1, 1).unwrap()
    };
    let last_month_end = month_start.pred_opt().unwrap();
    match spec.trim() {
        "today" => Ok((today, today)),
        "yesterday" => {
            let yesterday = today.pred_opt().unwrap();
            Ok((yesterday, yesterday))
        }
        "this_week" => Ok((week_start, week_start + chrono::Duration::days(6))),
        "last_week" => Ok((
            week_start - chrono::Duration::days(7),
            week_start + chrono::Duration::days(6) - chrono::Duration::days(7),
        )),
        "this_month" => Ok((month_start, month_end)),
        "last_month" => Ok((last_month_start, last_month_end)),
        other => anyhow::bail!(
            "不支持的 date_range：{other}（可选：today / yesterday / this_week / last_week / this_month / last_month）"
        ),
    }
}

/// 窗口内可记录事件的过滤聚合。`group_by` 控制计数维度（none = 只给总数
/// 与样本）；样本取窗口内最新 [`SAMPLE_MAX`] 条。窗口为本地日界（含端点）。
///
/// 聚合层不是 FRB 面（&Store/chrono 参数；RFC(fr.) 对话工具与 R3 报告管道
/// 经 Rust 内部调用），codegen 显式忽略。
#[flutter_rust_bridge::frb(ignore)]
pub fn query_event_stats(
    store: &Store,
    start: NaiveDate,
    end: NaiveDate,
    query: &EventStatsQuery,
    group_by: &str,
) -> Result<EventStatsOutcome> {
    anyhow::ensure!(start <= end, "统计窗口起点不得晚于终点（{start} > {end}）");
    let days = (end - start).num_days() + 1;
    anyhow::ensure!(
        days <= MAX_WINDOW_DAYS,
        "统计窗口不得超过 {MAX_WINDOW_DAYS} 天，请让用户收窄日期范围"
    );
    anyhow::ensure!(
        GROUP_BY_CHOICES.contains(&group_by),
        "不支持的 group_by：{group_by}（可选：{}）",
        GROUP_BY_CHOICES.join(" / ")
    );

    let want_type = fold_optional(query.event_type.as_deref());
    let want_person = fold_optional(query.person.as_deref());
    let want_project = fold_optional(query.project.as_deref());
    let want_activity = fold_optional(query.activity.as_deref());
    let want_text = fold_optional(query.text_contains.as_deref());

    let mut total = 0i64;
    let mut matched = 0i64;
    let mut groups: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut per_day: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut samples: Vec<EventStatsSample> = Vec::new();

    // events_between 返回顺序即最新在前（recorded_at DESC），样本直接取头部。
    let rows = store.events_between(start, end)?;
    for event in rows {
        // 与 daily_overview / get_period_stats 同一判定链。
        if !super::entry_is_recordable(store, &event.id) {
            continue;
        }
        total += 1;
        let parsed = store
            .event_analysis_detail(&event.id)?
            .and_then(|detail| detail.result_json)
            .and_then(|raw| super::EventAnalysisV1::parse(&raw).ok());

        if let Some(want) = &want_type {
            let has = parsed
                .as_ref()
                .map(|a| a.event_type.trim().to_lowercase() == want.to_lowercase())
                .unwrap_or(false);
            if !has {
                continue;
            }
        }
        let analysis = parsed.as_ref();
        if let Some(want) = &want_person {
            let has = analysis
                .map(|a| a.people.iter().any(|p| contains_fold(p, want)))
                .unwrap_or(false);
            if !has {
                continue;
            }
        }
        if let Some(want) = &want_project {
            let has = analysis
                .map(|a| a.projects.iter().any(|p| contains_fold(p, want)))
                .unwrap_or(false);
            if !has {
                continue;
            }
        }
        if let Some(want) = &want_activity {
            let has = analysis
                .map(|a| a.activities.iter().any(|p| contains_fold(p, want)))
                .unwrap_or(false);
            if !has {
                continue;
            }
        }
        if let Some(want) = &want_text {
            if !contains_fold(&event.raw_text, want) {
                continue;
            }
        }
        matched += 1;

        // 本地日界归属：rfc3339 转本地时区取日期；解析失败取头 10 字符兜底
        //（与 get_period_stats 同款）。
        let day = chrono::DateTime::parse_from_rfc3339(&event.recorded_at)
            .map(|dt| dt.with_timezone(&chrono::Local).date_naive().to_string())
            .unwrap_or_else(|_| event.recorded_at.chars().take(10).collect());
        *per_day.entry(day).or_insert(0) += 1;

        let when = chrono::DateTime::parse_from_rfc3339(&event.recorded_at)
            .map(|dt| {
                dt.with_timezone(&chrono::Local)
                    .format("%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|_| event.recorded_at.chars().take(10).collect());
        if samples.len() < SAMPLE_MAX {
            samples.push(EventStatsSample {
                when,
                text: event.raw_text.chars().take(SAMPLE_TEXT_CHARS).collect(),
            });
        }

        match group_by {
            "day" => {}
            "event_type" => {
                if let Some(label) = analysis
                    .map(|a| a.event_type.trim())
                    .filter(|s| !s.is_empty())
                {
                    *groups.entry(label.to_string()).or_insert(0) += 1;
                }
            }
            "person" => {
                for label in analysis.iter().flat_map(|a| a.people.iter()) {
                    let key = label.trim();
                    if !key.is_empty() {
                        *groups.entry(key.to_string()).or_insert(0) += 1;
                    }
                }
            }
            "project" => {
                for label in analysis.iter().flat_map(|a| a.projects.iter()) {
                    let key = label.trim();
                    if !key.is_empty() {
                        *groups.entry(key.to_string()).or_insert(0) += 1;
                    }
                }
            }
            "activity" => {
                for label in analysis.iter().flat_map(|a| a.activities.iter()) {
                    let key = label.trim();
                    if !key.is_empty() {
                        *groups.entry(key.to_string()).or_insert(0) += 1;
                    }
                }
            }
            _ => {}
        }
    }

    // group_by=day 的分组直接用 per_day：不补零——工具消费者要的是
    // 「哪些天有、各多少」，几十个零行只会撑爆结果。
    let mut grouped = if group_by == "day" {
        per_day.into_iter().collect::<Vec<_>>()
    } else {
        groups.into_iter().collect::<Vec<_>>()
    };
    grouped.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    grouped.truncate(GROUP_TOP_N);

    Ok(EventStatsOutcome {
        total,
        matched,
        grouped,
        samples,
    })
}

#[cfg(test)]
#[cfg(test)]
#[path = "event_stats_tests.rs"]
mod tests;
