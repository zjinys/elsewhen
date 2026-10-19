use super::*;

#[test]
fn entity_facts_are_idempotent_and_raise_confidence() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_id = store
        .insert_event(NewEvent::now("项目进入测试阶段"))
        .unwrap();
    let first = store
        .upsert_entity_fact(
            "project",
            "elsewhen",
            "进入测试阶段",
            chrono::Utc::now().to_rfc3339().as_str(),
            2,
            &event_id,
        )
        .unwrap();
    let second = store
        .upsert_entity_fact(
            "project",
            "elsewhen",
            "进入测试阶段",
            &first.occurred_at,
            4,
            &event_id,
        )
        .unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.confidence, 4);
    assert_eq!(
        store
            .list_entity_facts("project", "elsewhen")
            .unwrap()
            .len(),
        1
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}
#[test]
fn deleting_entity_fact_keeps_source_event() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_id = store
        .insert_event(crate::event::NewEvent::now("原始事实"))
        .unwrap();
    let fact = store
        .upsert_entity_fact(
            "person",
            "person/张三",
            "负责项目",
            "2026-01-01T00:00:00Z",
            3,
            &event_id,
        )
        .unwrap();
    assert!(store.delete_entity_fact(&fact.id).unwrap());
    assert!(
        store
            .list_entity_facts("person", "person/张三")
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.list_events().unwrap().len(), 1);
    let _ = std::fs::remove_file(path);
}
#[test]
fn entity_aliases_are_idempotent_and_scoped() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .add_entity_alias("project", "project/acme", "ACME")
        .unwrap();
    store
        .add_entity_alias("project", "project/acme", "ACME")
        .unwrap();
    assert_eq!(
        store
            .list_entity_aliases("project", "project/acme")
            .unwrap(),
        vec!["ACME"]
    );
    assert!(
        store
            .list_entity_aliases("project", "project/other")
            .unwrap()
            .is_empty()
    );
    let _ = std::fs::remove_file(path);
}
#[test]
fn relations_upsert_dedupe_and_list_both_directions() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let rel = RelationDraft {
        from_slug: "person/张三".to_string(),
        from_kind: "person".to_string(),
        to_slug: "kb-网站改版".to_string(),
        to_kind: "project".to_string(),
        relation: "负责".to_string(),
        note: Some("主导该项目".to_string()),
        confidence: 3,
        source_conversation_id: Some("conv-1".to_string()),
        source_event_id: None,
    };
    store.upsert_relation(&rel).unwrap();
    // 同一条重复写入：去重为 1 条，指向同一页双向都能查到
    store.upsert_relation(&rel).unwrap();
    let from_person = store.list_relations_for_page("person/张三").unwrap();
    let from_project = store.list_relations_for_page("kb-网站改版").unwrap();
    assert_eq!(from_person.len(), 1);
    assert_eq!(from_project.len(), 1);
    assert_eq!(from_person[0].relation, "负责");
    assert_eq!(from_person[0].to_slug, "kb-网站改版");
    assert_eq!(store.list_relations().unwrap().len(), 1);

    // 删除
    assert!(store.delete_relation(&from_person[0].id).unwrap());
    assert!(store.list_relations().unwrap().is_empty());
    let _ = std::fs::remove_file(path);
}
#[test]
fn entity_merge_and_undo_preserve_sources_and_restore_rows() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    entity_page(&store, "person/李四-a", "person", "李四（设计）");
    entity_page(&store, "person/李四", "person", "李四");
    entity_page(&store, "project/付款", "project", "付款项目");
    let event_id = store
        .insert_event(NewEvent::now("李四负责付款联调"))
        .unwrap();
    let fact = store
        .upsert_entity_fact(
            "person",
            "person/李四-a",
            "职责：付款联调",
            "2026-09-21",
            4,
            &event_id,
        )
        .unwrap();
    store
        .add_entity_alias("person", "person/李四-a", "设计李四")
        .unwrap();
    let relation = store
        .upsert_relation(&RelationDraft {
            from_slug: "person/李四-a".into(),
            from_kind: "person".into(),
            to_slug: "project/付款".into(),
            to_kind: "project".into(),
            relation: "负责".into(),
            note: None,
            confidence: 4,
            source_conversation_id: None,
            source_event_id: Some(event_id.clone()),
        })
        .unwrap();
    let todo = store
        .create_todo(
            "确认付款联调",
            "high",
            None,
            Some(&event_id),
            Some("person/李四-a"),
            None,
            false,
        )
        .unwrap();

    assert!(
        store
            .merge_entity("person", "person/李四-a", "person/李四")
            .unwrap()
    );
    assert_eq!(
        store
            .get_wiki_page("person/李四-a")
            .unwrap()
            .unwrap()
            .status,
        "merged"
    );
    assert_eq!(
        store.list_entity_facts("person", "person/李四").unwrap()[0].id,
        fact.id
    );
    assert_eq!(
        store.list_relations_for_page("person/李四").unwrap()[0].id,
        relation.id
    );
    assert_eq!(store.list_events().unwrap().len(), 1);
    assert_eq!(
        store.list_todos(None).unwrap()[0]
            .related_wiki_slug
            .as_deref(),
        Some("person/李四")
    );

    assert!(store.undo_entity_merge("person/李四-a").unwrap());
    assert_eq!(
        store
            .get_wiki_page("person/李四-a")
            .unwrap()
            .unwrap()
            .status,
        "active"
    );
    assert_eq!(
        store.list_entity_facts("person", "person/李四-a").unwrap()[0].id,
        fact.id
    );
    assert_eq!(
        store
            .list_entity_aliases("person", "person/李四-a")
            .unwrap(),
        vec!["设计李四"]
    );
    assert_eq!(
        store.list_relations_for_page("person/李四-a").unwrap()[0].id,
        relation.id
    );
    assert_eq!(store.list_events().unwrap()[0].raw_text, "李四负责付款联调");
    let restored_todo = store
        .list_todos(None)
        .unwrap()
        .into_iter()
        .find(|item| item.id == todo.id)
        .unwrap();
    assert_eq!(
        restored_todo.related_wiki_slug.as_deref(),
        Some("person/李四-a")
    );
    assert!(
        store
            .merge_entity("person", "person/李四-a", "person/李四")
            .unwrap()
    );
    assert!(store.undo_entity_merge("person/李四-a").unwrap());
    drop(store);
    let _ = std::fs::remove_file(path);
}
#[test]
fn entity_merge_deduplicates_and_undo_restores_source_copies() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    entity_page(&store, "topic/旧", "topic", "旧主题");
    entity_page(&store, "topic/新", "topic", "新主题");
    let event_id = store.insert_event(NewEvent::now("共同事实")).unwrap();
    store
        .upsert_entity_fact(
            "topic",
            "topic/旧",
            "状态：进行中",
            "2026-09-21",
            3,
            &event_id,
        )
        .unwrap();
    store
        .upsert_entity_fact(
            "topic",
            "topic/新",
            "状态：进行中",
            "2026-09-21",
            5,
            &event_id,
        )
        .unwrap();
    store
        .add_entity_alias("topic", "topic/旧", "共同别名")
        .unwrap();
    store
        .add_entity_alias("topic", "topic/新", "共同别名")
        .unwrap();
    store.merge_entity("topic", "topic/旧", "topic/新").unwrap();
    assert_eq!(
        store.list_entity_facts("topic", "topic/新").unwrap().len(),
        1
    );
    assert_eq!(
        store
            .list_entity_aliases("topic", "topic/新")
            .unwrap()
            .len(),
        1
    );
    store.undo_entity_merge("topic/旧").unwrap();
    assert_eq!(
        store.list_entity_facts("topic", "topic/旧").unwrap().len(),
        1
    );
    assert_eq!(
        store.list_entity_facts("topic", "topic/新").unwrap().len(),
        1
    );
    assert_eq!(
        store.list_entity_aliases("topic", "topic/旧").unwrap(),
        vec!["共同别名"]
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}
#[test]
fn entity_merge_rejects_invalid_targets_and_undo_is_atomic_after_changes() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    entity_page(&store, "person/a", "person", "A");
    entity_page(&store, "person/b", "person", "B");
    entity_page(&store, "project/b", "project", "B项目");
    assert!(store.merge_entity("person", "person/a", "missing").is_err());
    assert!(
        store
            .merge_entity("person", "person/a", "project/b")
            .is_err()
    );
    let event_id = store.insert_event(NewEvent::now("A事实")).unwrap();
    let fact = store
        .upsert_entity_fact(
            "person",
            "person/a",
            "团队：支付",
            "2026-09-21",
            3,
            &event_id,
        )
        .unwrap();
    store
        .merge_entity("person", "person/a", "person/b")
        .unwrap();
    store
        .connection
        .execute(
            "UPDATE entity_facts SET confidence=5 WHERE id=?1",
            [&fact.id],
        )
        .unwrap();
    assert!(store.undo_entity_merge("person/a").is_err());
    assert_eq!(
        store.get_wiki_page("person/a").unwrap().unwrap().status,
        "merged"
    );
    assert!(
        store
            .list_entity_facts("person", "person/a")
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store.list_entity_facts("person", "person/b").unwrap()[0].confidence,
        5
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}
/// `entity_merges` 对 `(entity_kind, source_slug)` **不设** UNIQUE。
///
/// 早期结构带这个唯一约束，导致同一个源实体第二次合并时插入直接失败。
/// 约束是历史迁移拆表去掉的，现在固化在 `migrations/01-baseline/up.sql` 里，所以这个测试盯住
/// 「约束没被加回来」+「审计快照仍在」两件事。
#[test]
fn entity_merges_allows_repeated_merge_of_same_source_and_keeps_audit() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let schema: String = store
        .connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='entity_merges'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        !schema
            .replace(' ', "")
            .contains("UNIQUE(entity_kind,source_slug)")
    );

    // 同一源实体合并到两个不同目标，都应成功
    store
        .connection
        .execute_batch(
            "INSERT INTO entity_merges (id,entity_kind,source_slug,target_slug,created_at)
               VALUES ('m1','person','person/a','person/b','2026-09-21');
             INSERT INTO entity_merges (id,entity_kind,source_slug,target_slug,created_at)
               VALUES ('m2','person','person/a','person/c','2026-09-22');
             INSERT INTO entity_merge_snapshots (merge_id,table_name,row_id,payload)
               VALUES ('m1','entity_aliases','a1','{}');
             INSERT INTO entity_merge_snapshots (merge_id,table_name,row_id,payload)
               VALUES ('m2','entity_aliases','a1','{}');",
        )
        .unwrap();

    for (table, expected) in [("entity_merges", 2), ("entity_merge_snapshots", 2)] {
        let n: i64 = store
            .connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, expected, "{table} 行数不对");
    }
    drop(store);
    let _ = std::fs::remove_file(path);
}
#[test]
fn relations_upsert_confidence_increments_once_per_event() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let draft = |source_event_id: Option<&str>| RelationDraft {
        from_slug: "person/张三".to_string(),
        from_kind: "person".to_string(),
        to_slug: "topic/网站改版".to_string(),
        to_kind: "topic".to_string(),
        relation: "参与".to_string(),
        note: None,
        confidence: 4,
        source_conversation_id: None,
        source_event_id: source_event_id.map(|s| s.to_string()),
    };
    // 首次（事件 E1）：confidence 落在提供值 4
    store.upsert_relation(&draft(Some("E1"))).unwrap();
    let read = |store: &Store| {
        let rel = store.upsert_relation(&draft(Some("E1"))).unwrap();
        rel
    };
    // 同一事件重放（reanalyze 场景）：幂等，confidence 不重复加权
    let rel = read(&store);
    assert_eq!(rel.confidence, 4, "同事件重放不得重复加 confidence");
    let src: Option<String> = store
        .connection
        .query_row(
            "SELECT source_event_id FROM relations WHERE from_slug='person/张三'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(src.as_deref(), Some("E1"));

    // 新事件来源（E2）：渐进增强 +1，封顶 5
    let rel = store.upsert_relation(&draft(Some("E2"))).unwrap();
    assert_eq!(rel.confidence, 5, "新事件来源应渐进 +1");
    let src: String = store
        .connection
        .query_row(
            "SELECT source_event_id FROM relations WHERE from_slug='person/张三'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(src, "E2");
    // 已封顶后再来新事件：保持 5
    let rel = store.upsert_relation(&draft(Some("E3"))).unwrap();
    assert_eq!(rel.confidence, 5, "置信封顶 5");
    let _ = std::fs::remove_file(path);
}
#[test]
fn resolve_entity_slug_normalizes_title_and_alias() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let page = |slug: &str, kind: &str, title: &str| {
        store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: slug.to_string(),
                    kind: kind.to_string(),
                    title: title.to_string(),
                    summary: String::new(),
                    content_md: format!("# {title}"),
                    tags: vec![],
                    source_event_ids: vec![],
                    status: "active".to_string(),
                    reason: "test".to_string(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )
            .unwrap();
    };
    page("person/王大山", "person", "王大山");
    // 标题命中（大小写/空白不敏感）
    assert_eq!(
        store.resolve_entity_slug("person", "王大山").unwrap(),
        Some("person/王大山".to_string())
    );
    // 别名命中：canonical 即页 slug
    store
        .add_entity_alias("person", "person/王大山", "小王")
        .unwrap();
    assert_eq!(
        store.resolve_entity_slug("person", "小王").unwrap(),
        Some("person/王大山".to_string())
    );
    // kind 不符 → 不得冒名：只有 topic 同名页时，person 解析为 None
    page("topic/王大山", "topic", "王大山");
    store.delete_wiki_page("person/王大山").unwrap();
    assert_eq!(store.resolve_entity_slug("person", "王大山").unwrap(), None);
    assert_eq!(
        store.resolve_entity_slug("topic", "王大山").unwrap(),
        Some("topic/王大山".to_string())
    );
    // 未命中 → None（调用方降级草拟，绝不猜）
    assert_eq!(store.resolve_entity_slug("person", "不存在").unwrap(), None);
    let _ = std::fs::remove_file(path);
}
