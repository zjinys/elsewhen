use super::*;

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
