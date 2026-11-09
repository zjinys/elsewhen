use super::*;
use std::io::{BufRead, Read, Write};
fn server(fail: bool) -> (String, std::thread::JoinHandle<serde_json::Value>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
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
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
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
        assert!(length < 100_000);
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let (status, response) = if fail {
            (
                "503 Service Unavailable",
                serde_json::json!({"error":{"message":"synthetic failure"}}),
            )
        } else {
            let input: crate::storage::compass::CompassInput =
                serde_json::from_str(payload["messages"][1]["content"].as_str().unwrap()).unwrap();
            let verdict = CompassVerdict {
                classification: "drifting".into(),
                confidence: 0.9,
                reason: "连续两天记录未开始访谈".into(),
                question: "本周的计划还适合吗？".into(),
                recommendations: vec!["先复盘优先级".into()],
                citations: input
                    .evidence
                    .iter()
                    .take(2)
                    .map(|e| crate::storage::compass::CompassCitation {
                        source_id: e.source_id.clone(),
                        quote: e.excerpt.clone(),
                    })
                    .collect(),
            };
            (
                "200 OK",
                serde_json::json!({"model":"unknown-budget-model","choices":[{"message":{"role":"assistant","content":serde_json::to_string(&verdict).unwrap()},"finish_reason":"stop"}],"usage":{"prompt_tokens":101,"completion_tokens":22,"total_tokens":123}}),
            )
        };
        let body = response.to_string();
        write!(socket,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        payload
    });
    (url, worker)
}
fn model(url: String, window: u32, reserve: u32) -> OpenAiCompatibleProvider {
    OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        base_url: url,
        api_key: "synthetic".into(),
        model: "unknown-budget-model".into(),
        context_window: Some(window),
        max_tokens: Some(reserve),
        ..Default::default()
    })
    .unwrap()
}
#[test]
fn exact_payload_and_failover_keep_the_same_snapshot_and_candidate_reserve() {
    let f = Fixture::new(&[
        "今天调整网页配色，没有联系客户",
        "今天设计图标，访谈尚未开始",
    ]);
    let (bad, bad_worker) = server(true);
    let (good, good_worker) = server(false);
    let id = uuid::Uuid::new_v4().to_string();
    let chain = FailoverProvider::new(vec![
        FailoverEntry::new(
            format!("bad-{id}"),
            "bad",
            Box::new(model(bad, 24000, 1024)),
        ),
        FailoverEntry::new(
            format!("good-{id}"),
            "good",
            Box::new(model(good, 24000, 1536)),
        ),
    ]);
    let card = tick_with_provider(&f.store, &chain).unwrap().unwrap();
    let bad = bad_worker.join().unwrap();
    let good = good_worker.join().unwrap();
    assert_eq!(bad["messages"], good["messages"]);
    assert_eq!(bad["max_tokens"], 1024);
    assert_eq!(good["max_tokens"], 1536);
    assert!(good.get("tools").is_none());
    let dto = crate::api::goals::compass_dto(&f.store, card).unwrap();
    assert_eq!(dto.assessment.unwrap().evidence.len(), 2);
    let usage: String = f
        .store
        .connection
        .query_row("SELECT usage_json FROM goal_compass_assessments", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(usage.contains("123"));
    let total: i64 = f
        .store
        .connection
        .query_row("SELECT SUM(total_tokens) FROM token_usage", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(total, 123);
}
