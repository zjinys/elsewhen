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

/// R2.5 通用恢复（2026-11-05）：只剩最近两轮、砍无可砍时，超大工具结果
/// 带披露地砰半而不是整轮拒绝；头部保留、协议配对不动、当前问题完整。
#[test]
fn oversized_recent_tool_result_is_truncated_with_disclosure_not_rejected() {
    let mut request = serde_json::json!({
        "messages": [
            {"role":"user","content":"请读这个列表"},
            {"role":"assistant","content":"","tool_calls":[{"id":"c1","type":"function","function":{"name":"t","arguments":"{}"}}]},
            tool_msg("c1", 60000),
            {"role":"user","content":"当前问题必须完整保留"}
        ]
    });
    let mut policies = RequestPolicies::plain(&request);
    // 窗口 20000、输出 2048 → 输入预算 16952：初始约 19.9K 必超；只剩
    // 两轮不可砍轮，只能走「截断近期工具结果」这条通用恢复路径。
    let count = policies
        .fit("unknown", &mut request, 20000, 2048)
        .expect("工具结果截断后应回到预算内");
    assert!(count <= 20000 - 2048 - 1000);
    let messages = request["messages"].as_array().unwrap();
    let content = messages.iter().find(|m| m["tool_call_id"] == "c1").unwrap()["content"]
        .as_str()
        .unwrap();
    assert!(
        content.contains("工具结果因超过请求预算被截断"),
        "应带自明截断声明"
    );
    assert!(content.starts_with("xxx"), "头部内容保留");
    // 协议配对原样（assistant tool_calls 与 tool 消息都在位）
    assert!(
        messages
            .iter()
            .any(|m| m["role"] == "assistant" && m["tool_calls"][0]["id"] == "c1")
    );
    assert!(
        messages
            .iter()
            .any(|m| m["content"] == "当前问题必须完整保留")
    );
}

/// 已截断的结果在更紧预算下继续收敛（strip 后重截），声明不叠加；
/// 同预算重试幂等。
#[test]
fn tool_result_truncation_converges_without_duplicate_markers() {
    let mut request = serde_json::json!({
        "messages": [
            {"role":"user","content":"q"},
            {"role":"assistant","content":"","tool_calls":[{"id":"c1","type":"function","function":{"name":"t","arguments":"{}"}}]},
            tool_msg("c1", 60000),
            {"role":"user","content":"latest"}
        ]
    });
    let mut policies = RequestPolicies::plain(&request);
    policies.fit("unknown", &mut request, 20000, 2048).unwrap();
    // 更紧预算：同一请求继续砰半，声明不重复。
    policies.fit("unknown", &mut request, 8000, 1024).unwrap();
    let content = request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["tool_call_id"] == "c1")
        .unwrap()["content"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        content.matches("工具结果因超过请求预算被截断").count(),
        1,
        "重复截断不得叠加声明"
    );
    let len = content.chars().count();
    assert!(len < 15200, "应继续收敛到约 15K 字符，实际 {len}");
    // 同预算重试幂等：内容不再变化。
    let before = content.clone();
    policies.fit("unknown", &mut request, 8000, 1024).unwrap();
    let after = request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["tool_call_id"] == "c1")
        .unwrap()["content"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(before, after);
}

/// 小工具结果（写操作回执等不可再生证据，低于截断下限）永不参与截断；
/// 确实装不下时仍是诚实失败，但错误给出可操作建议。
#[test]
fn small_tool_result_receipts_are_never_truncated() {
    let mut request = serde_json::json!({
        "messages": [
            {"role":"user","content":"q"},
            {"role":"assistant","content":"","tool_calls":[{"id":"c1","type":"function","function":{"name":"t","arguments":"{}"}}]},
            tool_msg("c1", 500),
            {"role":"user","content":"最新问题".repeat(300)}
        ]
    });
    let mut policies = RequestPolicies::plain(&request);
    let error = policies
        .fit("unknown", &mut request, 1200, 256)
        .unwrap_err()
        .to_string();
    assert!(error.contains("最近两轮对话不可省略"), "实际错误：{error}");
    assert!(error.contains("MCP"), "应给出可操作建议：{error}");
    let messages = request["messages"].as_array().unwrap();
    let content = messages.iter().find(|m| m["tool_call_id"] == "c1").unwrap()["content"]
        .as_str()
        .unwrap();
    assert_eq!(content.len(), 500, "回执类小结果必须原样保留");
}
