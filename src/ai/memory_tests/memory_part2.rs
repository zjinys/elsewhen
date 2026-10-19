#[allow(unused_imports)]
use super::*;

#[test]
fn compress_context_never_truncates_primary_prompt_even_if_over_budget() {
    let primary = "核心指令".repeat(100);
    let mut context = vec![
        ContextMessage::new("system", primary.clone()),
        ContextMessage::new("user", "当前问题"),
    ];

    compress_context(&mut context, 10);

    assert_eq!(context[0].content, primary);
    assert_eq!(context.last().unwrap().content, "当前问题");
}

#[test]
fn compress_context_does_nothing_when_within_budget() {
    let mut context = vec![ContextMessage::new("user", "短消息")];
    let before = context.clone();
    compress_context(&mut context, 40);
    assert_eq!(context.len(), before.len());
    assert_eq!(context[0].content, before[0].content);
}

#[test]
fn system_prompt_states_no_goals_when_empty() {
    let (store, conversation_id, path) = setup_store();
    let prompt = system_prompt_of(&store, &conversation_id);
    assert!(
        prompt.contains("尚未设定任何目标"),
        "空态应明说无目标，不静默省略：\n{prompt}"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn system_prompt_no_longer_carries_rule_corpus() {
    let (store, conversation_id, path) = setup_store();
    let rule_text = "和大型企业的人沟通重要事项必须留痕";
    store
        .add_rule(rule_text, crate::storage::RuleStatus::Active, None)
        .unwrap();
    let prompt = system_prompt_of(&store, &conversation_id);
    // 规则库解耦瘦身：基座只播报规模与查询入口，规则原文不随对话注入
    // （全文按需经 list_rules 工具获取），stable_setup 不随规则数膨胀。
    assert!(
        prompt.contains("个人规则库现有 1 条生效规则"),
        "实际:\n{prompt}"
    );
    assert!(prompt.contains("list_rules"), "应指名查询工具");
    assert!(
        !prompt.contains(rule_text),
        "规则原文不应再进基座，实际:\n{prompt}"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn system_prompt_injects_active_goals_with_phase_labels() {
    use crate::storage::GoalPhase;
    let (store, conversation_id, path) = setup_store();
    store
        .create_goal("三个月内发 1.0", GoalPhase::Near)
        .unwrap();
    store
        .create_goal("三年内决策有据", GoalPhase::Long)
        .unwrap();

    let prompt = system_prompt_of(&store, &conversation_id);
    assert!(prompt.contains("[近期] 三个月内发 1.0"), "实际:\n{prompt}");
    assert!(prompt.contains("[长远] 三年内决策有据"), "实际:\n{prompt}");
    let _ = std::fs::remove_file(path);
}

/// 目标段必须带反向护栏：只作背景，不评判偏离。
///
/// 这条护栏是需求里的硬约束，不是措辞偏好——没有它，对话里每轮都会拿用户
/// 的话去比目标，偏差检测就变成审计。
#[test]
fn goal_block_carries_guards_against_nagging() {
    use crate::storage::GoalPhase;
    let (store, conversation_id, path) = setup_store();
    store.create_goal("上线 v1", GoalPhase::Near).unwrap();
    let prompt = system_prompt_of(&store, &conversation_id);
    assert!(
        prompt.contains("不要评判用户是否偏离目标"),
        "缺少不评判偏离的护栏：\n{prompt}"
    );
    assert!(
        prompt.contains("不要把对话引向目标"),
        "缺少不把对话引向目标的护栏：\n{prompt}"
    );
    let _ = std::fs::remove_file(path);
}

/// 已归档目标不得注入：那是历史，不是当前追求。
#[test]
fn archived_goals_are_not_injected() {
    use crate::storage::GoalPhase;
    let (store, conversation_id, path) = setup_store();
    let goal = store.create_goal("已经放弃的事", GoalPhase::Mid).unwrap();
    store.archive_goal(&goal.id).unwrap();

    let prompt = system_prompt_of(&store, &conversation_id);
    assert!(
        !prompt.contains("已经放弃的事"),
        "已归档目标不应进上下文：\n{prompt}"
    );
    let _ = std::fs::remove_file(path);
}
