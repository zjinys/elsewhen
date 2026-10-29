use super::*;

struct TempDatabase(std::path::PathBuf);
impl Drop for TempDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(format!("{}-wal", self.0.display()));
        let _ = std::fs::remove_file(format!("{}-shm", self.0.display()));
    }
}

fn fixture() -> (TempDatabase, Store, String) {
    let path = std::env::temp_dir().join(format!(
        "elsewhen-summary-storage-{}.db",
        uuid::Uuid::new_v4()
    ));
    let store = Store::open(&path).unwrap();
    let conversation = store
        .create_conversation(Some("summary fixture"), None)
        .unwrap();
    (TempDatabase(path), store, conversation)
}

fn node(store: &Store, conversation: &str) -> SummaryNode {
    let messages = store.list_messages(conversation).unwrap();
    let spans: Vec<_> = messages
        .iter()
        .map(|message| HistorySpan {
            message_id: message.id.clone(),
            start_char: 0,
            end_char: message.content.chars().count(),
            message_basis: message_basis(message),
        })
        .collect();
    SummaryNode {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: conversation.into(),
        basis_hash: content_hash(&serde_json::to_string(&spans).unwrap()),
        spans,
        children: vec![],
        strategy: "test-v1".into(),
        provider_basis: "provider-v1".into(),
        model: Some("fixture-model".into()),
        body: SummaryBody {
            goals: vec!["整理任务".into()],
            constraints: vec!["不改原文".into()],
            facts: vec![],
            decisions: vec![],
            unfinished: vec!["等待验证".into()],
        },
        prompt_tokens: None,
        completion_tokens: None,
        elapsed_ms: 2,
    }
}

