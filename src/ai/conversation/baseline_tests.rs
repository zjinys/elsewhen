// Local HTTP fixtures only. Refresh the baseline explicitly, not as a timing gate.
#[test]
#[ignore = "local fixture benchmark; run explicitly when refreshing C00 baseline"]
fn chat_local_fixture_baseline() {
    use std::time::Instant;

    for scenario in ["direct", "read_tool"] {
        let mut samples = Vec::new();
        for _ in 0..8 {
            let answer = json!({"model":"fixture", "choices":[{"message":{"content":"这是本机测试答复。"},"finish_reason":"stop"}]});
            let mut replies = Vec::new();
            if scenario == "read_tool" {
                replies.push((200, json!({"model":"fixture", "choices":[{"message":{"content":"", "tool_calls":[{"id":"fixture-call", "type":"function", "function":{"name":"list_rules", "arguments":"{}"}}]}, "finish_reason":"tool_calls"}]})));
            }
            replies.push((200, answer));
            let (url, server) = protocol_http_fixture(replies);
            let (store, path) = temporary_database();
            store
                .upsert_ai_provider_config(&url, "fixture", "fixture-key")
                .unwrap();
            let conv = store
                .create_conversation(Some("chat baseline fixture"), None)
                .unwrap();
            let save_start = Instant::now();
            store
                .submit_conversation_input(&conv, "看看已有的个人规则", Some("baseline-input"))
                .unwrap();
            let save_us = save_start.elapsed().as_micros() as u64;
            let start = Instant::now();
            let reply = generate_conversation_reply(&conv, &store, None).unwrap();
            let reply_us = start.elapsed().as_micros() as u64;
            let requests = server.join().unwrap();
            assert!(!reply.is_empty());
            assert_eq!(requests.len(), if scenario == "direct" { 1 } else { 2 });
            samples.push(json!({
                "save_us":save_us, "reply_us":reply_us,
                "request_count":requests.len(),
                "input_estimates":requests.iter().map(|r| crate::ai::budget::input_tokens("fixture", r)).collect::<Vec<_>>()
            }));
            drop(store);
            let _ = std::fs::remove_file(path);
        }
        println!(
            "CHAT_BASELINE {}",
            json!({
                "version":"chat-local-v1", "scenario":scenario,
                "transport":"loopback_http_fixture", "model":"fixture",
                "window":32768, "output_reserve":4096,
                "provider_usage":"not_reported", "first_progress_us":null,
                "first_text_us":null, "cancel_us":null,
                "samples":samples
            })
        );
    }
}
