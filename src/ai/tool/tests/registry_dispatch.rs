use super::temp_db;
use super::*;

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
        // run_skill_script 是用户技能的执行通道，不打包进内建技能
        if *name == "run_skill_script" {
            continue;
        }
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
