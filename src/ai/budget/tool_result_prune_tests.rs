use super::*;

fn tool_msg(id: &str, size: usize) -> Value {
    serde_json::json!({"role":"tool","tool_call_id":id,"content":"x".repeat(size)})
}

#[test]
fn prunes_all_but_recent_three() {
    let mut messages = vec![
        serde_json::json!({"role":"user","content":"q"}),
        tool_msg("c1", 1000),
        serde_json::json!({"role":"assistant","content":"a1"}),
        tool_msg("c2", 1000),
        tool_msg("c3", 10),
        tool_msg("c4", 10),
        tool_msg("c5", 10),
    ];
    let replaced = prune_old_tool_results(&mut messages, 3);
    assert_eq!(replaced, 2, "7 条里应替换最早的 2 条工具结果");
    assert_eq!(
        messages[1]["content"].as_str().unwrap(),
        PRUNED_TOOL_RESULT_PLACEHOLDER
    );
    assert_eq!(
        messages[3]["content"].as_str().unwrap(),
        PRUNED_TOOL_RESULT_PLACEHOLDER
    );
    // 最近 3 个原样保留；tool_call_id 配对不动（exchange 校验依赖）
    assert_eq!(messages[4]["tool_call_id"], "c3");
    assert_eq!(messages[6]["content"].as_str().map(str::len), Some(10));
    assert_eq!(
        prune_old_tool_results(&mut messages, 3),
        0,
        "幂等：再跑不再替换"
    );
}

/// 场景一：早期工具结果占位化即足以回到预算内——第一轮对话必须完整保留。
/// 工具结果必须 **多于 keep=3** 才会触发占位（fixture 曾只有 3 条踩坑：
/// 「最近 3 次保留」语义下 3<=3 恰好全保留，prune 永远不发生）。
#[test]
fn pruning_old_tool_results_keeps_history_intact() {
    let mut request = serde_json::json!({
        "messages": [
            {"role":"user","content":"第一问"},
            {"role":"assistant","content":"第一答","tool_calls":[{"id":"c0","type":"function","function":{"name":"t","arguments":"{}"}}]},
            tool_msg("c0", 28000),
            {"role":"user","content":"第二问"},
            {"role":"assistant","content":"第二答","tool_calls":[{"id":"c1","type":"function","function":{"name":"t","arguments":"{}"}}]},
            tool_msg("c1", 9000),
            {"role":"user","content":"第三问"},
            {"role":"assistant","content":"第三答","tool_calls":[{"id":"c2","type":"function","function":{"name":"t","arguments":"{}"}}]},
            tool_msg("c2", 9000),
            {"role":"user","content":"第四问"},
            {"role":"assistant","content":"第四答","tool_calls":[{"id":"c3","type":"function","function":{"name":"t","arguments":"{}"}}]},
            tool_msg("c3", 9000),
            {"role":"user","content":"第五问，当前问题必须完整保留"},
        ]
    });
    let first_user = request["messages"][0]["content"].clone();
    let last_user = request["messages"][12]["content"].clone();
    let mut policies = RequestPolicies::plain(&request);
    let count = policies
        .fit("unknown", &mut request, 14000, 256)
        .expect("占位化后应回到预算内");
    assert!(count > 0);
    assert_eq!(
        policies.pruned_tool_results, 1,
        "只有 c0（最早一条）应被占位化"
    );
    let msgs = request["messages"].as_array().unwrap();
    // 第一轮完整保留（先于砍轮被应用）；当前问题在
    assert!(
        msgs.iter()
            .any(|m| m["role"] == "user" && m["content"] == first_user),
        "占位化即足够时不得砍历史轮"
    );
    assert!(
        msgs.iter()
            .any(|m| m["role"] == "user" && m["content"] == last_user)
    );
    // 近 3 个身位规则：c1、c2、c3 原样；c0 占位
    let c0 = msgs.iter().find(|m| m["tool_call_id"] == "c0").unwrap();
    assert_eq!(c0["content"].as_str(), Some(PRUNED_TOOL_RESULT_PLACEHOLDER));
    for id in ["c1", "c2", "c3"] {
        let c = msgs.iter().find(|m| m["tool_call_id"] == id).unwrap();
        assert_eq!(
            c["content"].as_str().map(str::len),
            Some(9000),
            "{id} 应原样保留"
        );
    }
}

/// 场景二：占位化不够仍超预算 → 回落到整轮删除路径，且如实计数。
#[test]
fn when_pruning_is_not_enough_turn_dropping_takes_over() {
    let mut request = serde_json::json!({
        "messages": [
            {"role":"user","content":"第一问"},
            {"role":"assistant","content":"第一答","tool_calls":[{"id":"c0","type":"function","function":{"name":"t","arguments":"{}"}}]},
            tool_msg("c0", 17500),
            {"role":"user","content":"第二问"},
            {"role":"assistant","content":"第二答","tool_calls":[{"id":"c1","type":"function","function":{"name":"t","arguments":"{}"}}]},
            tool_msg("c1", 21000),
            {"role":"user","content":"第三问"},
            {"role":"assistant","content":"第三答","tool_calls":[{"id":"c2","type":"function","function":{"name":"t","arguments":"{}"}}]},
            tool_msg("c2", 21000),
            {"role":"user","content":"第四问，当前问题必须完整保留"},
        ]
    });
    let first_user = request["messages"][0]["content"].clone();
    let last_user = request["messages"][9]["content"].clone();
    let mut policies = RequestPolicies::plain(&request);
    policies
        .fit("unknown", &mut request, 8000, 256)
        .expect("砍轮后应回到预算内");
    let msgs = request["messages"].as_array().unwrap();
    // 第一问被整轮删除（占位不够），当前问题与省略提示在
    assert!(
        !msgs
            .iter()
            .any(|m| m["role"] == "user" && m["content"] == first_user),
        "预算太紧时应砍整轮"
    );
    assert!(
        msgs.iter()
            .any(|m| m["role"] == "user" && m["content"] == last_user)
    );
    assert!(msgs.iter().any(|m| {
        m["role"] == "system"
            && m["content"]
                .as_str()
                .unwrap_or("")
                .contains("较早对话因请求预算已省略")
    }));
}
