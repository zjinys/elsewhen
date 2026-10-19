use super::*;

#[test]
fn imported_note_page_gets_note_prefix_kind_and_imported_area() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let page = crate::wiki::save_text_page(
        "第一行标题\n\n正文内容",
        None,
        &["随手".to_string()],
        &store,
    )
    .unwrap();

    assert!(
        page.slug.starts_with("note-"),
        "笔记页 slug 应带 note- 前缀，实际 {}",
        page.slug
    );
    assert_eq!(page.kind, "note", "笔记页 kind 应为 note");
    assert_eq!(page.area, "imported", "note- 前缀应归入 imported 分区");
    // 人工编辑时间与观点列存在且初始为空（v29 引入的两列）
    assert_eq!(page.human_edited_at, None);
    assert_eq!(page.opinion, None);
    assert!(page.tags.contains(&"note".to_string()), "note 锚点标签");

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn digest_preserve_respects_human_edited_and_material_pages() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_one = store.insert_event(NewEvent::now("与李四讨论方案")).unwrap();
    let event_two = store
        .insert_event(NewEvent::now("李四确认实施步骤"))
        .unwrap();

    // ① 人工编辑过的档案页：PreserveHumanEdits → 正文不动、证据照累、protected=true
    store
        .upsert_wiki_page(
            &wiki_draft("person/李四", "person", "AI 初稿"),
            ContentPolicy::Always,
        )
        .unwrap();
    store
        .save_wiki_page_content("person/李四", "人工定稿", "人工修正", None)
        .unwrap();
    let outcome = store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "person/李四".to_string(),
                kind: "person".to_string(),
                title: "李四（AI 想改名）".to_string(),
                summary: "AI 摘要".to_string(),
                content_md: "AI 想覆盖的新内容".to_string(),
                tags: vec!["ai".to_string()],
                source_event_ids: vec![event_one.clone(), event_two.clone()],
                status: "active".to_string(),
                reason: "digest".to_string(),
                source_url: None,
            },
            ContentPolicy::PreserveHumanEdits,
        )
        .unwrap();
    assert!(outcome.protected, "人工编辑页应被保护");
    let page = outcome.page;
    assert_eq!(page.content_md, "人工定稿", "正文不可被 digest 覆盖");
    assert_eq!(page.title, "person/李四", "标题不可被 digest 覆盖");
    assert_eq!(page.tags, Vec::<String>::new(), "tags 不可被 digest 覆盖");
    assert_eq!(
        page.source_event_ids,
        vec![event_one, event_two],
        "但证据应并集"
    );
    assert_eq!(page.evidence_count, 2);

    // ② 未人工编辑的档案页：PreserveHumanEdits → 正常整篇覆盖
    store
        .upsert_wiki_page(
            &wiki_draft("topic/新主题", "topic", "AI 第一版"),
            ContentPolicy::Always,
        )
        .unwrap();
    let outcome2 = store
        .upsert_wiki_page(
            &wiki_draft("topic/新主题", "topic", "AI 第二版"),
            ContentPolicy::PreserveHumanEdits,
        )
        .unwrap();
    assert!(!outcome2.protected);
    assert_eq!(outcome2.page.content_md, "AI 第二版");

    // ③ 素材页：PreserveHumanEdits → 永不覆盖（采集快照只读）
    store
        .upsert_wiki_page(
            &wiki_draft("tweet-9", "source", "原始素材"),
            ContentPolicy::Always,
        )
        .unwrap();
    let outcome3 = store
        .upsert_wiki_page(
            &wiki_draft("tweet-9", "source", "新素材内容"),
            ContentPolicy::PreserveHumanEdits,
        )
        .unwrap();
    assert!(outcome3.protected);
    assert_eq!(outcome3.page.content_md, "原始素材", "素材正文对 AI 只读");

    // ④ Always 策略 = 素材导入流程：允许刷新素材内容（仅所有者可写）
    let outcome4 = store
        .upsert_wiki_page(
            &wiki_draft("tweet-9", "source", "导入流程刷新"),
            ContentPolicy::Always,
        )
        .unwrap();
    assert!(!outcome4.protected);
    assert_eq!(outcome4.page.content_md, "导入流程刷新");

    // ⑤ Always 策略 = 确认制修订（save_wiki_revision：AI 草拟 → 用户确认后才落库）：
    //    人工编辑页也可被覆盖——这是文档承诺的修订通道，区别于 digest 的静默覆盖
    let outcome5 = store
        .upsert_wiki_page(
            &wiki_draft("person/李四", "person", "用户确认后的修订版本"),
            ContentPolicy::Always,
        )
        .unwrap();
    assert!(!outcome5.protected);
    assert_eq!(outcome5.page.content_md, "用户确认后的修订版本");
    assert!(
        outcome5.page.human_edited_at.is_some(),
        "确认制修订不触碰 human_edited_at 列：曾被人动过的事实开关永久保留"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn set_wiki_opinion_only_on_material_and_audits() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .upsert_wiki_page(
            &wiki_draft("tweet-8", "source", "素材"),
            ContentPolicy::Always,
        )
        .unwrap();

    // 认可
    let p = store.set_wiki_opinion("tweet-8", Some("endorse")).unwrap();
    assert_eq!(p.opinion.as_deref(), Some("endorse"));

    // 不认可
    let p = store.set_wiki_opinion("tweet-8", Some("reject")).unwrap();
    assert_eq!(p.opinion.as_deref(), Some("reject"));

    // 清空（未表态）
    let p = store.set_wiki_opinion("tweet-8", None).unwrap();
    assert_eq!(p.opinion, None);

    // 非法值拒绝
    assert!(store.set_wiki_opinion("tweet-8", Some("meh")).is_err());

    // 非素材页拒绝（即使人工编辑过也一样）
    store
        .upsert_wiki_page(
            &wiki_draft("person/王五", "person", "x"),
            ContentPolicy::Always,
        )
        .unwrap();
    assert!(
        store
            .set_wiki_opinion("person/王五", Some("endorse"))
            .is_err()
    );

    // 审计日志
    let log = store.list_wiki_log(10).unwrap();
    assert!(log.iter().any(|(_, e)| e.contains("素材评价")));

    let _ = std::fs::remove_file(path);
}

