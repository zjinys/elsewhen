fn auto_stub() -> Stub {
    Stub(r#"{"kind":"method","title":"事务边界方法","content_md":"先明确原子边界，再提交。","applicable_when":"多步写入","reason":"原文说明了提交边界"}"#.into())
}

struct Replies(std::cell::RefCell<Vec<String>>);
impl AiProvider for Replies {
    fn generate_reply_with_tools(
        &self,
        _: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        let text = self.0.borrow_mut().remove(0);
        Ok(AiReply::text(text))
    }
}

#[test]
fn compilation_repairs_invalid_output_once_without_saving_half_a_page() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/repair", "SQLite 事务边界");
    let replies = Replies(std::cell::RefCell::new(vec![
        "好的，我来整理".into(),
        compile_stub().0,
    ]));
    let id = propose_knowledge(s, &original.slug, "method", &replies).unwrap();
    assert!(replies.0.borrow().is_empty());
    assert_eq!(s.list_wiki_pages(None, None).unwrap().len(), 1);
    assert_eq!(s.knowledge_proposals(None).unwrap().len(), 1);
    s.resolve_knowledge_proposal(&id, true).unwrap();
    let invalid = Replies(std::cell::RefCell::new(vec!["".into(), "{}".into()]));
    assert!(propose_knowledge(s, &original.slug, "case", &invalid).is_err());
    assert_eq!(s.knowledge_proposals(None).unwrap().len(), 1);
}

#[test]
fn compiled_pages_navigate_both_ways_without_based_on_or_backfill() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/link", "原料");
    let id = propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap();
    let method = s.resolve_knowledge_proposal(&id, true).unwrap().unwrap();
    assert!(method.based_on.is_none());
    assert_eq!(
        s.knowledge_output_pages(&original.slug).unwrap()[0].id,
        method.id
    );
    assert_eq!(
        s.knowledge_origin_pages(&method.slug).unwrap()[0].id,
        original.id
    );
    source(s, "https://example.com/link", "更新原料");
    assert_eq!(
        s.knowledge_origin_pages(&method.slug).unwrap()[0].id,
        original.id
    );
    assert_eq!(s.knowledge_output_pages(&original.slug).unwrap().len(), 1);
}

#[test]
fn automatic_source_compilation_saves_reference_once_and_preserves_original() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/auto", "原文里的事务边界");
    assert_eq!(
        crate::knowledge_background::run_automatic_source_compilation(s, &auto_stub()).unwrap(),
        1
    );
    let method = s.knowledge_output_pages(&original.slug).unwrap().remove(0);
    assert!(method.human_edited_at.is_none());
    let metadata = s.knowledge_metadata(&method.slug).unwrap();
    assert_eq!(metadata.strength, "reference");
    assert!(metadata.confirmed_at.is_none());
    assert_eq!(
        s.get_wiki_page(&original.slug).unwrap().unwrap().content_md,
        original.content_md
    );
    let no_calls = Replies(std::cell::RefCell::new(vec![]));
    assert_eq!(
        crate::knowledge_background::run_automatic_source_compilation(s, &no_calls).unwrap(),
        0
    );
    let reopened = Store::open(&db.path).unwrap();
    assert_eq!(
        crate::knowledge_background::run_automatic_source_compilation(&reopened, &no_calls)
            .unwrap(),
        0
    );
    assert_eq!(s.list_wiki_pages(None, None).unwrap().len(), 2);
}

#[test]
fn automatic_source_compilation_respects_pending_rejection_and_human_edits() {
    for decision in ["pending", "rejected", "edited"] {
        let db = Db::new();
        let s = &db.store;
        let original = source(s, "https://example.com/protect", "原文事务边界");
        let id = propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap();
        if decision == "rejected" {
            s.resolve_knowledge_proposal(&id, false).unwrap();
        }
        if decision == "edited" {
            let page = s.resolve_knowledge_proposal(&id, true).unwrap().unwrap();
            s.save_wiki_page_content(&page.slug, "我的手工结论", "manual", None)
                .unwrap();
        }
        let no_calls = Replies(std::cell::RefCell::new(vec![]));
        assert_eq!(
            crate::knowledge_background::run_automatic_source_compilation(s, &no_calls).unwrap(),
            0
        );
        assert!(
            s.save_automatic_reference(&id).is_err(),
            "手工提案不能被后台采纳"
        );
        assert_eq!(
            s.knowledge_proposals(None).unwrap()[0].status,
            if decision == "edited" {
                "accepted"
            } else {
                decision
            }
        );
    }
}

