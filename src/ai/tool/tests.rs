//! AI 工具行为测试（从 mod.rs 内联外移，只经 ToolRegistry::dispatch 公共入口）。

use super::*;

use super::*;

fn temp_db() -> (Store, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "elsewhen-tool-test-{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Store::open(&path).unwrap();
    (store, path)
}

#[test]
fn project_import_confirmation_saves_the_previewed_report() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let directory = std::env::temp_dir().join(format!(
        "elsewhen-project-preview-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("README.md"),
        "# Preview fixture\nA test project.",
    )
    .unwrap();

    let call = ToolCall::new(
        "import_directory_as_project",
        json!({"directory": directory.to_string_lossy()}),
    );
    let preview = dispatch(&call, &ToolRegistry::default(), &store, &conv);
    assert!(
        preview.content.contains("尚未写入知识库"),
        "{}",
        preview.content
    );
    let actions = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(actions.len(), 1);
    let args: Value = serde_json::from_str(&actions[0].args_json).unwrap();
    let expected = args["content_md"].as_str().unwrap().to_string();

    // The original directory is removed after preview; confirmation must save
    // the captured report and must not rescan or invoke the provider again.
    std::fs::remove_dir_all(&directory).unwrap();
    execute_pending_action(&store, &actions[0]).unwrap();
    let page = store
        .list_wiki_pages(Some("project"), None)
        .unwrap()
        .into_iter()
        .find(|p| p.content_md == expected)
        .expect("confirmed project page should contain the previewed report");
    assert_eq!(page.content_md, expected);

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn registry_has_builtin_tools() {
    let registry = ToolRegistry::default();
    let names = registry.names();
    for expected in [
        "list_rules",
        "list_wiki_pages",
        "get_wiki_page",
        "search_knowledge_base",
        "fetch_tweet",
        "fetch_page",
        "record_event",
        "save_knowledge_draft",
        "list_todos",
        "create_todo",
        "propose_people_relations",
        "batch_extract_people_relations",
        "list_events_by_date",
        "archive_conversations_by_title",
        "rename_wiki_page",
        "import_url_to_wiki",
        "save_wiki_revision",
    ] {
        assert!(names.contains(&expected), "缺少工具 {expected}");
    }
    assert_eq!(registry.provider_specs().len(), names.len());
}

#[test]
fn registry_can_hide_record_event_for_auto_recorded_conversations() {
    let registry = ToolRegistry::default();
    let names: Vec<_> = registry
        .provider_specs_for(false)
        .into_iter()
        .map(|spec| spec.name)
        .collect();
    assert!(!names.iter().any(|name| name == "record_event"));
    assert!(names.iter().any(|name| name == "list_events_by_date"));
    assert!(!registry.prompt_block_for(false).contains("record_event"));
}

#[test]
fn dispatch_unknown_tool_is_error_message() {
    let (store, path) = temp_db();
    let registry = ToolRegistry::default();
    let call = ToolCall::new("not_a_tool", json!({}));
    let result = dispatch(&call, &registry, &store, "conv-1");
    assert!(result.content.contains("未知工具"), "{}", result.content);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn list_rules_read_tool_works() {
    let (store, path) = temp_db();
    store
        .add_rule(
            "和大型企业的人沟通重要事项必须留痕",
            crate::storage::RuleStatus::Active,
            None,
        )
        .unwrap();
    let registry = ToolRegistry::default();
    let call = ToolCall::new("list_rules", json!({}));
    let result = dispatch(&call, &registry, &store, "conv-1");
    assert!(result.content.contains("留痕"), "{}", result.content);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn record_event_writes_directly() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let registry = ToolRegistry::default();
    let call = ToolCall::new(
        "record_event",
        json!({"raw_text": "昨天和张玮沟通了双链路付款"}),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(result.content.contains("已保存事件"), "{}", result.content);
    // 直接写入了 events 真源
    assert_eq!(store.list_events().unwrap().len(), 1);
    // 不产生待确认动作
    assert!(store
        .pending_actions_for_conversation(&conv)
        .unwrap()
        .is_empty());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn record_event_does_not_duplicate_unified_conversation_input() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    store
        .submit_conversation_input(&conv, "今天整理了树莓派板子", Some("input-1"))
        .unwrap();
    let registry = ToolRegistry::default();
    let call = ToolCall::new("record_event", json!({"raw_text": "今天整理了树莓派板子"}));

    let result = dispatch(&call, &registry, &store, &conv);

    assert!(
        result.content.contains("无需重复记录"),
        "{}",
        result.content
    );
    assert_eq!(store.list_events().unwrap().len(), 1);
    assert_eq!(store.analysis_job_stats().unwrap().pending, 1);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn execute_record_event_writes_real_source() {
    let (store, path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let action_id = store
        .create_pending_action(
            &conv,
            "record_event",
            &json!({"raw_text": "去健身房练了背部"}).to_string(),
        )
        .unwrap();
    let pa = store.pending_actions_for_conversation(&conv).unwrap();
    let summary = execute_pending_action(&store, &pa[0]).unwrap();
    assert!(summary.contains("已保存"), "{summary}");
    assert_eq!(store.list_events().unwrap().len(), 1);
    let _ = action_id;
    drop(store);
    let _ = std::fs::remove_file(path);
}

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
    assert!(store
        .pending_actions_for_conversation(&conv)
        .unwrap()
        .is_empty());
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
                {"name": "张玮", "role_note": "双链路付款项目产研负责人"},
                {"name": "和太极", "role_note": "外部合作方"}
            ],
            "relations": [
                {"person": "张玮", "target": "双链路付款", "relation": "负责"},
                {"person": "和太极", "target": "双链路付款", "relation": "合作"}
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
    assert!(summary.contains("张玮"), "{summary}");
    assert!(summary.contains("和太极"), "{summary}");
    assert!(summary.contains("负责"), "{summary}");
    assert_eq!(
        store.list_wiki_pages(Some("person"), None).unwrap().len(),
        2
    );
    assert_eq!(
        store.list_relations_for_page("person/张玮").unwrap().len(),
        1
    );
    assert_eq!(
        store
            .list_relations_for_page("topic/双链路付款")
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
        .submit_conversation_input(&conv, "@张伟 正在负责 #付款流程", Some("fact-1"))
        .unwrap();
    let call = ToolCall::new(
        "propose_people_relations",
        json!({
            "people":[{"name":"张伟","role_note":"付款流程负责人"}],
            "relations":[{"person":"张伟","target":"付款流程","relation":"负责"}]
        }),
    );
    let result = dispatch(&call, &ToolRegistry::default(), &store, &conv);
    assert!(result.content.contains("待确认"), "{}", result.content);
    let pending = store.pending_actions_for_conversation(&conv).unwrap();
    execute_pending_action(&store, &pending[0]).unwrap();

    let person = store.find_wiki_page_by_title("张伟").unwrap().unwrap();
    let target = store.find_wiki_page_by_title("付款流程").unwrap().unwrap();
    let person_facts = store.list_entity_facts(&person.kind, &person.slug).unwrap();
    let target_facts = store.list_entity_facts(&target.kind, &target.slug).unwrap();
    assert_eq!(person_facts.len(), 1);
    assert_eq!(target_facts.len(), 1);
    assert_eq!(person_facts[0].source_event_id, input.event_id.unwrap());
    assert!(target_facts[0].fact_text.contains("张伟"));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn list_events_by_date_returns_days_events() {
    let (store, path) = temp_db();
    let registry = ToolRegistry::default();
    let today = chrono::Local::now().date_naive();
    let yesterday = today - chrono::Duration::days(1);
    store
        .insert_event(crate::event::NewEvent {
            raw_text: "今天的事件",
            occurred_at: chrono::Utc::now(),
            recorded_at: chrono::Utc::now(),
            source: "test",
        })
        .unwrap();
    store
        .insert_event(crate::event::NewEvent {
            raw_text: "昨天的事件",
            occurred_at: chrono::Utc::now() - chrono::Duration::days(1),
            recorded_at: chrono::Utc::now() - chrono::Duration::days(1),
            source: "test",
        })
        .unwrap();

    let call = ToolCall::new(
        "list_events_by_date",
        json!({"date": today.format("%Y-%m-%d").to_string()}),
    );
    let result = dispatch(&call, &registry, &store, "conv-1");
    assert!(result.content.contains("今天的事件"), "{}", result.content);
    assert!(!result.content.contains("昨天的事件"), "{}", result.content);
    assert!(result.content.contains("条事件"), "{}", result.content);

    let call = ToolCall::new(
        "list_events_by_date",
        json!({"date": yesterday.format("%Y-%m-%d").to_string()}),
    );
    let result = dispatch(&call, &registry, &store, "conv-1");
    assert!(result.content.contains("昨天的事件"), "{}", result.content);

    // 坏格式与空结果都友好返回
    let call = ToolCall::new("list_events_by_date", json!({"date": "2026/06/20"}));
    let result = dispatch(&call, &registry, &store, "conv-1");
    assert!(result.content.contains("YYYY-MM-DD"), "{}", result.content);
    let call = ToolCall::new("list_events_by_date", json!({"date": "2030-01-01"}));
    let result = dispatch(&call, &registry, &store, "conv-1");
    assert!(
        result.content.contains("没有事件记录"),
        "{}",
        result.content
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn archive_conversations_by_title_is_confirm_gated_then_archives() {
    let (store, path) = temp_db();
    let registry = ToolRegistry::default();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    // 主对话：空标题（显示为「新对话」）、Hello 两个、其他
    store.create_conversation(None, None).unwrap(); // 显示「新对话」
    store
        .create_conversation(Some("Hello World"), None)
        .unwrap();
    store
        .create_conversation(Some("Phase Hello 2"), None)
        .unwrap();
    store.create_conversation(Some("其他"), None).unwrap();
    // 知识页内聊天：不应被匹配
    store
        .create_wiki_chat_conversation("person/x", "页内对话")
        .unwrap();

    // 1) 包含匹配：草拟两个 Hello，未确认前不归档
    let call = ToolCall::new(
        "archive_conversations_by_title",
        json!({"contains": "hello"}),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(result.content.contains("2 个匹配"), "{}", result.content);
    assert!(result.content.contains("Hello World"), "{}", result.content);
    let pendings = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(pendings.len(), 1);
    assert_eq!(store.list_conversations().unwrap().len(), 5, "确认前不归档");

    // 2) 确认执行：归档 2 个（执行不删 pending，手动删以模拟确认流）
    let summary = execute_pending_action(&store, &pendings[0]).unwrap();
    assert!(summary.contains("已归档 2 个对话"), "{summary}");
    store.delete_pending_action(&pendings[0].id).unwrap();
    let remaining_titles: Vec<String> = store
        .list_conversations()
        .unwrap()
        .iter()
        .map(|c| c.title.as_deref().unwrap_or("新对话").to_string())
        .collect();
    assert_eq!(remaining_titles.len(), 3);
    for t in ["t", "新对话", "其他"] {
        assert!(
            remaining_titles.iter().any(|x| x == t),
            "缺 {t}: {remaining_titles:?}"
        );
    }

    // 3) 精确匹配「新对话」= 空标题会话
    let call = ToolCall::new("archive_conversations_by_title", json!({"title": "新对话"}));
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(result.content.contains("1 个匹配"), "{}", result.content);
    let pendings = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(pendings.len(), 1);
    execute_pending_action(&store, &pendings[0]).unwrap();
    store.delete_pending_action(&pendings[0].id).unwrap();
    let remaining_titles: Vec<String> = store
        .list_conversations()
        .unwrap()
        .iter()
        .map(|c| c.title.as_deref().unwrap_or("新对话").to_string())
        .collect();
    assert_eq!(remaining_titles.len(), 2);
    assert!(
        !remaining_titles.iter().any(|x| x == "新对话"),
        "{remaining_titles:?}"
    );

    // 4) 无匹配：友好错误，不登记
    let call = ToolCall::new(
        "archive_conversations_by_title",
        json!({"contains": "不存在的"}),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(result.content.contains("没有找到"), "{}", result.content);
    assert!(store
        .pending_actions_for_conversation(&conv)
        .unwrap()
        .is_empty());

    // 5) 知识页聊天始终未被动过
    assert!(
        store
            .get_conversation(
                &store
                    .find_wiki_chat_conversation("person/x")
                    .unwrap()
                    .unwrap()
            )
            .unwrap()
            .unwrap()
            .archived
            == false
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn rename_wiki_page_is_confirm_gated_then_renames() {
    let (store, path) = temp_db();
    let registry = ToolRegistry::default();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    store
        .upsert_wiki_page(
            &crate::storage::WikiPageDraft {
                slug: "topic/付款流程".to_string(),
                kind: "topic".to_string(),
                title: "付款流程".to_string(),
                summary: "付款流程（自动建档）".to_string(),
                content_md: "# 付款流程".to_string(),
                tags: vec![],
                source_event_ids: vec![],
                status: "active".to_string(),
                reason: "test".to_string(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    store
        .upsert_relation(&crate::storage::RelationDraft {
            from_slug: "person/谭俊".to_string(),
            from_kind: "person".to_string(),
            to_slug: "topic/付款流程".to_string(),
            to_kind: "topic".to_string(),
            relation: "跟进".to_string(),
            note: None,
            confidence: 3,
            source_conversation_id: Some(conv.clone()),
            source_event_id: None,
        })
        .unwrap();

    // 1) 草拟：确认前不动任何数据
    let call = ToolCall::new(
        "rename_wiki_page",
        json!({"slug": "topic/付款流程", "new_title": "fpso111 尾款", "reason": "项目真名更正"}),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(result.content.contains("重命名为"), "{}", result.content);
    assert!(
        result.content.contains("1 条联系人关系"),
        "{}",
        result.content
    );
    let pendings = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(pendings.len(), 1);
    assert!(
        store.get_wiki_page("topic/付款流程").unwrap().is_some(),
        "确认前不应改名"
    );

    // 2) 确认执行：改名 + 关系引用迁移
    let summary = execute_pending_action(&store, &pendings[0]).unwrap();
    assert!(summary.contains("fpso111 尾款"), "{summary}");
    assert!(summary.contains("迁移了 1 条关系引用"), "{summary}");
    assert!(store.get_wiki_page("topic/付款流程").unwrap().is_none());
    assert_eq!(
        store
            .get_wiki_page("topic/fpso111-尾款")
            .unwrap()
            .unwrap()
            .title,
        "fpso111 尾款"
    );
    assert_eq!(
        store.list_relations().unwrap()[0].to_slug,
        "topic/fpso111-尾款"
    );

    // 3) 不存在 / 标题没变：友好错误，不登记 pending
    let call = ToolCall::new(
        "rename_wiki_page",
        json!({"slug": "topic/不存在", "new_title": "x"}),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(
        result.content.contains("没有 slug=topic/不存在 的页面"),
        "{}",
        result.content
    );
    let call = ToolCall::new(
        "rename_wiki_page",
        json!({"slug": "topic/fpso111-尾款", "new_title": "fpso111 尾款"}),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(
        result.content.contains("本来就是这个标题"),
        "{}",
        result.content
    );
    assert_eq!(
        store.pending_actions_for_conversation(&conv).unwrap().len(),
        1
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn save_wiki_revision_rejects_missing_slug_and_never_creates_ghost_page() {
    let (store, path) = temp_db();
    let registry = ToolRegistry::default();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    // 页不存在（如改名后的旧 slug）：草拟阶段直接报错，不登记 pending
    let call = ToolCall::new(
        "save_wiki_revision",
        json!({
            "slug": "topic/fpso111整船项目",
            "title": "FPSO111整船项目",
            "content_md": "# x",
            "change_note": "补充内容"
        }),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(
        result
            .content
            .contains("没有 slug=topic/fpso111整船项目 的页面"),
        "{}",
        result.content
    );
    assert!(store
        .pending_actions_for_conversation(&conv)
        .unwrap()
        .is_empty());
    assert!(
        store
            .get_wiki_page("topic/fpso111整船项目")
            .unwrap()
            .is_none(),
        "不能新建幽灵页"
    );

    // 绕过草拟直接登记 pending + 执行：页面不存在时必须报错而非 upsert 新建
    store
        .create_pending_action(
            &conv,
            "save_wiki_revision",
            &json!({
                "slug": "topic/fpso111整船项目",
                "title": "FPSO111整船项目",
                "content_md": "# x",
                "change_note": "补充内容"
            })
            .to_string(),
        )
        .unwrap();
    let pa = store.pending_actions_for_conversation(&conv).unwrap();
    let err = execute_pending_action(&store, &pa[0]).unwrap_err();
    assert!(err.to_string().contains("知识页不存在"), "{err}");
    assert!(store
        .get_wiki_page("topic/fpso111整船项目")
        .unwrap()
        .is_none());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn fetch_page_strips_html() {
    let text = html_to_text(
        "<html><head><style>.x{}</style></head><body><h1>标题</h1><p>正文内容</p></body></html>",
    );
    assert!(text.contains("标题"), "{text}");
    assert!(text.contains("正文内容"), "{text}");
    assert!(!text.contains("<"), "{text}");
}

#[test]
fn strip_blocks_survives_unicode_prefix_and_case_variant() {
    // to_lowercase 会让 İ（2 字节）变成 i̇（3 字节）：旧实现按小写化后的索引
    // 切原串会 panic，新实现按字符边界定位。
    assert_eq!(
        strip_blocks("İstanbul <search>x</search>", "<search>"),
        "İstanbul "
    );
    // 大小写变体应被识别并按原字节推进。
    assert_eq!(
        strip_blocks("前缀 <Search>内容</Search>", "<search>"),
        "前缀 "
    );
    assert_eq!(strip_blocks("没有标签", "<search>"), "没有标签");
}
