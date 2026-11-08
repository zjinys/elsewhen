use super::*;

#[test]
fn summary_intent_respects_local_scope_negation_and_existing_tasks() {
    let db = Db::new();
    page(&db.store, "开头。中间。尾部。");
    let original = task(&db.store, "local_question");
    for query in [
        "总结一下",
        "请总结这篇文章",
        "概括这一页的要点",
        "帮我提炼关键事实",
        "请总结全文，不要只读前2800字",
        "Please summarize this article",
        "不要修改，请总结全文",
    ] {
        let expanded = summary_task(&original, query).unwrap_or_else(|| panic!("未识别：{query}"));
        assert_eq!(expanded.kind, "full_summary");
        assert_eq!(expanded.pages, original.pages);
    }
    for query in [
        "这个词是什么意思",
        "不用总结全文",
        "先别总结，解释这个概念",
        "只总结这段",
        "总结第一段，不要总结全文",
        "Summarize this paragraph",
        "Do not summarize this article",
        "Please do not ever summarize this article",
        "Never summarize the entire page",
        "Explain how to summarize this article",
        "The phrase \"summarize this article\" is an example",
        "```text\nSummarize the entire article\n```",
    ] {
        assert!(
            summary_task(&original, query).is_none(),
            "错误扩大范围：{query}"
        );
    }
    let mut selected = original.clone();
    selected.start_char = Some(0);
    selected.end_char = Some(3);
    assert!(summary_task(&selected, "总结一下").is_none());
    let expanded = summary_task(&selected, "不要只总结选区，请总结整篇文章").unwrap();
    assert_eq!(expanded.start_char, None);
    assert_eq!(expanded.end_char, None);
    for kind in [
        "local_edit",
        "full_revision",
        "full_summary",
        "compare",
        "none",
    ] {
        let mut explicit = original.clone();
        explicit.kind = kind.into();
        assert!(summary_task(&explicit, "总结全文").is_none());
    }
}

#[test]
fn automatic_page_summary_covers_all_characters_without_changing_saved_scope() {
    let db = Db::new();
    let original = format!("{}尾部限制：仅在周末使用。", "正文事实。".repeat(1450));
    page(&db.store, &original);
    let conversation = db
        .store
        .create_wiki_chat_conversation("task-page", "summary")
        .unwrap();
    let original_task = task(&db.store, "local_question");
    db.store
        .save_page_task_input(&conversation, "总结这篇文章", "summary", &original_task)
        .unwrap();
    let provider = stub(false);
    let material = prepare(
        &db.store,
        &conversation,
        &original_task,
        "总结这篇文章",
        &provider,
        "fixture",
    )
    .unwrap();
    assert!(material.content.contains("页面任务=full_summary"));
    assert!(material.content.contains("保留尾部限制"));
    assert!(material.content.contains("各连续片段均已处理"));
    let calls = provider.calls.borrow();
    assert!(calls.len() >= 3);
    assert_eq!(
        calls
            .iter()
            .map(|messages| messages.last().unwrap().content.as_str())
            .collect::<String>(),
        original
    );
    assert_eq!(
        db.store.current_page_task(&conversation).unwrap().unwrap(),
        original_task
    );
    assert!(
        db.store
            .pending_actions_for_conversation(&conversation)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn imported_page_summary_reads_foreground_when_cache_is_missing_and_reuses_ready_cache() {
    let db = Db::new();
    let original = format!("{}尾部限制：仅在周末使用。", "导入原文。".repeat(1450));
    let imported =
        crate::wiki::save_text_page(&original, Some("新导入的长文章"), &[], &db.store).unwrap();
    assert_eq!(imported.area, "imported");
    let conversation = db
        .store
        .create_wiki_chat_conversation(&imported.slug, "summary")
        .unwrap();
    let task = PageTask {
        kind: "local_question".into(),
        pages: vec![db.store.page_task_basis(&imported.slug).unwrap().1],
        start_char: None,
        end_char: None,
    };
    let source = db
        .store
        .page_source_snapshots(&imported.slug)
        .unwrap()
        .into_iter()
        .find(|source| source.content_md == original)
        .unwrap();
    assert!(!crate::knowledge::reading::ready(&db.store, &source).unwrap());
    let provider = stub(false);
    let material = prepare(
        &db.store,
        &conversation,
        &task,
        "总结这篇文章",
        &provider,
        "fixture",
    )
    .unwrap();
    assert!(material.content.contains("保留尾部限制"));
    assert_eq!(
        provider
            .calls
            .borrow()
            .iter()
            .map(|messages| messages.last().unwrap().content.as_str())
            .collect::<String>(),
        original
    );
    assert!(!crate::knowledge::reading::ready(&db.store, &source).unwrap());
    db.store.connection.execute(
        "INSERT INTO knowledge_source_readings(snapshot_id,strategy_version,level,part,start_char,end_char,summary,quotes_json,is_root) VALUES(?1,?2,1,0,0,?3,'完整缓存归纳：保留尾部限制','[]',1)",
        rusqlite::params![source.id, crate::knowledge::reading::VERSION, original.chars().count() as i64],
    ).unwrap();
    let cached_provider = stub(false);
    let cached = prepare(
        &db.store,
        &conversation,
        &task,
        "总结这篇文章",
        &cached_provider,
        "fixture",
    )
    .unwrap();
    assert!(cached.content.contains("完整缓存归纳：保留尾部限制"));
    assert!(cached_provider.calls.borrow().is_empty());
    assert_eq!(
        db.store
            .get_wiki_page(&imported.slug)
            .unwrap()
            .unwrap()
            .content_md,
        original
    );
}

#[test]
fn automatic_summary_never_reports_full_coverage_after_a_truncated_chunk() {
    struct Truncated(RefCell<usize>);
    impl AiProvider for Truncated {
        fn generate_reply_with_tools(
            &self,
            _: Vec<ContextMessage>,
            _: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            let mut calls = self.0.borrow_mut();
            *calls += 1;
            let mut reply = AiReply::text("局部归纳");
            if *calls == 2 {
                reply.finish_reason = Some("length".into());
            }
            Ok(reply)
        }
    }
    let db = Db::new();
    page(&db.store, &"长文。".repeat(2500));
    let conversation = db
        .store
        .create_wiki_chat_conversation("task-page", "summary")
        .unwrap();
    let provider = Truncated(RefCell::new(0));
    let result = prepare(
        &db.store,
        &conversation,
        &task(&db.store, "local_question"),
        "请总结全文",
        &provider,
        "fixture",
    );
    assert!(result.unwrap_err().to_string().contains("未建立完整结果"));
    assert_eq!(*provider.0.borrow(), 2);
    assert!(
        db.store
            .pending_actions_for_conversation(&conversation)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn automatic_summary_rejects_oversized_material_before_reading_any_chunk() {
    let db = Db::new();
    let imported = crate::wiki::save_text_page(
        &"字".repeat(CHUNK * (MAX_PARTS + 1)),
        Some("超长导入文章"),
        &[],
        &db.store,
    )
    .unwrap();
    let conversation = db
        .store
        .create_wiki_chat_conversation(&imported.slug, "summary")
        .unwrap();
    let task = PageTask {
        kind: "local_question".into(),
        pages: vec![db.store.page_task_basis(&imported.slug).unwrap().1],
        start_char: None,
        end_char: None,
    };
    let provider = stub(false);
    assert!(
        prepare(
            &db.store,
            &conversation,
            &task,
            "总结这篇文章",
            &provider,
            "fixture"
        )
        .unwrap_err()
        .to_string()
        .contains("超过32段")
    );
    assert!(provider.calls.borrow().is_empty());
}
