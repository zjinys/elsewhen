fn repair_fixture(
    s: &Store,
) -> (
    crate::storage::WikiPage,
    crate::storage::WikiPage,
    crate::storage::WikiPage,
) {
    let a = source(
        s,
        "https://example.com/repair-a",
        "来源甲明确建议在线操作。",
    );
    let b = source(
        s,
        "https://example.com/repair-b",
        "来源乙强调仅允许离线操作。",
    );
    let id = propose_knowledge(s, &a.slug, "method", &compile_stub()).unwrap();
    let p = s.resolve_knowledge_proposal(&id, true).unwrap().unwrap();
    s.bind_page_sources(&p.id, &[s.source_history(&b.slug).unwrap()[0].id.clone()])
        .unwrap();
    (a, b, p)
}

#[test]
fn source_repair_replaces_evidence_atomically_preserves_rules_and_rejects_stale_confirmation() {
    let db = Db::new();
    let s = &db.store;
    let (a, b, p) = repair_fixture(s);
    s.set_knowledge_metadata(&p.slug, "个人离线规则", "rule")
        .unwrap();
    s.set_wiki_opinion(&a.slug, Some("reject")).unwrap();
    assert!(citation_for_page(s, &p, "test".into(), 100)
        .unwrap()
        .is_none());
    assert!(propose_knowledge(s, &p.slug, "revision", &compile_stub()).is_err());
    let b_id = s.source_history(&b.slug).unwrap()[0].id.clone();
    let id = prepare_source_repair(s, &p.slug, &[b_id.clone()], &compile_stub()).unwrap();
    assert_eq!(s.page_source_snapshots(&p.slug).unwrap().len(), 2);
    assert!(s.publish_reference_in_tx(&id).unwrap().is_none());
    // A source repair cannot be accepted piecemeal to relabel old paragraphs.
    s.resolve_knowledge_proposal(&id, true).unwrap();
    assert_eq!(s.page_source_snapshots(&p.slug).unwrap()[0].id, b_id);
    assert_eq!(s.knowledge_metadata(&p.slug).unwrap().strength, "rule");
    assert!(citation_for_page(
        s,
        &s.get_wiki_page(&p.slug).unwrap().unwrap(),
        "test".into(),
        100
    )
    .unwrap()
    .is_some());
    let next=prepare_source_repair(s,&p.slug,&[b_id],&Stub(r#"{"title":"新知识","content_md":"仅限离线","applicable_when":"离线","reason":"修复"}"#.into())).unwrap();
    source(s, "https://example.com/repair-b", "乙的新版本");
    assert!(s.resolve_knowledge_proposal(&next, true).is_err());
    assert_eq!(
        s.source_history(&a.slug).unwrap()[0].content_md,
        a.content_md
    );
    assert!(
        prepare_source_repair(s, &p.slug, &[], &Replies(std::cell::RefCell::new(vec![])))
            .unwrap_err()
            .to_string()
            .contains("依据不足")
    );
}

#[test]
fn reendorsed_source_recovers_skipped_refresh_after_restart_without_revoking_rejection() {
    let db = Db::new();
    let s = &db.store;
    let (a, _, p) = repair_fixture(s);
    source(s, "https://example.com/repair-a", "甲 v2");
    s.set_wiki_opinion(&a.slug, Some("reject")).unwrap();
    crate::knowledge_background::run_knowledge_refresh(
        s,
        &Replies(std::cell::RefCell::new(vec![])),
    )
    .unwrap();
    let status: String = s
        .connection
        .query_row(
            "SELECT status FROM knowledge_refresh_jobs WHERE page_id=?1",
            [&p.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "skipped");
    s.set_wiki_opinion(&a.slug, Some("endorse")).unwrap();
    let reopened = Store::open(&db.path).unwrap();
    assert_eq!(
        crate::knowledge_background::run_knowledge_refresh(&reopened, &compile_stub()).unwrap(),
        1
    );
    let proposal = reopened
        .knowledge_proposals(Some(&p.slug))
        .unwrap()
        .into_iter()
        .find(|p| p.status == "pending")
        .unwrap();
    reopened
        .resolve_knowledge_proposal(&proposal.id, false)
        .unwrap();
    reopened.set_wiki_opinion(&a.slug, Some("reject")).unwrap();
    reopened.set_wiki_opinion(&a.slug, None).unwrap();
    assert_eq!(
        crate::knowledge_background::run_knowledge_refresh(
            &reopened,
            &Replies(std::cell::RefCell::new(vec![]))
        )
        .unwrap(),
        0
    );
    assert!(reopened
        .knowledge_proposals(Some(&p.slug))
        .unwrap()
        .iter()
        .any(|p| p.id == proposal.id && p.status == "rejected"));
}

#[test]
fn human_correction_invalidates_transitive_derivatives_and_review_uses_current_upstream() {
    let db = Db::new();
    let s = &db.store;
    let (_, _, p) = repair_fixture(s);
    let d = s
        .create_derivative(&p.slug, "摘要", "产物", "旧产物", "测试")
        .unwrap();
    let d2 = s
        .create_derivative(&d.slug, "文案", "下游", "旧下游", "测试")
        .unwrap();
    assert!(!dependencies::stale(s, &d2.id).unwrap());
    s.set_knowledge_metadata(&p.slug, "人工纠正：仅限离线", "rule")
        .unwrap();
    assert!(dependencies::stale(s, &d.id).unwrap());
    assert!(dependencies::stale(s, &d2.id).unwrap());
    assert!(s
        .knowledge_issues(None)
        .unwrap()
        .iter()
        .any(|i| i.page_slug == d.slug && i.kind == "upstream_changed"));
    assert!(citation_for_page(s, &d2, "test".into(), 100)
        .unwrap()
        .is_none());
    struct Corrected;
    impl AiProvider for Corrected {
        fn generate_reply_with_tools(
            &self,
            messages: Vec<ContextMessage>,
            _: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            assert!(messages
                .iter()
                .any(|m| m.content.contains("人工纠正：仅限离线")));
            Ok(AiReply::text(compile_stub().0))
        }
    }
    let id = propose_knowledge(s, &d.slug, "revision", &Corrected).unwrap();
    s.save_wiki_page_content(&p.slug, "人工再次纠正", "user", None)
        .unwrap();
    assert!(s.resolve_knowledge_proposal(&id, true).is_err());
    let id = propose_knowledge(s, &d.slug, "revision", &Corrected).unwrap();
    s.resolve_knowledge_proposal(&id, true).unwrap();
    assert!(!dependencies::stale(s, &d.id).unwrap());
    assert!(dependencies::stale(s, &d2.id).unwrap());
    let id = propose_knowledge(s, &d2.slug, "revision", &compile_stub()).unwrap();
    s.resolve_knowledge_proposal(&id, true).unwrap();
    assert!(!dependencies::stale(s, &d2.id).unwrap());
}

#[test]
fn cross_page_issue_resolution_requires_matching_evidence_current_revision_and_explanation() {
    let db = Db::new();
    let s = &db.store;
    let (a, b, p) = repair_fixture(s);
    let ids = vec![
        s.source_history(&a.slug).unwrap()[0].id.clone(),
        s.source_history(&b.slug).unwrap()[0].id.clone(),
    ];
    s.connection.execute("INSERT INTO knowledge_semantic_issues(fingerprint,page_id,page_hash,kind,description,snapshot_ids,created_at) VALUES('conflict',?1,?2,'conflict','甲乙冲突',?3,'2026-09-29')",rusqlite::params![a.id,crate::storage::knowledge::content_hash(&a.content_md),serde_json::to_string(&ids).unwrap()]).unwrap();
    s.save_wiki_page_content(&p.slug, "区分甲乙两种适用条件", "user", None)
        .unwrap();
    let revision:String=s.connection.query_row("SELECT id FROM wiki_revisions WHERE page_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT 1",[&p.id],|r|r.get(0)).unwrap();
    assert!(maintenance::resolution_targets(s, "conflict")
        .unwrap()
        .iter()
        .any(|v| v.id == p.id));
    assert!(maintenance::resolve_issue(s, "conflict", &a.slug, &revision, "说明").is_err());
    assert!(maintenance::resolve_issue(s, "conflict", &p.slug, &revision, "").is_err());
    assert!(maintenance::resolve_issue(s, "conflict", &p.slug, "wrong", "说明").is_err());
    maintenance::resolve_issue(
        s,
        "conflict",
        &p.slug,
        &revision,
        "分别说明两种条件，不裁定谁对谁错",
    )
    .unwrap();
    assert!(!s
        .knowledge_issues(None)
        .unwrap()
        .iter()
        .any(|i| i.fingerprint == "conflict"));
    for slug in [&a.slug, &p.slug] {
        let history = maintenance::history(s, slug, 0).unwrap();
        let record = history.iter().find(|r| r.id == "conflict").unwrap();
        assert_eq!(record.revision_id.as_ref(), Some(&revision));
        assert_eq!(
            record.result_content.as_deref(),
            Some("区分甲乙两种适用条件")
        );
        assert_eq!(record.description, "甲乙冲突");
    }
}

#[test]
fn review_history_keeps_rejection_and_dismissal_after_page_changes() {
    let db = Db::new();
    let s = &db.store;
    let (_, _, p) = repair_fixture(s);
    s.save_wiki_page_content(&p.slug, "原知识 [[missing-review]]", "user", None)
        .unwrap();
    let issue = s
        .knowledge_issues(Some(&p.slug))
        .unwrap()
        .into_iter()
        .find(|i| i.kind == "broken_link")
        .unwrap();
    s.dismiss_knowledge_issue(&issue.fingerprint).unwrap();
    let id = propose_knowledge(s, &p.slug, "revision", &compile_stub()).unwrap();
    s.resolve_knowledge_proposal(&id, false).unwrap();
    s.save_wiki_page_content(&p.slug, "后来编辑", "user", None)
        .unwrap();
    let h = maintenance::history(s, &p.slug, 0).unwrap();
    let rejected = h.iter().find(|r| r.id == id).unwrap();
    assert_eq!(
        rejected.before_content.as_deref(),
        Some("原知识 [[missing-review]]")
    );
    assert_eq!(
        rejected.original_content.as_deref(),
        Some("先开启事务，再提交写入。")
    );
    assert_eq!(rejected.result_content, None);
    assert!(h
        .iter()
        .any(|r| r.action == "dismissed" && r.description.contains("missing-review")));
}

#[test]
fn work_inventory_includes_unstarted_reading_waiting_refresh_and_all_pages_beyond_fifty_runs() {
    let db = Db::new();
    let s = &db.store;
    let (a, _, p) = repair_fixture(s);
    source(s, "https://example.com/repair-a", &"长文".repeat(900));
    for n in 0..30 {
        source(s, &format!("https://example.com/queue-{n}"), "另一份原料");
    }
    let first = queue::list(s, 0, None).unwrap();
    assert!(first.waiting >= 3);
    assert!(first.has_more);
    assert!(first.total > 60);
    let mut all = first.items;
    all.extend(queue::list(s, 50, None).unwrap().items);
    assert!(all
        .iter()
        .any(|i| i.page_slug == p.slug && i.status == "waiting"));
    assert!(all
        .iter()
        .any(|i| i.page_slug == a.slug && i.task == "source-reading" && i.status == "pending"));
    s.set_wiki_opinion(&a.slug, Some("reject")).unwrap();
    let skipped = queue::list(s, 0, Some("skipped")).unwrap();
    assert!(skipped.items.iter().any(|i| i.page_slug == p.slug));
    assert!(skipped.items.iter().all(|i| i.status == "skipped"));
}