#[test]
fn automatic_compilation_can_skip_and_retries_failures_after_backoff() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/skip", "没有足够信息");
    let invalid = Stub("not json".into());
    assert_eq!(
        crate::knowledge_background::run_automatic_source_compilation(s, &invalid).unwrap(),
        0
    );
    assert!(s.knowledge_output_pages(&original.slug).unwrap().is_empty());
    let no_calls = Replies(std::cell::RefCell::new(vec![]));
    assert_eq!(
        crate::knowledge_background::run_automatic_source_compilation(s, &no_calls).unwrap(),
        0
    );
    s.connection
        .execute(
            "UPDATE knowledge_background_runs SET finished_at='2000-01-01T00:00:00+00:00'",
            [],
        )
        .unwrap();
    let skip = Stub(r#"{"kind":"skip","reason":"材料不足"}"#.into());
    assert_eq!(
        crate::knowledge_background::run_automatic_source_compilation(s, &skip).unwrap(),
        0
    );
    assert!(s.knowledge_proposals(None).unwrap().is_empty());
    assert_eq!(
        crate::knowledge_background::run_automatic_source_compilation(s, &no_calls).unwrap(),
        0
    );
}

#[test]
fn automatic_compilation_recovers_interrupted_lease_without_duplicate_calls() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/recover", "原料");
    let snapshot = s.source_history(&original.slug).unwrap().remove(0);
    s.connection.execute("INSERT INTO knowledge_background_runs(id,task,input_key,status,started_at) VALUES ('old','source-compilation',?1,'running',?2)",
        rusqlite::params![snapshot.id,chrono::Utc::now().to_rfc3339()]).unwrap();
    let no_calls = Replies(std::cell::RefCell::new(vec![]));
    assert_eq!(
        crate::knowledge_background::run_automatic_source_compilation(s, &no_calls).unwrap(),
        0
    );
    s.connection
        .execute(
            "UPDATE knowledge_background_runs SET started_at='2000-01-01T00:00:00+00:00'",
            [],
        )
        .unwrap();
    assert_eq!(
        crate::knowledge_background::run_automatic_source_compilation(s, &no_calls).unwrap(),
        0
    );
    s.connection
        .execute(
            "UPDATE knowledge_background_runs SET finished_at='2000-01-01T00:00:00+00:00'",
            [],
        )
        .unwrap();
    assert_eq!(
        crate::knowledge_background::run_automatic_source_compilation(s, &auto_stub()).unwrap(),
        1
    );
}

#[test]
fn automatic_reference_updates_new_source_version_but_never_promotes_a_rule() {
    for strength in ["reference", "method", "rule"] {
        let db = Db::new();
        let s = &db.store;
        let original = source(s, "https://example.com/version", "原料版本一");
        crate::knowledge_background::run_automatic_source_compilation(s, &auto_stub()).unwrap();
        let method = s.knowledge_output_pages(&original.slug).unwrap().remove(0);
        source(s, "https://example.com/version", "原料版本二");
        assert_eq!(
            crate::knowledge_background::run_automatic_source_compilation(s, &auto_stub()).unwrap(),
            1
        );
        assert_eq!(
            s.knowledge_output_pages(&original.slug).unwrap()[0].id,
            method.id
        );
        assert_eq!(s.page_source_snapshots(&method.slug).unwrap()[0].version, 2);
        s.set_knowledge_metadata(&method.slug, "我确认的适用条件", strength)
            .unwrap();
        source(s, "https://example.com/version", "原料版本三");
        let no_calls = Replies(std::cell::RefCell::new(vec![]));
        assert_eq!(
            crate::knowledge_background::run_automatic_source_compilation(s, &no_calls).unwrap(),
            0
        );
        assert_eq!(s.page_source_snapshots(&method.slug).unwrap()[0].version, 2);
        assert_eq!(
            s.knowledge_metadata(&method.slug).unwrap().strength,
            strength
        );
    }
}

