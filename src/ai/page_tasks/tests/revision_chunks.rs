use super::super::*;
use super::accept;
use super::comparison_proposal;
use super::evidenced_page;
use super::page;
use super::stub;
use super::task;
use super::*;

#[test]
fn local_selection_is_unicode_exact_and_edit_preserves_other_paragraphs() {
    let db = Db::new();
    let s = &db.store;
    page(s, "前文😀\n旧段落\n后文");
    let c = s
        .create_wiki_chat_conversation("task-page", "test")
        .unwrap();
    let mut t = task(s, "local_edit");
    t.start_char = Some(4);
    t.end_char = Some(7);
    s.save_page_task_input(&c, "修订选区", "key", &t).unwrap();
    let msg = prepare(s, &c, &t, "修订", &stub(false), "stub").unwrap();
    assert!(msg.content.contains("旧段落"));
    assert!(!msg.content.contains("前文😀"));
    let result = dispatch(
        &ToolCall::new(
            "save_wiki_revision",
            json!({"slug":"task-page","title":"不能改标题","content_md":"新内容","change_note":"edit","save_as":"revision"}),
        ),
        &ToolRegistry::default(),
        s,
        &c,
    );
    assert!(result.success, "{:?}", result);
    let pending = s.pending_actions_for_conversation(&c).unwrap();
    let args: serde_json::Value = serde_json::from_str(&pending[0].args_json).unwrap();
    assert_eq!(args["content_md"], "前文😀\n新内容\n后文");
    assert_eq!(args["title"], "Task material");
    page(s, "人工改稿");
    assert!(crate::ai::tool::execute_pending_action(s, &pending[0]).is_err());
}

#[test]
fn local_question_is_bounded_and_full_summary_reaches_tail() {
    let db = Db::new();
    let s = &db.store;
    page(
        s,
        &format!("{}\n尾部限制：仅在周末使用。", "常规材料\n".repeat(1800)),
    );
    let c = s
        .create_wiki_chat_conversation("task-page", "test")
        .unwrap();
    let t = task(s, "local_question");
    let provider = stub(false);
    let local = prepare(s, &c, &t, "尾部限制", &provider, "stub").unwrap();
    assert!(local.content.chars().count() < 2300);
    assert!(local.content.contains("尾部限制"));
    assert_eq!(provider.calls.borrow().len(), 0);
    let full = prepare(
        s,
        &c,
        &task(s, "full_summary"),
        "全文总结",
        &provider,
        "stub",
    )
    .unwrap();
    assert!(full.content.contains("保留尾部限制"));
    assert!(full.content.contains("非逐字原文"));
    assert_eq!(
        provider
            .calls
            .borrow()
            .iter()
            .flat_map(|m| m.last().unwrap().content.chars())
            .collect::<String>(),
        s.get_wiki_page("task-page").unwrap().unwrap().content_md
    );
}

#[test]
fn summary_chunks_keep_negation_together_and_preserve_exact_coverage() {
    let db = Db::new();
    let s = &db.store;
    let text = format!(
        "{}。不表示试验已经完成。{}",
        "背".repeat(2994),
        "尾部材料。".repeat(80)
    );
    page(s, &text);
    let c = s
        .create_wiki_chat_conversation("task-page", "test")
        .unwrap();
    let provider = stub(false);
    prepare(
        s,
        &c,
        &task(s, "full_summary"),
        "全文总结",
        &provider,
        "stub",
    )
    .unwrap();
    let calls = provider.calls.borrow();
    assert_eq!(calls.len(), 2);
    assert!(
        calls[1]
            .last()
            .unwrap()
            .content
            .starts_with("不表示试验已经完成。")
    );
    assert!(
        calls
            .iter()
            .all(|messages| messages.last().unwrap().content.chars().count() <= CHUNK)
    );
    assert_eq!(
        calls
            .iter()
            .map(|m| m.last().unwrap().content.as_str())
            .collect::<String>(),
        text
    );
    assert!(calls[0].iter().any(|m| m.content.contains("只读相邻原文") && m.content.contains("不表示试验已经完成")));
}

