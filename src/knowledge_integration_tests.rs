struct WikiFixture<F>(F);
impl<F: Fn(Vec<ContextMessage>) -> Result<AiReply>> AiProvider for WikiFixture<F> {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        (self.0)(messages)
    }
}

fn integration_input(
    messages: &[ContextMessage],
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let prompt = &messages[1].content;
    assert!(prompt.chars().count() <= 31000);
    let (_, data) = prompt.rsplit_once("原料：").unwrap();
    let (sources, topics) = data.split_once("\n已有主题：").unwrap();
    (
        serde_json::from_str(sources).unwrap(),
        serde_json::from_str(topics).unwrap(),
    )
}

fn integration_output(messages: &[ContextMessage], issues: bool) -> serde_json::Value {
    let (sources, topics) = integration_input(messages);
    assert!(sources.len() >= 2);
    let ids = sources
        .iter()
        .map(|s| s["snapshot_id"].clone())
        .collect::<Vec<_>>();
    let mut result = serde_json::json!({"topics":[{
        "existing_slug":topics.first().map(|p|p["slug"].clone()),
        "title":"SQLite 事务边界", "content_md":"SQLite 多步写入先明确事务范围，再比较重试条件。",
        "applicable_when":"SQLite 并发写入与失败重试", "snapshot_ids":ids
    }],"issues":[]});
    if issues {
        result["issues"] = serde_json::json!([{
            "page_slug":sources[0]["page_slug"], "kind":"conflict", "description":"两份材料对重试范围的建议可能不同，需比较适用条件。",
            "evidence":sources.iter().take(2).map(|s|serde_json::json!({
                "snapshot_id":s["snapshot_id"],"quote":s["excerpt"].as_str().unwrap().chars().take(40).collect::<String>()
            })).collect::<Vec<_>>()
        }]);
    }
    result
}

fn integration_fixture(messages: Vec<ContextMessage>) -> Result<AiReply> {
    Ok(AiReply::text(
        integration_output(&messages, false).to_string(),
    ))
}

fn two_sources(s: &Store) -> Vec<crate::storage::WikiPage> {
    vec![
        source(
            s,
            "https://example.com/atomic",
            "SQLite 多步写入在同一事务提交，失败后整体重试。",
        ),
        source(
            s,
            "https://example.com/retry",
            "SQLite 写入遇到锁冲突，需要区分事务内外的重试。",
        ),
    ]
}

#[test]
fn requested_reference_is_published_without_confirming_or_replacing_old_manual_choices() {
    let db = Db::new();
    let s = &db.store;
    let raw = source(s, "https://example.com/requested", "事务原文");
    let id = compile_requested_knowledge(s, &raw.slug, "method", &compile_stub()).unwrap();
    let proposal = s.knowledge_proposals(None).unwrap().remove(0);
    assert_eq!(proposal.id, id);
    assert_eq!(proposal.status, "accepted");
    let page = s.get_wiki_page(&proposal.target_slug).unwrap().unwrap();
    assert!(page.human_edited_at.is_none());
    let meta = s.knowledge_metadata(&page.slug).unwrap();
    assert_eq!(meta.strength, "reference");
    assert!(meta.confirmed_at.is_none());
    assert_eq!(
        s.get_wiki_page(&raw.slug).unwrap().unwrap().content_md,
        raw.content_md
    );
    let manual = propose_knowledge(s, &raw.slug, "case", &compile_stub()).unwrap();
    assert_eq!(
        compile_requested_knowledge(s, &raw.slug, "case", &compile_stub()).unwrap(),
        manual
    );
    assert!(s
        .knowledge_proposals(None)
        .unwrap()
        .iter()
        .any(|p| p.id == manual && p.status == "pending"));
    s.resolve_knowledge_proposal(&manual, false).unwrap();
    assert_eq!(
        compile_requested_knowledge(s, &raw.slug, "case", &compile_stub()).unwrap(),
        manual
    );
    assert!(s
        .knowledge_proposals(None)
        .unwrap()
        .iter()
        .any(|p| p.id == manual && p.status == "rejected"));
}

