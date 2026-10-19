use super::super::*;
use super::accept;
use super::comparison_proposal;
use super::evidenced_page;
use super::page;
use super::stub;
use super::task;
use super::*;

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
