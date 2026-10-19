#[test]
fn incubation_gate_blocks_automatic_delivery_but_not_explicit_reads() {
    let db = Db::new();
    let s = &db.store;
    let page = incubating_source(
        s,
        "https://example.com/incubate",
        "滨河步道的无障碍坡道设计",
    );
    // 自动投递被闸门拦下：召回、造引用都不产出
    assert!(
        crate::knowledge::select_knowledge(s, "无障碍坡道设计", "test", 400)
            .unwrap()
            .is_empty()
    );
    assert!(
        crate::knowledge::citation_for_page(s, &page, "自动通道".into(), 100)
            .unwrap()
            .is_none()
    );
    // 健康巡检可视：孵化中 + 从未露面
    let report = health_report(s, 30).unwrap();
    assert_eq!(report.pages_incubating, 1);
    // 按需读取豁免：用户明确打开页面时造引用照常
    assert!(
        crate::knowledge::citation_for_page_explicit(s, &page, "用户明确打开此页".into(), 100)
            .unwrap()
            .is_some()
    );
    // 纳入后闸门放行自动投递
    s.set_wiki_opinion(&page.slug, Some("endorse")).unwrap();
    assert_eq!(
        s.get_wiki_page(&page.slug)
            .unwrap()
            .unwrap()
            .opinion
            .as_deref(),
        Some("endorse"),
        "確认双写后 wiki_pages 已纳入"
    );
    let refreshed = s.get_wiki_page(&page.slug).unwrap().unwrap();
    assert!(
        crate::knowledge::citation_for_page(s, &refreshed, "纳入后自动通道".into(), 100)
            .unwrap()
            .is_some()
    );
    assert_eq!(health_report(s, 30).unwrap().pages_incubating, 0);
}

#[test]
fn decide_source_requires_adopted_derivative_for_endorse_and_logs_negative_knowledge() {
    let db = Db::new();
    let s = &db.store;
    let page = incubating_source(s, "https://example.com/decide", "间歇性断食的代谢机制");
    use crate::knowledge::workflows;
    workflows::decide_source(s, &page.slug, "endorse", "").unwrap_err();
    // 造派生稿并采用 →ENDORSE 的前置条件满足
    let der = s
        .create_derivative(
            &page.slug,
            "AI 加工",
            "断食要点整理",
            "机制整理：进食窗口、胰岛素曲线",
            "test derivative",
        )
        .unwrap();
    let revision: String = s
        .connection
        .query_row(
            "SELECT id FROM wiki_revisions WHERE page_id=?1 ORDER BY created_at DESC LIMIT 1",
            [&der.id],
            |r| r.get(0),
        )
        .unwrap();
    crate::knowledge::workflows::adopt(s, &der.slug, &revision, true).unwrap();
    let endorsed = workflows::decide_source(s, &page.slug, "endorse", "").unwrap();
    assert_eq!(endorsed.opinion.as_deref(), Some("endorse"));
    assert_eq!(
        s.connection
            .query_row::<String, _, _>(
                "SELECT state FROM knowledge_reading_states r JOIN wiki_pages p ON p.id=r.page_id WHERE p.slug=?1",
                [&page.slug], |r| r.get(0)
            )
            .unwrap(),
        "adopted",
        "纳入即置 adopted 阅读状态"
    );

    // 丢弃为负知识：必填原因 + archived 留痕 + 台账写 reject_reason
    let page2 = incubating_source(s, "https://example.com/discard", "降压药的服药时间表");
    let der2 = s
        .create_derivative(&page2.slug, "AI 加工", "服药时间整理", "整理稿", "test")
        .unwrap();
    let revision2: String = s
        .connection
        .query_row(
            "SELECT id FROM wiki_revisions WHERE page_id=?1 ORDER BY created_at DESC LIMIT 1",
            [&der2.id],
            |r| r.get(0),
        )
        .unwrap();
    crate::knowledge::workflows::adopt(s, &der2.slug, &revision2, true).unwrap();
    workflows::decide_source(s, &page2.slug, "reject", "").unwrap_err();
    let rejected =
        workflows::decide_source(s, &page2.slug, "reject", "与用户医嘱冲突，不采信").unwrap();
    assert_eq!(rejected.status, "archived", "丢弃 = 留痕不物理删");
    assert_eq!(
        s.rejection_for_source(Some("https://example.com/discard"), "降压药的服药时间表")
            .unwrap()
            .as_deref(),
        Some("与用户医嘱冲突，不采信"),
        "同 identity 再导入时能看到否定原因"
    );
    let rejected_list = s.rejected_sources().unwrap();
    assert_eq!(rejected_list.len(), 1);
    assert_eq!(rejected_list[0].reject_reason, "与用户医嘱冲突，不采信");
    assert_eq!(
        health_report(s, 30).unwrap().negative_sources,
        1,
        "负知识体量进健康巡检"
    );
    workflows::decide_source(s, &page2.slug, "endorse", "").unwrap_err();
}