#[test]
fn update_project_path_validates_and_backfills_legacy_pages() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    // 存量页：只有正文快照、没有 source_url
    let dir_a = std::env::temp_dir().join("elsewhen-proj-a");
    let dir_b = std::env::temp_dir().join("elsewhen-proj-b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();
    let content = format!(
        "# 资产档案：A\n\n## 资产边界\n- 项目目录：`{}`\n",
        dir_a.display()
    );
    store
        .upsert_wiki_page(
            &wiki_draft("project/a", "project", &content),
            ContentPolicy::Always,
        )
        .unwrap();
    // 新 open 触发幂等回填
    drop(store);
    let store = Store::open(&path).unwrap();
    let page = store.get_wiki_page("project/a").unwrap().unwrap();
    let url = page.source_url.clone().unwrap();
    assert!(url.starts_with("file://"));
    // 回填按正文原样记录（不做 canonicalize，目录可能已搬走）
    assert_eq!(crate::wiki::file_url_to_path(&url).unwrap(), dir_a);
    // 二次 open 不再重复写 updated_at（幂等无副作用）
    let updated_before = page.updated_at.clone();
    drop(store);
    let store = Store::open(&path).unwrap();
    let page2 = store.get_wiki_page("project/a").unwrap().unwrap();
    assert_eq!(page2.updated_at, updated_before);
    // 改路径：新目录必须存在
    assert!(
        store
            .update_project_path("project/a", "/definitely/not/here")
            .is_err()
    );
    let moved = store
        .update_project_path("project/a", &dir_b.display().to_string())
        .unwrap();
    let new_url = moved.source_url.clone().unwrap();
    assert!(new_url.starts_with("file://"));
    assert_eq!(
        crate::wiki::file_url_to_path(&new_url).unwrap(),
        dir_b.canonicalize().unwrap()
    );
    // 正文快照行同步更新
    assert!(moved.content_md.contains(&dir_b.display().to_string()));
    assert!(!moved.content_md.contains(&dir_a.display().to_string()));
    // 非项目页拒绝
    store
        .upsert_wiki_page(
            &wiki_draft("person/王五", "person", "x"),
            ContentPolicy::Always,
        )
        .unwrap();
    assert!(
        store
            .update_project_path("person/王五", &dir_b.display().to_string())
            .is_err()
    );
    std::fs::remove_dir_all(&dir_a).ok();
    std::fs::remove_dir_all(&dir_b).ok();
    let _ = std::fs::remove_file(path);
}

#[test]
fn digest_protection_does_not_record_revisions_or_opinion_changes() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event = store.insert_event(NewEvent::now("与赵六确认方案")).unwrap();
    store
        .upsert_wiki_page(
            &wiki_draft("person/赵六", "person", "v1 AI"),
            ContentPolicy::Always,
        )
        .unwrap();
    store
        .save_wiki_page_content("person/赵六", "v1 人工", "修正", None)
        .unwrap();
    let revs_before = store.list_wiki_revisions("person/赵六").unwrap().len();

    // 受保护写回：不追加 revision（内容没变，纯证据累加）
    let outcome = store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "person/赵六".to_string(),
                kind: "person".to_string(),
                title: "赵六".to_string(),
                summary: "s".to_string(),
                content_md: "AI 想覆盖".to_string(),
                tags: vec![],
                source_event_ids: vec![event],
                status: "active".to_string(),
                reason: "digest".to_string(),
                source_url: None,
            },
            ContentPolicy::PreserveHumanEdits,
        )
        .unwrap();
    assert!(outcome.protected);
    let revs_after = store.list_wiki_revisions("person/赵六").unwrap().len();
    assert_eq!(revs_before, revs_after, "保护降级写回不应追加 revision");

    // opinion / human_edited_at 不受 upsert 影响
    store
        .upsert_wiki_page(
            &wiki_draft("tweet-7", "source", "素材"),
            ContentPolicy::Always,
        )
        .unwrap();
    store.set_wiki_opinion("tweet-7", Some("endorse")).unwrap();
    let before = store.get_wiki_page("tweet-7").unwrap().unwrap();
    assert!(before.opinion.is_some());
    store
        .upsert_wiki_page(
            &wiki_draft("tweet-7", "source", "改不了"),
            ContentPolicy::PreserveHumanEdits,
        )
        .unwrap();
    let after = store.get_wiki_page("tweet-7").unwrap().unwrap();
    assert_eq!(
        after.opinion,
        Some("endorse".to_string()),
        "评价不被 digest 清掉"
    );

    let _ = std::fs::remove_file(path);
}
