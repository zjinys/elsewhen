use super::*;
use crate::{
    ai::{
        provider::AiReply,
        tool::{ToolCall, ToolRegistry, ToolSpec, dispatch},
    },
    storage::{ContentPolicy, WikiPageDraft},
};
use std::cell::RefCell;

fn evidenced_page(store: &Store, slug: &str, content: &str, events: Vec<String>) -> WikiPage {
    store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: slug.into(),
                kind: "topic".into(),
                title: slug.into(),
                summary: "fixture".into(),
                content_md: content.into(),
                tags: vec![],
                source_event_ids: events,
                status: "active".into(),
                reason: "fixture".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap()
        .page
}

fn comparison_proposal(
    store: &Store,
    first: &WikiPage,
    second: &WikiPage,
) -> crate::storage::approvals::ChatProposal {
    let conversation = store
        .create_wiki_chat_conversation(&first.slug, "comparison fixture")
        .unwrap();
    let task = PageTask {
        kind: "compare".into(),
        pages: vec![
            store.page_task_basis(&first.slug).unwrap().1,
            store.page_task_basis(&second.slug).unwrap().1,
        ],
        start_char: None,
        end_char: None,
    };
    store
        .save_page_task_input(
            &conversation,
            "对照这两份材料并草拟成果",
            "comparison",
            &task,
        )
        .unwrap();
    let result = dispatch(
        &ToolCall::new(
            "save_wiki_revision",
            json!({
                "slug": first.slug, "title": "对照成果", "content_md": "综合两份材料的对照。",
                "change_note": "comparison", "save_as": "derivative", "content_type": "对照"
            }),
        ),
        &ToolRegistry::default(),
        store,
        &conversation,
    );
    assert!(result.success, "{result:?}");
    let pending = store
        .pending_actions_for_conversation(&conversation)
        .unwrap();
    store
        .chat_proposal("action", &pending[0].id, Some(&conversation))
        .unwrap()
        .unwrap()
}

fn accept(
    store: &Store,
    proposal: &crate::storage::approvals::ChatProposal,
) -> crate::storage::approvals::ChatDecision {
    crate::ai::approval::resolve_chat_proposal(
        store,
        &proposal.kind,
        &proposal.id,
        proposal.conversation_id.as_deref(),
        proposal.version,
        &proposal.approval_token,
        "accept",
    )
    .unwrap()
}

#[test]
fn comparison_artifact_retains_every_source_event_and_knowledge_dependency() {
    let db = Db::new();
    let s = &db.store;
    let source_a = crate::wiki::save_text_page("source alpha", Some("raw alpha"), &[], s).unwrap();
    let source_b = crate::wiki::save_text_page("source beta", Some("raw beta"), &[], s).unwrap();
    let event_a = s
        .insert_event(crate::event::NewEvent::now("event alpha"))
        .unwrap();
    let event_b = s
        .insert_event(crate::event::NewEvent::now("event beta"))
        .unwrap();
    let first = evidenced_page(
        s,
        "comparison-alpha",
        "knowledge alpha",
        vec![event_a.clone()],
    );
    let second = evidenced_page(
        s,
        "comparison-beta",
        "knowledge beta",
        vec![event_b.clone()],
    );
    let snapshot_a = s.page_source_snapshots(&source_a.slug).unwrap()[0]
        .id
        .clone();
    let snapshot_b = s.page_source_snapshots(&source_b.slug).unwrap()[0]
        .id
        .clone();
    s.bind_page_sources(&first.id, &[snapshot_a.clone()])
        .unwrap();
    s.bind_page_sources(&second.id, &[snapshot_b.clone()])
        .unwrap();
    let proposal = comparison_proposal(s, &first, &second);
    assert_eq!(accept(s, &proposal).status, "succeeded");
    let artifact = s.find_wiki_page_by_title("对照成果").unwrap().unwrap();
    let mut snapshots: Vec<_> = s
        .page_source_snapshots(&artifact.slug)
        .unwrap()
        .into_iter()
        .map(|source| source.id)
        .collect();
    snapshots.sort();
    let mut expected = vec![snapshot_a, snapshot_b];
    expected.sort();
    assert_eq!(snapshots, expected);
    let mut events = artifact.source_event_ids.clone();
    events.sort();
    let mut expected = vec![event_a, event_b.clone()];
    expected.sort();
    assert_eq!(events, expected);
    let dependencies: i64 = s.connection.query_row(
        "SELECT COUNT(*) FROM knowledge_dependencies WHERE page_id=?1 AND upstream_id IN (?2,?3)",
        rusqlite::params![artifact.id, first.id, second.id], |row| row.get(0),
    ).unwrap();
    assert_eq!(dependencies, 2);
    assert!(!crate::knowledge::dependencies::stale(s, &artifact.id).unwrap());
    evidenced_page(
        s,
        &second.slug,
        "second material changed after acceptance",
        vec![event_b],
    );
    assert!(crate::knowledge::dependencies::stale(s, &artifact.id).unwrap());
}

