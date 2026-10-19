use super::*;

#[test]
fn openai_response_keeps_normal_finish_reason() {
    // 正常路径不能被这次改动影响
    let json = r#"{"choices":[{"message":{"content":"hi"},"finish_reason":"stop"}]}"#;
    let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.choices[0].finish_reason.as_deref(), Some("stop"));
    assert_eq!(resp.choices[0].message.content.as_deref(), Some("hi"));
}

#[test]
fn openai_response_parses_tool_calls() {
    let json = r#"{
        "choices":[{"message":{
            "content":null,
            "tool_calls":[
                {"id":"call_1","type":"function","function":{"name":"list_rules","arguments":"{}"}},
                {"id":"call_2","type":"function","function":{"name":"get_wiki_page","arguments":"{\"slug\":\"kb-test\"}"}}
            ]
        }}],
        "model":"gpt-4o",
        "usage":{"prompt_tokens":12,"completion_tokens":5,"total_tokens":17}
    }"#;
    let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
    let msg = &resp.choices[0].message;
    assert!(msg.content.is_none());
    let calls = msg.tool_calls.clone().map(response_tool_calls).unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].name, "list_rules");
    assert_eq!(calls[1].name, "get_wiki_page");
    assert_eq!(calls[1].arguments["slug"], "kb-test");
    // raw arguments 必须原样保留（OpenAI 回传要求逐字节一致，含原始空格）
    assert_eq!(
        calls[1].raw_arguments.as_deref(),
        Some("{\"slug\":\"kb-test\"}")
    );
    // 生成 id 的 ToolCall 可被原生协议回传
    let echoed = openai_message_tool_calls(&calls);
    assert_eq!(echoed.len(), 2);
    assert_eq!(echoed[1].function.arguments, "{\"slug\":\"kb-test\"}");
}

#[test]
fn legacy_function_call_is_normalized_without_losing_arguments_or_body() {
    for tools in [serde_json::Value::Null, serde_json::json!([])] {
        let raw = serde_json::json!({"choices":[{
            "message": {"content":"检查规则", "tool_calls": tools,
                "function_call":{"name":"list_rules", "arguments":"{  }"}},
            "finish_reason":"function_call"
        }]});
        let resp: OpenAiResponse = serde_json::from_value(raw).unwrap();
        let calls = response_message_tool_calls(&resp.choices[0].message);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "list_rules");
        assert_eq!(calls[0].arguments, serde_json::json!({}));
        assert_eq!(calls[0].raw_arguments.as_deref(), Some("{  }"));
        assert!(!calls[0].id.is_empty());
        let echoed = openai_message_tool_calls(&calls);
        assert_eq!(echoed[0].id, calls[0].id);
        assert_eq!(echoed[0].function.arguments, "{  }");
        assert_eq!(resp.choices[0].message.content.as_deref(), Some("检查规则"));
    }
}

#[test]
fn modern_and_legacy_fields_do_not_duplicate_a_tool_action() {
    let raw = serde_json::json!({"choices":[{"message":{
        "tool_calls":[{"id":"modern", "function":{"name":"list_rules", "arguments":"{}"}}],
        "function_call":{"name":"list_rules", "arguments":"{}"}
    }}]});
    let resp: OpenAiResponse = serde_json::from_value(raw).unwrap();
    let calls = response_message_tool_calls(&resp.choices[0].message);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id, "modern");
}
