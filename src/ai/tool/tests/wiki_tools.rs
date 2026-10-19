use super::temp_db;
use super::*;

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
