use super::*;
use crate::ai::{provider::ProviderEvent, tool::ToolSpec};
use std::cell::{Cell, RefCell};

struct TempDatabase(std::path::PathBuf);
impl Drop for TempDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(format!("{}-wal", self.0.display()));
        let _ = std::fs::remove_file(format!("{}-shm", self.0.display()));
    }
}

fn fixture() -> (TempDatabase, Store, String) {
    let path =
        std::env::temp_dir().join(format!("elsewhen-summary-ai-{}.db", uuid::Uuid::new_v4()));
    let store = Store::open(&path).unwrap();
    let conversation = store
        .create_conversation(Some("summary fixture"), None)
        .unwrap();
    (TempDatabase(path), store, conversation)
}

fn add_history(store: &Store, conversation: &str) -> String {
    store
        .send_message(
            conversation,
            "user",
            &format!("不要改动原文。{}", "historical background ".repeat(2000)),
            None,
        )
        .unwrap();
    store
        .send_message(
            conversation,
            "assistant",
            "方案仍待审阅，不代表已经执行。",
            None,
        )
        .unwrap();
    store
        .send_message(conversation, "user", "保留最近完整交换", None)
        .unwrap();
    store
        .send_message(conversation, "assistant", "这是最近答复", None)
        .unwrap();
    store
        .send_message(conversation, "user", "当前问题必须原样保留", None)
        .unwrap()
}

fn context(store: &Store, conversation: &str) -> Vec<ContextMessage> {
    let mut context = vec![ContextMessage::new("system", "required rules")];
    context.extend(
        store
            .list_messages(conversation)
            .unwrap()
            .into_iter()
            .map(ContextMessage::from_record),
    );
    context
}

fn good_reply() -> AiReply {
    AiReply::text(
        json!({
            "goals":["整理任务"],"constraints":["不修改原文"],"facts":[],
            "decisions":[],"unfinished":["方案等待审阅，尚未执行"]
        })
        .to_string(),
    )
}

struct FakeProvider {
    requests: Cell<usize>,
    reply: AiReply,
    on_request: RefCell<Option<Box<dyn FnOnce()>>>,
}
impl FakeProvider {
    fn new(reply: AiReply) -> Self {
        Self {
            requests: Cell::new(0),
            reply,
            on_request: RefCell::new(None),
        }
    }
}
impl AiProvider for FakeProvider {
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        assert!(tools.is_none());
        assert_eq!(
            messages.len(),
            2,
            "summary data is one required input, not trimmable history"
        );
        assert_eq!(messages[1].role, "user");
        self.requests.set(self.requests.get() + 1);
        if let Some(action) = self.on_request.borrow_mut().take() {
            action();
        }
        Ok(self.reply.clone())
    }
    fn generate_observed(
        &self,
        messages: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
        observer: &mut dyn FnMut(ProviderEvent<'_>) -> Result<()>,
    ) -> Result<AiReply> {
        observer(ProviderEvent::Poll)?;
        observer(ProviderEvent::TextDelta("摘要内部正文，不应进入用户回复"))?;
        let result = self.generate_reply_with_tools(messages, tools)?;
        observer(ProviderEvent::Poll)?;
        Ok(result)
    }
}

#[test]
fn long_history_gets_a_cached_non_destructive_summary_and_keeps_recent_turns() {
    let (_temp, store, conversation) = fixture();
    add_history(&store, &conversation);
    let originals = store.list_messages(&conversation).unwrap();
    let provider = FakeProvider::new(good_reply());
    let mut first = context(&store, &conversation);
    apply_summary(
        &store,
        &conversation,
        &provider,
        "fixture",
        "config-v1",
        "gpt-4o",
        1800,
        &mut first,
    )
    .unwrap();
    assert!(
        first
            .iter()
            .any(|message| message.segment.purpose == SegmentPurpose::Summary)
    );
    assert!(
        first
            .iter()
            .any(|message| message.content == "保留最近完整交换")
    );
    assert!(
        first
            .iter()
            .any(|message| message.content == "这是最近答复")
    );
    assert!(
        first
            .iter()
            .any(|message| message.content == "当前问题必须原样保留")
    );
    let requests = provider.requests.get();
    assert!(requests > 0 && requests <= MAX_REQUESTS);
    let mut second = context(&store, &conversation);
    apply_summary(
        &store,
        &conversation,
        &provider,
        "fixture",
        "config-v1",
        "gpt-4o",
        1800,
        &mut second,
    )
    .unwrap();
    assert_eq!(
        provider.requests.get(),
        requests,
        "identical ranges and strategy reuse all nodes"
    );
    assert!(
        second
            .iter()
            .any(|message| message.segment.purpose == SegmentPurpose::Summary)
    );
    assert_eq!(
        store
            .list_messages(&conversation)
            .unwrap()
            .iter()
            .map(message_basis)
            .collect::<Vec<_>>(),
        originals.iter().map(message_basis).collect::<Vec<_>>()
    );
}

