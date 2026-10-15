use super::*;
use serde_json::json;

fn call(ids: &[&str]) -> Value {
    json!({"role":"assistant","tool_calls":ids.iter().map(|id| json!({
        "id":id,"type":"function","function":{"name":"lookup","arguments":"{}"}
    })).collect::<Vec<_>>()})
}

fn result(id: &str) -> Value {
    json!({"role":"tool","tool_call_id":id,"content":"verified result"})
}

#[test]
fn native_parallel_results_may_arrive_in_any_order() {
    let mut request = json!({"messages":[
        {"role":"system","content":"rules"},
        {"role":"user","content":"question"},
        call(&["a", "b"]),result("b"),result("a"),
        call(&["c"]),result("c")
    ]});
    let before = request.clone();
    fit_request("unknown", &mut request, 4096, 512).unwrap();
    assert_eq!(request, before);
}

#[test]
fn malformed_native_exchanges_fail_before_any_budget_trimming() {
    let cases = vec![
        vec![result("orphan")],
        vec![call(&["a"])],
        vec![call(&["a", "b"]), result("a")],
        vec![call(&["a"]), result("wrong")],
        vec![call(&["a"]), result("a"), result("a")],
        vec![call(&["a", "a"]), result("a")],
        vec![call(&["a"]), result("a"), call(&["a"]), result("a")],
        vec![call(&["a"]), call(&["b"]), result("a"), result("b")],
        vec![
            call(&["a"]),
            json!({"role":"user","content":"interruption"}),
            result("a"),
        ],
        vec![
            call(&["a"]),
            json!({"role":"system","content":"interruption"}),
            result("a"),
        ],
        vec![call(&[""]), result("")],
        vec![call(&["  "]), result("  ")],
        vec![json!({"role":"assistant","tool_calls":[{"function":{"name":"lookup"}}]})],
        vec![json!({"role":"assistant","tool_calls":[{"id":17}]})],
        vec![json!({"role":"assistant","tool_calls":"invalid"})],
        vec![
            call(&["a"]),
            json!({"role":"tool","content":"missing identity"}),
        ],
        vec![call(&["a"]), json!({"role":"tool","tool_call_id":17})],
        vec![
            json!({"role":"user","tool_calls":[{"id":"a"}]}),
            result("a"),
        ],
        vec![json!({"role":"assistant","tool_call_id":"a","content":"wrong role"})],
    ];
    for malformed in cases {
        let mut messages = vec![
            json!({"role":"system","content":"rules"}),
            json!({"role":"user","content":"old".repeat(5000)}),
        ];
        messages.extend(malformed);
        messages.push(json!({"role":"user","content":"current question"}));
        messages.push(json!({"role":"system","content":"required evidence"}));
        let mut request = json!({"messages":messages});
        let before = request.clone();
        let error = fit_request("unknown", &mut request, 2048, 256).unwrap_err();
        assert!(error.downcast_ref::<RequestBudgetError>().is_some());
        assert!(
            error.to_string().contains("原生工具上下文校验失败"),
            "{error}"
        );
        assert_eq!(
            request, before,
            "malformed history must not be hidden by trimming"
        );
    }
}

#[test]
fn trimming_removes_whole_exchanges_and_preserves_current_evidence() {
    let protected = vec![
        json!({"role":"user","content":"current question"}),
        json!({"role":"system","content":"required evidence"}),
        call(&["current-a", "current-b"]),
        result("current-b"),
        result("current-a"),
    ];
    let mut messages = vec![
        json!({"role":"system","content":"rules"}),
        json!({"role":"user","content":"old".repeat(3000)}),
        call(&["old-a", "old-b"]),
        result("old-a"),
        result("old-b"),
        json!({"role":"assistant","content":"old answer"}),
    ];
    messages.extend(protected.clone());
    let mut request = json!({"messages":messages});
    fit_request("unknown", &mut request, 2048, 256).unwrap();
    let messages = request["messages"].as_array().unwrap();
    assert_eq!(&messages[messages.len() - protected.len()..], &protected);
    assert!(!request.to_string().contains("old-a"));
    assert!(!request.to_string().contains("old-b"));
}

#[test]
fn post_selection_validation_rejects_an_exchange_broken_by_optional_removal() {
    let mut request = json!({"messages":[
        {"role":"user","content":"current question"},call(&["a"]),
        {"role":"tool","tool_call_id":"a","content":"large".repeat(2000)},
        {"role":"system","content":"required evidence"}
    ]});
    let error =
        fit_request_with_background("unknown", &mut request, 2048, 256, &mut vec![2]).unwrap_err();
    assert!(error.to_string().contains("原生工具上下文校验失败"));
    assert!(
        request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["content"] == "current question")
    );
    assert!(
        request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["content"] == "required evidence")
    );
}

#[test]
fn normalized_text_tool_history_is_not_native_protocol() {
    let mut request = json!({"messages":[
        {"role":"system","content":"rules"},
        {"role":"user","content":"question"},
        {"role":"assistant","content":"[工具调用]{\"name\":\"lookup\",\"arguments\":{}}"},
        {"role":"system","content":"工具执行结果：verified result"},
        {"role":"assistant","content":"An example mentions tool_call_id and tool_calls, not an invocation."}
    ]});
    let before = request.clone();
    fit_request("unknown", &mut request, 4096, 512).unwrap();
    assert_eq!(request, before);
}

#[test]
fn logical_identity_checks_survive_ollama_wire_omission() {
    use crate::ai::{memory::ContextMessage, tool::ToolCall};
    let mut tool = ToolCall::new("lookup", json!({}));
    tool.id = "logical-a".into();
    let context = vec![
        ContextMessage::new("user", "question"),
        ContextMessage::assistant_with_tool_calls(String::new(), vec![tool], None),
        ContextMessage::tool_result("logical-a".into(), "verified result".into()),
    ];
    let mut policies = RequestPolicies::from_context(&context);
    let mut request = json!({"messages":[
        {"role":"user","content":"question"},
        {"role":"assistant","tool_calls":[{"function":{"name":"lookup","arguments":{}}}]},
        {"role":"tool","content":"verified result"}
    ]});
    policies.fit("unknown", &mut request, 4096, 512).unwrap();
    let mut bad_context = context;
    bad_context[2].tool_call_id = Some("wrong".into());
    let mut policies = RequestPolicies::from_context(&bad_context);
    assert!(policies.fit("unknown", &mut request, 4096, 512).is_err());
}
