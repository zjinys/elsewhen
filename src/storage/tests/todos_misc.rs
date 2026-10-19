use super::*;

#[test]
fn update_todo_edits_fields_and_clears_optionals() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let todo = store
        .create_todo(
            "跟进付款",
            "normal",
            Some("2026-09-20"),
            None,
            None,
            Some("原始说明"),
            false,
        )
        .unwrap();

    // 编辑：标题/说明/优先级/截止都改
    store
        .update_todo(
            &todo.id,
            "跟进网站改版",
            Some("已和张三对齐时间"),
            Some("high"),
            Some("2026-09-18"),
        )
        .unwrap();
    let updated = store.list_todos(None).unwrap();
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0].title, "跟进网站改版");
    assert_eq!(updated[0].note.as_deref(), Some("已和张三对齐时间"));
    assert_eq!(updated[0].priority, "high");
    assert_eq!(updated[0].due_at.as_deref(), Some("2026-09-18"));
    assert_eq!(updated[0].status, TodoStatus::Open, "编辑不改状态");

    // 清除可选字段：传 None
    store
        .update_todo(&todo.id, "跟进网站改版", None, None, None)
        .unwrap();
    let cleared = store.list_todos(None).unwrap();
    assert!(cleared[0].note.is_none());
    assert!(cleared[0].due_at.is_none());
    assert_eq!(cleared[0].priority, "normal");

    // 空标题拒绝
    assert!(
        store
            .update_todo(&todo.id, "   ", None, None, None)
            .is_err()
    );
    let _ = std::fs::remove_file(path);
}
#[test]
fn work_item_link_is_lazy_idempotent_and_keeps_completed_history() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let todo = store
        .create_todo(
            "讨论发布策略",
            "high",
            Some("2026-09-30"),
            None,
            None,
            Some("先收集约束"),
            false,
        )
        .unwrap();

    // 创建待办不会隐式制造知识页；页面升级后关联可回溯。
    assert!(todo.related_wiki_slug.is_none());
    store
        .set_todo_related_wiki_slug(&todo.id, "topic/discuss-release")
        .unwrap();
    let linked = store.list_todos(None).unwrap();
    assert_eq!(
        linked[0].related_wiki_slug.as_deref(),
        Some("topic/discuss-release")
    );

    // 重复升级只覆盖同一关联，不产生第二条待办或改变其字段。
    store
        .set_todo_related_wiki_slug(&todo.id, "topic/discuss-release")
        .unwrap();
    assert_eq!(store.list_todos(None).unwrap().len(), 1);
    store
        .update_todo_status(&todo.id, TodoStatus::Done)
        .unwrap();
    let completed = store.list_todos(None).unwrap();
    assert_eq!(completed[0].status, TodoStatus::Done);
    assert_eq!(
        completed[0].related_wiki_slug.as_deref(),
        Some("topic/discuss-release")
    );
    let _ = std::fs::remove_file(path);
}
#[test]
fn get_todo_includes_archived_history_for_on_demand_migration() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let todo = store
        .create_todo("归档后仍可讨论", "normal", None, None, None, None, false)
        .unwrap();
    store
        .update_todo_status(&todo.id, TodoStatus::Archived)
        .unwrap();
    let found = store.get_todo(&todo.id).unwrap().unwrap();
    assert_eq!(found.status, TodoStatus::Archived);
    assert!(store.list_todos(None).unwrap().is_empty());
    let _ = std::fs::remove_file(path);
}
#[test]
fn list_conversations_excludes_wiki_page_chats() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let normal = store.create_conversation(Some("普通对话"), None).unwrap();
    let wiki_chat = store
        .create_wiki_chat_conversation("person/李四", "处理本页")
        .unwrap();
    // 知识页聊天有 wiki_page_slug 关联
    let conv = store.get_conversation(&wiki_chat).unwrap().unwrap();
    assert_eq!(conv.wiki_page_slug.as_deref(), Some("person/李四"));

    let listed = store.list_conversations().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, normal);
    assert!(listed.iter().all(|c| c.wiki_page_slug.is_none()));

    // 归档列表同样不出现知识页聊天
    store.set_conversation_archived(&normal, true).unwrap();
    let archived = store.list_archived_conversations().unwrap();
    assert!(archived.iter().all(|c| c.wiki_page_slug.is_none()));
    assert_eq!(archived.len(), 1);
    let _ = std::fs::remove_file(path);
}
#[test]
fn message_ordering_is_deterministic_on_same_timestamp() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let conversation_id = store.create_conversation(None, None).unwrap();
    // 两条消息刻意用完全相同的 created_at（旧实现没有 id tiebreak，
    // 同毫秒时 SQLite 返回顺序不确定，跨调用会翻转）。
    let same_ts = "2026-09-25T10:00:00.000000000Z";
    for (_index, (id, content)) in [("msg-tie-2", "b"), ("msg-tie-1", "a")]
        .into_iter()
        .enumerate()
    {
        store
            .connection
            .execute(
                "INSERT INTO messages (id, conversation_id, parent_message_id, role, content, created_at)
                 VALUES (?1, ?2, NULL, 'user', ?3, ?4)",
                params![id, conversation_id, content, same_ts],
            )
            .unwrap();
    }
    // list_messages / get_child_messages 都应按 id ASC 破平，顺序稳定。
    let messages = store.list_messages(&conversation_id).unwrap();
    let ids: Vec<&str> = messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["msg-tie-1", "msg-tie-2"],
        "同时间戳消息按 id ASC 稳定排序"
    );
    let child = store.get_child_messages("msg-tie-1").unwrap();
    assert!(child.is_empty());
    let _ = std::fs::remove_file(path);
}
#[test]
fn guidance_feedback_records_clears_and_fails_closed() {
    let store = Store::open(&temporary_database()).unwrap();
    let event_id = store.insert_event(NewEvent::now("复购节点的教训")).unwrap();
    store
        .connection
        .execute(
            "INSERT INTO event_analyses(id,event_id,prompt_version,result_json,created_at) \
             VALUES('an-1',?1,'v1',?2,'2026-01-01T00:00:00Z')",
            rusqlite::params![
                event_id,
                r#"{"guidance":{"related":[{"slug":"topic/fb","note":"复购前先看库存"}]}}"#
            ],
        )
        .unwrap();

    // 读面初始：analysis_id 暴露、feedback None。
    let items = store.page_guidance_items("topic/fb").unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].analysis_id, "an-1");
    assert_eq!(items[0].feedback, None);

    // 未知值 fail-closed；analysis 不存在 → false。
    assert!(
        store
            .set_guidance_feedback("an-1", "topic/fb", Some("复购前先看库存"), Some("自动推断"))
            .is_err()
    );
    assert!(
        !store
            .set_guidance_feedback("an-none", "topic/fb", Some("x"), Some("helpful"))
            .unwrap()
    );

    // 记录 → 覆盖 → 读面注入；清除 → None 且行删净。
    assert!(
        store
            .set_guidance_feedback("an-1", "topic/fb", Some("复购前先看库存"), Some("helpful"))
            .unwrap()
    );
    assert_eq!(
        store.page_guidance_items("topic/fb").unwrap()[0]
            .feedback
            .as_deref(),
        Some("helpful")
    );
    assert!(
        store
            .set_guidance_feedback(
                "an-1",
                "topic/fb",
                Some("复购前先看库存"),
                Some("not_relevant")
            )
            .unwrap()
    );
    assert_eq!(
        store.page_guidance_items("topic/fb").unwrap()[0]
            .feedback
            .as_deref(),
        Some("not_relevant")
    );
    assert!(
        store
            .set_guidance_feedback("an-1", "topic/fb", Some("复购前先看库存"), None)
            .unwrap()
    );
    assert_eq!(
        store.page_guidance_items("topic/fb").unwrap()[0].feedback,
        None
    );
    assert_eq!(
        store
            .connection
            .query_row("SELECT count(*) FROM guidance_feedback", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn memo_same_title_recreate_reuses_slug_and_overwrites_content() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    // 记事本单页语义的基础：同 slug 同标题重复建档**复用同一页**（unique_slug
    // 语义），UI 侧才能用固定标题做「点击即 ensure」。
    let first = store
        .create_wiki_page("memo", "记事本", "第一条快记", &[])
        .unwrap();
    let second = store.create_wiki_page("memo", "记事本", "", &[]).unwrap();
    assert_eq!(second.slug, first.slug);
    // ⚠️ 语义锁定：upsert(Always) 对已存在页**会用新 draft 覆盖正文**——
    // 这正是 UI ensure 必须「先查（getWikiPage）后建」的原因：页面已存在时
    // 不得再调 create，否则空正文会清掉快记历史。
    let reread = store.get_wiki_page(&first.slug).unwrap().unwrap();
    assert_eq!(reread.content_md, "");
    assert_eq!(reread.title, "记事本");
}
#[test]
fn memo_append_is_single_transaction_and_syncs() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let page = store
        .create_wiki_page("memo", "想法收集", "第一条", &[])
        .unwrap();
    assert_eq!(page.kind, "memo");
    assert!(page.slug.starts_with("memo/"));

    let appended = store
        .append_wiki_page_content(&page.slug, "第二条", "快记")
        .unwrap();
    assert!(appended.content_md.contains("第一条"));
    assert!(appended.content_md.contains("第二条"));
    // 追加置位人工持有（digest 不整篇覆盖）
    assert!(appended.human_edited_at.is_some());

    // memo 不进 digest 的可写 kind
    assert!(crate::wiki::validate_digest_kind("insight"));
    assert!(!crate::wiki::validate_digest_kind("memo"));
}
#[test]
fn memo_append_rejects_source_and_oversize() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    // 素材页只读
    let draft = crate::storage::WikiPageDraft {
        slug: "note-x".into(),
        kind: "note".into(),
        title: "x".into(),
        summary: String::new(),
        content_md: "body".into(),
        tags: vec![],
        source_event_ids: vec![],
        status: "confirmed".into(),
        reason: "t".into(),
        source_url: None,
    };
    store
        .upsert_wiki_page(&draft, crate::storage::ContentPolicy::Always)
        .unwrap();
    assert!(store.append_wiki_page_content("note-x", "y", "").is_err());
    // 空追加拒绝
    let memo = store.create_wiki_page("memo", "m", "a", &[]).unwrap();
    assert!(
        store
            .append_wiki_page_content(&memo.slug, "   ", "")
            .is_err()
    );
}
#[test]
fn reminder_pop_confirm_snooze_roundtrip() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let past = (chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
    let todo = store
        .create_todo("喝水", "normal", Some(&past), None, None, None, true)
        .unwrap();

    // 到期弹出（本机未触发）
    let due = store
        .due_reminders(&chrono::Utc::now().to_rfc3339(), "dev-a")
        .unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].todo_id, todo.id);
    // 普通待办（reminder=false）不弹
    let plain = store
        .create_todo("普通", "normal", Some(&past), None, None, None, false)
        .unwrap();
    let due = store
        .due_reminders(&chrono::Utc::now().to_rfc3339(), "dev-a")
        .unwrap();
    assert!(due.iter().all(|d| d.todo_id != plain.id));

    // 记录 fire 后不再重弹
    store.record_reminder_fire(&todo.id, "dev-a").unwrap();
    let due = store
        .due_reminders(&chrono::Utc::now().to_rfc3339(), "dev-a")
        .unwrap();
    assert!(due.is_empty(), "fire 后同设备不重弹");
    // 另一台设备仍能弹（本机表不同步）
    let due = store
        .due_reminders(&chrono::Utc::now().to_rfc3339(), "dev-b")
        .unwrap();
    assert_eq!(due.len(), 1);

    // snooze：due_at 推后 + fire 行删除（同事务）→ 之后可在新时刻重响
    store.snooze_reminder(&todo.id, 10, "dev-a").unwrap();
    let t = store.get_todo(&todo.id).unwrap().unwrap();
    assert!(t.due_at.unwrap() > chrono::Utc::now().to_rfc3339());
    // 未到新时刻不弹
    let due = store
        .due_reminders(&chrono::Utc::now().to_rfc3339(), "dev-a")
        .unwrap();
    assert!(due.is_empty());
    // 到了新时刻（fire 已删）能重弹——这是评审 3.1 修掉的关键 bug
    let future = (chrono::Utc::now() + chrono::Duration::minutes(11)).to_rfc3339();
    let due = store.due_reminders(&future, "dev-a").unwrap();
    assert_eq!(due.len(), 1, "snooze 后 fire 已清，新时刻必须重响");

    // 确认 = 完成 + 清 fire
    store.record_reminder_fire(&todo.id, "dev-a").unwrap();
    store.confirm_reminder(&todo.id, "dev-a").unwrap();
    let t = store.get_todo(&todo.id).unwrap().unwrap();
    assert_eq!(t.status, crate::storage::TodoStatus::Done);
}
