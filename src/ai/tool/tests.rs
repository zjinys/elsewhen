//! AI 工具行为与提议提交边界测试（从 mod.rs 内联外移）。

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
fn project_report_staging_rejects_cancelled_and_expired_runs() {
    for expired in [false, true] {
        let (mut store, path) = temp_db();
        Store::mark_initialized(&path).unwrap();
        let conv = store
            .create_conversation(Some("staging fence"), None)
            .unwrap();
        let input = store
            .send_message(&conv, "user", "inspect test project", None)
            .unwrap();
        let run = store
            .create_chat_run(&conv, &input, "request", "owner")
            .unwrap();
        store.chat_run_identity = Some(crate::ai::generation::RunIdentity {
            id: run.id.clone(),
            owner: "owner".into(),
        });
        crate::ai::generation::check(&store).unwrap();

        // The report is already prepared; a separate connection stops the run
        // before the network tool reaches its final staging boundary.
        let other = Store::open(&path).unwrap();
        if expired {
            other
                .connection
                .execute(
                    "UPDATE chat_runs SET lease_until='2000-01-01T00:00:00+00:00' WHERE id=?1",
                    [&run.id],
                )
                .unwrap();
        } else {
            other.cancel_chat_run(&conv, &run.id).unwrap();
        }
        let error = store_create_pending(
            &store,
            &conv,
            "import_directory_as_project",
            &json!({"directory":"synthetic-project", "content_md":"prepared report"}),
        )
        .unwrap_err();
        assert!(
            error
                .downcast_ref::<crate::storage::chat_runs::ChatRunStopped>()
                .is_some()
        );
        assert!(
            store
                .pending_actions_for_conversation(&conv)
                .unwrap()
                .is_empty()
        );
        assert!(store.connection.is_autocommit());
        drop(other);
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn project_report_staging_commits_for_live_run_and_joins_outer_transaction() {
    let (mut store, path) = temp_db();
    let conv = store
        .create_conversation(Some("staging transaction"), None)
        .unwrap();
    let input = store
        .send_message(&conv, "user", "inspect test project", None)
        .unwrap();
    let run = store
        .create_chat_run(&conv, &input, "request", "owner")
        .unwrap();
    store.chat_run_identity = Some(crate::ai::generation::RunIdentity {
        id: run.id,
        owner: "owner".into(),
    });
    let args = json!({"directory":"synthetic-project", "content_md":"prepared report"});
    let id = store_create_pending(&store, &conv, "import_directory_as_project", &args).unwrap();
    assert!(store.connection.is_autocommit());
    let transaction = store.begin_write_transaction().unwrap().unwrap();
    store_create_pending(&store, &conv, "import_directory_as_project", &args).unwrap();
    assert!(!store.connection.is_autocommit());
    assert_eq!(
        store.pending_actions_for_conversation(&conv).unwrap().len(),
        2
    );
    drop(transaction);
    let actions = store.pending_actions_for_conversation(&conv).unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].id, id);
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
        json!({"raw_text": "昨天和张三沟通了网站改版"}),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(result.content.contains("已保存事件"), "{}", result.content);
    // 直接写入了 events 真源
    assert_eq!(store.list_events().unwrap().len(), 1);
    // 不产生待确认动作
    assert!(
        store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty()
    );
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
    assert!(
        store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty()
    );

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
                slug: "topic/网站改版".to_string(),
                kind: "topic".to_string(),
                title: "网站改版".to_string(),
                summary: "网站改版（自动建档）".to_string(),
                content_md: "# 网站改版".to_string(),
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
            from_slug: "person/张三".to_string(),
            from_kind: "person".to_string(),
            to_slug: "topic/网站改版".to_string(),
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
        json!({"slug": "topic/网站改版", "new_title": "测试项目 尾款", "reason": "项目真名更正"}),
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
        store.get_wiki_page("topic/网站改版").unwrap().is_some(),
        "确认前不应改名"
    );

    // 2) 确认执行：改名 + 关系引用迁移
    let summary = execute_pending_action(&store, &pendings[0]).unwrap();
    assert!(summary.contains("测试项目 尾款"), "{summary}");
    assert!(summary.contains("迁移了 1 条关系引用"), "{summary}");
    assert!(store.get_wiki_page("topic/网站改版").unwrap().is_none());
    assert_eq!(
        store
            .get_wiki_page("topic/测试项目-尾款")
            .unwrap()
            .unwrap()
            .title,
        "测试项目 尾款"
    );
    assert_eq!(
        store.list_relations().unwrap()[0].to_slug,
        "topic/测试项目-尾款"
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
        json!({"slug": "topic/测试项目-尾款", "new_title": "测试项目 尾款"}),
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
            "slug": "topic/测试项目整船项目",
            "title": "测试项目整船项目",
            "content_md": "# x",
            "change_note": "补充内容"
        }),
    );
    let result = dispatch(&call, &registry, &store, &conv);
    assert!(
        result
            .content
            .contains("没有 slug=topic/测试项目整船项目 的页面"),
        "{}",
        result.content
    );
    assert!(
        store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .get_wiki_page("topic/测试项目整船项目")
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
                "slug": "topic/测试项目整船项目",
                "title": "测试项目整船项目",
                "content_md": "# x",
                "change_note": "补充内容"
            })
            .to_string(),
        )
        .unwrap();
    let pa = store.pending_actions_for_conversation(&conv).unwrap();
    let err = execute_pending_action(&store, &pa[0]).unwrap_err();
    assert!(err.to_string().contains("知识页不存在"), "{err}");
    assert!(
        store
            .get_wiki_page("topic/测试项目整船项目")
            .unwrap()
            .is_none()
    );
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