#[test]
fn unpunctuated_summary_chunks_keep_bounded_neighbours_without_duplicate_coverage() {
    let db = Db::new();
    let s = &db.store;
    let text = format!(
        "{}不表示试验已经完成{}",
        "字".repeat(2994),
        "尾".repeat(1500)
    );
    page(s, &text);
    let c = s
        .create_wiki_chat_conversation("task-page", "test")
        .unwrap();
    let provider = stub(false);
    prepare(
        s,
        &c,
        &task(s, "full_summary"),
        "全文总结",
        &provider,
        "stub",
    )
    .unwrap();
    let calls = provider.calls.borrow();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls
            .iter()
            .map(|m| m.last().unwrap().content.as_str())
            .collect::<String>(),
        text
    );
    assert!(calls[1][1].content.contains("不表示试验已"));
    assert!(calls[1][1].content.chars().count() < 300);
}

#[test]
fn full_revision_covers_every_part_and_only_creates_review_proposal() {
    let db = Db::new();
    let s = &db.store;
    let original = "材料".repeat(4000);
    page(s, &original);
    let c = s
        .create_wiki_chat_conversation("task-page", "test")
        .unwrap();
    let t = task(s, "full_revision");
    s.save_page_task_input(&c, "全页修订", "key", &t).unwrap();
    let provider = stub(true);
    let result = revise_full(s, &c, &t, "修订", &provider, "stub", &[]).unwrap();
    assert!(result.contains("3 个连续片段"));
    assert_eq!(provider.calls.borrow().len(), 3);
    assert_eq!(
        s.get_wiki_page("task-page").unwrap().unwrap().content_md,
        original
    );
    let pending = s.pending_actions_for_conversation(&c).unwrap();
    assert_eq!(pending.len(), 1);
    let args: serde_json::Value = serde_json::from_str(&pending[0].args_json).unwrap();
    assert_eq!(args["content_md"], "edited-0\nedited-1\nedited-2\n");
}

#[test]
fn changed_page_during_full_revision_cannot_create_partial_proposal() {
    struct Changing<'a>(&'a Store);
    impl AiProvider for Changing<'_> {
        fn generate_reply_with_tools(
            &self,
            _: Vec<ContextMessage>,
            _: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            page(self.0, "用户的新版本");
            Ok(AiReply::text(r#"{"part":0,"content_md":"旧版本的修订"}"#))
        }
    }
    let db = Db::new();
    let s = &db.store;
    page(s, "原文");
    let c = s
        .create_wiki_chat_conversation("task-page", "test")
        .unwrap();
    assert!(
        revise_full(
            s,
            &c,
            &task(s, "full_revision"),
            "修订",
            &Changing(s),
            "stub",
            &[]
        )
        .is_err()
    );
    assert!(s.pending_actions_for_conversation(&c).unwrap().is_empty());
    assert_eq!(
        s.get_wiki_page("task-page").unwrap().unwrap().content_md,
        "用户的新版本"
    );
}

#[test]
fn incomplete_or_truncated_revision_is_not_a_proposal() {
    struct Truncated;
    impl AiProvider for Truncated {
        fn generate_reply_with_tools(
            &self,
            _: Vec<ContextMessage>,
            _: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            let mut reply = AiReply::text(r#"{"part":0,"content_md":"不完整"}"#);
            reply.finish_reason = Some("length".into());
            Ok(reply)
        }
    }
    let db = Db::new();
    let s = &db.store;
    page(s, "原文");
    let c = s
        .create_wiki_chat_conversation("task-page", "test")
        .unwrap();
    assert!(
        revise_full(
            s,
            &c,
            &task(s, "full_revision"),
            "修订",
            &Truncated,
            "stub",
            &[]
        )
        .is_err()
    );
    assert!(s.pending_actions_for_conversation(&c).unwrap().is_empty());
}
