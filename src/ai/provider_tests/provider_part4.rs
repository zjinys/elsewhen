use super::*;

#[test]
fn ollama_response_parses_eval_counts() {
    let json = r#"{"message":{"content":"hi"},"model":"llama3","prompt_eval_count":20,"eval_count":8}"#;
    let resp: OllamaResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.model.as_deref(), Some("llama3"));
    assert_eq!(resp.prompt_eval_count, Some(20));
    assert_eq!(resp.eval_count, Some(8));
}

#[test]
fn provider_error_body_truncates_long_bodies() {
    // 短 body 原样保留（含关键诊断信息）
    let short = provider_error_body(429, "rate limit exceeded");
    assert_eq!(short, "429: rate limit exceeded");
    // 长 body 截断到上限并标记，避免完整服务端报错堆栈落库/回显
    let long_body = "x".repeat(1000);
    let truncated = provider_error_body(500, &long_body);
    assert!(truncated.chars().count() < 400);
    assert!(truncated.ends_with("…（已截断）"));
    assert!(!truncated.contains(&long_body));
    // 空白 body 不做无意义截断
    assert_eq!(provider_error_body(500, "   "), "500: ");
}
