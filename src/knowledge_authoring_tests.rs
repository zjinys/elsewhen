#[test]
fn conversation_draft_reuses_verified_sources_and_remains_retrievable() {
    use crate::ai::tool::{
        dispatch, dispatch_with_knowledge, execute_pending_action, ToolCall, ToolRegistry,
    };
    let db = Db::new();
    let s = &db.store;
    let raw = source(
        s,
        "https://example.com/chat-evidence",
        "SQLite 事务失败必须整体回滚，重试需要确认幂等。",
    );
    let conv = s
        .create_conversation(Some("保存有出处的方法"), None)
        .unwrap();
    let hits = select_knowledge(s, "SQLite 事务", "test", 6000).unwrap();
    let call = ToolCall::new(
        "save_knowledge_draft",
        serde_json::json!({
            "title":"事务重试的条件", "kind":"method", "content_md":format!("重试之前确认幂等。[[{}]]",raw.slug),
            "source_slugs":[raw.slug], "applicable_when":"SQLite 事务失败重试"
        }),
    );
    // A forged source identifier is rejected, even if it happens to exist in the DB.
    assert!(!dispatch(&call, &ToolRegistry::default(), s, &conv).success);
    assert!(s
        .pending_actions_for_conversation(&conv)
        .unwrap()
        .is_empty());
    let result = dispatch_with_knowledge(&call, &ToolRegistry::default(), s, &conv, &hits);
    assert!(result.success, "{}", result.content);
    let pending = s.pending_actions_for_conversation(&conv).unwrap().remove(0);
    assert!(s
        .find_wiki_page_by_title("事务重试的条件")
        .unwrap()
        .is_none());
    execute_pending_action(s, &pending).unwrap();
    let saved = s
        .find_wiki_page_by_title("事务重试的条件")
        .unwrap()
        .unwrap();
    assert!(saved.human_edited_at.is_some());
    assert_eq!(s.page_source_snapshots(&saved.slug).unwrap().len(), 1);
    assert_eq!(
        s.knowledge_origin_pages(&saved.slug).unwrap()[0].slug,
        raw.slug
    );
    assert_eq!(
        s.knowledge_metadata(&saved.slug).unwrap().strength,
        "reference"
    );
    assert!(select_knowledge(s, "SQLite 事务重试", "test", 12000)
        .unwrap()
        .iter()
        .any(|c| c.page_slug == saved.slug));
    // A later request to save the answer can inherit its real citations after restart.
    let answer = format!("先检查幂等，再重试。[[{}]]", raw.slug);
    record_usage(
        s,
        "conversation",
        &format!(
            "{conv}:{}",
            crate::storage::knowledge::content_hash(&answer)
        ),
        &hits,
        &hits,
    )
    .unwrap();
    s.send_message(&conv, "assistant", &answer, None).unwrap();
    let reopened = Store::open(&db.path).unwrap();
    let mut later = call.clone();
    later.arguments["title"] = "重试前置核对".into();
    assert!(dispatch(&later, &ToolRegistry::default(), &reopened, &conv).success);
}

#[test]
fn conversation_draft_rechecks_sources_at_confirmation_and_keeps_free_notes_unattributed() {
    use crate::ai::tool::{
        dispatch, dispatch_with_knowledge, execute_pending_action, ToolCall, ToolRegistry,
    };
    for change in ["version", "reject"] {
        let db = Db::new();
        let s = &db.store;
        let raw = source(
            s,
            "https://example.com/stale-chat",
            "SQLite 事务原料，需要保留来源。",
        );
        let conv = s.create_conversation(Some("stale"), None).unwrap();
        let hits = select_knowledge(s, "SQLite 事务", "test", 6000).unwrap();
        let call = ToolCall::new(
            "save_knowledge_draft",
            serde_json::json!({"title":"有出处的总结","content_md":"总结","source_slugs":[raw.slug]}),
        );
        assert!(dispatch_with_knowledge(&call, &ToolRegistry::default(), s, &conv, &hits).success);
        let pending = s.pending_actions_for_conversation(&conv).unwrap().remove(0);
        if change == "version" {
            source(
                s,
                "https://example.com/stale-chat",
                "SQLite 新原文推翻旧结论。",
            );
        } else {
            s.set_wiki_opinion(&raw.slug, Some("reject")).unwrap();
        }
        assert!(execute_pending_action(s, &pending).is_err());
        assert!(s.find_wiki_page_by_title("有出处的总结").unwrap().is_none());
        let note = ToolCall::new(
            "save_knowledge_draft",
            serde_json::json!({"title":"自己的灵感","content_md":"还没有验证的思路"}),
        );
        assert!(dispatch(&note, &ToolRegistry::default(), s, &conv).success);
        let note = s
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .into_iter()
            .find(|p| p.args_json.contains("自己的灵感"))
            .unwrap();
        execute_pending_action(s, &note).unwrap();
        let page = s.find_wiki_page_by_title("自己的灵感").unwrap().unwrap();
        assert!(page.human_edited_at.is_some());
        assert!(citation_for_page(s, &page, "test".into(), 1000)
            .unwrap()
            .is_none());
    }
}

#[test]
fn conversation_rule_revision_preserves_strength_and_rejects_metadata_races() {
    use crate::ai::tool::{dispatch, execute_pending_action, ToolCall, ToolRegistry};
    let db = Db::new();
    let s = &db.store;
    let raw = source(s, "https://example.com/rule-chat", "SQLite 事务失败回滚。");
    let id = propose_knowledge(s, &raw.slug, "method", &compile_stub()).unwrap();
    let page = s.resolve_knowledge_proposal(&id, true).unwrap().unwrap();
    s.set_knowledge_metadata(&page.slug, "SQLite 写入", "rule")
        .unwrap();
    let conv = s.create_conversation(Some("rule"), None).unwrap();
    let call = ToolCall::new(
        "save_wiki_revision",
        serde_json::json!({"slug":page.slug,"title":page.title,"content_md":"先验证幂等，再开启 SQLite 事务。","save_as":"revision","change_note":"明确重试条件","applicable_when":"SQLite 幂等写入"}),
    );
    let result = dispatch(&call, &ToolRegistry::default(), s, &conv);
    assert!(result.success && result.content.contains("保留原强度"));
    let pending = s.pending_actions_for_conversation(&conv).unwrap().remove(0);
    assert_eq!(
        s.get_wiki_page(&page.slug).unwrap().unwrap().content_md,
        page.content_md
    );
    s.set_knowledge_metadata(&page.slug, "SQLite 关键写入", "rule")
        .unwrap();
    assert!(execute_pending_action(s, &pending).is_err());
    s.delete_pending_action(&pending.id).unwrap();
    assert!(dispatch(&call, &ToolRegistry::default(), s, &conv).success);
    let fresh = s.pending_actions_for_conversation(&conv).unwrap().remove(0);
    execute_pending_action(s, &fresh).unwrap();
    let metadata = s.knowledge_metadata(&page.slug).unwrap();
    assert_eq!(metadata.strength, "rule");
    assert_eq!(metadata.applicable_when, "SQLite 幂等写入");
    assert_eq!(s.page_source_snapshots(&page.slug).unwrap().len(), 1);
}