#[test]
fn comparison_rejects_a_secondary_material_whose_source_was_already_stale() {
    let db = Db::new();
    let s = &db.store;
    let primary =
        crate::wiki::save_text_page("primary source", Some("primary fixture"), &[], s).unwrap();
    let source =
        crate::wiki::save_text_page("secondary source v1", Some("secondary source"), &[], s)
            .unwrap();
    let secondary = evidenced_page(s, "secondary-knowledge", "based on old source", vec![]);
    let snapshot = s.page_source_snapshots(&source.slug).unwrap()[0].id.clone();
    s.bind_page_sources(&secondary.id, &[snapshot]).unwrap();
    s.upsert_wiki_page(
        &WikiPageDraft {
            slug: source.slug.clone(),
            kind: source.kind.clone(),
            title: source.title.clone(),
            summary: "updated".into(),
            content_md: "secondary source v2".into(),
            tags: source.tags.clone(),
            source_event_ids: vec![],
            status: "active".into(),
            reason: "new snapshot".into(),
            source_url: source.source_url.clone(),
        },
        ContentPolicy::Always,
    )
    .unwrap();
    // The displayed basis is current, but the page still cites the old snapshot.
    let proposal = comparison_proposal(s, &primary, &secondary);
    let receipt = accept(s, &proposal);
    assert_eq!(receipt.status, "failed");
    assert!(s.find_wiki_page_by_title("对照成果").unwrap().is_none());
}

#[test]
fn comparison_dependency_failure_rolls_back_the_whole_artifact() {
    let db = Db::new();
    let s = &db.store;
    let primary =
        crate::wiki::save_text_page("primary source", Some("rollback primary"), &[], s).unwrap();
    let event = s
        .insert_event(crate::event::NewEvent::now("secondary event"))
        .unwrap();
    let secondary = evidenced_page(s, "rollback-secondary", "secondary knowledge", vec![event]);
    let proposal = comparison_proposal(s, &primary, &secondary);
    s.connection.execute_batch("CREATE TRIGGER fail_comparison_dependency BEFORE INSERT ON knowledge_dependencies BEGIN SELECT RAISE(ABORT, 'injected dependency failure'); END;").unwrap();
    assert_eq!(accept(s, &proposal).status, "failed");
    assert!(s.find_wiki_page_by_title("对照成果").unwrap().is_none());
    let artifacts: i64 = s
        .connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_artifact_versions",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(artifacts, 0);
}

