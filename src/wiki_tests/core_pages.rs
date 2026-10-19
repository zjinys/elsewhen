use super::*;

#[test]
fn slug_validation() {
    assert!(validate_slug("recurring-cost/dongguan-huizhou-commute"));
    assert!(validate_slug("habit/morning-run"));
    assert!(validate_slug("recurring-cost/东莞惠州通勤")); // 中文 slug 允许
    assert!(!validate_slug("Recurring Cost/xx")); // 空格
    assert!(!validate_slug("a//b")); // 多层
    assert!(!validate_slug("-lead"));
    assert!(!validate_slug("a-b-"));
}

#[test]
fn slugify_works() {
    assert_eq!(slugify("副业不是加法是乘法"), "副业不是加法是乘法");
    assert_eq!(
        slugify("Dongguan-Huizhou Commute"),
        "dongguan-huizhou-commute"
    );
    assert_eq!(slugify("  顺风车 2.0  计划  "), "顺风车-2-0-计划");
    assert_eq!(slugify("您/好 世界"), "您-好-世界");
    assert_eq!(slugify("!!!#"), "page"); // 全符号 → fallback
}

#[test]
fn normalize_extraction_backfills_annotations_and_filters_hallucinations() {
    use crate::event::AnnotationSet;

    let parsed = serde_json::json!({
        "people": [
            {"name": "李四", "role_note": "对接网站改版"},
            {"name": "项目负责人"}
        ],
        "relations": [
            {"person": "李四", "target": "网站改版", "relation": "负责", "note": ""},
            {"person": "不存在的人", "target": "幻想的项目", "relation": "负责", "note": ""}
        ]
    });
    let ann = AnnotationSet {
        people: vec!["李四".into(), "孙七（客户）".into()],
        targets: vec!["网站改版".into()],
    };
    let out = normalize_extraction(&parsed, &ann);
    let people = out["people"].as_array().unwrap();
    let names: Vec<&str> = people.iter().map(|p| p["name"].as_str().unwrap()).collect();
    // LLM 漏掉的标注实体兜底补进 people
    assert!(names.contains(&"孙七（客户）"));
    assert_eq!(people[people.len() - 1]["role_note"].as_str().unwrap(), "");
    // 幻觉关系被过滤（person 不在实体集里）
    let relations = out["relations"].as_array().unwrap();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0]["person"], "李四");
    assert_eq!(relations[0]["target"], "网站改版");
}

