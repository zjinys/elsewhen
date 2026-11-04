//! 计划拆解工具的确认制流程测试（P2）：未确认零落库、确认后整计划落库、
//! 第一步激活其余排队、追加拆解序号接续。

use super::*;

#[test]
fn create_plan_tool_is_confirm_gated_then_commits_plan_with_steps() {
    let (store, path) = temp_db();
    let registry = ToolRegistry::default();
    let conv = store.create_conversation(Some("计划讨论"), None).unwrap();

    // 1) 工具调用只登记草案：未确认零落库。
    let call = ToolCall::new(
        "create_plan",
        json!({
            "intent_text": "3 年内月入 10 万",
            "vision": "可持续的独立收入",
            "assessment": "主业占用 60% 时间，需要先验证付费意愿",
            "assumptions": [
                {"kind": "时间窗", "text": "3 年"},
                {"kind": "现状", "text": "现在没有付费收入"}
            ],
            "steps": [
                {"content": "第一步：验证付费意愿", "phase": "near", "done_criteria": "10 次访谈完成"},
                {"content": "第二步：做出最小可售卖方案", "phase": "near"},
                {"content": "第三步：拿到首笔收入", "phase": "mid"}
            ]
        }),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(result.success, "{}", result.content);
    assert!(result.content.contains("待确认"), "{}", result.content);
    assert!(store.list_plans().unwrap().is_empty(), "确认前零落库");
    let pendings = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(pendings.len(), 1);
    assert_eq!(pendings[0].action, "create_plan");

    // 2) 确认执行：计划 + 3 步落库；第一步激活、其余排队。
    let summary = execute_pending_action(&store, &pendings[0]).unwrap();
    assert!(summary.contains("已创建计划"), "{summary}");
    // 执行不删 pending，手动删以模拟确认流的清理（与既有测试同惯例）。
    store.delete_pending_action(&pendings[0].id).unwrap();
    let plans = store.list_plans().unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].intent_text, "3 年内月入 10 万");
    let plan_id = plans[0].id.clone();
    let steps = store.list_plan_steps(&plan_id).unwrap();
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0].status, crate::storage::GoalStatus::Active);
    assert_eq!(steps[1].status, crate::storage::GoalStatus::Paused);
    assert_eq!(steps[2].status, crate::storage::GoalStatus::Paused);

    // 3) add_plan_steps：提案待确认 → 确认后序号接续、全部排队。
    let call = ToolCall::new(
        "add_plan_steps",
        json!({
            "plan_id": plan_id,
            "steps": [{"content": "第四步：规模化", "phase": "long"}]
        }),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(result.success, "{}", result.content);
    let pendings = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(pendings.len(), 1, "追加提案待确认");
    assert!(store.list_plan_steps(&plan_id).unwrap().len() == 3, "确认前不追加");
    execute_pending_action(&store, &pendings[0]).unwrap();
    let steps = store.list_plan_steps(&plan_id).unwrap();
    assert_eq!(steps.len(), 4);
    assert_eq!(steps[3].step_index, Some(4));
    assert_eq!(steps[3].status, crate::storage::GoalStatus::Paused);

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn create_plan_rejects_empty_steps_without_registering_a_draft() {
    let (store, path) = temp_db();
    let registry = ToolRegistry::default();
    let conv = store.create_conversation(Some("t"), None).unwrap();

    let call = ToolCall::new("create_plan", json!({"intent_text": "x", "steps": []}));
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(!result.success, "{}", result.content);
    assert!(
        store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty(),
        "校验失败不登记草案"
    );

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn plan_proposal_supports_itemized_edit_then_accept_through_review_contract() {
    let (store, path) = temp_db();
    let registry = ToolRegistry::default();
    let conv = store.create_conversation(Some("计划审阅"), None).unwrap();

    let call = ToolCall::new(
        "create_plan",
        json!({
            "intent_text": "3 年内月入 10 万",
            "vision": "",
            "assessment": null,
            "assumptions": [{"kind": "现状", "text": "没有付费收入"}],
            "steps": [{"content": "第一步：验证付费意愿", "phase": "near"}]
        }),
    );
    dispatch(&call, &registry, &store, &conv);
    let pa = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(pa.len(), 1);

    let proposal = store
        .chat_proposal("action", &pa[0].id, Some(&conv))
        .unwrap()
        .unwrap();

    // 逐项纠正：假设与步骤改写；意图原文与其余键必须原样带回（身份锚不可改）。
    let edited = json!({
        "intent_text": "3 年内月入 10 万",
        "vision": "",
        "assessment": null,
        "assumptions": [{"kind": "路径", "text": "先做 10 次访谈"}],
        "steps": [{"content": "修正后的第一步", "phase": "near"}]
    })
    .to_string();
    let updated = store
        .edit_chat_proposal(
            "action",
            &pa[0].id,
            Some(&conv),
            proposal.version,
            &proposal.approval_token,
            &edited,
        )
        .unwrap();
    assert_ne!(
        updated.approval_token, proposal.approval_token,
        "纠正后审批 token 重新绑定"
    );

    // 新版本接受并按纠正后的草案执行。
    let decision = crate::ai::approval::resolve_chat_proposal(
        &store,
        "action",
        &pa[0].id,
        Some(&conv),
        updated.version,
        &updated.approval_token,
        "accept",
    )
    .unwrap();
    assert_eq!(decision.status, "succeeded", "{}", decision.result);
    let plans = store.list_plans().unwrap();
    assert_eq!(plans.len(), 1);
    assert!(plans[0].assumptions.contains("先做 10 次访谈"));
    let steps = store.list_plan_steps(&plans[0].id).unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].content, "修正后的第一步", "采纳的是纠正后的版本");

    drop(store);
    let _ = std::fs::remove_file(path);
}
