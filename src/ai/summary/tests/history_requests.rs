use super::super::*;
use super::add_history;
use super::context;
use super::fixture;
use super::good_reply;
use super::*;

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
    // 第二次走生产装配路径：第一次摘要成功即固化 auto 翻篇标记，重跑
    // 直接读标记跳过边界前原文（零新请求，比节点级缓存更强的复用）。
    let mut second =
        crate::ai::memory::prepare_uncompressed_context(&conversation, &store).unwrap();
    assert_eq!(
        provider.requests.get(),
        requests,
        "持久化标记复用不产生新摘要请求"
    );
    assert!(
        second
            .iter()
            .any(|message| message.segment.purpose == SegmentPurpose::Summary)
    );
    assert!(
        second
            .iter()
            .any(|message| message.content.contains("已翻篇的历史摘要"))
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
fn background_preheat_advances_summary_tree_outside_generation_turns() {
    // P1 追赶修复(b)：后台 tick 以生产同参参数推进摘要树，用户生成轮
    // 零消耗。provider 注入（FakeProvider），HTTP 链路留给解锁态验收。
    let dir = std::env::temp_dir().join(format!("elsewhen-preheat-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("preheat.db")).unwrap();
    let conversation = store
        .create_conversation(Some("预热 fixture"), None)
        .unwrap();
    let pad = "历史细节与随访决定，保证上下文足够大。".repeat(20);
    for round in 0..140 {
        store
            .send_message(
                &conversation,
                "user",
                &format!("第{round}轮提问：{pad}"),
                None,
            )
            .unwrap();
        store
            .send_message(
                &conversation,
                "assistant",
                &format!("第{round}轮回答：{pad}"),
                None,
            )
            .unwrap();
    }
    let provider = FakeProvider::new(good_reply());
    let nodes_before: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
        .unwrap();
    // 生产同款 budget 尺度（window 128K 级）：触发线必然被上下文跨过。
    // 树需求约 92 请求 ∈ (MAX_REQUESTS=64, tick 预算 128)：单批撞顶后，
    // (c) 的多批循环必须在同一 tick 内续跑完，且不触发 tick 预算熔断。
    let advanced = background_preheat_with(
        &store,
        &conversation,
        &provider,
        "mock",
        "mock-basis",
        "gpt-4o",
        8_000,
    )
    .unwrap();
    let nodes_after: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM chat_summary_nodes", [], |r| r.get(0))
        .unwrap();
    assert!(advanced > 0, "预热应推进至少一个摘要节点");
    assert_eq!(nodes_after - nodes_before, advanced);
    // auto mark 已建：二次预热走边界 drain 后的早退/稳态小批，不重算全史。
    let mark: i64 = store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM conversation_page_marks WHERE kind='auto'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(mark > 0, "预热成功后应固化为 auto 翻篇标记");
    // (c) 的核心断言：单批 64 不够建完这棵树，循环必须多批推进。
    let total = provider.requests.get();
    assert!(
        total > super::probe_max_requests(),
        "树需求超过单批上限，tick 内应多批推进（实际请求 {total}）"
    );
    // 账本：本次预热应留 summary-preheat 记录
    let ledger: i64 = store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_background_runs WHERE task='summary-preheat'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ledger, 1, "预热应写运行账本");
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn preheat_channel_matches_uses_run_history_facts() {
    // 渠道错配防线：最近一次真实生成的 provider_name 是唯一判定事实。
    let dir = std::env::temp_dir().join(format!("elsewhen-preheat-guard-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("guard.db")).unwrap();
    let conversation = store
        .create_conversation(Some("判定 fixture"), None)
        .unwrap();
    // 无生成历史：无从判定 → 必须不预热。
    assert!(!super::preheat_channel_matches(&store, &conversation, "mock").unwrap());
    // 造一条历史 run 请求（直接插表，绕过 create_chat_run 的消息校验）。
    store
        .send_message(&conversation, "user", "你好", None)
        .unwrap();
    let user_id: String = store
        .connection
        .query_row(
            "SELECT id FROM messages WHERE conversation_id=?1 AND role='user' ORDER BY created_at LIMIT 1",
            [&conversation],
            |r| r.get(0),
        )
        .unwrap();
    store
        .connection
        .execute(
            "INSERT INTO chat_runs(id,request_key,conversation_id,input_message_id,input_hash,status,phase,executor_id,lease_until,created_at,updated_at)
             VALUES ('run-guard','key-guard',?1,?2,'hash','completed','answering','x','2000-01-01T00:00:00Z','2000-01-01T00:00:00Z','2000-01-01T00:00:00Z')",
            rusqlite::params![conversation, user_id],
        )
        .unwrap();
    store
        .connection
        .execute(
            "INSERT INTO chat_run_requests(id,run_id,provider_name,model,status,started_at)
             VALUES ('req-guard','run-guard','生产渠道',NULL,'completed','2000-01-01T00:00:01Z')",
            [],
        )
        .unwrap();
    assert!(super::preheat_channel_matches(&store, &conversation, "生产渠道").unwrap());
    assert!(!super::preheat_channel_matches(&store, &conversation, "别的渠道").unwrap());
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}
