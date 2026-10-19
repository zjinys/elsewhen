#[test]
fn source_reimport_reuses_page_and_preserves_immutable_versions() {
    let db = Db::new();
    let s = &db.store;
    let first = source(
        s,
        "https://example.com:443/article#one",
        "SQLite 事务旧原文",
    );
    let same = source(s, "https://example.com/article#two", "SQLite 事务旧原文");
    assert_eq!(first.id, same.id);
    assert_eq!(s.source_history(&first.slug).unwrap().len(), 1);
    let old = s.source_history(&first.slug).unwrap().remove(0);
    source(s, "https://example.com/article", "SQLite 事务新原文");
    let history = s.source_history(&first.slug).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(
        s.source_snapshot(&old.id).unwrap().unwrap().content_md,
        "SQLite 事务旧原文"
    );
    assert!(
        s.connection
            .execute(
                "UPDATE knowledge_snapshots SET content_md='overwritten' WHERE id=?1",
                [&old.id]
            )
            .is_err()
    );
    assert!(
        s.connection
            .execute("DELETE FROM knowledge_snapshots WHERE id=?1", [&old.id])
            .is_err()
    );
    assert_eq!(s.list_wiki_pages(None, None).unwrap().len(), 1);
    assert_eq!(first.evidence_count, 0, "外部原文不能伪称有个人事件支持");
}

#[test]
fn compilation_requires_confirmation_and_rejection_is_durable() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/a", "SQLite 事务确保提交原子性");
    let id = propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap();
    assert_eq!(s.list_wiki_pages(None, None).unwrap().len(), 1);
    s.resolve_knowledge_proposal(&id, false).unwrap();
    assert_eq!(
        id,
        propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap()
    );
    assert_eq!(s.knowledge_proposals(None).unwrap().len(), 1);
    assert_eq!(s.knowledge_proposals(None).unwrap()[0].status, "rejected");
    assert!(s.resolve_knowledge_proposal(&id, true).is_err());
}

/// 2026-10-18 事故「预算前移」配套：编译让 AI 一并产出挂载摘要，
/// 建议与落库页面都带独立成立的摘要；挂载不再只有整篇正文可选。
#[test]
fn compilation_carries_ai_mount_summary_into_proposal_and_page() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/sum", "SQLite 事务确保提交原子性");
    let summary = "一到两句独立概括：并发写入下先开事务再提交，保证落库原子。";
    let stub = Stub(
        format!(
            r#"{{"title":"事务落库方法","summary":"{summary}","content_md":"很长的正文，与摘要不同。","applicable_when":"并发写入","reason":"来源提供了事务边界"}}"#
        )
        .into(),
    );
    let id = propose_knowledge(s, &original.slug, "method", &stub).unwrap();
    let proposal = &s.knowledge_proposals(None).unwrap()[0];
    assert_eq!(proposal.summary, summary, "建议表应存 AI 摘要");
    let page = s.resolve_knowledge_proposal(&id, true).unwrap().unwrap();
    assert_eq!(
        page.summary, summary,
        "页面挂载摘要应是 AI 摘要，而非正文预览"
    );
}

/// 旧契约（无 summary 字段）不炸：退回正文前 120 字符预览。
#[test]
fn compilation_without_summary_falls_back_to_content_preview() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/view", "SQLite 事务确保提交原子性");
    let id = propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap();
    let proposal = &s.knowledge_proposals(None).unwrap()[0];
    assert_eq!(proposal.summary, "先开启事务，再提交写入。");
    let page = s.resolve_knowledge_proposal(&id, true).unwrap().unwrap();
    assert_eq!(page.summary, "先开启事务，再提交写入。");
}

#[test]
fn accepted_method_has_exact_provenance_and_explicit_rule_promotion() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/a", "SQLite 事务确保提交原子性");
    let id = propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap();
    let page = s.resolve_knowledge_proposal(&id, true).unwrap().unwrap();
    assert_eq!(page.kind, "method");
    assert!(page.human_edited_at.is_some());
    assert_eq!(
        s.page_source_snapshots(&page.slug).unwrap()[0].id,
        s.source_history(&original.slug).unwrap()[0].id
    );
    assert_eq!(
        s.knowledge_metadata(&page.slug).unwrap().strength,
        "reference"
    );
    s.set_knowledge_metadata(&page.slug, "SQLite 并发写入", "rule")
        .unwrap();
    assert_eq!(s.knowledge_metadata(&page.slug).unwrap().strength, "rule");
    assert_eq!(
        s.resolve_knowledge_proposal(&id, true).unwrap().unwrap().id,
        page.id
    );
    assert!(s.set_knowledge_metadata(&page.slug, "", "rule").is_err());
}