#[test]
fn unique_slug_avoids_different_title_collision_but_reuses_same_title() {
    let path = std::env::temp_dir().join(format!(
        "elsewhen-wiki-unique-slug-{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Store::open(&path).unwrap();
    // 先用不同标题占住 person/zhang-wei 这个 slug
    store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "person/zhang-wei".into(),
                kind: "person".into(),
                title: "Zhang Wei".into(),
                summary: String::new(),
                content_md: String::new(),
                tags: vec![],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "test".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    // 不同标题 → 后缀避让
    assert_eq!(
        unique_slug(&store, "person/zhang-wei", "李四").unwrap(),
        "person/zhang-wei-2"
    );
    // 同名 slug 但标题相同 → 直接复用
    assert_eq!(
        unique_slug(&store, "person/zhang-wei", "Zhang Wei").unwrap(),
        "person/zhang-wei"
    );
    // 空闲 slug 直接用
    assert_eq!(
        unique_slug(&store, "topic/shuang-lian-lu", "网站改版").unwrap(),
        "topic/shuang-lian-lu"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn index_md_groups_by_kind() {
    let pages = vec![
        WikiPage {
            id: "1".into(),
            slug: "recurring-cost/dg-huizhou".into(),
            kind: "recurring_cost".into(),
            title: "往返".into(),
            summary: "每周通勤".into(),
            content_md: "x".into(),
            tags: vec![],
            source_event_ids: vec!["a".into(), "b".into(), "c".into()],
            evidence_count: 3,
            first_seen_at: String::new(),
            last_seen_at: String::new(),
            status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
            source_url: None,
            area: "insight".into(),
            based_on: None,
            content_type: None,
            human_edited_at: None,
            opinion: None,
        },
        WikiPage {
            id: "2".into(),
            slug: "capability/rust".into(),
            kind: "capability".into(),
            title: "Rust".into(),
            summary: "系统编程".into(),
            content_md: "y".into(),
            tags: vec![],
            source_event_ids: vec![],
            evidence_count: 0,
            first_seen_at: String::new(),
            last_seen_at: String::new(),
            status: "active".into(),
            created_at: String::new(),
            updated_at: String::new(),
            source_url: None,
            area: "insight".into(),
            based_on: None,
            content_type: None,
            human_edited_at: None,
            opinion: None,
        },
    ];
    let md = build_index_md(&pages);
    assert!(md.contains("### recurring_cost"));
    assert!(md.contains("3 条事件支持"));
    assert!(md.contains("### capability"));
}

#[test]
fn update_wiki_tags_normalizes_and_persists() {
    let path = std::env::temp_dir().join(format!(
        "elsewhen-wiki-tags-test-{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Store::open(&path).unwrap();
    let draft = WikiPageDraft {
        slug: "topic/tag-test".into(),
        kind: "topic".into(),
        title: "标签测试".into(),
        summary: "s".into(),
        content_md: "正文".into(),
        tags: vec!["旧".into()],
        source_event_ids: vec![],
        status: "active".into(),
        reason: "t".into(),
        source_url: None,
    };
    store
        .upsert_wiki_page(&draft, ContentPolicy::Always)
        .unwrap();

    // 去 #、去空白、去重、忽略空串，保持顺序
    let updated = store
        .update_wiki_tags(
            "topic/tag-test",
            &[" #工作 ".into(), "工作".into(), "  ".into(), "Rust".into()],
        )
        .unwrap();
    assert_eq!(updated.tags, vec!["工作".to_string(), "Rust".to_string()]);
    // 重新读取应持久化
    let reread = store.get_wiki_page("topic/tag-test").unwrap().unwrap();
    assert_eq!(reread.tags, vec!["工作".to_string(), "Rust".to_string()]);

    // 清空标签
    let cleared = store.update_wiki_tags("topic/tag-test", &[]).unwrap();
    assert!(cleared.tags.is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn save_text_page_keeps_tags_and_note_anchor() {
    let path = std::env::temp_dir().join(format!(
        "elsewhen-wiki-text-tags-test-{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Store::open(&path).unwrap();
    let tags = vec![" #工作 ".to_string(), "Rust".to_string(), "".to_string()];
    let page = save_text_page("这是一段要保存的笔记正文", Some("我的笔记"), &tags, &store).unwrap();
    assert_eq!(
        page.kind, "note",
        "用户粘贴笔记归素材档 kind=note（M1 语义拆分）"
    );
    assert_eq!(page.title, "我的笔记");
    assert!(page.tags.contains(&"工作".to_string()), "{:?}", page.tags);
    assert!(page.tags.contains(&"Rust".to_string()), "{:?}", page.tags);
    assert!(
        page.tags.contains(&"note".to_string()),
        "应保留 note 锚点标签: {:?}",
        page.tags
    );
    assert!(
        !page.tags.iter().any(|t| t.is_empty()),
        "不应有空标签: {:?}",
        page.tags
    );
    // 空文本拒绝
    assert!(save_text_page("   ", Some("x"), &[], &store).is_err());
    let _ = std::fs::remove_file(path);
}

#[test]
fn apply_people_relations_builds_pages_and_relations() {
    let path = std::env::temp_dir().join(format!(
        "elsewhen-wiki-relations-test-{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Store::open(&path).unwrap();

    // 预置一个已存在的目标页（模拟 digest 已建档的项目）
    store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "project/shuanglian".into(),
                kind: "project".into(),
                title: "网站改版".into(),
                summary: "项目简介".into(),
                content_md: "正文".into(),
                tags: vec![],
                source_event_ids: vec!["evt-1".into()],
                status: "active".into(),
                reason: "digest".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();

    let args = serde_json::json!({
        "people": [
            {"name": "张三", "role_note": "网站改版项目的产研负责人"},
            {"name": "孙七", "role_note": "客户对接人"}
        ],
        "relations": [
            {"person": "张三", "target": "网站改版", "relation": "负责", "note": "主导项目推进"},
            {"person": "孙七", "target": "网站改版", "relation": "参与", "note": ""}
        ]
    });
    let summary = apply_people_relations(&args, &store, "conv-1").unwrap();
    assert!(summary.contains("张三"), "{summary}");
    assert!(summary.contains("负责"), "{summary}");

    // 联系人页建档且分别是 person / topic 目标复用已有 project 页
    let zhangwei = store.get_wiki_page("person/张三").unwrap().unwrap();
    assert_eq!(zhangwei.kind, "person");
    assert!(zhangwei.content_md.contains("产研负责人"));
    let liting = store.get_wiki_page("person/孙七").unwrap().unwrap();
    assert_eq!(liting.kind, "person");

    // 目标页：已存在的 project 页被复用，没有自动建 topic 页
    assert!(store.get_wiki_page("topic/网站改版").unwrap().is_none());
    assert_eq!(
        store
            .find_wiki_page_by_title("网站改版")
            .unwrap()
            .unwrap()
            .kind,
        "project"
    );

    // 关系双向可见
    let rels = store.list_relations_for_page("person/张三").unwrap();
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0].relation, "负责");
    assert_eq!(rels[0].to_slug, "project/shuanglian");
    assert_eq!(
        store
            .list_relations_for_page("project/shuanglian")
            .unwrap()
            .len(),
        2
    );

    // 再次提交同样的联系人 → 不重复建档
    let again = apply_people_relations(
        &serde_json::json!({"people": [{"name": "张三", "role_note": "产研负责人"}]}),
        &store,
        "conv-1",
    )
    .unwrap();
    assert!(again.contains("张三"));
    assert_eq!(
        store.list_wiki_pages(Some("person"), None).unwrap().len(),
        2,
        "不应重复建档"
    );

    // 空草拟拒绝
    assert!(apply_people_relations(&serde_json::json!({}), &store, "conv-1").is_err());
    let _ = std::fs::remove_file(path);
}

#[test]
fn apply_people_relations_uses_explicit_disambiguation_slugs() {
    let path = std::env::temp_dir().join(format!(
        "elsewhen-wiki-disambiguation-{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Store::open(&path).unwrap();
    for slug in ["person/李四-市场部", "person/李四-设计"] {
        store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: slug.into(),
                    kind: "person".into(),
                    title: "李四".into(),
                    summary: slug.into(),
                    content_md: "正文".into(),
                    tags: vec![],
                    source_event_ids: vec![],
                    status: "active".into(),
                    reason: "test".into(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )
            .unwrap();
    }
    let args = serde_json::json!({
        "people": [{"name":"李四"}],
        "relations": [{"person":"李四","target":"新项目","relation":"负责","from_slug":"person/李四-设计"}]
    });
    let summary = apply_people_relations(&args, &store, "conv-1").unwrap();
    assert!(summary.contains("李四"));
    assert_eq!(
        store
            .list_relations_for_page("person/李四-设计")
            .unwrap()
            .len(),
        1
    );
    assert!(
        store
            .list_relations_for_page("person/李四-市场部")
            .unwrap()
            .is_empty()
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}
