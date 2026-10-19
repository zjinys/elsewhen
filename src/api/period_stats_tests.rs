use super::*;

use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// 与 record_event_tests 同款：pin 数据目录使 AppConfig::load 指向临时库。
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
    // parse 校验：不可记录结果不能标 kind=event（真实链路里默认弃记
    // 的闲聊就是 chitchat），造数必须同律，否则 parse 失败口径会漂。
    let kind = if recordable { "event" } else { "chitchat" };
    format!(
        r#"{{"schema_version":"{EVENT_ANALYSIS_VERSION}","event_type":"{event_type}",
         "confidence":0.9,"summary":"s","clarifications":[],
         "people":{people},"projects":{projects},"activities":{activities},
         "follow_ups":[],"recordable":{recordable},"kind":"{kind}"}}"#,
        people = serde_json::to_string(people).unwrap(),
        projects = serde_json::to_string(projects).unwrap(),
        activities = serde_json::to_string(activities).unwrap(),
    )
}

/// 造一条带分析结果的事件，返回 event_id。
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

/// 手动可记录性决策（覆盖口径用的最高优先级源）。
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

fn open_pinned_store() -> (DataDirGuard, Store) {
    let dir = std::env::temp_dir().join(format!(
        "elsewhen-period-stats-{}",
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

#[test]
fn period_stats_rejects_invalid_input() {
    let (_guard, _store) = open_pinned_store();
    assert!(get_period_stats("year".into(), "2026-10-14".into()).is_err());
    assert!(get_period_stats("week".into(), "2026/10/14".into()).is_err());
}

#[test]
fn week_and_month_windows_are_calendar_aligned() {
    let (_guard, store) = open_pinned_store();
    // 2026-10-12 是周一：锚定周三（10-14）的周窗口应为 10-12..10-18。
    let monday = chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
    seed_events_with_dates(
        &store,
        &[
            ("周五", monday + chrono::Duration::days(4)),
            ("下周一", monday + chrono::Duration::days(7)),
        ],
        true,
    );
    let week = get_period_stats("week".into(), "2026-10-14".into()).unwrap();
    assert_eq!(week.start_date, "2026-10-12");
    assert_eq!(week.end_date, "2026-10-18");
    assert_eq!(week.event_count, 1, "下周一的事件不得混入本周");
    assert_eq!(week.daily_counts.len(), 7, "日分布必须铺满整周");

    // 月窗口：10 月锚点 → 10-01..10-31，跨月事件隔离。
    let oct1 = chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
    let sep30 = chrono::NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
    seed_events_with_dates(&store, &[("九月末", sep30), ("月初", oct1)], true);
    let month = get_period_stats("month".into(), "2026-10-14".into()).unwrap();
    assert_eq!(month.start_date, "2026-10-01");
    assert_eq!(month.end_date, "2026-10-31");
    assert_eq!(month.daily_counts.len(), 31, "日分布必须铺满整月");
    assert_eq!(
        month.event_count, 3,
        "周五/下周一(10-19)/月初 属十月；九月末不属于"
    );
}

#[test]
fn aggregation_ranks_and_counts_deterministically() {
    let (_guard, store) = open_pinned_store();
    let monday = chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
    let seed = |store: &Store, text: &str, json: String| {
        seed_event(store, text, local_noon_utc(monday), &json)
    };
    seed(
        &store,
        "a",
        analysis_json("work", &["amma"], &["p1"], &["coding"], true),
    );
    seed(
        &store,
        "b",
        analysis_json("work", &["zoe", "amma"], &["p1"], &["coding"], true),
    );
    seed(
        &store,
        "c",
        analysis_json("idea", &["amma"], &["p2"], &["coding"], true),
    );
    // 待办两件本周完成。
    for tid in ["t1", "t2"] {
        store
            .connection
            .execute(
                "INSERT INTO todos (id,title,status,updated_at,created_at)
                 VALUES (?1,'t','done',?2,?2)",
                rusqlite::params![
                    tid,
                    local_noon_utc(monday + chrono::Duration::days(1)).to_rfc3339()
                ],
            )
            .unwrap();
    }
    store
        .connection
        .execute(
            "INSERT INTO todos (id,title,status,updated_at,created_at)
             VALUES ('t-todo-open','t','open',?1,?1)",
            rusqlite::params![local_noon_utc(monday).to_rfc3339()],
        )
        .unwrap();

    let stats = get_period_stats("week".into(), "2026-10-14".into()).unwrap();
    assert_eq!(stats.event_count, 3);
    assert_eq!(stats.todo_done_count, 2, "open 待办不属于完成数");
    // 次数降序、同次数字典序（work=2 先于 idea=1；amma=3 稳居首）。
    assert_eq!(stats.by_event_type[0].label, "work");
    assert_eq!(stats.by_event_type[0].count, 2);
    assert_eq!(stats.by_event_type[1].label, "idea");
    assert_eq!(stats.by_person[0].label, "amma");
    assert_eq!(stats.by_person[0].count, 3);
    assert_eq!(stats.by_project[0].label, "p1");
    assert_eq!(stats.by_activity[0].label, "coding");
    assert_eq!(stats.by_activity[0].count, 3);
    // 同次数字典序：zoe(1) 排 amma 之外尾段
    let zoe = stats
        .by_person
        .iter()
        .find(|r| r.label == "zoe")
        .expect("zoe 应在榜内");
    assert_eq!(zoe.count, 1);
    let day_key = monday.to_string();
    let day_count = &stats
        .daily_counts
        .iter()
        .find(|d| d.date == day_key)
        .expect("当日必有分布条目")
        .count;
    assert_eq!(*day_count, 3, "三件事都是本地周一 → 当日分布=3");
}

#[test]
fn non_recordable_events_are_excluded_from_stats() {
    let (_guard, store) = open_pinned_store();
    let monday = chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
    // 分析判定不可记录 → 排除。
    seed_event(
        &store,
        "chitchat 默认不记",
        local_noon_utc(monday),
        &analysis_json("chitchat", &["zoe"], &[], &[], false),
    );
    // 分析判不可记录但手动决策翻案 → 计入（manual 覆盖）。
    let revived = seed_event(
        &store,
        "手动翻案",
        local_noon_utc(monday),
        &analysis_json("work", &["amma"], &["p1"], &["coding"], false),
    );
    seed_decision(&store, &revived, true);
    let stats = get_period_stats("week".into(), "2026-10-14".into()).unwrap();
    assert_eq!(stats.event_count, 1, "只有翻案事件计入");
    assert_eq!(stats.by_person.len(), 1);
    assert_eq!(stats.by_person[0].label, "amma");
}

fn seed_events_with_dates(store: &Store, events: &[(&str, chrono::NaiveDate)], recordable: bool) {
    for (text, date) in events {
        seed_event(
            store,
            text,
            local_noon_utc(*date),
            &analysis_json("work", &[], &[], &[], recordable),
        );
    }
}