#[test]
fn queued_history_summary_keeps_answers_with_their_original_requests() {
    let (_temp, store, conversation) = fixture();
    let texts = [
        format!("early constraint {}", "history ".repeat(2500)),
        "second question".into(),
        "third question".into(),
    ];
    let inputs: Vec<_> = texts
        .iter()
        .map(|text| {
            let id = store
                .send_message(&conversation, "user", text, None)
                .unwrap();
            store.enqueue_chat_input(&conversation, &id).unwrap();
            id
        })
        .collect();
    store.resume_chat_queue(&conversation, "process").unwrap();
    for answer in ["first answer", "second answer"] {
        let run = store
            .claim_next_chat_input(&conversation, "process", "worker")
            .unwrap()
            .unwrap();
        store
            .finish_chat_run(&run.id, "worker", "completed", Some(answer), None)
            .unwrap();
    }
    let snapshot = store.snapshot_for_input(&conversation, &inputs[2]).unwrap();
    let mut messages = context(&snapshot, &conversation);
    let provider = FakeProvider::new(good_reply());
    apply_summary(
        &snapshot,
        &conversation,
        &provider,
        "fixture",
        "config",
        "gpt-4o",
        1800,
        &mut messages,
    )
    .unwrap();
    assert_eq!(
        messages
            .iter()
            .filter(|m| m.segment.purpose == SegmentPurpose::History)
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>(),
        ["second question", "second answer", "third question"]
    );
    let history = snapshot.list_messages(&conversation).unwrap();
    let root: String = store.connection.query_row(
        "SELECT id FROM chat_summary_nodes WHERE json_array_length(spans_json)=(SELECT MAX(json_array_length(spans_json)) FROM chat_summary_nodes) LIMIT 1",
        [], |row| row.get(0)).unwrap();
    let spans = snapshot
        .summary_history_spans(&conversation, &root, 0, 32)
        .unwrap();
    assert!(
        spans.iter().any(|span| span.message_id == history[1].id),
        "the first answer is summarized with the first request, not kept as the second turn"
    );
    assert!(
        spans
            .iter()
            .all(|span| span.message_id == inputs[0] || span.message_id == history[1].id)
    );
}

#[test]
fn unicode_chunks_cover_the_whole_message_including_the_tail() {
    let (_temp, store, conversation) = fixture();
    let text = format!("{}末尾的关键约束", "🦀测".repeat(5000));
    store
        .send_message(&conversation, "user", &text, None)
        .unwrap();
    let messages = store.list_messages(&conversation).unwrap();
    let parts = split_history(&messages).unwrap();
    assert_eq!(
        parts
            .iter()
            .map(|part| part.text.as_str())
            .collect::<String>(),
        text
    );
    assert!(parts.last().unwrap().text.ends_with("末尾的关键约束"));
    assert!(
        parts
            .iter()
            .all(|part| part.text.chars().count() <= CHUNK_CHARS)
    );
    store
        .validate_history_spans(
            &conversation,
            &parts
                .iter()
                .map(|part| part.span.clone())
                .collect::<Vec<_>>(),
        )
        .unwrap();
}

#[test]
fn short_or_current_only_history_never_requests_a_summary() {
    let (_temp, store, conversation) = fixture();
    store
        .send_message(&conversation, "user", &"current input ".repeat(2000), None)
        .unwrap();
    let provider = FakeProvider::new(good_reply());
    let mut messages = context(&store, &conversation);
    let before = messages.last().unwrap().content.clone();
    apply_summary(
        &store,
        &conversation,
        &provider,
        "fixture",
        "config",
        "gpt-4o",
        500,
        &mut messages,
    )
    .unwrap();
    assert_eq!(provider.requests.get(), 0);
    assert_eq!(messages.last().unwrap().content, before);
}

