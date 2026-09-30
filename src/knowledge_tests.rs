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
    store
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
        .page
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
    assert!(s
        .connection
        .execute(
            "UPDATE knowledge_snapshots SET content_md='overwritten' WHERE id=?1",
            [&old.id]
        )
        .is_err());
    assert!(s
        .connection
        .execute("DELETE FROM knowledge_snapshots WHERE id=?1", [&old.id])
        .is_err());
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
    assert!(select_knowledge(s, "SQLite 事务", "conversation", 4000)
        .unwrap()
        .is_empty());
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
    assert!(s
        .knowledge_issues(Some(&page.slug))
        .unwrap()
        .iter()
        .any(|i| i.kind == "source_changed"));
    assert!(
        !select_knowledge(s, "SQLite 并发写入", "conversation", 4000)
            .unwrap()
            .iter()
            .any(|c| c.page_slug == page.slug)
    );
    let revision = propose_knowledge(s, &page.slug, "revision", &compile_stub()).unwrap();
    s.resolve_knowledge_proposal(&revision, true).unwrap();
    assert!(!s
        .knowledge_issues(Some(&page.slug))
        .unwrap()
        .iter()
        .any(|i| i.kind == "source_changed"));
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

#[test]
fn legacy_v3_upgrade_preserves_originals_and_is_idempotent() {
    let path = std::env::temp_dir().join(format!("elsewhen-legacy-{}.db", uuid::Uuid::new_v4()));
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(include_str!("../migrations/01-baseline/up.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../migrations/02-goals/up.sql"))
        .unwrap();
    conn.execute_batch(include_str!(
        "../migrations/03-drop-legacy-migrations/up.sql"
    ))
    .unwrap();
    conn.execute_batch("PRAGMA user_version=3").unwrap();
    for (id, kind, body, url, created, updated, based_on) in [
        (
            "older",
            "source",
            "旧原文",
            Some("https://example.com/legacy"),
            "2026-01-02",
            "2026-01-02",
            None,
        ),
        (
            "newer",
            "source",
            "新原文",
            Some("https://example.com/legacy#fragment"),
            "2026-01-01",
            "2026-01-04",
            None,
        ),
        (
            "text",
            "note",
            "粘贴原文",
            None,
            "2026-01-01",
            "2026-01-01",
            None,
        ),
        (
            "file",
            "source",
            "# 文件\n\n- 本地路径：`/tmp/explicit.md`\n\n## 内容摘录\n\n文件原文",
            None,
            "2026-01-01",
            "2026-01-01",
            None,
        ),
        (
            "derived",
            "derivative",
            "旧版整理",
            None,
            "2026-01-03",
            "2026-01-03",
            Some("older"),
        ),
        (
            "history-source",
            "source",
            "后来更新的原文",
            Some("https://example.com/history"),
            "2026-01-01",
            "2026-01-04",
            None,
        ),
        (
            "history-derived",
            "derivative",
            "使用历史原文的整理",
            None,
            "2026-01-02",
            "2026-01-02",
            Some("history-source"),
        ),
    ] {
        conn.execute("INSERT INTO wiki_pages(id,slug,kind,title,content_md,source_url,first_seen_at,last_seen_at,created_at,updated_at,based_on)
            VALUES (?1,?1,?2,?1,?3,?4,?5,?5,?5,?6,?7)",rusqlite::params![id,kind,body,url,created,updated,based_on]).unwrap();
    }
    conn.execute(
        "UPDATE wiki_pages SET tags='[\"local-source\"]' WHERE id='file'",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO wiki_revisions(id,page_id,content_md,reason,created_at)
        VALUES ('old-revision','history-source','修订历史里的原文','import','2026-01-01')",
        [],
    )
    .unwrap();
    drop(conn);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.list_wiki_pages(None, None).unwrap().len(), 7);
    assert_eq!(
        store.page_source_snapshots("history-derived").unwrap()[0].content_md,
        "修订历史里的原文"
    );
    assert_eq!(store.source_history("older").unwrap().len(), 2);
    assert_eq!(
        store.source_history("older").unwrap()[0].content_md,
        "新原文",
        "最新来源按实际更新时间排列"
    );
    assert_eq!(
        store.get_wiki_page("older").unwrap().unwrap().content_md,
        "旧原文"
    );
    assert_eq!(
        store.page_source_snapshots("older").unwrap()[0].content_md,
        "旧原文",
        "旧别名页不能伪装成引用新版本"
    );
    assert_eq!(
        store.page_source_snapshots("derived").unwrap()[0].content_md,
        "旧原文"
    );
    assert_eq!(store.source_history("file").unwrap()[0].source_kind, "file");
    assert!(store
        .knowledge_issues(Some("derived"))
        .unwrap()
        .iter()
        .any(|i| i.kind == "source_changed"));
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.source_history("older").unwrap().len(), 2);
    drop(store);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn old_proposal_remains_resolvable_beyond_list_limit() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/a", "SQLite 事务说明");
    let id = propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap();
    for index in 0..205 {
        s.connection.execute("INSERT INTO knowledge_proposals(id,dedupe_key,target_slug,kind,title,content_md,reason,created_at)
            VALUES (?1,?1,?1,'method','another','body','test','2000-01-01')",[format!("proposal-{index}")]).unwrap();
    }
    assert!(!s
        .knowledge_proposals(None)
        .unwrap()
        .iter()
        .any(|p| p.id == id));
    assert!(s.resolve_knowledge_proposal(&id, true).unwrap().is_some());
}