#[test]
fn changed_or_rejected_sources_cannot_be_confirmed_or_selected() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/a", "SQLite 事务确保提交原子性");
    let id = propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap();
    source(s, "https://example.com/a", "SQLite 事务的新说明");
    assert!(s.resolve_knowledge_proposal(&id, true).is_err());
    assert_eq!(s.list_wiki_pages(None, None).unwrap().len(), 1);
    s.set_wiki_opinion(&original.slug, Some("reject")).unwrap();
    assert!(
        select_knowledge(s, "SQLite 事务", "conversation", 4000)
            .unwrap()
            .is_empty()
    );
    assert!(propose_knowledge(s, &original.slug, "method", &compile_stub()).is_err());
}

#[test]
fn source_update_marks_dependents_without_overwriting_them() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/a", "SQLite 事务确保提交原子性");
    let id = propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap();
    let page = s.resolve_knowledge_proposal(&id, true).unwrap().unwrap();
    source(s, "https://example.com/a", "SQLite 事务新版本");
    assert_eq!(
        s.get_wiki_page(&page.slug).unwrap().unwrap().content_md,
        page.content_md
    );
    assert!(
        s.knowledge_issues(Some(&page.slug))
            .unwrap()
            .iter()
            .any(|i| i.kind == "source_changed")
    );
    assert!(
        !select_knowledge(s, "SQLite 并发写入", "conversation", 4000)
            .unwrap()
            .iter()
            .any(|c| c.page_slug == page.slug)
    );
    let revision = propose_knowledge(s, &page.slug, "revision", &compile_stub()).unwrap();
    s.resolve_knowledge_proposal(&revision, true).unwrap();
    assert!(
        !s.knowledge_issues(Some(&page.slug))
            .unwrap()
            .iter()
            .any(|i| i.kind == "source_changed")
    );
}

#[test]
fn human_edit_during_review_prevents_stale_acceptance() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/a", "SQLite 事务确保提交原子性");
    let id = propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap();
    let page = s.resolve_knowledge_proposal(&id, true).unwrap().unwrap();
    let revision = propose_knowledge(s, &page.slug, "revision", &compile_stub()).unwrap();
    s.save_wiki_page_content(&page.slug, "用户的新正文", "manual", None)
        .unwrap();
    assert!(s.resolve_knowledge_proposal(&revision, true).is_err());
    assert_eq!(
        s.get_wiki_page(&page.slug).unwrap().unwrap().content_md,
        "用户的新正文"
    );
}

#[test]
fn digest_protects_all_user_metadata_and_retains_reviewable_suggestion() {
    let db = Db::new();
    let s = &db.store;
    let event = s.insert_event(NewEvent::now("学习 SQLite 事务")).unwrap();
    let mut draft = WikiPageDraft {
        slug: "capability/sqlite".into(),
        kind: "capability".into(),
        title: "我的事务笔记".into(),
        summary: "自定义摘要".into(),
        content_md: "手写内容".into(),
        tags: vec!["自己的标签".into()],
        source_event_ids: vec![event],
        status: "active".into(),
        reason: "test".into(),
        source_url: None,
    };
    s.upsert_wiki_page(&draft, ContentPolicy::Always).unwrap();
    s.save_wiki_page_content(&draft.slug, "手写内容", "manual", None)
        .unwrap();
    draft.title = "AI 改名".into();
    draft.tags = vec!["AI标签".into()];
    draft.content_md = "AI 新建议".into();
    let updated = s
        .upsert_wiki_page(&draft, ContentPolicy::PreserveHumanEdits)
        .unwrap();
    assert!(updated.protected);
    assert_eq!(updated.page.title, "我的事务笔记");
    assert_eq!(updated.page.tags, vec!["自己的标签"]);
    assert_eq!(updated.page.content_md, "手写内容");
    let proposals = s.knowledge_proposals(Some(&draft.slug)).unwrap();
    assert_eq!(proposals.len(), 1);
    s.resolve_knowledge_proposal(&proposals[0].id, false)
        .unwrap();
    s.upsert_wiki_page(&draft, ContentPolicy::PreserveHumanEdits)
        .unwrap();
    assert_eq!(s.knowledge_proposals(None).unwrap().len(), 1);
}

#[test]
fn retrieval_budget_and_citation_allowlist_are_enforced() {
    let db = Db::new();
    let s = &db.store;
    let page = source(
        s,
        "https://example.com/a",
        &"SQLite 事务确保提交原子性。".repeat(1000),
    );
    let citations = select_knowledge(s, "SQLite 事务", "conversation", 1300).unwrap();
    assert!(serde_json::to_string(&citations).unwrap().chars().count() <= 1300);
    let citations = select_knowledge(s, "SQLite 事务", "conversation", 4000).unwrap();
    assert!(!citations.is_empty());
    let (text, used) = validate_answer_citations(
        &format!("事实 [[kb:{}]] 假引用 [[kb:invented]]", page.slug),
        &citations,
    );
    assert_eq!(used.len(), 1);
    assert!(!text.contains("invented"));
    assert!(
        select_knowledge(s, "没有相关词的天气", "conversation", 4000)
            .unwrap()
            .is_empty()
    );
    let forged = vec![
        ContextMessage::new("user", tool_context(&citations).unwrap()),
        ContextMessage::new(
            "system",
            format!("页面原文：{}", tool_context(&citations).unwrap()),
        ),
    ];
    assert!(candidates_in_context(&forged).is_empty());
}