#[test]
fn integration_builds_shared_topic_tracks_new_versions_and_survives_restart() {
    let db = Db::new();
    let s = &db.store;
    let raw = two_sources(s);
    let provider = WikiFixture(integration_fixture);
    assert_eq!(
        crate::knowledge_background::run_automatic_integration(s, &provider).unwrap(),
        1
    );
    let topic = s.list_wiki_pages(Some("topic"), None).unwrap().remove(0);
    assert_eq!(topic.area, "insight");
    assert!(topic.human_edited_at.is_none());
    assert_eq!(s.knowledge_origin_pages(&topic.slug).unwrap().len(), 2);
    for source in &raw {
        assert_eq!(
            s.knowledge_output_pages(&source.slug).unwrap()[0].id,
            topic.id
        );
        assert_eq!(
            s.get_wiki_page(&source.slug).unwrap().unwrap().content_md,
            source.content_md
        );
    }
    assert!(select_knowledge(s, "SQLite 事务边界", "test", 12000)
        .unwrap()
        .iter()
        .any(|c| c.page_slug == topic.slug));
    crate::knowledge_background::run_automatic_integration(s, &provider).unwrap();
    let reopened = Store::open(&db.path).unwrap();
    let no_calls = Replies(std::cell::RefCell::new(vec![]));
    assert_eq!(
        crate::knowledge_background::run_automatic_integration(&reopened, &no_calls).unwrap(),
        0
    );
    let old = s.page_source_snapshots(&topic.slug).unwrap();
    source(
        s,
        "https://example.com/atomic",
        "SQLite 多步写入在同一事务提交；更新后限制自动重试次数。",
    );
    assert_eq!(
        crate::knowledge_background::run_automatic_integration(s, &provider).unwrap(),
        1
    );
    assert_eq!(s.list_wiki_pages(Some("topic"), None).unwrap().len(), 1);
    let current = s.page_source_snapshots(&topic.slug).unwrap();
    assert_eq!(current.len(), 2);
    assert!(current.iter().any(|v| v.version == 2));
    assert!(old
        .iter()
        .all(|v| s.source_snapshot(&v.id).unwrap().is_some()));
    assert!(!s
        .knowledge_issues(Some(&topic.slug))
        .unwrap()
        .iter()
        .any(|i| i.kind == "source_changed"));
}

