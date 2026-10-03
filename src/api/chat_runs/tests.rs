use super::*;
use serde_json::json;

struct Fixture {
    store: Option<Store>,
    directory: std::path::PathBuf,
    conversation: String,
    input: String,
}

impl Fixture {
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("elsewhen-run-api-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("fixture.db");
        let store = Store::open(&path).unwrap();
        Store::mark_initialized(&path).unwrap();
        let conversation = store
            .create_conversation(Some("run API fixture"), None)
            .unwrap();
        let input = store
            .send_message(&conversation, "user", "synthetic input", None)
            .unwrap();
        Self {
            store: Some(store),
            directory,
            conversation,
            input,
        }
    }

    fn store(&self) -> &Store {
        self.store.as_ref().unwrap()
    }

    fn run(&self) -> ChatRun {
        self.store()
            .create_chat_run(&self.conversation, &self.input, "request", "owner")
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.store.take();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn snapshot_and_events_preserve_identity_and_cursor() {
    let f = Fixture::new();
    let run = f.run();
    f.store()
        .append_chat_run_event(&run.id, "owner", "text_delta", &json!({"text":"first"}))
        .unwrap();
    let before = snapshot(f.store(), &f.conversation, &run.id)
        .unwrap()
        .unwrap();
    assert_eq!(before.input_message_id, f.input);
    assert_eq!(before.content, "first");
    f.store()
        .append_chat_run_event(&run.id, "owner", "text_delta", &json!({"text":" second"}))
        .unwrap();
    let page = event_page(f.store(), &f.conversation, &run.id, before.sequence, 10).unwrap();
    assert_eq!(page.run_id, run.id);
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].sequence, before.sequence + 1);
    assert_eq!(page.next_sequence, page.events[0].sequence);
    assert!(!page.has_more);
    let empty = event_page(f.store(), &f.conversation, &run.id, page.next_sequence, 10).unwrap();
    assert!(empty.events.is_empty());
    assert_eq!(empty.next_sequence, page.next_sequence);
}

#[test]
fn event_pages_are_bounded_and_do_not_skip_rows() {
    let f = Fixture::new();
    let run = f.run();
    for _ in 0..260 {
        f.store()
            .append_chat_run_event(&run.id, "owner", "text_delta", &json!({"text":"x"}))
            .unwrap();
    }
    let first = event_page(f.store(), &f.conversation, &run.id, 0, u32::MAX).unwrap();
    assert_eq!(first.events.len(), 255);
    assert!(first.has_more);
    let second = event_page(
        f.store(),
        &f.conversation,
        &run.id,
        first.next_sequence,
        u32::MAX,
    )
    .unwrap();
    assert_eq!(second.events.len(), 6);
    assert!(!second.has_more);
    assert_eq!(second.events[0].sequence, first.next_sequence + 1);
    assert_eq!(second.next_sequence, 261);
    assert!(event_page(f.store(), &f.conversation, &run.id, -1, 10).is_err());
    assert!(event_page(f.store(), &f.conversation, &run.id, 262, 10).is_err());
}

#[test]
fn snapshot_events_and_cancel_reject_cross_conversation_ids() {
    let f = Fixture::new();
    let run = f.run();
    let other = f
        .store()
        .create_conversation(Some("other conversation"), None)
        .unwrap();
    assert!(snapshot(f.store(), &other, &run.id).is_err());
    assert!(event_page(f.store(), &other, &run.id, 0, 10).is_err());
    assert!(cancel(f.store(), &other, &run.id).is_err());
    assert_eq!(
        f.store().chat_run(&run.id).unwrap().unwrap().status,
        "preparing"
    );
}

#[test]
fn repeated_cancel_is_idempotent_and_does_not_change_completed_runs() {
    let f = Fixture::new();
    let run = f.run();
    let first = cancel(f.store(), &f.conversation, &run.id)
        .unwrap()
        .unwrap();
    let second = cancel(f.store(), &f.conversation, &run.id)
        .unwrap()
        .unwrap();
    assert_eq!(first.status, "cancelling");
    assert_eq!(first.sequence, second.sequence);
    f.store()
        .finish_chat_run(&run.id, "owner", "cancelled", None, None)
        .unwrap();
    let next = f
        .store()
        .create_chat_run(&f.conversation, &f.input, "second request", "next owner")
        .unwrap();
    f.store()
        .finish_chat_run(
            &next.id,
            "next owner",
            "completed",
            Some("verified reply"),
            None,
        )
        .unwrap();
    let completed = cancel(f.store(), &f.conversation, &next.id)
        .unwrap()
        .unwrap();
    assert_eq!(completed.status, "completed");
    assert_eq!(completed.content, "verified reply");
    assert!(completed.final_message_id.is_some());
    assert_eq!(f.store().list_messages(&f.conversation).unwrap().len(), 2);
}

#[test]
fn deletion_returns_no_snapshot_and_cannot_be_cancelled_or_replayed() {
    let f = Fixture::new();
    let run = f.run();
    f.store()
        .set_conversation_archived(&f.conversation, true)
        .unwrap();
    assert!(
        f.store()
            .delete_archived_conversation(&f.conversation)
            .unwrap()
    );
    assert!(
        snapshot(f.store(), &f.conversation, &run.id)
            .unwrap()
            .is_none()
    );
    assert!(
        cancel(f.store(), &f.conversation, &run.id)
            .unwrap()
            .is_none()
    );
    assert!(event_page(f.store(), &f.conversation, &run.id, 0, 10).is_err());
}
