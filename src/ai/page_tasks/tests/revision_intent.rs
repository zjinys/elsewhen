//! 修改意图识别与任务自动升级（评审：说了修改不该被问答任务挡住）。

use super::super::{escalate_page_task, explicit_page_revision_intent};
use super::*;

#[test]
fn explicit_edit_requests_are_recognized() {
    // 真实案例：用户第二次输入（此前两次被问答任务拦下）
    assert!(explicit_page_revision_intent(
        "我设置编辑了，复查的是嘟嘟，待确认中的妞妞和妮妮是同一个娃，你去修改吧"
    ));
    assert!(explicit_page_revision_intent(
        "把这页更新一下，嘟嘟复查的是视力"
    ));
    assert!(explicit_page_revision_intent("补充：两个孩子都近视了"));
    assert!(explicit_page_revision_intent("please update this page"));
    assert!(explicit_page_revision_intent("去把这页修改了"));

    // 纯问句和对修改的评论不算
    assert!(!explicit_page_revision_intent(
        "嘟嘟不是眼睛有问题，只是2个娃都近视了"
    ));
    assert!(!explicit_page_revision_intent("这页的信息如何修改？"));
    assert!(!explicit_page_revision_intent("不用修改，就这样"));
    assert!(!explicit_page_revision_intent("是否需要补充什么内容吗？"));
    assert!(!explicit_page_revision_intent("帮我看看修改记录有哪些"));
}

#[test]
fn escalation_upgrades_qa_and_summary_to_edit_tasks() {
    let db = Db::new();
    let s = &db.store;
    page(s, "第一段\n第二段");
    let c = s
        .create_wiki_chat_conversation("task-page", "test")
        .unwrap();

    // 问答无选区 → 全页修订
    let t = task(s, "local_question");
    let upgraded = escalate_page_task(&t).unwrap();
    assert_eq!(upgraded.kind, "full_revision");
    assert_eq!(upgraded.pages, t.pages);
    assert_eq!(upgraded.start_char, None);
    assert_eq!(upgraded.end_char, None);
    s.save_page_task_input(&c, "帮我修改这一页", "k1", &upgraded)
        .unwrap();

    // 全文总结 → 全页修订
    let t = task(s, "full_summary");
    assert_eq!(escalate_page_task(&t).unwrap().kind, "full_revision");

    // 带选区的问答 → 局部编辑并保留选区
    let mut t = task(s, "local_question");
    t.start_char = Some(0);
    t.end_char = Some(3);
    let upgraded = escalate_page_task(&t).unwrap();
    assert_eq!(upgraded.kind, "local_edit");
    assert_eq!(upgraded.start_char, Some(0));
    assert_eq!(upgraded.end_char, Some(3));
}

#[test]
fn escalation_skips_editable_materialless_and_compare_tasks() {
    let db = Db::new();
    let s = &db.store;
    page(s, "第一\n第二\n第三\n第四\n第五");

    // 已是编辑任务：原样不升级
    let t = task(s, "local_edit");
    let mut with_selection = t.clone();
    with_selection.start_char = Some(0);
    with_selection.end_char = Some(2);
    assert!(escalate_page_task(&with_selection).is_none());
    assert!(escalate_page_task(&task(s, "full_revision")).is_none());

    // 独立问题（无页面材料）不升级
    let none = PageTask {
        kind: "none".into(),
        pages: vec![],
        start_char: None,
        end_char: None,
    };
    assert!(escalate_page_task(&none).is_none());

    // 多页对照不自动升级（第一页未必是用户想改的对象）
    let other = evidenced_page(s, "compare-b", "对照材料 B", vec![]);
    let mut compare = task(s, "local_question");
    compare.kind = "compare".into();
    compare
        .pages
        .push(s.page_task_basis(&other.slug).unwrap().1);
    assert!(escalate_page_task(&compare).is_none());
}

#[test]
fn escalated_full_revision_task_passes_validation_and_revises() {
    let db = Db::new();
    let s = &db.store;
    let content = "事实甲\n事实乙\n事实丙";
    page(s, content);
    let c = s
        .create_wiki_chat_conversation("task-page", "test")
        .unwrap();
    let t = task(s, "local_question");
    let escalated = escalate_page_task(&t).unwrap();
    let validate_context = serde_json::json!({});
    let _ = validate_context;
    // 升级后的任务必须自身合法（版本基线、选区约束都过）
    escalated.clone().validate(s).unwrap();
    s.save_page_task_input(&c, "帮我修改这一页", "k2", &escalated)
        .unwrap();

    // revise_full 用升级任务跑通：分块产出一份待审草稿，save_as=revision
    let result = revise_full(
        s,
        &c,
        &escalated,
        "把事实乙改成事实丙的补充",
        &stub(true),
        "stub",
        &[],
    )
    .unwrap();
    assert!(result.contains("尚未保存"));
    let pending = s.pending_actions_for_conversation(&c).unwrap();
    assert_eq!(pending.len(), 1);
    let args: serde_json::Value = serde_json::from_str(&pending[0].args_json).unwrap();
    assert_eq!(args["save_as"], "revision");
    let body = args["content_md"].as_str().unwrap().trim();
    // 桩逐块返回 edited-i；非编辑任务升级后走同一修订流水，按原序拼接
    assert!(
        body.starts_with("edited-")
            && body
                .split("edited-")
                .all(|part| part.is_empty() || part.chars().all(|c| c.is_ascii_digit())),
        "unexpected body: {body}"
    );
}
