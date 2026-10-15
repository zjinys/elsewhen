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
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 与 period_stats_tests 同款：pin 数据目录使 Store 指向临时库。
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

    /// 本地日历日正午（归属唯一、不贴边），转 UTC 供 event 插入。
    /// 库内 recorded_at 统一 UTC rfc3339，时刻必须派生自本地日界，
    /// 禁止插带时区偏移的字面量（字典序比较的坑见 storage::review 注释）。
    fn local_noon_utc(date: chrono::NaiveDate) -> chrono::DateTime<chrono::Utc> {
        use chrono::TimeZone;
        let naive = date.and_hms_opt(12, 0, 0).unwrap();
        chrono::Local
            .from_local_datetime(&naive)
            .single()
            .expect("本地正午无 DST 歧义")
            .with_timezone(&chrono::Utc)
    }

    fn analysis_json(
        event_type: &str,
        people: &[&str],
        projects: &[&str],
        activities: &[&str],
        recordable: bool,
    ) -> String {
        // parse 校验：不可记录结果不能标 kind=event，造数必须同律。
        let kind = if recordable { "event" } else { "chitchat" };
        format!(
            r#"{{"schema_version":"{}","event_type":"{event_type}",
             "confidence":0.9,"summary":"s","clarifications":[],
             "people":{people},"projects":{projects},"activities":{activities},
             "follow_ups":[],"recordable":{recordable},"kind":"{kind}"}}"#,
            super::super::EVENT_ANALYSIS_VERSION,
            people = serde_json::to_string(people).unwrap(),
            projects = serde_json::to_string(projects).unwrap(),
            activities = serde_json::to_string(activities).unwrap(),
        )
    }

    fn open_pinned_store() -> (DataDirGuard, Store) {
        let dir = std::env::temp_dir().join(format!(
            "elsewhen-event-stats-{}",
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

    fn seed_decision(store: &Store, event_id: &str, recordable: bool) {
        store
            .connection
            .execute(
                "INSERT INTO event_recordability_decisions
                 (id,event_id,recordable,kind,reason,created_at)
                 VALUES (?1,?2,?3,'event','manual',?4)",
                rusqlite::params![
                    format!("decision-{event_id}"),
                    event_id,
                    recordable as i64,
                    chrono::Utc::now().to_rfc3339()
                ],
            )
            .unwrap();
    }

    fn no_filter() -> EventStatsQuery {
        EventStatsQuery::default()
    }

    #[test]
    fn groups_counts_and_excludes_unrecordable() {
        let (_guard, store) = open_pinned_store();
        let today = chrono::Local::now().date_naive();
        let walk = analysis_json("运动", &["张三"], &[], &["散步"], true);
        let read = analysis_json("阅读", &[], &[], &["阅读"], true);
        // chitchat：parse 校验要求 kind=chitchat 且 recordable=false。
        let chat = analysis_json("闲聊", &[], &[], &["散步"], false);

        for day in [0, 1, 2] {
            seed_event(
                &store,
                &format!("和张三在公园散步散步{day}"),
                local_noon_utc(today - chrono::Duration::days(day)),
                &walk,
            );
        }
        seed_event(&store, "读完了半本书", local_noon_utc(today), &read);
        seed_event(&store, "今天天气不错", local_noon_utc(today), &chat);

        // 手动决策覆盖：分析说 recordable=true 也不算（manual 优先级最高）。
        let overridden = seed_event(&store, "想说的事", local_noon_utc(today), &walk);
        seed_decision(&store, &overridden, false);

        let outcome = query_event_stats(
            &store,
            today - chrono::Duration::days(6),
            today,
            &no_filter(),
            "activity",
        )
        .unwrap();
        assert_eq!(
            outcome.total, 4,
            "chitchat 与 manual 覆盖者不进统计（recordable 判定链）"
        );
        assert_eq!(outcome.matched, 4);
        // 散步 3（含 chitchat 那条也要排除）/阅读 1；确定性排序。
        assert_eq!(outcome.grouped[0], ("散步".to_string(), 3));
        assert_eq!(outcome.grouped[1], ("阅读".to_string(), 1));
    }

    #[test]
    fn combined_filters_and_case_folding() {
        let (_guard, store) = open_pinned_store();
        let today = chrono::Local::now().date_naive();
        let a = analysis_json("工作", &["李四"], &[], &["Coding"], true);
        let b = analysis_json("工作", &["王五"], &[], &["coding"], true);
        let c = analysis_json("工作", &["李四", "王五"], &[], &["散步"], true);

        seed_event(&store, "写代码一上午", local_noon_utc(today), &a);
        seed_event(&store, "调参", local_noon_utc(today), &b);
        seed_event(&store, "和李四王五散步", local_noon_utc(today), &c);

        // activity=「coding」大小写折叠命中 2 条（Coding/coding 各一），
        // person=李四 收窄到 1 条。
        let query = EventStatsQuery {
            activity: Some("coding".to_string()),
            person: Some("李四".to_string()),
            ..Default::default()
        };
        let outcome = query_event_stats(&store, today, today, &query, "person").unwrap();
        assert_eq!(outcome.total, 3);
        assert_eq!(outcome.matched, 1);
        assert_eq!(outcome.grouped, vec![("李四".to_string(), 1)]);
    }

    #[test]
    fn text_contains_matches_raw_text() {
        let (_guard, store) = open_pinned_store();
        let today = chrono::Local::now().date_naive();
        let a = analysis_json("工作", &[], &[], &["写代码"], true);
        seed_event(&store, "聚焦部署流水线失败问题", local_noon_utc(today), &a);
        seed_event(&store, "午饭吃了面", local_noon_utc(today), &a);

        let query = EventStatsQuery {
            text_contains: Some("流水线".to_string()),
            ..Default::default()
        };
        let outcome = query_event_stats(&store, today, today, &query, "none").unwrap();
        assert_eq!(outcome.matched, 1);
        assert_eq!(outcome.samples.len(), 1);
        assert!(outcome.samples[0].text.contains("流水线"));
    }

    #[test]
    fn empty_result_is_legal() {
        let (_guard, store) = open_pinned_store();
        let today = chrono::Local::now().date_naive();
        let outcome = query_event_stats(
            &store,
            today - chrono::Duration::days(2),
            today,
            &no_filter(),
            "activity",
        )
        .unwrap();
        assert_eq!(outcome.total, 0);
        assert_eq!(outcome.matched, 0);
        assert!(outcome.grouped.is_empty());
        assert!(outcome.samples.is_empty());
    }

    #[test]
    fn fails_closed_on_invalid_input() {
        let (_guard, store) = open_pinned_store();
        let today = chrono::Local::now().date_naive();
        // 未知 group_by / 反转窗口 / 超长窗口，全部报错不猜。
        assert!(query_event_stats(&store, today, today, &no_filter(), "week").is_err());
        assert!(
            query_event_stats(
                &store,
                today,
                today - chrono::Duration::days(1),
                &no_filter(),
                "day"
            )
            .is_err()
        );
        assert!(
            query_event_stats(
                &store,
                today - chrono::Duration::days(400),
                today,
                &no_filter(),
                "day"
            )
            .is_err()
        );
        assert!(resolve_date_range("fortnight", today).is_err());
    }

    #[test]
    fn date_range_bounds() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 15).unwrap(); // 周四
        // ISO 周一为始（与 get_period_stats 同款）。
        let (s, e) = resolve_date_range("this_week", today).unwrap();
        assert_eq!(
            (s, e),
            (
                chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 10, 18).unwrap(),
            )
        );
        let (s, e) = resolve_date_range("last_week", today).unwrap();
        assert_eq!(
            (s, e),
            (
                chrono::NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 10, 11).unwrap(),
            )
        );
        let (s, e) = resolve_date_range("this_month", today).unwrap();
        assert_eq!(
            (s, e),
            (
                chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 10, 31).unwrap(),
            )
        );
        // 跨年：1 月的「上个月」是去年 12 月。
        let jan = chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let (s, e) = resolve_date_range("last_month", jan).unwrap();
        assert_eq!(
            (s, e),
            (
                chrono::NaiveDate::from_ymd_opt(2025, 12, 1).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
            )
        );
    }

    #[test]
    fn samples_newest_first_and_truncated() {
        let (_guard, store) = open_pinned_store();
        let today = chrono::Local::now().date_naive();
        let a = analysis_json("note", &[], &[], &["散步"], true);
        let long_text = "长".repeat(400);
        for day in 0..13 {
            let text = if day == 0 {
                long_text.clone()
            } else {
                format!("第{day}次散步")
            };
            seed_event(
                &store,
                &text,
                local_noon_utc(today - chrono::Duration::days(day)),
                &a,
            );
        }
        let outcome = query_event_stats(
            &store,
            today - chrono::Duration::days(12),
            today,
            &no_filter(),
            "none",
        )
        .unwrap();
        assert_eq!(outcome.matched, 13);
        // 样本封顶 12 条、最新在前（day=0 是最新）。
        assert_eq!(outcome.samples.len(), 12);
        assert_eq!(outcome.samples[0].text.chars().count(), 240, "原文超长截断");
        assert!(outcome.samples[1].text.contains("第1次散步"));
    }
}