#[test]
fn summary_cache_preserves_originals_and_unknown_usage() {
    let (_temp, store, conversation) = fixture();
    store
        .send_message(&conversation, "user", "原始约束：不要修改原文。", None)
        .unwrap();
    let before = store.list_messages(&conversation).unwrap()[0]
        .content
        .clone();
    let summary = node(&store, &conversation);
    store.commit_summary_node(&summary).unwrap();
    store.commit_summary_node(&summary).unwrap();
    let cached = store
        .summary_node(&conversation, &summary.basis_hash, "test-v1", "provider-v1")
        .unwrap()
        .unwrap();
    assert_eq!(cached.body, summary.body);
    assert_eq!(cached.prompt_tokens, None);
    assert_eq!(cached.completion_tokens, None);
    assert_eq!(
        store.list_messages(&conversation).unwrap()[0].content,
        before
    );
    assert!(
        store
            .summary_node(
                &conversation,
                &summary.basis_hash,
                "new-strategy",
                "provider-v1"
            )
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .summary_node(
                &conversation,
                &summary.basis_hash,
                "test-v1",
                "new-provider"
            )
            .unwrap()
            .is_none()
    );
    let count: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn commit_and_cache_read_check_real_rows_not_the_frozen_snapshot() {
    let (_temp, store, conversation) = fixture();
    let id = store
        .send_message(&conversation, "user", "old original", None)
        .unwrap();
    let snapshot = store.snapshot_for_input(&conversation, &id).unwrap();
    let summary = node(&snapshot, &conversation);
    store.commit_summary_node(&summary).unwrap();
    store
        .connection
        .execute(
            "UPDATE messages SET content='edited original' WHERE id=?1",
            [&id],
        )
        .unwrap();
    assert_eq!(
        snapshot.list_messages(&conversation).unwrap()[0].content,
        "old original"
    );
    assert!(snapshot.commit_summary_node(&summary).is_err());
    assert!(
        snapshot
            .summary_node(&conversation, &summary.basis_hash, "test-v1", "provider-v1")
            .is_err()
    );
    assert!(
        snapshot
            .read_conversation_history_excerpt(&conversation, &id, 0, 100)
            .is_err()
    );
}

#[test]
fn cancellation_prevents_summary_commit() {
    let (_temp, store, conversation) = fixture();
    let input = store
        .send_message(&conversation, "user", "original", None)
        .unwrap();
    let run = store
        .create_chat_run(&conversation, &input, "summary-cancel", "owner")
        .unwrap();
    let mut snapshot = store.snapshot_for_input(&conversation, &input).unwrap();
    snapshot.chat_run_identity = Some(crate::ai::generation::RunIdentity {
        id: run.id.clone(),
        owner: "owner".into(),
    });
    let summary = node(&snapshot, &conversation);
    store.cancel_chat_run(&conversation, &run.id).unwrap();
    assert!(snapshot.commit_summary_node(&summary).is_err());
    let count: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn coverage_rejects_holes_overlaps_and_cross_conversation_messages() {
    let (_temp, store, conversation) = fixture();
    store
        .send_message(&conversation, "user", "abcdefghij", None)
        .unwrap();
    let summary = node(&store, &conversation);
    let mut a = summary.spans[0].clone();
    let mut b = a.clone();
    a.end_char = 4;
    b.start_char = 5;
    assert!(
        store
            .validate_history_spans(&conversation, &[a.clone(), b.clone()])
            .is_err()
    );
    b.start_char = 3;
    assert!(
        store
            .validate_history_spans(&conversation, &[a.clone(), b.clone()])
            .is_err()
    );
    b.start_char = 4;
    store
        .validate_history_spans(&conversation, &[a, b])
        .unwrap();
    let other = store.create_conversation(Some("other"), None).unwrap();
    assert!(
        store
            .validate_history_spans(&other, &summary.spans)
            .is_err()
    );
}

#[test]
fn exact_excerpt_is_bounded_and_cannot_read_later_inputs() {
    let (_temp, store, conversation) = fixture();
    let original = format!("{}尾部证据", "测".repeat(3000));
    let id = store
        .send_message(&conversation, "user", &original, None)
        .unwrap();
    let snapshot = store.snapshot_for_input(&conversation, &id).unwrap();
    let future = store
        .send_message(&conversation, "user", "not in current run", None)
        .unwrap();
    let excerpt = snapshot
        .read_conversation_history_excerpt(&conversation, &id, 0, usize::MAX)
        .unwrap();
    assert_eq!(excerpt.text.chars().count(), 2400);
    assert!(excerpt.has_more);
    let tail = snapshot
        .read_conversation_history_excerpt(&conversation, &id, 3000, 20)
        .unwrap();
    assert_eq!(tail.text, "尾部证据");
    assert!(!tail.has_more);
    assert!(
        snapshot
            .read_conversation_history_excerpt(&conversation, &future, 0, 100)
            .is_err()
    );
    assert!(
        snapshot
            .read_conversation_history_excerpt(&conversation, &id, 5000, 100)
            .is_err()
    );
}

#[test]
fn explicit_parent_edges_stay_causally_ordered() {
    let (_temp, store, conversation) = fixture();
    let a = store
        .send_message(&conversation, "user", "a", None)
        .unwrap();
    store
        .send_message(&conversation, "assistant", "b", Some(&a))
        .unwrap();
    validate_history_path(&store.list_messages(&conversation).unwrap()).unwrap();
    // 同父两条 habit_checkin（真机现场 2026-10-29 06:03/06:04）：合法分叉，
    // 排序器全序已给出确定的合并路径，两条都是真实历史，都必须可摘要。
    store
        .send_message(&conversation, "user", "checkin-1", Some(&a))
        .unwrap();
    store
        .send_message(&conversation, "user", "checkin-2", Some(&a))
        .unwrap();
    validate_history_path(&store.list_messages(&conversation).unwrap()).unwrap();
    // 悬空父仍拒。
    let mut history = store.list_messages(&conversation).unwrap();
    history.truncate(2);
    history[0].parent_message_id = Some("missing-parent".into());
    assert!(validate_history_path(&history).is_err());
    // 父排在子之后（倒挂）仍拒。
    let mut history = store.list_messages(&conversation).unwrap();
    history.reverse();
    assert!(validate_history_path(&history).is_err());
    // 环仍拒。
    let mut history = store.list_messages(&conversation).unwrap();
    history[1].parent_message_id = Some(history[2].id.clone());
    history[2].parent_message_id = Some(history[1].id.clone());
    assert!(validate_history_path(&history).is_err());
}

#[test]
fn merge_requires_the_exact_child_coverage() {
    let (_temp, store, conversation) = fixture();
    store
        .send_message(&conversation, "user", "original", None)
        .unwrap();
    let child = node(&store, &conversation);
    store.commit_summary_node(&child).unwrap();
    let mut parent = child.clone();
    parent.id = "parent".into();
    parent.basis_hash = "parent-basis".into();
    parent.children = vec![child.id];
    parent.spans[0].end_char -= 1;
    assert!(store.commit_summary_node(&parent).is_err());
    parent.spans = child.spans;
    store.commit_summary_node(&parent).unwrap();
}

#[test]
fn v12_upgrade_keeps_old_messages_and_creates_no_inferred_summaries() {
    let temp = TempDatabase(
        std::env::temp_dir().join(format!("elsewhen-v12-{}.db", uuid::Uuid::new_v4())),
    );
    let mut connection = rusqlite::Connection::open(&temp.0).unwrap();
    crate::storage::new_migrations::initialize_test_version(&mut connection, 12).unwrap();
    connection.execute_batch("INSERT INTO conversations(id,title,created_at,updated_at) VALUES('legacy','legacy','t','t');
        INSERT INTO messages(id,conversation_id,role,content,created_at) VALUES('old-message','legacy','user','legacy original','t');").unwrap();
    crate::storage::new_migrations::initialize(&mut connection).unwrap();
    let content: String = connection
        .query_row(
            "SELECT content FROM messages WHERE id='old-message'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(content, "legacy original");
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
