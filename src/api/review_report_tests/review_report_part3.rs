use super::*;

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
