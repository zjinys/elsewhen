use super::*;
use crate::{
    ai::{
        provider::AiReply,
        tool::{ToolCall, ToolRegistry, ToolSpec, dispatch},
    },
    storage::{ContentPolicy, WikiPageDraft},
};
use std::cell::RefCell;

// 公共测试辅助（原文件平移，pub(super) 供分组子文件复用）。
pub(super) fn evidenced_page(
    store: &Store,
    slug: &str,
    content: &str,
    events: Vec<String>,
) -> WikiPage {
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

pub(super) fn comparison_proposal(
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

pub(super) fn accept(
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

pub(super) fn page(store: &Store, text: &str) -> WikiPage {
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

pub(super) fn task(store: &Store, kind: &str) -> PageTask {
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

pub(super) fn stub(revise: bool) -> Stub {
    Stub {
        calls: RefCell::new(vec![]),
        revise,
    }
}

mod artifact_atomicity;

mod revision_chunks;
