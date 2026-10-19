use super::*;

#[test]
fn recognizes_known_gateway_wordings() {
    let openai = serde_json::json!({"error":{"code":"context_length_exceeded","message":"..." }});
    let anthropic = serde_json::json!({"error":{"message":"prompt is too long: 200000 tokens > 100000 maximum"}});
    let ollamaish = serde_json::json!({"error":"input token limit exceeded"});
    let chinese = serde_json::json!({"error":{"message":"上下文长度超过限制"}});
    for body in [openai, anthropic, ollamaish, chinese] {
        assert!(is_context_overflow(400, &body), "未识别: {body}");
    }
}

#[test]
fn ignores_other_errors_and_statuses() {
    let rate = serde_json::json!({"error":{"message":"rate limit exceeded"}});
    let tool_err = serde_json::json!({"error":{"message":"prompt is too long but impossible"}});
    assert!(!is_context_overflow(400, &rate));
    // 仅在 400/413/422 判定
    assert!(!is_context_overflow(
        500,
        &serde_json::json!({"error":{"message":"maximum context length"}})
    ));
    assert!(!is_context_overflow(429, &tool_err));
    // 网关 5xx 带超长文案也不误判为重试信号
    assert!(!is_context_overflow(
        502,
        &serde_json::json!({"error":{"message":"maximum context length"}})
    ));
}
