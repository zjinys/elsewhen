use super::*;

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
