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

#[test]
fn derivative_of_unattributed_topic_degrades_to_no_dependency_but_fails_when_stale() {
    // 事故 2026-11-02：「存为主题」创建的无来源 topic 页无法被派生依赖
    // （capture 的引用核验拒收），用户在页面聊天草拟派生产物后确认执行
    // 报「上游知识已失效」。无依据 ≠ 失效：派生侧降级为不建依赖边，
    // 保留 based_on 追溯；失效类原因仍然拒绝。
    let db = Db::new();
    let s = &db.store;
    let topic = s
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "topic-15e2de08".into(),
                kind: "topic".into(),
                title: "一个女装换装的应用".into(),
                summary: "用户粘贴的主题页".into(),
                content_md: "应用背景与定位正文".into(),
                tags: vec!["topic".into()],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "用户粘贴文本导入".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap()
        .page;
    // 无来源 topic 页依旧不可作引用依据（引用契约不变）。
    assert!(citation_for_page(s, &topic, "test".into(), 100)
        .unwrap()
        .is_none());
    // 但派生物可以创建，且降级为无依赖边。
    let derivative = s
        .create_derivative_with_generation(
            &topic.slug,
            "抖音文案",
            "宣传计划",
            "系列文案正文",
            "草拟测试",
            None,
        )
        .unwrap();
    assert_eq!(derivative.based_on.as_deref(), Some(topic.slug.as_str()));
    let edges: i64 = s
        .connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_dependencies WHERE page_id=?1",
            [&derivative.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(edges, 0, "无依据上游应降级为不建依赖边");
    // 派生物自身无来源，采用时应跳过引用核对（失效由 stale/归档检查覆盖）。
    let revision: String = s
        .connection
        .query_row(
            "SELECT id FROM wiki_revisions WHERE page_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT 1",
            [&derivative.id],
            |r| r.get(0),
        )
        .unwrap();
    workflows::adopt(s, &derivative.slug, &revision, true).unwrap();
    // 对照：归档上游仍然拒绝派生。
    s.connection
        .execute(
            "UPDATE wiki_pages SET status='archived' WHERE id=?1",
            [&topic.id],
        )
        .unwrap();
    assert!(s
        .create_derivative_with_generation(
            &topic.slug,
            "抖音文案",
            "第二条",
            "正文",
            "草拟测试",
            None,
        )
        .is_err());
}

#[test]
fn save_wiki_revision_derivative_executes_on_unattributed_topic() {
    // 端到端回归（事故 2026-11-02 提案 4d173751）：页面聊天草拟派生产物 →
    let db = Db::new();
    let s = &db.store;
    let topic = s
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "topic-15e2de08".into(),
                kind: "topic".into(),
                title: "一个女装换装的应用".into(),
                summary: "x".into(),
                content_md: "应用正文".into(),
                tags: vec!["topic".into()],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "用户粘贴文本导入".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap()
        .page;
    let conv = s.create_conversation(Some("t"), None).unwrap();
    let basis = crate::knowledge::authoring::revision_basis(s, &topic).unwrap();
    let args = serde_json::json!({
        "slug": topic.slug,
        "title": "nvzhuang 抖音宣传系列·文案与素材计划",
        "content_md": "## 文案\n正文",
        "change_note": "草拟抖音宣传系列文案与素材清单",
        "save_as": "derivative",
        "content_type": "抖音文案",
        "base_hash": crate::storage::knowledge::content_hash("应用正文"),
        "revision_basis": basis,
        "page_task": {
            "kind": "local_question",
            "pages": [{
                "slug": topic.slug,
                "revision": basis,
            }],
        },
    });
    let pa = PendingAction {
        id: "test-1".into(),
        conversation_id: conv,
        action: "save_wiki_revision".into(),
        args_json: args.to_string(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    let result = crate::ai::tool::execute_pending_action(s, &pa);
    match &result {
        Ok(msg) => println!("SUCCESS: {msg}"),
        Err(e) => panic!("REPLAY FAILED: {e:#}"),
    }
}

#[test]
fn knowledge_draft_with_parent_slug_attaches_as_derivative() {
    // 事故 2026-11-02 §7：页面聊天草拟的知识页应能挂靠到父页（based_on + artifact_versions）
    let db = Db::new();
    let s = &db.store;
    let parent = s
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "topic-15e2de08".into(),
                kind: "topic".into(),
                title: "一个女装换装的应用".into(),
                summary: "x".into(),
                content_md: "应用正文".into(),
                tags: vec!["topic".into()],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "用户粘贴文本导入".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap()
        .page;
    let draft = WikiPageDraft {
        slug: "kb-nvzhuang-test-12345678".into(),
        kind: "method".into(),
        title: "nvzhuang 抖音宣传系列·文案与素材计划".into(),
        summary: "文案".into(),
        content_md: "## 文案\n正文".into(),
        tags: vec!["method".into()],
        source_event_ids: vec![],
        status: "active".into(),
        reason: "由 AI 工具草拟、用户确认后保存".into(),
        source_url: None,
    };
    let args = serde_json::json!({ "parent_slug": parent.slug });
    let page = crate::knowledge::authoring::save_draft(s, &draft, &args).unwrap();
    assert_eq!(page.based_on.as_deref(), Some("topic-15e2de08"));
    let n: i64 = s
        .connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_artifact_versions WHERE page_id=?1 AND parent_id=?2",
            [&page.id, &parent.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);
    // 父页详情产物版本查询能返回
    let listed = crate::knowledge::workflows::versions(s, &parent.slug, 0).unwrap();
    assert!(listed.iter().any(|v| v.slug == page.slug));
}

#[test]
fn knowledge_draft_parent_slug_end_to_end_via_pending_action() {
    use crate::storage::PendingAction;
    let db = Db::new();
    let s = &db.store;
    let parent = s
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "topic-15e2de08".into(),
                kind: "topic".into(),
                title: "一个女装换装的应用".into(),
                summary: "x".into(),
                content_md: "应用正文".into(),
                tags: vec!["topic".into()],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "用户粘贴文本导入".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap()
        .page;
    let conv = s.create_conversation(Some("t"), None).unwrap();
    let args = serde_json::json!({
        "title": "nvzhuang 部署手册·派生测试",
        "content_md": "## 部署\n三步",
        "kind": "method",
        "tags": ["nvzhuang"],
        "verified_evidence": [],
        "applicable_when": "",
        "parent_slug": parent.slug,
    });
    let pa = PendingAction {
        id: "test-parent-1".into(),
        conversation_id: conv.clone(),
        action: "save_knowledge_draft".into(),
        args_json: args.to_string(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    let msg = crate::ai::tool::execute_pending_action(s, &pa).unwrap();
    println!("EXEC: {msg}");
    let page = s.find_wiki_page_by_title("nvzhuang 部署手册·派生测试").unwrap().unwrap();
    assert_eq!(page.based_on.as_deref(), Some("topic-15e2de08"));
    // 父页不存在 → 拒绝且不落库
    let bad = serde_json::json!({ "title":"孤儿页测试","content_md":"x","parent_slug":"no-such-page" });
    let pa2 = PendingAction {
        id: "test-parent-2".into(),
        conversation_id: conv,
        action: "save_knowledge_draft".into(),
        args_json: bad.to_string(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    assert!(crate::ai::tool::execute_pending_action(s, &pa2).is_err());
    assert!(s.find_wiki_page_by_title("孤儿页测试").unwrap().is_none());
}