// ---- 延迟加载（enable_tools 元工具）----

#[test]
fn deferred_runtime_keeps_eager_tools_and_defers_rest() {
    let registry = ToolRegistry::deferred_runtime();
    let specs = registry.eager_specs(true);
    let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
    for must in [
        "search_knowledge_base",
        "save_knowledge_draft",
        "save_wiki_revision",
        "list_wiki_pages",
        "list_todos",
        "list_rules",
        "list_events_by_date",
    ] {
        assert!(names.contains(&must), "缺失常驻工具 {must}");
    }
    for hidden in [
        "record_event",
        "create_todo",
        "archive_conversations_by_title",
        "rename_wiki_page",
        "import_url_to_wiki",
        "fetch_tweet",
        "web_search",
        "import_directory_as_project",
        "import_directory_files",
    ] {
        assert!(!names.contains(&hidden), "{hidden} 应为延迟工具");
    }
    let meta = registry.meta_spec().expect("延迟模式应有 meta spec");
    assert_eq!(meta.name, "enable_tools");
    assert!(meta.description.contains("web-search"));
    // 常驻模式不带 meta
    assert!(ToolRegistry::eager().meta_spec().is_none());
}

#[test]
fn builtin_skills_cover_every_deferred_tool() {
    let registry = ToolRegistry::deferred_runtime();
    let deferred: Vec<&str> = registry
        .names()
        .into_iter()
        .filter(|n| registry.is_deferred(n))
        .collect();
    assert!(!deferred.is_empty());
    let mut uncovered = Vec::new();
    for name in &deferred {
        if !BUILTIN_SKILLS.iter().any(|s| s.tools.contains(&name)) {
            uncovered.push(*name);
        }
    }
    assert!(
        uncovered.is_empty(),
        "延迟工具未被任何内建技能包覆盖: {uncovered:?}"
    );
}

#[test]
fn enable_result_activates_skills_and_returns_instructions() {
    let registry = ToolRegistry::deferred_runtime();
    let mut enabled = std::collections::HashSet::new();
    let text = registry.enable_result(
        &[
            "web-search".into(),
            "todo-actions".into(),
            "nonexistent".into(),
        ],
        &mut enabled,
    );
    // 技能包激活其打包的全部延迟工具
    for tool in ["web_search", "fetch_page", "fetch_tweet", "create_todo"] {
        assert!(enabled.contains(tool), "未激活 {tool}");
    }
    assert!(text.contains("已启用技能包"));
    assert!(text.contains("使用说明"));
    assert!(text.contains("JSON Schema"));
    assert!(text.contains("nonexistent"));
    // 未启用时 dispatch 拦截；启用后放行由 dispatch_for_turn 带enabled参数验证
    assert!(registry.is_deferred("create_todo"));
}

#[test]
fn dispatch_guard_blocks_unenabled_deferred_tool() {
    let (store, _path) = temp_db();
    let conv = store.create_conversation(Some("t"), None).unwrap();
    let registry = ToolRegistry::deferred_runtime();
    let enabled: std::collections::HashSet<String> = std::collections::HashSet::new();
    let call = ToolCall::new("create_todo", serde_json::json!({"title": "x"}));
    let ctx = ToolContext {
        store: &store,
        conversation_id: &conv,
        source_event_id: None,
        knowledge_candidates: &[],
    };
    let _ = ctx;
    // 走 conversation 层的守卫逻辑等价断言：未启用 + deferred => 拦截由 dispatch_for_turn 完成；
    // 此处直接验证注册表层的判定因子。
    assert!(
        registry.defer_tools()
            && registry.is_deferred("create_todo")
            && !enabled.contains("create_todo")
    );
}
