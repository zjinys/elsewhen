use crate::ai::{
    memory::ContextMessage,
    provider::{AiProvider, AiReply},
    tool::ToolSpec,
};
use crate::{
    event::NewEvent,
    knowledge::*,
    storage::{ContentPolicy, Store, WikiPageDraft},
};
use anyhow::Result;

include!("knowledge_flow_tests.rs");
include!("knowledge_integration_tests.rs");
include!("knowledge_authoring_tests.rs");
include!("knowledge_maintenance_tests.rs");
include!("knowledge_repair_tests.rs");
include!("knowledge_workflow_tests.rs");

struct Db {
    store: Store,
    path: std::path::PathBuf,
}
impl Db {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("elsewhen-knowledge-{}.db", uuid::Uuid::new_v4()));
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
fn source(store: &Store, url: &str, text: &str) -> crate::storage::WikiPage {
    let page = store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: format!("source-{}", &uuid::Uuid::new_v4().to_string()[..8]),
                kind: "source".into(),
                title: "SQLite 事务".into(),
                summary: text.into(),
                content_md: text.into(),
                tags: vec!["SQLite".into()],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "test import".into(),
                source_url: Some(url.into()),
            },
            ContentPolicy::Always,
        )
        .unwrap()
        .page;
    // 测试页模拟「既有已纳入」行（migration 50 回填后的存量语义）；
    // 新导入行的孵化语义由专门闸门测试覆盖。
    store.set_wiki_opinion(&page.slug, Some("endorse")).unwrap();
    page
}
struct Stub(String);
impl AiProvider for Stub {
    fn generate_reply_with_tools(
        &self,
        _: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        Ok(AiReply {
            content: self.0.clone(),
            tool_calls: vec![],
            usage: None,
            model: Some("stub".into()),
            reasoning_content: None,
            finish_reason: None,
        })
    }
}
fn compile_stub() -> Stub {
    Stub(r#"{"title":"SQLite 事务方法","content_md":"先开启事务，再提交写入。","applicable_when":"SQLite 并发写入","reason":"来源提供了事务边界"}"#.into())
}

fn incubating_source(store: &Store, url: &str, text: &str) -> crate::storage::WikiPage {
    // 与 source() 同构但不表态：模拟新导入行的「孵化中」缺省（NULL）。
    store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: format!("source-{}", &uuid::Uuid::new_v4().to_string()[..8]),
                kind: "source".into(),
                title: "孵化中的来源".into(),
                summary: text.into(),
                content_md: text.into(),
                tags: vec![],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "test import".into(),
                source_url: Some(url.into()),
            },
            ContentPolicy::Always,
        )
        .unwrap()
        .page
}

include!("knowledge_source_tests.rs");
include!("knowledge_component_tests.rs");
include!("knowledge_mount_tests.rs");