#[test]
fn restored_page_task_uses_only_the_last_input_and_never_refreshes_tokens() {
    let db = Db::new();
    let s = &db.store;
    page(s, "displayed v1");
    let conversation = s
        .create_wiki_chat_conversation("task-page", "restore fixture")
        .unwrap();
    assert!(s.latest_page_task_json(&conversation).unwrap().is_none());
    let original = task(s, "local_question");
    let saved = serde_json::to_string(&original).unwrap();
    let input = s
        .save_page_task_input(&conversation, "first question", "restore", &original)
        .unwrap();
    s.send_message(&conversation, "assistant", "first answer", None)
        .unwrap();
    page(s, "new v2 content");
    assert_eq!(
        s.latest_page_task_json(&conversation).unwrap().as_deref(),
        Some(saved.as_str())
    );
    assert_ne!(original.pages[0], s.page_task_basis("task-page").unwrap().1);
    let reopened = Store::open(&db.path).unwrap();
    assert_eq!(
        reopened
            .latest_page_task_json(&conversation)
            .unwrap()
            .as_deref(),
        Some(saved.as_str())
    );
    s.send_message(&conversation, "user", "legacy input without task", None)
        .unwrap();
    assert!(s.latest_page_task_json(&conversation).unwrap().is_none());
    let frozen = s.snapshot_for_input(&conversation, &input).unwrap();
    assert_eq!(
        frozen
            .latest_page_task_json(&conversation)
            .unwrap()
            .as_deref(),
        Some(saved.as_str())
    );
}

struct Db {
    store: Store,
    path: std::path::PathBuf,
}
impl Db {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("elsewhen-page-task-{}.db", uuid::Uuid::new_v4()));
        Self {
            store: Store::open(&path).unwrap(),
            path,
        }
    }
}
impl Drop for Db {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
fn page(store: &Store, text: &str) -> WikiPage {
    store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "task-page".into(),
                kind: "topic".into(),
                title: "Task material".into(),
                summary: "test".into(),
                content_md: text.into(),
                tags: vec![],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "fixture".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap()
        .page
}
fn task(store: &Store, kind: &str) -> PageTask {
    PageTask {
        kind: kind.into(),
        pages: vec![store.page_task_basis("task-page").unwrap().1],
        start_char: None,
        end_char: None,
    }
}
struct Stub {
    calls: RefCell<Vec<Vec<ContextMessage>>>,
    revise: bool,
}
impl AiProvider for Stub {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        let index = self.calls.borrow().len();
        let content = if self.revise {
            json!({"part":index,"content_md":format!("edited-{index}\n")}).to_string()
        } else if messages.last().unwrap().content.contains("尾部限制") {
            "保留尾部限制".into()
        } else {
            "保留该段事实".into()
        };
        self.calls.borrow_mut().push(messages);
        Ok(AiReply::text(content))
    }
}
fn stub(revise: bool) -> Stub {
    Stub {
        calls: RefCell::new(vec![]),
        revise,
    }
}

#[test]
fn page_task_input_is_atomic_idempotent_and_version_bound() {
    let db = Db::new();
    let s = &db.store;
    page(s, "😀原始内容");
    let c = s
        .create_wiki_chat_conversation("task-page", "test")
        .unwrap();
    let t = task(s, "local_question");
    let id = s.save_page_task_input(&c, "问题", "stable", &t).unwrap();
    page(s, "新的人工内容");
    assert_eq!(
        s.save_page_task_input(&c, "问题", "stable", &t).unwrap(),
        id
    );
    assert!(
        s.save_page_task_input(&c, "另一问题", "stable", &t)
            .is_err()
    );
    assert!(s.save_page_task_input(&c, "问题", "new", &t).is_err());
    assert_eq!(s.list_messages(&c).unwrap().len(), 1);
    let events: i64 = s
        .connection
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(events, 0);
}

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

#[test]
fn independent_input_does_not_inherit_previous_page_task() {
    let db = Db::new();
    let s = &db.store;
    page(s, "旧任务证据");
    let c = s
        .create_wiki_chat_conversation("task-page", "test")
        .unwrap();
    s.save_page_task_input(&c, "总结", "a", &task(s, "full_summary"))
        .unwrap();
    let none = PageTask {
        kind: "none".into(),
        pages: vec![],
        start_char: None,
        end_char: None,
    };
    s.save_page_task_input(&c, "解释另一概念", "b", &none)
        .unwrap();
    let current = task_for_run(s, &c).unwrap().unwrap();
    assert_eq!(current.kind, "none");
    assert!(
        !prepare(s, &c, &current, "另一问题", &stub(false), "stub")
            .unwrap()
            .content
            .contains("旧任务证据")
    );
}
