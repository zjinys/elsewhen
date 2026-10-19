use super::super::*;
use super::add_history;
use super::context;
use super::fixture;
use super::good_reply;
use super::*;

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
fn incomplete_bounded_work_keeps_leaf_cache_without_claiming_full_coverage() {
    let (_temp, store, conversation) = fixture();
    // 上限 64 后 45 条消息一轮就能建完（约 15 个请求）；200 条让全量
    // build 需求（约 90+ 请求）超出单轮配额，维持「撞顶后留下叶缓存、
    // 第二轮续跑」的测试意图。
    for _ in 0..200 {
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
