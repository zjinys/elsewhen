use super::temp_db;
use super::*;

#[test]
fn execute_knowledge_draft_creates_wiki_page() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    store
        .create_pending_action(
            &conv,
            "save_knowledge_draft",
            &json!({"title": "大企业沟通留痕", "content_md": "和大型企业的人沟通重要事项必须留痕。", "kind": "principle", "tags": ["沟通"]}).to_string(),
        )
        .unwrap();
    let pa = store.pending_actions_for_conversation(&conv).unwrap();
    let summary = execute_pending_action(&store, &pa[0]).unwrap();
    assert!(summary.contains("已保存"), "{summary}");
    let pages = store.list_wiki_pages(None, None).unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].kind, "principle");
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn confirm_knowledge_draft_only_executes_selected_action_once() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let other_conv = store.create_conversation(Some("other"), None).unwrap();
    let draft_id = store
        .create_pending_action(
            &conv,
            "save_knowledge_draft",
            &json!({"title":"闲鱼卖 CM4", "content_md":"完整正文", "kind":"topic"}).to_string(),
        )
        .unwrap();
    let todo_id = store
        .create_pending_action(
            &conv,
            "create_todo",
            &json!({"title":"其他待办"}).to_string(),
        )
        .unwrap();

    assert!(confirm_knowledge_draft(&store, &other_conv, &draft_id).is_err());
    assert!(confirm_knowledge_draft(&store, &conv, &todo_id).is_err());
    assert_eq!(
        store.pending_actions_for_conversation(&conv).unwrap().len(),
        2
    );

    let result = confirm_knowledge_draft(&store, &conv, &draft_id).unwrap();
    assert!(result.contains("slug="), "{result}");
    assert_eq!(
        store
            .find_wiki_page_by_title("闲鱼卖 CM4")
            .unwrap()
            .unwrap()
            .content_md,
        "完整正文"
    );
    assert!(confirm_knowledge_draft(&store, &conv, &draft_id).is_err());
    let remaining = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, todo_id);
    assert!(store.list_todos(None).unwrap().is_empty());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn decline_knowledge_draft_preserves_other_actions_and_pages() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let other = store.create_conversation(Some("other"), None).unwrap();
    let draft_id = store
        .create_pending_action(
            &conv,
            "save_knowledge_draft",
            &json!({"title":"待删除草稿", "content_md":"正文"}).to_string(),
        )
        .unwrap();
    let keep_id = store
        .create_pending_action(
            &conv,
            "save_knowledge_draft",
            &json!({"title":"保留草稿", "content_md":"正文"}).to_string(),
        )
        .unwrap();
    let todo_id = store
        .create_pending_action(
            &conv,
            "create_todo",
            &json!({"title":"其他动作"}).to_string(),
        )
        .unwrap();
    assert!(decline_knowledge_draft(&store, &other, &draft_id).is_err());
    assert!(decline_knowledge_draft(&store, &conv, &todo_id).is_err());
    decline_knowledge_draft(&store, &conv, &draft_id).unwrap();
    assert!(decline_knowledge_draft(&store, &conv, &draft_id).is_err());
    assert!(confirm_knowledge_draft(&store, &conv, &draft_id).is_err());
    let pending = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(pending.len(), 2);
    assert!(pending.iter().any(|action| action.id == keep_id));
    assert!(pending.iter().any(|action| action.id == todo_id));
    assert!(store.list_wiki_pages(None, None).unwrap().is_empty());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn knowledge_draft_does_not_overwrite_same_title() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    store
        .upsert_wiki_page(
            &crate::storage::WikiPageDraft {
                slug: "topic/沟通复盘".to_string(),
                kind: "topic".to_string(),
                title: "沟通复盘".to_string(),
                summary: "原摘要".to_string(),
                content_md: "原有内容".to_string(),
                tags: vec![],
                source_event_ids: vec![],
                status: "active".to_string(),
                reason: "test".to_string(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    let result = dispatch(
        &ToolCall::new(
            "save_knowledge_draft",
            json!({"title":"沟通复盘", "content_md":"新内容"}),
        ),
        &ToolRegistry::default(),
        &store,
        &conv,
    );
    assert!(
        result.content.contains("没有新建或覆盖"),
        "{}",
        result.content
    );
    assert!(
        store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .find_wiki_page_by_title("沟通复盘")
            .unwrap()
            .unwrap()
            .content_md,
        "原有内容"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn knowledge_draft_deduplicates_pending_title_and_confirmation() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let registry = ToolRegistry::default();
    let args = json!({"title":"闲鱼卖 CM4", "content_md":"第一版内容", "kind":"topic"});

    let first = dispatch(
        &ToolCall::new("save_knowledge_draft", args.clone()),
        &registry,
        &store,
        &conv,
    );
    assert!(first.content.contains("待确认"), "{}", first.content);
    let second = dispatch(
        &ToolCall::new(
            "save_knowledge_draft",
            json!({
                "title":" 闲鱼卖 CM4 ",
                "content_md":"第二版内容",
                "kind":"topic"
            }),
        ),
        &registry,
        &store,
        &conv,
    );
    assert!(second.content.contains("不重复创建"), "{}", second.content);
    let actions = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(actions.len(), 1);

    execute_pending_action(&store, &actions[0]).unwrap();
    // 即使历史/并发路径再次执行同一动作，也不会产生第二张随机 slug 页面。
    execute_pending_action(&store, &actions[0]).unwrap();
    let pages = store.list_wiki_pages(Some("topic"), None).unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].title, "闲鱼卖 CM4");

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn propose_people_relations_is_confirm_gated_then_saves() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let registry = ToolRegistry::default();

    // 1) 调用工具：只登记待确认动作，断言尚未写任何页面
    let call = ToolCall::new(
        "propose_people_relations",
        json!({
            "people": [
                {"name": "张三", "role_note": "网站改版项目产研负责人"},
                {"name": "王五", "role_note": "外部合作方"}
            ],
            "relations": [
                {"person": "张三", "target": "网站改版", "relation": "负责"},
                {"person": "王五", "target": "网站改版", "relation": "合作"}
            ]
        }),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(result.content.contains("待确认"), "{}", result.content);
    assert!(
        store.list_wiki_pages(None, None).unwrap().is_empty(),
        "确认前不应建档"
    );
    let pendings = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(pendings.len(), 1);

    // 2) 确认后执行：人物页 + 目标页 + 关系落地
    let summary = execute_pending_action(&store, &pendings[0]).unwrap();
    assert!(summary.contains("张三"), "{summary}");
    assert!(summary.contains("王五"), "{summary}");
    assert!(summary.contains("负责"), "{summary}");
    assert_eq!(
        store.list_wiki_pages(Some("person"), None).unwrap().len(),
        2
    );
    assert_eq!(
        store.list_relations_for_page("person/张三").unwrap().len(),
        1
    );
    assert_eq!(
        store
            .list_relations_for_page("topic/网站改版")
            .unwrap()
            .len(),
        2,
        "目标页自动建档，且两边关系都能查到"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn confirmed_relation_from_conversation_creates_sourced_facts() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let input = store
        .submit_conversation_input(&conv, "@李四 正在负责 #网站改版", Some("fact-1"))
        .unwrap();
    let call = ToolCall::new(
        "propose_people_relations",
        json!({
            "people":[{"name":"李四","role_note":"网站改版负责人"}],
            "relations":[{"person":"李四","target":"网站改版","relation":"负责"}]
        }),
    );
    let result = dispatch(&call, &ToolRegistry::default(), &store, &conv);
    assert!(result.content.contains("待确认"), "{}", result.content);
    let pending = store.pending_actions_for_conversation(&conv).unwrap();
    execute_pending_action(&store, &pending[0]).unwrap();

    let person = store.find_wiki_page_by_title("李四").unwrap().unwrap();
    let target = store.find_wiki_page_by_title("网站改版").unwrap().unwrap();
    let person_facts = store.list_entity_facts(&person.kind, &person.slug).unwrap();
    let target_facts = store.list_entity_facts(&target.kind, &target.slug).unwrap();
    assert_eq!(person_facts.len(), 1);
    assert_eq!(target_facts.len(), 1);
    assert_eq!(person_facts[0].source_event_id, input.event_id.unwrap());
    assert!(target_facts[0].fact_text.contains("李四"));
    drop(store);
    let _ = std::fs::remove_file(path);
}
