use super::*;
use std::io::{BufRead, Read, Write};

fn server(fail: bool) -> (String, std::thread::JoinHandle<serde_json::Value>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut socket = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(std::time::Instant::now() < deadline, "HTTP request missing");
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut reader = std::io::BufReader::new(socket.try_clone().unwrap());
        let mut length = 0;
        loop {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).unwrap() > 0);
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse::<usize>().unwrap();
            }
        }
        assert!(length < 131072);
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let (status, reply) = if fail {
            (
                "503 Service Unavailable",
                json!({"error":{"message":"synthetic unavailable"}}),
            )
        } else {
            let user = payload["messages"][1]["content"].as_str().unwrap();
            let content = if user.contains("[1] ") {
                assert!(user.contains("verified_original_quotes"));
                json!([{"kind":"topic","slug":"topic/long-event","title":"Long event","content":"START 17; MIDDLE not approved; TAIL 2026-12-01","source_event_ids":["1"]}]).to_string()
            } else {
                let input: serde_json::Value =
                    serde_json::from_str(user.split_once('\n').unwrap().1).unwrap();
                let start = input["start"].as_u64().unwrap() as usize;
                let quote: String = input["text"].as_str().unwrap().chars().take(20).collect();
                serde_json::to_string(&ReadingOutput {
                    summary: "START 17 units".into(),
                    quotes: vec![ReadingQuote {
                        start,
                        end: start + quote.chars().count(),
                        text: quote,
                    }],
                })
                .unwrap()
            };
            (
                "200 OK",
                json!({"id":"fixture","model":"unknown-byte-budget-model","choices":[{"index":0,"message":{"role":"assistant","content":content},"finish_reason":"stop"}],"usage":{"prompt_tokens":50,"completion_tokens":50,"total_tokens":100}}),
            )
        };
        let body = reply.to_string();
        write!(socket,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        payload
    });
    (url, worker)
}
fn http_provider(url: String, window: u32, reserve: u32) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        base_url: url,
        api_key: "synthetic".into(),
        model: "unknown-byte-budget-model".into(),
        context_window: Some(window),
        max_tokens: Some(reserve),
        ..Default::default()
    })
    .unwrap()
}
fn chain() -> (
    FailoverProvider,
    Vec<std::thread::JoinHandle<serde_json::Value>>,
) {
    let (bad, bad_worker) = server(true);
    let (good, good_worker) = server(false);
    let unique = uuid::Uuid::new_v4().to_string();
    (
        FailoverProvider::new(vec![
            FailoverEntry::new(
                format!("bad-{unique}"),
                "synthetic bad",
                Box::new(http_provider(bad, 9000, 1536)),
            ),
            FailoverEntry::new(
                format!("good-{unique}"),
                "synthetic good",
                Box::new(http_provider(good, 6500, 1024)),
            ),
        ]),
        vec![bad_worker, good_worker],
    )
}
fn verify_payloads(workers: Vec<std::thread::JoinHandle<serde_json::Value>>) {
    let payloads: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(payloads.len(), 2);
    assert_eq!(payloads[0]["max_tokens"], 1536);
    assert_eq!(payloads[1]["max_tokens"], 1024);
    assert_eq!(
        payloads[0]["messages"][1]["content"],
        payloads[1]["messages"][1]["content"]
    );
    for (payload, (window, reserve)) in payloads.iter().zip([(9000, 1536), (6500, 1024)]) {
        let limit = crate::ai::budget::input_limit(window, reserve).unwrap();
        assert!(
            crate::ai::budget::input_tokens("unknown-byte-budget-model", payload) + 512 <= limit
        );
    }
}

#[test]
fn real_http_leaf_and_final_failover_respect_each_serialized_window_and_output_reserve() {
    let f = long_fixture();
    let (provider, workers) = chain();
    assert!(matches!(
        crate::knowledge::event_reading::advance_leaves(&f.store, &provider, &f.task.id).unwrap(),
        ReadingTick::Advanced { .. }
    ));
    verify_payloads(workers);
    read_all(&f, &TreeModel::new("ok"));
    let (provider, workers) = chain();
    assert!(
        finalize_event_reading(&f.store, &provider, &f.batch, &f.task.id)
            .unwrap()
            .is_some()
    );
    verify_payloads(workers);
    assert_eq!(
        f.store
            .get_wiki_page("topic/long-event")
            .unwrap()
            .unwrap()
            .source_event_ids,
        vec![f.task.event_id.clone()]
    );
}

#[test]
fn strict_reader_does_not_resend_after_server_context_rejection() {
    let f = long_fixture();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut socket = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(e) => panic!("{e}"),
            }
        };
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut reader = std::io::BufReader::new(socket.try_clone().unwrap());
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse::<usize>().unwrap();
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let body =
            r#"{"error":{"code":"context_length_exceeded","message":"synthetic rejection"}}"#;
        write!(socket,"HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(
            matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock),
            "same node must not be sent twice to one channel"
        );
    });
    let provider = http_provider(url, 6500, 1024);
    assert!(matches!(
        crate::knowledge::event_reading::advance_leaves(&f.store, &provider, &f.task.id).unwrap(),
        ReadingTick::Failed { .. }
    ));
    server.join().unwrap();
    assert!(
        f.store.event_reading_nodes(&f.task.id).unwrap()[0]
            .output
            .is_none()
    );
}
