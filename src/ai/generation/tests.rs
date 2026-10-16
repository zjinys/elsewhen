use super::*;
use std::io::{Read, Write};

#[test]
fn auxiliary_stream_keeps_answer_draft_and_accounts_separate_usage() {
    use crate::ai::{
        memory::ContextMessage,
        provider::{AiProvider, AiReply, ProviderEvent, TokenUsage},
        tool::ToolSpec,
    };
    struct SummaryProvider;
    impl AiProvider for SummaryProvider {
        fn generate_reply_with_tools(
            &self,
            _: Vec<ContextMessage>,
            tools: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            assert!(tools.is_none());
            let mut reply = AiReply::text("generated summary, not answer");
            reply.usage = Some(TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                        prompt_cached_tokens: None,
        });
            Ok(reply)
        }
        fn generate_observed(
            &self,
            messages: Vec<ContextMessage>,
            tools: Option<&[ToolSpec]>,
            observer: &mut dyn FnMut(ProviderEvent<'_>) -> Result<()>,
        ) -> Result<AiReply> {
            observer(ProviderEvent::TextDelta("generated summary, not answer"))?;
            self.generate_reply_with_tools(messages, tools)
        }
    }
    let path = std::env::temp_dir().join(format!("elsewhen-auxiliary-{}.db", uuid::Uuid::new_v4()));
    let mut store = Store::open(&path).unwrap();
    let conversation = store.create_conversation(Some("auxiliary"), None).unwrap();
    let input = store
        .send_message(&conversation, "user", "input", None)
        .unwrap();
    let run = store
        .create_chat_run(&conversation, &input, "auxiliary", "owner")
        .unwrap();
    store.chat_run_identity = Some(RunIdentity {
        id: run.id.clone(),
        owner: "owner".into(),
    });
    store
        .flush_chat_run_checkpoint(&run.id, "owner", "existing answer draft")
        .unwrap();
    auxiliary_request(
        &store,
        &SummaryProvider,
        "fixture",
        vec![ContextMessage::new("user", "summarize")],
    )
    .unwrap();
    let snapshot = store.chat_run(&run.id).unwrap().unwrap();
    assert_eq!(snapshot.content, "existing answer draft");
    assert_eq!(snapshot.request_count, 1);
    let events = store.chat_run_events(&run.id, 0, 100).unwrap();
    assert!(!events.iter().any(|event| event.kind == "draft_reset"));
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == "text_delta")
            .count(),
        1
    );
    let total: i64 = store
        .connection
        .query_row(
            "SELECT SUM(total_tokens) FROM token_usage WHERE conversation_id=?1",
            [&conversation],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(total, 15);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn observable_run_checkpoints_and_cancels_without_final_message() {
    let path =
        std::env::temp_dir().join(format!("elsewhen-stream-run-{}.db", uuid::Uuid::new_v4()));
    let store = Store::open(&path).unwrap();
    Store::mark_initialized(&path).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    store
        .upsert_ai_provider_config(
            &format!("http://{}", listener.local_addr().unwrap()),
            "gpt-4o",
            "synthetic-key",
        )
        .unwrap();
    let conversation = store
        .create_conversation(Some("stream fixture"), None)
        .unwrap();
    let input = store
        .send_message(&conversation, "user", "解释一个简单概念", None)
        .unwrap();
    let (release, wait) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0; 65536];
        let _ = socket.read(&mut request);
        let body = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"普通说明。已保存全部记录。\"}}]}\n\n";
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 99999\r\n\r\n{body}").unwrap();
        socket.flush().unwrap();
        let _ = wait.recv_timeout(Duration::from_secs(5));
    });
    let run = start(&path, &conversation, &input, "request-key").unwrap();
    let until = Instant::now() + Duration::from_secs(4);
    loop {
        let snapshot = store.chat_run(&run.id).unwrap().unwrap();
        if !snapshot.content.is_empty() {
            assert_eq!(snapshot.content, "普通说明。");
            break;
        }
        assert!(Instant::now() < until, "no safe checkpoint: {:?}", snapshot);
        std::thread::sleep(Duration::from_millis(20));
    }
    let cancelled_at = Instant::now();
    store.cancel_chat_run(&conversation, &run.id).unwrap();
    loop {
        let snapshot = store.chat_run(&run.id).unwrap().unwrap();
        if snapshot.status == "cancelled" {
            assert_eq!(snapshot.content, "普通说明。");
            assert!(snapshot.final_message_id.is_none());
            break;
        }
        assert!(
            cancelled_at.elapsed() < Duration::from_secs(2),
            "cancellation did not stop silent HTTP"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(store.list_messages(&conversation).unwrap().len(), 1);
    let _ = release.send(());
    server.join().unwrap();
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn shutdown_does_not_stop_another_executor() {
    let path = std::env::temp_dir().join(format!("elsewhen-shutdown-{}.db", uuid::Uuid::new_v4()));
    let store = Store::open(&path).unwrap();
    let conversation = store
        .create_conversation(Some("other process"), None)
        .unwrap();
    let input = store
        .send_message(&conversation, "user", "input", None)
        .unwrap();
    let run = store
        .create_chat_run(&conversation, &input, "other", "other-process")
        .unwrap();
    assert_eq!(shutdown(&path).unwrap(), 0);
    store
        .check_chat_run_owner(&run.id, "other-process")
        .unwrap();
    assert_eq!(
        store.chat_run(&run.id).unwrap().unwrap().status,
        "preparing"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}
