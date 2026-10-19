use super::super::super::*;
use super::native_reply;
use super::*;

#[test]
fn status_of_a_previously_saved_page_is_verified_since_the_original_request() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    store
        .send_message(&conv, "user", "保存这份测试笔记", None)
        .unwrap();
    crate::wiki::save_text_page("已保存正文", Some("测试笔记"), &[], &store).unwrap();
    store
        .send_message(&conv, "assistant", "已保存「测试笔记」。", None)
        .unwrap();
    store
        .send_message(&conv, "user", "搞定了吗？", None)
        .unwrap();
    assert!(!FollowUp::load(&store, &conv).unwrap().unwrap().unresolved);
    let mut context = vec![ContextMessage::new("user", "搞定了吗？")];
    let provider =
        RecordingProvider::new(vec![AiReply::text("已保存「测试笔记」，之前就完成了。")]);
    let result = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert!(result.content.contains("之前就完成"));
    assert_eq!(provider.requests.borrow().len(), 1);
    assert_eq!(store.list_wiki_pages(None, None).unwrap().len(), 1);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn unverified_old_save_claim_is_not_treated_as_a_completed_task() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    for (role, text) in [
        ("user", "保存成测试笔记"),
        ("assistant", "已保存「测试笔记」。"),
        ("user", "？"),
    ] {
        store.send_message(&conv, role, text, None).unwrap();
    }
    assert!(FollowUp::load(&store, &conv).unwrap().unwrap().unresolved);
    let result = progress_fallback(&store, &conv, &[], &[], None).unwrap();
    assert!(!result.contains("已保存"));
    assert!(result.contains("还没有完成"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn native_intermediate_body_is_echoed_and_last_round_finalizes() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let mut context = vec![ContextMessage::new("user", "查一下规则")];
    let mut first = native_reply("已经整理出的要点需要结合规则检查", "list_rules", json!({}));
    first.reasoning_content = Some("provider-private-reasoning".into());
    let provider = RecordingProvider::new(vec![
        first,
        native_reply("", "list_rules", json!({})),
        native_reply("", "list_rules", json!({})),
        AiReply::text("已经查完，目前没有规则。"),
    ]);
    let result = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    let requests = provider.requests.borrow();
    let echoed = requests[1]
        .0
        .iter()
        .find(|m| m.tool_calls.is_some())
        .unwrap();
    assert_eq!(echoed.content, "已经整理出的要点需要结合规则检查");
    assert_eq!(
        echoed.reasoning_content.as_deref(),
        Some("provider-private-reasoning")
    );
    assert_eq!(requests.len(), MAX_TOOL_ROUNDS);
    assert!(!requests.last().unwrap().1);
    assert_eq!(result.content, "已经查完，目前没有规则。");
    assert!(!result.content.contains("private-reasoning"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn finalization_never_dispatches_native_or_text_writes() {
    for native in [true, false] {
        let (store, path) = temporary_database();
        let conv = store.create_conversation(Some("test"), None).unwrap();
        let mut context = vec![ContextMessage::new("user", "查一下规则")];
        let final_reply = if native {
            native_reply(
                "已保存",
                "save_knowledge_draft",
                json!({"title":"unexpected", "content_md":"no"}),
            )
        } else {
            AiReply::text(
                "[工具调用]{\"name\":\"save_knowledge_draft\",\"arguments\":{\"title\":\"unexpected\",\"content_md\":\"no\"}}",
            )
        };
        let provider = RecordingProvider::new(vec![
            native_reply("", "list_rules", json!({})),
            native_reply("", "list_rules", json!({})),
            native_reply("", "list_rules", json!({})),
            final_reply,
        ]);
        let result = run_agent_loop(
            &provider,
            "test-provider",
            &mut context,
            &ToolRegistry::default(),
            &store,
            &conv,
            chrono::Utc::now(),
        )
        .unwrap();
        assert!(
            store
                .pending_actions_for_conversation(&conv)
                .unwrap()
                .is_empty()
        );
        assert!(!result.content.trim().is_empty());
        assert!(!result.content.contains("已保存"));
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn protected_material_rejects_an_impossible_revision_before_creating_pending() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    let page = crate::wiki::save_text_page("原文", Some("视频原料"), &[], &store).unwrap();
    let args = json!({"slug":page.slug, "title":"改进稿", "content_md":"新内容", "change_note":"改进", "save_as":"revision"});
    let registry = ToolRegistry::default();
    let rejected = dispatch(
        &ToolCall::new("save_wiki_revision", args.clone()),
        &registry,
        &store,
        &conv,
    );
    assert!(!rejected.success);
    assert!(rejected.content.contains("derivative"));
    assert!(
        store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty()
    );
    let mut allowed = args;
    allowed["save_as"] = json!("derivative");
    let drafted = dispatch(
        &ToolCall::new("save_wiki_revision", allowed),
        &registry,
        &store,
        &conv,
    );
    assert!(drafted.success);
    assert_eq!(
        store.pending_actions_for_conversation(&conv).unwrap().len(),
        1
    );
    assert_eq!(
        store.get_wiki_page(&page.slug).unwrap().unwrap().content_md,
        "原文"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn status_question_shows_existing_draft_without_confirming_or_duplicating_it() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    for (role, text) in [
        ("user", "帮我把视频 prompt 保存成派生稿"),
        ("assistant", "请稍等"),
        ("user", "？"),
    ] {
        store.send_message(&conv, role, text, None).unwrap();
    }
    store
        .create_pending_action(
            &conv,
            "save_knowledge_draft",
            &json!({"title":"视频草稿", "content_md":"完整视频草稿正文"}).to_string(),
        )
        .unwrap();
    assert!(pending_review_context(&store, &conv).unwrap().is_some());
    assert!(store.list_wiki_pages(None, None).unwrap().is_empty());
    let mut context = vec![ContextMessage::new("user", "？")];
    let provider = RecordingProvider::new(vec![
        native_reply(
            "",
            "save_knowledge_draft",
            json!({"title":"重复草稿", "content_md":"no"}),
        ),
        AiReply::text(""),
        AiReply::text(""),
        AiReply::text(""),
    ]);
    let result = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(
        store.pending_actions_for_conversation(&conv).unwrap().len(),
        1
    );
    assert!(store.list_wiki_pages(None, None).unwrap().is_empty());
    assert!(result.content.contains("完整视频草稿正文"));
    assert!(result.content.contains("尚未执行保存"));
    assert!(!result.content.contains("模型"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn successful_confirmation_survives_failure_to_generate_final_words() {
    let (store, path) = temporary_database();
    let conv = store.create_conversation(Some("test"), None).unwrap();
    store
        .create_pending_action(
            &conv,
            "save_knowledge_draft",
            &json!({"title":"测试知识", "content_md":"有用内容"}).to_string(),
        )
        .unwrap();
    store.send_message(&conv, "user", "好", None).unwrap();
    let proposal = store
        .list_chat_proposals(Some(&conv), 0, 10)
        .unwrap()
        .remove(0);
    let receipt = crate::ai::approval::resolve_chat_proposal(
        &store,
        &proposal.kind,
        &proposal.id,
        Some(&conv),
        proposal.version,
        &proposal.approval_token,
        "accept",
    )
    .unwrap();
    assert_eq!(receipt.status, "succeeded");
    let summary = Some(receipt.result);
    let mut context = vec![
        ContextMessage::new("user", "好"),
        ContextMessage::new(
            "system",
            format!(
                "（内部记录）你刚才提议的写操作已被用户确认，执行结果如下：\n{}",
                summary.unwrap()
            ),
        ),
    ];
    let provider = RecordingProvider::new((0..5).map(|_| AiReply::text("")).collect());
    let result = run_agent_loop(
        &provider,
        "test-provider",
        &mut context,
        &ToolRegistry::default(),
        &store,
        &conv,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(store.list_wiki_pages(None, None).unwrap().len(), 1);
    assert!(result.content.contains("测试知识"));
    assert!(!result.content.contains("还没有完成"));
    assert!(!result.content.contains("重发"));
    drop(store);
    let _ = std::fs::remove_file(path);
}