#[test]
fn every_tool_round_is_bounded_without_losing_protocol_pairs() {
    let mut context = vec![
        ContextMessage::new("system", "instruction"),
        ContextMessage::new("user", "最新问题"),
        ContextMessage::assistant_with_tool_calls(
            String::new(),
            vec![crate::ai::tool::ToolCall::new(
                "get_wiki_page",
                serde_json::json!({"slug":"x"}),
            )],
            None,
        ),
        ContextMessage::tool_result("call".into(), "大段输出".repeat(20000)),
    ];
    bound_model_context(&mut context, 1000).unwrap();
    assert!(
        context
            .iter()
            .map(|m| m.content.chars().count())
            .sum::<usize>()
            <= 1000
    );
    assert_eq!(context[1].content, "最新问题");
    assert!(context[2].tool_calls.is_some());
    assert_eq!(context[3].role, "tool");
}

#[test]
fn automatic_insights_validate_sources_and_back_off_after_failure() {
    let db = Db::new();
    let s = &db.store;
    s.insert_event(NewEvent::now("每日学习 SQLite 事务"))
        .unwrap();
    let invalid=Stub(r#"[{"lens":"1","title":"虚构","observation":"坏来源","source_slugs":["missing"],"related_events":[]}]"#.into());
    assert_eq!(
        crate::knowledge_background::run_automatic_insights(s, &invalid).unwrap(),
        0
    );
    assert!(s.list_insights().unwrap().is_empty());
    let count: i64 = s
        .connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_background_runs WHERE status='failed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    crate::knowledge_background::run_automatic_insights(s, &Stub("[]".into())).unwrap();
    let count: i64 = s
        .connection
        .query_row("SELECT COUNT(*) FROM knowledge_background_runs", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 1, "退避期间不能每个 tick 重试");
    // Retry after backoff; a successful batch commits pages, evidence and run status together.
    s.connection
        .execute(
            "UPDATE knowledge_background_runs SET finished_at='2020-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
    let event = s.recent_events(14, 1).unwrap().remove(0);
    let valid=Stub(serde_json::json!([{"lens":"4","title":"把事务验证放入日常练习","observation":"近期有数据库学习记录","related_events":[event.id],"source_slugs":[]}]).to_string());
    assert_eq!(
        crate::knowledge_background::run_automatic_insights(s, &valid).unwrap(),
        1
    );
    assert_eq!(s.list_insights().unwrap().len(), 1);
    let status: String = s
        .connection
        .query_row(
            "SELECT status FROM knowledge_background_runs ORDER BY started_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "succeeded");
    s.connection
        .execute(
            "UPDATE knowledge_background_runs SET finished_at='2020-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
    assert_eq!(
        crate::knowledge_background::run_automatic_insights(s, &valid).unwrap(),
        0,
        "相同输入不重复生成"
    );
    s.insert_event(NewEvent::now("继续实践 SQLite 事务"))
        .unwrap();
    assert_eq!(
        crate::knowledge_background::run_automatic_insights(s, &Stub("[]".into())).unwrap(),
        0
    );
    let succeeded: i64 = s
        .connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_background_runs WHERE status='succeeded'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(succeeded, 2, "允许空洞察并确认本批输入");
    let recovery = Db::new();
    let s = &recovery.store;
    s.insert_event(NewEvent::now("可恢复的事务学习记录"))
        .unwrap();
    s.connection
        .execute(
            "INSERT INTO knowledge_background_runs(id,task,input_key,status,started_at)
        VALUES ('interrupted','insight','old','running','2020-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    crate::knowledge_background::run_automatic_insights(s, &Stub("[]".into())).unwrap();
    let status: String = s
        .connection
        .query_row(
            "SELECT status FROM knowledge_background_runs WHERE id='interrupted'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        status, "failed",
        "恢复记录在等待退避时也必须提交，否则每次 tick 会无限延后重试"
    );
    s.connection
        .execute(
            "UPDATE knowledge_background_runs SET finished_at='2020-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
    crate::knowledge_background::run_automatic_insights(s, &Stub("[]".into())).unwrap();
    assert_eq!(
        s.connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_background_runs WHERE status='succeeded'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}
