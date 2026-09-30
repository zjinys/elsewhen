#[test]
fn reading_and_artifact_versions_keep_originals_and_select_exact_revision() {
    let db = Db::new();
    let s = &db.store;
    let p = source(s, "https://example.com/workflow", "原文不能被采用流程覆盖");
    let original = s.source_history(&p.slug).unwrap()[0].content_md.clone();
    workflows::set_reading_state(s, &p.slug, "valuable").unwrap();
    assert_eq!(
        workflows::browse(s, "", Some("imported"), None, None, Some("valuable"), 0)
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(workflows::set_reading_state(s, &p.slug, "invented").is_err());
    let a = s
        .create_derivative(&p.slug, "脚本", "第一版", "脚本正文 A", "test")
        .unwrap();
    let b = s
        .create_derivative(&p.slug, "脚本", "第二版", "脚本正文 B", "test")
        .unwrap();
    let revision = |page: &crate::storage::WikiPage| -> String {
        s.connection.query_row("SELECT id FROM wiki_revisions WHERE page_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT 1",[&page.id],|r|r.get(0)).unwrap()
    };
    workflows::adopt(s, &a.slug, &revision(&a), true).unwrap();
    workflows::adopt(s, &b.slug, &revision(&b), true).unwrap();
    let v = workflows::versions(s, &p.slug, 0).unwrap();
    assert_eq!(v.iter().filter(|v| v.adopted).count(), 1);
    assert_eq!(v.iter().find(|v| v.adopted).unwrap().version, 2);
    let old = revision(&b);
    s.save_wiki_page_content(&b.slug, "人工更新的脚本", "user", None)
        .unwrap();
    assert!(workflows::adopt(s, &b.slug, &old, true).is_err());
    assert_eq!(
        workflows::versions(s, &p.slug, 0).unwrap()[0]
            .adopted_revision
            .as_deref(),
        Some(old.as_str())
    );
    workflows::adopt(s, &b.slug, &revision(&b), false).unwrap();
    assert!(workflows::versions(s, &p.slug, 0)
        .unwrap()
        .iter()
        .all(|v| !v.adopted));
    assert_eq!(
        s.get_wiki_page(&p.slug).unwrap().unwrap().content_md,
        original
    );
    assert_eq!(s.source_history(&p.slug).unwrap()[0].content_md, original);
}

#[test]
fn organization_history_and_undo_are_atomic_and_guard_later_changes() {
    let db = Db::new();
    let s = &db.store;
    let (_, _, p) = repair_fixture(s);
    s.connection
        .execute("UPDATE wiki_pages SET kind='topic' WHERE id=?1", [&p.id])
        .unwrap();
    let snapshots = s
        .page_source_snapshots(&p.slug)
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect::<Vec<_>>();
    let result = serde_json::json!([
        {"title":"拆分甲","content_md":"第一部分","applicable_when":"甲","snapshot_ids":snapshots,"event_ids":[]},
        {"title":"拆分乙","content_md":"第二部分","applicable_when":"乙","snapshot_ids":snapshots,"event_ids":[]}
    ]);
    let id =
        organization::prepare(s, &[p.slug.clone()], "split", &Stub(result.to_string())).unwrap();
    let out = organization::resolve(s, &id, true).unwrap();
    assert_eq!(
        organization::history(s, &out[0], 0).unwrap()[0].status,
        "accepted"
    );
    organization::undo(s, &id).unwrap();
    assert_eq!(s.get_wiki_page(&p.slug).unwrap().unwrap().status, "active");
    assert_eq!(
        s.get_wiki_page(&out[0]).unwrap().unwrap().status,
        "archived"
    );
    assert_eq!(
        organization::history(s, &out[0], 0).unwrap()[0].status,
        "undone"
    );
    assert!(organization::undo(s, &id).is_err());
    let mut result2 = result.clone();
    result2[0]["title"] = "另甲".into();
    result2[1]["title"] = "另乙".into();
    let id =
        organization::prepare(s, &[p.slug.clone()], "split", &Stub(result2.to_string())).unwrap();
    let out = organization::resolve(s, &id, true).unwrap();
    s.save_wiki_page_content(&out[0], "用户已编辑", "user", None)
        .unwrap();
    assert!(organization::undo(s, &id).is_err());
    assert_eq!(
        s.get_wiki_page(&p.slug).unwrap().unwrap().status,
        "archived"
    );
    assert_eq!(
        s.get_wiki_page(&out[0]).unwrap().unwrap().content_md,
        "用户已编辑"
    );
}

#[test]
fn dependencies_prepare_in_order_and_preserve_rejected_decisions() {
    let db = Db::new();
    let s = &db.store;
    let (_, _, p) = repair_fixture(s);
    let a = s
        .create_derivative(&p.slug, "摘要", "A", "旧摘要", "test")
        .unwrap();
    let b = s
        .create_derivative(&a.slug, "脚本", "B", "旧脚本", "test")
        .unwrap();
    s.save_wiki_page_content(&p.slug, "新增人工纠正", "user", None)
        .unwrap();
    crate::knowledge_background::run_knowledge_refresh(s, &compile_stub()).unwrap();
    let id: String = s
        .connection
        .query_row(
            "SELECT id FROM knowledge_proposals WHERE target_slug=?1 AND status='pending'",
            [&a.slug],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        s.get_wiki_page(&a.slug).unwrap().unwrap().content_md,
        "旧摘要"
    );
    assert_eq!(
        s.connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_proposals WHERE target_slug=?1",
                [&b.slug],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    s.resolve_knowledge_proposal(&id, false).unwrap();
    crate::knowledge_background::run_knowledge_refresh(
        s,
        &Replies(std::cell::RefCell::new(vec![])),
    )
    .unwrap();
    let pending: i64 = s
        .connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_proposals WHERE target_slug=?1 AND status='pending'",
            [&a.slug],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pending, 0);
    s.save_wiki_page_content(&p.slug, "第二次人工纠正", "user", None)
        .unwrap();
    crate::knowledge_background::run_knowledge_refresh(s, &compile_stub()).unwrap();
    let id: String = s
        .connection
        .query_row(
            "SELECT id FROM knowledge_proposals WHERE target_slug=?1 AND status='pending'",
            [&a.slug],
            |r| r.get(0),
        )
        .unwrap();
    s.resolve_knowledge_proposal(&id, true).unwrap();
    crate::knowledge_background::run_knowledge_refresh(s, &compile_stub()).unwrap();
    assert_eq!(s.connection.query_row("SELECT COUNT(*) FROM knowledge_proposals WHERE target_slug=?1 AND status='pending'",[&b.slug],|r|r.get::<_,i64>(0)).unwrap(),1);
}

#[test]
fn suggestion_feedback_is_reversible_scoped_and_does_not_execute_actions() {
    let db = Db::new();
    let s = &db.store;
    let conversation = s.create_conversation(None, None).unwrap();
    let user = s
        .send_message(&conversation, "user", "请给个建议", None)
        .unwrap();
    let msg = s
        .send_message(
            &conversation,
            "assistant",
            "建议先备份，再更新系统。",
            Some(&user),
        )
        .unwrap();
    assert!(workflows::save_feedback(s, &user, "accepted", "请给个建议", None).is_err());
    assert!(workflows::save_feedback(s, &msg, "accepted", "虚构的建议", None).is_err());
    workflows::save_feedback(s, &msg, "ignored", "先备份", None).unwrap();
    assert!(workflows::feedback_context(s, &conversation)
        .unwrap()
        .contains("ignored"));
    workflows::save_feedback(s, &msg, "rewritten", "先备份", Some("先确认备份可恢复")).unwrap();
    assert!(workflows::feedback_context(s, &conversation)
        .unwrap()
        .contains("先确认备份可恢复"));
    workflows::save_feedback(s, &msg, "cleared", "先备份", None).unwrap();
    assert!(workflows::feedback_context(s, &conversation)
        .unwrap()
        .is_empty());
    assert_eq!(
        s.connection
            .query_row("SELECT COUNT(*) FROM suggestion_feedback", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert!(s
        .pending_actions_for_conversation(&conversation)
        .unwrap()
        .is_empty());
}

#[test]
fn generation_provenance_survives_provider_changes_and_legacy_epochs_are_checked() {
    let db = Db::new();
    let s = &db.store;
    let (_, _, p) = repair_fixture(s);
    let conversation = s.create_conversation(None, None).unwrap();
    s.send_message(&conversation, "user", "请生成一篇离线使用的脚本", None)
        .unwrap();
    workflows::record_generation(s, &conversation, "脚本正文", Some("actual-model-v1")).unwrap();
    let d = s
        .create_derivative(&p.slug, "脚本", "版本一", "脚本正文", "test")
        .unwrap();
    workflows::attach_generation(s, &d, &conversation).unwrap();
    let v = workflows::versions(s, &p.slug, 0).unwrap();
    assert_eq!(v[0].model.as_deref(), Some("actual-model-v1"));
    assert_eq!(
        v[0].instruction.as_deref(),
        Some("请生成一篇离线使用的脚本")
    );
    s.connection
        .execute(
            "UPDATE knowledge_dependencies SET basis_epoch=NULL WHERE page_id=?1",
            [&d.id],
        )
        .unwrap();
    dependencies::backfill_epochs(s).unwrap();
    assert!(!dependencies::stale(s, &d.id).unwrap());
    s.save_wiki_page_content(&p.slug, "改变上游正文", "user", None)
        .unwrap();
    s.connection
        .execute(
            "UPDATE knowledge_dependencies SET basis_epoch=NULL WHERE page_id=?1",
            [&d.id],
        )
        .unwrap();
    dependencies::backfill_epochs(s).unwrap();
    assert!(dependencies::stale(s, &d.id).unwrap());
}

#[test]
#[ignore = "synthetic scale benchmark; run explicitly with --ignored --nocapture"]
fn synthetic_wiki_scale() {
    use std::time::Instant;
    let db = Db::new();
    let s = &db.store;
    let n: usize = std::env::var("ELSEWHEN_SCALE_PAGES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10000);
    assert!((100..=100000).contains(&n));
    let text = "这是一段用于测量全文查询和后台队列的合成资料，包含适用条件和反例。".repeat(60);
    let tx = s.connection.unchecked_transaction().unwrap();
    for i in 0..n {
        let id = format!("scale-{i:06}");
        s.connection.execute("INSERT INTO wiki_pages(id,slug,kind,title,content_md,first_seen_at,last_seen_at,created_at,updated_at,area) VALUES(?1,?1,'source',?1,?2,'2026-09-29','2026-09-29','2026-09-29','2026-09-29','imported')",rusqlite::params![id,text]).unwrap();
        s.connection.execute("INSERT INTO knowledge_sources(id,identity,kind,created_at) VALUES(?1,?1,'url','2026-09-29')",[&id]).unwrap();
        s.connection
            .execute("INSERT INTO knowledge_source_pages VALUES(?1,?1)", [&id])
            .unwrap();
        s.connection
            .execute(
                "INSERT INTO knowledge_snapshots VALUES(?1,?1,1,?1,?2,?1,'2026-09-29')",
                rusqlite::params![id, text],
            )
            .unwrap();
    }
    tx.commit().unwrap();
    let start = Instant::now();
    let q = queue::list(s, 0, None).unwrap();
    let queue_ms = start.elapsed().as_millis();
    assert_eq!(q.items.len(), 50);
    assert_eq!(q.total, n as i64 * 3);
    let start = Instant::now();
    let page = workflows::browse(s, "", Some("imported"), None, None, None, 0).unwrap();
    let first_ms = start.elapsed().as_millis();
    assert_eq!(page.items.len(), 50);
    let start = Instant::now();
    let _ = workflows::browse(s, "适用条件", Some("imported"), None, None, None, 0).unwrap();
    let search_ms = start.elapsed().as_millis();
    let start=Instant::now();let _=workflows::browse(s,"不存在的罕见关键词",Some("imported"),None,None,None,0).unwrap();let missing_ms=start.elapsed().as_millis();
    let start=Instant::now();let _=workflows::browse(s,"",Some("imported"),None,None,None,(n-100) as i64).unwrap();let deep_ms=start.elapsed().as_millis();
    let start = Instant::now();
    let _ = select_knowledge(s, "适用条件 反例", "scale", 5000).unwrap();
    let retrieval_ms = start.elapsed().as_millis();
    eprintln!("scale pages={n},chars_per_page={},db_bytes={},queue_ms={queue_ms},first_page_ms={first_ms},search_ms={search_ms},missing_ms={missing_ms},deep_ms={deep_ms},retrieval_ms={retrieval_ms}",text.chars().count(),std::fs::metadata(&db.path).unwrap().len());
}
