//! event_stats 内嵌测试平移。

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