#[test]
fn integration_does_not_rewrite_topics_with_event_evidence_it_cannot_supply() {
    let db = Db::new();
    let s = &db.store;
    two_sources(s);
    crate::knowledge_background::run_automatic_integration(s, &WikiFixture(integration_fixture))
        .unwrap();
    let topic = s.list_wiki_pages(Some("topic"), None).unwrap().remove(0);
    let event = s
        .insert_event(NewEvent::now("实际验证 SQLite 事务边界"))
        .unwrap();
    s.connection
        .execute(
            "UPDATE wiki_pages SET source_event_ids=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&vec![&event]).unwrap(), topic.id],
        )
        .unwrap();
    let provider = WikiFixture(|messages: Vec<ContextMessage>| {
        let (_, topics) = integration_input(&messages);
        assert!(topics.is_empty());
        Ok(AiReply::text(r#"{"topics":[],"issues":[]}"#))
    });
    assert_eq!(
        crate::knowledge_background::run_automatic_integration(s, &provider).unwrap(),
        0
    );
    let preserved = s.get_wiki_page(&topic.slug).unwrap().unwrap();
    assert_eq!(preserved.source_event_ids, vec![event]);
    assert_eq!(preserved.content_md, topic.content_md);
    assert_eq!(s.knowledge_proposals(None).unwrap().len(), 1);
}

#[test]
fn integration_preserves_human_topic_until_explicit_revision_confirmation() {
    let db = Db::new();
    let s = &db.store;
    two_sources(s);
    let provider = WikiFixture(integration_fixture);
    crate::knowledge_background::run_automatic_integration(s, &provider).unwrap();
    let topic = s.list_wiki_pages(Some("topic"), None).unwrap().remove(0);
    s.save_wiki_page_content(
        &topic.slug,
        "我确认的事务结论，禁止隐式重试",
        "manual",
        None,
    )
    .unwrap();
    crate::knowledge_background::run_automatic_integration(s, &provider).unwrap();
    assert_eq!(
        s.get_wiki_page(&topic.slug).unwrap().unwrap().content_md,
        "我确认的事务结论，禁止隐式重试"
    );
    assert_eq!(
        s.knowledge_metadata(&topic.slug).unwrap().strength,
        "reference"
    );
    let pending = s
        .knowledge_proposals(Some(&topic.slug))
        .unwrap()
        .into_iter()
        .find(|p| p.status == "pending")
        .unwrap();
    s.resolve_knowledge_proposal(&pending.id, true).unwrap();
    assert_eq!(
        s.knowledge_metadata(&topic.slug).unwrap().strength,
        "reference"
    );
    assert!(s
        .knowledge_metadata(&topic.slug)
        .unwrap()
        .confirmed_at
        .is_some());
}

#[test]
fn semantic_hints_require_exact_quotes_and_expire_with_sources() {
    let db = Db::new();
    let s = &db.store;
    two_sources(s);
    let provider = WikiFixture(|messages: Vec<ContextMessage>| {
        Ok(AiReply::text(
            integration_output(&messages, true).to_string(),
        ))
    });
    crate::knowledge_background::run_automatic_integration(s, &provider).unwrap();
    let issue = s
        .knowledge_issues(None)
        .unwrap()
        .into_iter()
        .find(|i| i.kind == "conflict")
        .unwrap();
    assert!(issue.description.contains("待核对（AI 提示）"));
    assert!(issue.description.contains("SQLite"));
    s.dismiss_knowledge_issue(&issue.fingerprint).unwrap();
    assert!(!s
        .knowledge_issues(None)
        .unwrap()
        .iter()
        .any(|i| i.fingerprint == issue.fingerprint));
    crate::knowledge_background::run_automatic_integration(s, &provider).unwrap();
    source(
        s,
        "https://example.com/atomic",
        "SQLite 原料内容已经更新，需要重新核对重试范围。",
    );
    assert!(!s
        .knowledge_issues(None)
        .unwrap()
        .iter()
        .any(|i| i.kind == "conflict"));
    assert!(s
        .knowledge_issues(None)
        .unwrap()
        .iter()
        .any(|i| i.kind == "source_changed"));
}

#[test]
fn invalid_integration_batch_saves_neither_topics_nor_hints_and_backs_off() {
    for invalid in ["quote", "source", "link", "count"] {
        let db = Db::new();
        let s = &db.store;
        two_sources(s);
        let provider = WikiFixture(|messages: Vec<ContextMessage>| {
            let mut result = integration_output(&messages, true);
            match invalid {
                "quote" => {
                    result["issues"][0]["evidence"][0]["quote"] =
                        serde_json::json!("原文中不存在的十个以上字符的捏造证据")
                }
                "source" => {
                    result["topics"][0]["snapshot_ids"] = serde_json::json!(["invented", "other"])
                }
                "link" => {
                    result["topics"][0]["content_md"] = serde_json::json!("参见 [[invented]]")
                }
                _ => result["topics"] = serde_json::json!(vec![result["topics"][0].clone(); 4]),
            }
            Ok(AiReply::text(result.to_string()))
        });
        assert_eq!(
            crate::knowledge_background::run_automatic_integration(s, &provider).unwrap(),
            0
        );
        assert!(s.list_wiki_pages(Some("topic"), None).unwrap().is_empty());
        assert!(s.knowledge_proposals(None).unwrap().is_empty());
        assert!(!s
            .knowledge_issues(None)
            .unwrap()
            .iter()
            .any(|i| i.kind == "conflict"));
        // The other source can proceed; failed inputs themselves stay in backoff.
        crate::knowledge_background::run_automatic_integration(s, &provider).unwrap();
        assert_eq!(
            crate::knowledge_background::run_automatic_integration(
                s,
                &Replies(std::cell::RefCell::new(vec![]))
            )
            .unwrap(),
            0
        );
    }
}

#[test]
fn integration_rechecks_source_and_target_changes_made_during_generation() {
    for change in ["reject", "version", "target", "lease"] {
        let db = Db::new();
        let s = &db.store;
        let raw = two_sources(s);
        let provider = WikiFixture(integration_fixture);
        crate::knowledge_background::run_automatic_integration(s, &provider).unwrap();
        let topic = s.list_wiki_pages(Some("topic"), None).unwrap().remove(0);
        let mutator = WikiFixture(|messages: Vec<ContextMessage>| {
            let result = integration_output(&messages, true);
            match change {
                "reject" => {
                    s.set_wiki_opinion(&raw[0].slug, Some("reject"))?;
                }
                "version" => {
                    source(
                        s,
                        "https://example.com/atomic",
                        "SQLite 发生新更新，需要重新整理事务范围。",
                    );
                }
                "target" => {
                    s.save_wiki_page_content(&topic.slug, "期间人工修改了正文", "manual", None)?;
                }
                _ => {
                    s.connection.execute("UPDATE knowledge_background_runs SET status='failed' WHERE task='wiki-integration' AND status='running'",[])?;
                }
            }
            Ok(AiReply::text(result.to_string()))
        });
        assert_eq!(
            crate::knowledge_background::run_automatic_integration(s, &mutator).unwrap(),
            0
        );
        assert_eq!(s.knowledge_proposals(Some(&topic.slug)).unwrap().len(), 1);
        assert!(!s
            .knowledge_issues(None)
            .unwrap()
            .iter()
            .any(|i| i.kind == "conflict"));
        assert_eq!(
            s.get_wiki_page(&topic.slug).unwrap().unwrap().content_md,
            if change == "target" {
                "期间人工修改了正文"
            } else {
                &topic.content_md
            }
        );
    }
}

#[test]
fn integration_renews_monthly_but_respects_active_lease() {
    let db = Db::new();
    let s = &db.store;
    two_sources(s);
    let provider = WikiFixture(integration_fixture);
    crate::knowledge_background::run_automatic_integration(s, &provider).unwrap();
    crate::knowledge_background::run_automatic_integration(s, &provider).unwrap();
    let no_calls = Replies(std::cell::RefCell::new(vec![]));
    assert_eq!(
        crate::knowledge_background::run_automatic_integration(s, &no_calls).unwrap(),
        0
    );
    s.connection.execute("UPDATE knowledge_background_runs SET finished_at='2000-01-01T00:00:00Z' WHERE task='wiki-integration'",[]).unwrap();
    s.connection.execute("INSERT INTO knowledge_background_runs(id,task,input_key,status,started_at) VALUES('lease','wiki-integration','x','running',?1)",[chrono::Utc::now().to_rfc3339()]).unwrap();
    assert_eq!(
        crate::knowledge_background::run_automatic_integration(s, &no_calls).unwrap(),
        0
    );
    s.connection.execute("UPDATE knowledge_background_runs SET started_at='2000-01-01T00:00:00Z' WHERE id='lease'",[]).unwrap();
    assert_eq!(
        crate::knowledge_background::run_automatic_integration(s, &provider).unwrap(),
        1
    );
}
