use super::*;
use crate::ai::{
    memory::ContextMessage,
    provider::{AiProvider, AiReply},
    tool::ToolSpec,
};

struct Temp(std::path::PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn fixture() -> (Temp, Store, String, String, String) {
    let path =
        std::env::temp_dir().join(format!("elsewhen-supplement-{}.db", uuid::Uuid::new_v4()));
    let store = Store::open(&path).unwrap();
    let conversation = store.create_conversation(Some("supplement"), None).unwrap();
    let original = store
        .send_message(&conversation, "user", "整理材料", None)
        .unwrap();
    let run = store
        .create_chat_run(&conversation, &original, "run", "owner")
        .unwrap();
    let input = store
        .send_message(&conversation, "user", "只保留项目相关部分", None)
        .unwrap();
    (Temp(path), store, conversation, run.id, input)
}
struct Probe(bool);
impl AiProvider for Probe {
    fn generate_reply_with_tools(
        &self,
        _: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        panic!("preflight must not send model request")
    }
    fn accepts_supplement(
        &self,
        messages: &[ContextMessage],
        _: Option<&[ToolSpec]>,
    ) -> Result<bool> {
        assert!(
            messages
                .last()
                .unwrap()
                .content
                .contains("只保留项目相关部分")
        );
        Ok(self.0)
    }
}
#[test]
fn consumption_is_atomic_durable_and_never_queued_twice() {
    let (_temp, store, cid, run, input) = fixture();
    let id = store
        .enqueue_chat_supplement(&cid, &input, &run, "{}")
        .unwrap();
    assert_eq!(
        id,
        store
            .enqueue_chat_supplement(&cid, &input, &run, "{}")
            .unwrap()
    );
    let supplement = store.next_chat_supplement(&cid, &run).unwrap().unwrap();
    assert!(
        store
            .consume_chat_supplement(&run, "owner", &supplement)
            .unwrap()
    );
    assert!(
        !store
            .consume_chat_supplement(&run, "owner", &supplement)
            .unwrap()
    );
    assert!(store.next_chat_supplement(&cid, &run).unwrap().is_none());
    let events = store.chat_run_events(&run, 0, 100).unwrap();
    assert_eq!(
        events.iter().filter(|e| e.kind == "input_injected").count(),
        1
    );
    assert!(!events.iter().any(|e| e.payload_json.contains("只保留")));
    let reopened = Store::open(&store.path).unwrap();
    let row = reopened
        .list_chat_input_queue(&cid, 0, 100)
        .unwrap()
        .remove(0);
    assert_eq!(row.status, "injected");
    assert_eq!(row.run_id.as_deref(), Some(run.as_str()));
    assert_eq!(reopened.list_messages(&cid).unwrap().len(), 2);
}
#[test]
fn withdrawal_cancel_and_changed_input_win_over_late_injection() {
    let (_temp, store, cid, run, input) = fixture();
    let id = store
        .enqueue_chat_supplement(&cid, &input, &run, "{}")
        .unwrap();
    let stale = store.next_chat_supplement(&cid, &run).unwrap().unwrap();
    assert!(store.withdraw_chat_input(&cid, id).unwrap());
    assert!(
        !store
            .consume_chat_supplement(&run, "owner", &stale)
            .unwrap()
    );
    let next = store
        .send_message(&cid, "user", "第二条补充", None)
        .unwrap();
    store
        .enqueue_chat_supplement(&cid, &next, &run, "{}")
        .unwrap();
    let input = store.next_chat_supplement(&cid, &run).unwrap().unwrap();
    store
        .connection
        .execute("UPDATE messages SET content='edited' WHERE id=?1", [&next])
        .unwrap();
    assert!(
        store
            .consume_chat_supplement(&run, "owner", &input)
            .is_err()
    );
    store.cancel_chat_run(&cid, &run).unwrap();
    assert!(
        store
            .consume_chat_supplement(&run, "owner", &input)
            .is_err()
    );
    assert!(store.chat_queue_paused(&cid, "dispatcher").unwrap());
}
#[test]
fn event_failure_rolls_back_consumption() {
    let (_temp, store, cid, run, input) = fixture();
    store
        .enqueue_chat_supplement(&cid, &input, &run, "{}")
        .unwrap();
    let input = store.next_chat_supplement(&cid, &run).unwrap().unwrap();
    store.connection.execute_batch("CREATE TRIGGER fail_injection BEFORE INSERT ON chat_run_events WHEN NEW.kind='input_injected' BEGIN SELECT RAISE(ABORT,'fault'); END;").unwrap();
    assert!(
        store
            .consume_chat_supplement(&run, "owner", &input)
            .is_err()
    );
    assert!(store.next_chat_supplement(&cid, &run).unwrap().is_some());
}
#[test]
fn unsupported_attachments_are_preserved_not_dispatched_as_text() {
    let (_temp, store, cid, run, input) = fixture();
    let envelope = r#"{"attachments":[{"id":"file-fixture","kind":"unsupported"}]}"#;
    store
        .enqueue_chat_supplement(&cid, &input, &run, envelope)
        .unwrap();
    let input = store.next_chat_supplement(&cid, &run).unwrap().unwrap();
    assert!(!store.supplement_is_plain(&input).unwrap());
    store
        .finish_chat_run(&run, "owner", "completed", Some("done"), None)
        .unwrap();
    store.resume_chat_queue(&cid, "dispatcher").unwrap();
    assert!(
        store
            .claim_next_chat_input(&cid, "dispatcher", "next")
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .create_chat_run(&cid, &input.input_id, "manual", "next")
            .is_err()
    );
    let json: String = store
        .connection
        .query_row(
            "SELECT context_json FROM chat_input_queue WHERE id=?1",
            [input.queue_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&json).unwrap(),
        serde_json::from_str::<Value>(envelope).unwrap()
    );
}
#[test]
fn terminal_targets_preserve_pending_supplements_and_require_explicit_continue() {
    for state in ["completed", "failed", "cancelled", "interrupted"] {
        let (_temp, store, cid, run, input) = fixture();
        let queue_id = store
            .enqueue_chat_supplement(&cid, &input, &run, "{}")
            .unwrap();
        store.resume_chat_queue(&cid, "dispatcher").unwrap();
        if state == "interrupted" {
            store
                .connection
                .execute(
                    "UPDATE chat_runs SET lease_until='2000-01-01T00:00:00Z' WHERE id=?1",
                    [&run],
                )
                .unwrap();
            assert_eq!(store.recover_expired_chat_runs().unwrap(), 1);
        } else {
            store
                .finish_chat_run(&run, "owner", state, Some("fixture end"), None)
                .unwrap();
        }
        let reopened = Store::open(&store.path).unwrap();
        let row = reopened
            .list_chat_input_queue(&cid, 0, 100)
            .unwrap()
            .remove(0);
        assert_eq!(row.id, queue_id);
        assert_eq!(row.status, "pending");
        assert_eq!(row.defer_reason.as_deref(), Some("target_closed"));
        assert!(reopened.chat_queue_paused(&cid, "dispatcher").unwrap());
        assert!(
            reopened
                .claim_next_chat_input(&cid, "dispatcher", "next")
                .unwrap()
                .is_none()
        );
        reopened.resume_chat_queue(&cid, "dispatcher").unwrap();
        let next = reopened
            .claim_next_chat_input(&cid, "dispatcher", "next")
            .unwrap()
            .unwrap();
        assert_eq!(next.input_message_id, input);
        assert!(
            reopened
                .claim_next_chat_input(&cid, "dispatcher", "other")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            reopened
                .list_messages(&cid)
                .unwrap()
                .iter()
                .filter(|m| m.role == "user")
                .count(),
            2
        );
    }
}

#[test]
fn closing_targets_does_not_requeue_injected_or_withdrawn_supplements() {
    let (_temp, store, cid, run, input) = fixture();
    store
        .enqueue_chat_supplement(&cid, &input, &run, "{}")
        .unwrap();
    let supplement = store.next_chat_supplement(&cid, &run).unwrap().unwrap();
    store
        .consume_chat_supplement(&run, "owner", &supplement)
        .unwrap();
    let withdrawn = store.send_message(&cid, "user", "不再补充", None).unwrap();
    let id = store
        .enqueue_chat_supplement(&cid, &withdrawn, &run, "{}")
        .unwrap();
    store.withdraw_chat_input(&cid, id).unwrap();
    store.resume_chat_queue(&cid, "dispatcher").unwrap();
    store
        .finish_chat_run(&run, "owner", "completed", Some("done"), None)
        .unwrap();
    let rows = store.list_chat_input_queue(&cid, 0, 100).unwrap();
    assert_eq!(rows[0].status, "injected");
    assert_eq!(rows[1].status, "withdrawn");
    assert!(rows.iter().all(|r| r.defer_reason.is_none()));
    assert!(!store.chat_queue_paused(&cid, "dispatcher").unwrap());
}

#[test]
fn terminal_event_failure_rolls_back_supplement_fallback_and_pause() {
    let (_temp, store, cid, run, input) = fixture();
    store
        .enqueue_chat_supplement(&cid, &input, &run, "{}")
        .unwrap();
    store.resume_chat_queue(&cid, "dispatcher").unwrap();
    store.connection.execute_batch("CREATE TRIGGER fail_terminal BEFORE INSERT ON chat_run_events WHEN NEW.kind='completed' BEGIN SELECT RAISE(ABORT,'fault'); END;").unwrap();
    assert!(
        store
            .finish_chat_run(&run, "owner", "completed", Some("done"), None)
            .is_err()
    );
    assert_eq!(store.chat_run(&run).unwrap().unwrap().status, "preparing");
    assert!(
        store.list_chat_input_queue(&cid, 0, 100).unwrap()[0]
            .defer_reason
            .is_none()
    );
    assert!(!store.chat_queue_paused(&cid, "dispatcher").unwrap());
    assert_eq!(store.list_messages(&cid).unwrap().len(), 2);
}

#[test]
fn page_task_supplements_require_the_exact_original_scope() {
    use crate::storage::{ContentPolicy, WikiPageDraft, chat_page_tasks::PageTask};
    for variant in ["same", "different", "legacy", "changed_after_probe"] {
        let (_temp, store, _, _, _) = fixture();
        let page = store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: "supplement-page".into(),
                    kind: "topic".into(),
                    title: "Synthetic scope".into(),
                    summary: "fixture".into(),
                    content_md: "原文第一段。原文第二段。".into(),
                    tags: vec![],
                    source_event_ids: vec![],
                    status: "active".into(),
                    reason: "fixture".into(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )
            .unwrap()
            .page;
        let cid = store
            .create_wiki_chat_conversation(&page.slug, "scope")
            .unwrap();
        let original_task = PageTask {
            kind: "local_question".into(),
            pages: vec![store.page_task_basis(&page.slug).unwrap().1],
            start_char: Some(0),
            end_char: Some(6),
        };
        let original = if variant == "legacy" {
            store.send_message(&cid, "user", "整理原文", None).unwrap()
        } else {
            store
                .save_page_task_input(&cid, "整理原文", "original", &original_task)
                .unwrap()
        };
        let run = store
            .create_chat_run(&cid, &original, "page-run", "owner")
            .unwrap();
        let mut supplement_task = original_task.clone();
        if variant == "different" {
            supplement_task.start_char = Some(6);
            supplement_task.end_char = Some(12);
        }
        let input = store
            .save_page_task_input(&cid, "只保留项目相关部分", "supplement", &supplement_task)
            .unwrap();
        let id = store
            .enqueue_chat_supplement(&cid, &input, &run.id, "{}")
            .unwrap();
        let queued = store.next_chat_supplement(&cid, &run.id).unwrap().unwrap();
        assert_eq!(
            store.supplement_is_plain(&queued).unwrap(),
            matches!(variant, "same" | "changed_after_probe")
        );
        if variant == "changed_after_probe" {
            supplement_task.start_char = Some(6);
            supplement_task.end_char = Some(12);
            store
                .connection
                .execute(
                    "UPDATE chat_page_tasks SET task_json=?1 WHERE input_message_id=?2",
                    params![serde_json::to_string(&supplement_task).unwrap(), input],
                )
                .unwrap();
        }
        let consumed = store
            .consume_chat_supplement(&run.id, "owner", &queued)
            .unwrap();
        assert_eq!(consumed, variant == "same");
        let row = store.list_chat_input_queue(&cid, 0, 100).unwrap().remove(0);
        assert_eq!(row.id, id);
        if !consumed {
            assert_eq!(row.status, "pending");
            assert_eq!(row.defer_reason.as_deref(), Some("context_changed"));
            assert!(
                store
                    .chat_run_events(&run.id, 0, 100)
                    .unwrap()
                    .iter()
                    .all(|e| e.kind != "input_injected")
            );
        }
    }
}

#[test]
fn budget_refusal_keeps_eligibility_and_success_adds_required_context() {
    let (_temp, mut store, cid, run, input) = fixture();
    store
        .enqueue_chat_supplement(&cid, &input, &run, "{}")
        .unwrap();
    store.chat_run_identity = Some(crate::ai::generation::RunIdentity {
        id: run.clone(),
        owner: "owner".into(),
    });
    let messages = vec![ContextMessage::new("user", "原始问题")];
    assert!(
        crate::ai::supplements::inject_next(&store, &cid, &Probe(false), &messages, None)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store.list_chat_input_queue(&cid, 0, 100).unwrap()[0]
            .defer_reason
            .as_deref(),
        Some("budget_or_protocol")
    );
    let injected = crate::ai::supplements::inject_next(&store, &cid, &Probe(true), &messages, None)
        .unwrap()
        .unwrap();
    assert_eq!(
        injected.segment.purpose,
        crate::ai::memory::SegmentPurpose::TaskSupplement
    );
    assert!(store.next_chat_supplement(&cid, &run).unwrap().is_none());
}