struct ManualDuringCompilation<'a> {
    store: &'a Store,
    slug: &'a str,
}
impl AiProvider for ManualDuringCompilation<'_> {
    fn generate_reply_with_tools(
        &self,
        _: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        propose_knowledge(self.store, self.slug, "method", &compile_stub())?;
        Ok(AiReply::text(auto_stub().0))
    }
}

#[test]
fn manual_proposal_created_during_generation_cannot_be_auto_accepted() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/race", "原文事务");
    let provider = ManualDuringCompilation {
        store: s,
        slug: &original.slug,
    };
    assert_eq!(
        crate::knowledge_background::run_automatic_source_compilation(s, &provider).unwrap(),
        0
    );
    assert!(s.knowledge_output_pages(&original.slug).unwrap().is_empty());
    assert_eq!(s.knowledge_proposals(None).unwrap().len(), 1);
    assert_eq!(s.knowledge_proposals(None).unwrap()[0].status, "pending");
}

#[test]
fn existing_text_derivative_does_not_prevent_automatic_reference_compilation() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/derivative", "原文事务");
    // A summary cites the same source, but is not yet structured knowledge.
    let draft = crate::storage::WikiPageDraft {
        slug: "derivative-summary".into(),
        kind: "note".into(),
        title: "整理稿".into(),
        summary: "摘要".into(),
        content_md: "摘要".into(),
        tags: vec![],
        source_event_ids: vec![],
        status: "active".into(),
        reason: "test".into(),
        source_url: None,
    };
    let summary = s
        .upsert_wiki_page(&draft, crate::storage::ContentPolicy::Always)
        .unwrap()
        .page;
    s.bind_page_sources(
        &summary.id,
        &[s.source_history(&original.slug).unwrap()[0].id.clone()],
    )
    .unwrap();
    assert_eq!(
        crate::knowledge_background::run_automatic_source_compilation(s, &auto_stub()).unwrap(),
        1
    );
    assert!(s
        .knowledge_output_pages(&original.slug)
        .unwrap()
        .iter()
        .any(|p| p.kind == "method"));
}

struct EditDuringCompilation<'a> {
    store: &'a Store,
    slug: &'a str,
}
impl AiProvider for EditDuringCompilation<'_> {
    fn generate_reply_with_tools(
        &self,
        _: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        self.store
            .save_wiki_page_content(self.slug, "生成期间新增的人工事实", "manual", None)?;
        Ok(AiReply::text(compile_stub().0))
    }
}

#[test]
fn compilation_captures_revision_base_before_calling_the_model() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/edit-race", "原文");
    let id = propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap();
    let page = s.resolve_knowledge_proposal(&id, true).unwrap().unwrap();
    let provider = EditDuringCompilation {
        store: s,
        slug: &page.slug,
    };
    let revision = propose_knowledge(s, &page.slug, "revision", &provider).unwrap();
    assert!(s.resolve_knowledge_proposal(&revision, true).is_err());
    assert_eq!(
        s.get_wiki_page(&page.slug).unwrap().unwrap().content_md,
        "生成期间新增的人工事实"
    );
}

#[test]
fn many_cooling_failures_do_not_starve_a_new_source() {
    let db = Db::new();
    let s = &db.store;
    for index in 0..101 {
        let page = source(s, &format!("https://example.com/backlog/{index}"), "原料");
        let snapshot = s.source_history(&page.slug).unwrap().remove(0);
        s.connection.execute("INSERT INTO knowledge_background_runs(id,task,input_key,status,started_at,finished_at)
            VALUES (?1,'source-compilation',?2,'failed',?3,?3)",
            rusqlite::params![format!("failed-{index}"),snapshot.id,chrono::Utc::now().to_rfc3339()]).unwrap();
    }
    let ready = source(s, "https://example.com/ready", "新的事务材料");
    assert_eq!(
        crate::knowledge_background::run_automatic_source_compilation(s, &auto_stub()).unwrap(),
        1
    );
    assert_eq!(s.knowledge_output_pages(&ready.slug).unwrap().len(), 1);
}