#[test]
fn citation_recheck_rejects_changed_or_disavowed_material() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/a", "SQLite 事务说明");
    let selected = select_knowledge(s, "SQLite 事务", "conversation", 4000).unwrap();
    assert_eq!(selected.len(), 1);
    source(s, "https://example.com/a", "SQLite 事务更新");
    assert!(current_candidates(s, &selected).unwrap().is_empty());
    let selected = select_knowledge(s, "SQLite 事务", "conversation", 4000).unwrap();
    s.set_wiki_opinion(&original.slug, Some("reject")).unwrap();
    assert!(current_candidates(s, &selected).unwrap().is_empty());
}

#[test]
fn tweet_aliases_share_identity_and_rule_revisions_require_explicit_confirmation() {
    let db = Db::new();
    let s = &db.store;
    let original = source(
        s,
        "https://twitter.com/author/status/123?s=20",
        "SQLite 事务说明",
    );
    assert_eq!(
        source(s, "https://x.com/i/status/123", "SQLite 事务说明").id,
        original.id
    );
    let id = propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap();
    let page = s.resolve_knowledge_proposal(&id, true).unwrap().unwrap();
    s.set_knowledge_metadata(&page.slug, "独特适用场景", "rule")
        .unwrap();
    assert!(select_knowledge(s, "独特适用場景", "conversation", 4000)
        .unwrap()
        .iter()
        .any(|c| c.page_slug == page.slug));
    let id = propose_knowledge(s, &page.slug, "revision", &compile_stub()).unwrap();
    assert!(s
        .knowledge_proposals(Some(&page.slug))
        .unwrap()
        .iter()
        .any(|p| p.id == id && p.status == "pending"));
    assert_eq!(
        s.knowledge_metadata(&page.slug).unwrap().applicable_when,
        "独特适用场景"
    );
    s.resolve_knowledge_proposal(&id, true).unwrap();
    assert_eq!(s.knowledge_metadata(&page.slug).unwrap().strength, "rule");
}

#[test]
fn file_import_confirms_captured_originals_and_detects_concurrent_updates() {
    let db = Db::new();
    let s = &db.store;
    let dir = std::env::temp_dir().join(format!("elsewhen-files-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let file = dir.join("note.md");
    std::fs::write(&file, "文件的原始正文").unwrap();
    let preview = crate::local_sources::preview_file_import(s, &dir).unwrap();
    assert!(s.list_wiki_pages(None, None).unwrap().is_empty());
    std::fs::write(&file, "确认期间文件发生变化").unwrap();
    let report = crate::local_sources::confirm_file_import(s, &preview).unwrap();
    let slug = &report.pages[0];
    assert_eq!(
        s.get_wiki_page(slug).unwrap().unwrap().content_md,
        "文件的原始正文",
        "只写入实际确认的版本"
    );
    assert_eq!(s.source_history(slug).unwrap()[0].source_kind, "file");
    let pending = crate::local_sources::preview_file_import(s, &dir).unwrap();
    assert!(pending.entries[0]
        .previous_excerpt
        .as_ref()
        .unwrap()
        .contains("首处变化"));
    std::fs::write(&file, "第三个版本").unwrap();
    let newer = crate::local_sources::preview_file_import(s, &dir).unwrap();
    crate::local_sources::confirm_file_import(s, &newer).unwrap();
    assert!(crate::local_sources::confirm_file_import(s, &pending).is_err());
    assert_eq!(s.source_history(slug).unwrap().len(), 2);
    assert_eq!(s.list_wiki_pages(None, None).unwrap().len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn topic_analysis_binds_only_applicable_sources() {
    let db = Db::new();
    let s = &db.store;
    let source = source(
        s,
        "https://example.com/transactions",
        "SQLite 事务确保写入一致性",
    );
    let slug = crate::local_sources::plan_topic(s, "SQLite 事务").unwrap();
    assert_eq!(
        s.page_source_snapshots(&slug).unwrap()[0].id,
        s.source_history(&source.slug).unwrap()[0].id
    );
    assert_eq!(
        latest_citations(s, "topic", &slug).unwrap()[0].page_slug,
        source.slug
    );
    s.set_wiki_opinion(&source.slug, Some("reject")).unwrap();
    crate::local_sources::plan_topic(s, "SQLite 事务").unwrap();
    assert!(latest_citations(s, "topic", &slug).unwrap().is_empty());
}

#[test]
fn chat_revision_does_not_overwrite_edits_made_after_the_preview() {
    use crate::ai::tool::{dispatch, execute_pending_action, ToolCall, ToolRegistry};
    let db = Db::new();
    let store = &db.store;
    let original = source(store, "https://example.com/revision", "SQLite 事务说明");
    let id = propose_knowledge(store, &original.slug, "method", &compile_stub()).unwrap();
    let page = store
        .resolve_knowledge_proposal(&id, true)
        .unwrap()
        .unwrap();
    let conversation = store.create_conversation(Some("编辑"), None).unwrap();
    let call = ToolCall::new(
        "save_wiki_revision",
        serde_json::json!({"slug":page.slug,"title":"修订标题","content_md":"模型修订正文","change_note":"补充","save_as":"revision"}),
    );
    dispatch(&call, &ToolRegistry::default(), store, &conversation);
    let pending = store
        .pending_actions_for_conversation(&conversation)
        .unwrap()
        .remove(0);
    store
        .save_wiki_page_content(&page.slug, "稍后人工修订", "user", None)
        .unwrap();
    assert!(execute_pending_action(store, &pending).is_err());
    assert_eq!(
        store.get_wiki_page(&page.slug).unwrap().unwrap().content_md,
        "稍后人工修订"
    );
}
