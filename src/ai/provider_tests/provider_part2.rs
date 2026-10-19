use super::*;

#[test]
fn openai_response_parses_usage_and_model() {
    let json = r#"{"choices":[{"message":{"content":"hi"}}],"model":"gpt-4o","usage":{"prompt_tokens":12,"completion_tokens":5,"total_tokens":17}}"#;
    let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.model.as_deref(), Some("gpt-4o"));
    let u = resp.usage.unwrap();
    assert_eq!(u.prompt_tokens, Some(12));
    assert_eq!(u.completion_tokens, Some(5));
    assert_eq!(u.total_tokens, Some(17));
}

#[test]
fn openai_response_without_usage_is_none() {
    // 有些 provider 不返回 usage，应容忍缺失
    let json = r#"{"choices":[{"message":{"content":"hi"}}]}"#;
    let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
    assert!(resp.usage.is_none());
    assert!(resp.model.is_none());
}

/// 实测坏响应（2026-09-29，hub.oaifree.com + gpt-4o，8/8 复现）：
/// 声称调用了函数，却把 tool_calls 整个丢掉，message 里只剩 role。
/// `completion_tokens` 有值，只说明上游报告了用量。
///
/// 这条测试的作用是钉住「finish_reason 必须被接住」：没有它，这种响应
/// 和真的「模型返回空」在下游完全无法区分，排查只能靠人工重放请求。
#[test]
fn openai_response_keeps_finish_reason_when_tool_calls_are_dropped() {
    let json = r#"{
        "choices":[{
            "message":{"role":"assistant"},
            "finish_reason":"function_call",
            "logprobs":null
        }],
        "object":"chat.completion",
        "model":"gpt-4o",
        "usage":{"prompt_tokens":5759,"completion_tokens":58,"total_tokens":5817}
    }"#;
    let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
    let choice = &resp.choices[0];

    // 坏响应的三个特征，缺一不可
    assert_eq!(choice.finish_reason.as_deref(), Some("function_call"));
    assert!(choice.message.content.is_none());
    assert!(choice.message.tool_calls.is_none());

    // 用量独立保留，不能推导出被计费内容在链路中哪个位置丢失。
    assert_eq!(resp.usage.unwrap().completion_tokens, Some(58));
}

#[test]
fn openai_response_tolerates_missing_finish_reason() {
    // 老 provider 不给这个字段，不能因此解析失败
    let json = r#"{"choices":[{"message":{"content":"hi"}}]}"#;
    let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
    assert!(resp.choices[0].finish_reason.is_none());
}