#[test]
fn invalid_and_truncated_results_do_not_create_summary_nodes() {
    let (_temp, store, conversation) = fixture();
    add_history(&store, &conversation);
    for reply in [
        AiReply::text("not JSON"),
        AiReply {
            finish_reason: Some("length".into()),
            ..good_reply()
        },
    ] {
        let provider = FakeProvider::new(reply);
        let mut messages = context(&store, &conversation);
        apply_summary(
            &store,
            &conversation,
            &provider,
            "fixture",
            "config",
            "gpt-4o",
            1800,
            &mut messages,
        )
        .unwrap();
        assert!(
            !messages
                .iter()
                .any(|message| message.segment.purpose == SegmentPurpose::Summary)
        );
        assert!(
            messages
                .iter()
                .any(|message| message.content.contains("摘要尚未完成或已失效"))
        );
    }
    let count: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn summary_auxiliary_stream_never_creates_answer_deltas() {
    let (_temp, store, conversation) = fixture();
    let input = add_history(&store, &conversation);
    let run = store
        .create_chat_run(&conversation, &input, "summary-observer", "worker")
        .unwrap();
    let mut snapshot = store.snapshot_for_input(&conversation, &input).unwrap();
    snapshot.chat_run_identity = Some(crate::ai::generation::RunIdentity {
        id: run.id.clone(),
        owner: "worker".into(),
    });
    let mut messages = context(&snapshot, &conversation);
    let provider = FakeProvider::new(good_reply());
    apply_summary(
        &snapshot,
        &conversation,
        &provider,
        "fixture",
        "config",
        "gpt-4o",
        1800,
        &mut messages,
    )
    .unwrap();
    assert!(
        messages
            .iter()
            .any(|message| message.segment.purpose == SegmentPurpose::Summary)
    );
    let events = store.chat_run_events(&run.id, 0, 256).unwrap();
    assert!(
        !events
            .iter()
            .any(|event| matches!(event.kind.as_str(), "draft_reset" | "text_delta"))
    );
    assert!(store.chat_run(&run.id).unwrap().unwrap().content.is_empty());
}

#[test]
fn cancellation_and_real_history_edits_prevent_injection() {
    for cancel in [false, true] {
        let (_temp, store, conversation) = fixture();
        let input = add_history(&store, &conversation);
        let run = store
            .create_chat_run(&conversation, &input, "summary-conflict", "worker")
            .unwrap();
        let mut snapshot = store.snapshot_for_input(&conversation, &input).unwrap();
        snapshot.chat_run_identity = Some(crate::ai::generation::RunIdentity {
            id: run.id.clone(),
            owner: "worker".into(),
        });
        let provider = FakeProvider::new(good_reply());
        let path = store.path.clone();
        let conversation_copy = conversation.clone();
        let original = store.list_messages(&conversation).unwrap()[0].id.clone();
        *provider.on_request.borrow_mut() = Some(Box::new(move || {
            let writer = Store::open(&path).unwrap();
            if cancel {
                writer.cancel_chat_run(&conversation_copy, &run.id).unwrap();
            } else {
                writer
                    .connection
                    .execute(
                        "UPDATE messages SET content='concurrent edit' WHERE id=?1",
                        [&original],
                    )
                    .unwrap();
            }
        }));
        let mut messages = context(&snapshot, &conversation);
        let result = apply_summary(
            &snapshot,
            &conversation,
            &provider,
            "fixture",
            "config",
            "gpt-4o",
            1800,
            &mut messages,
        );
        assert_eq!(result.is_err(), cancel);
        assert!(
            !messages
                .iter()
                .any(|message| message.segment.purpose == SegmentPurpose::Summary)
        );
        let count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[test]
fn incomplete_bounded_work_keeps_leaf_cache_without_claiming_full_coverage() {
    let (_temp, store, conversation) = fixture();
    for _ in 0..45 {
        store
            .send_message(&conversation, "user", &"historical ".repeat(500), None)
            .unwrap();
    }
    let provider = FakeProvider::new(good_reply());
    let mut first = context(&store, &conversation);
    apply_summary(
        &store,
        &conversation,
        &provider,
        "fixture",
        "config",
        "gpt-4o",
        1800,
        &mut first,
    )
    .unwrap();
    assert_eq!(provider.requests.get(), MAX_REQUESTS);
    assert!(
        !first
            .iter()
            .any(|message| message.segment.purpose == SegmentPurpose::Summary)
    );
    let count: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, MAX_REQUESTS as i64);
    let mut second = context(&store, &conversation);
    apply_summary(
        &store,
        &conversation,
        &provider,
        "fixture",
        "config",
        "gpt-4o",
        1800,
        &mut second,
    )
    .unwrap();
    assert!(provider.requests.get() <= MAX_REQUESTS * 2);
    let count_after: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
        .unwrap();
    assert!(count_after > count);
}
