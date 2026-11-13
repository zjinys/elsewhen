use super::*;

#[test]
fn find_wiki_page_by_tag_exact_matches_json_array_member() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    // tag 精确落在 JSON 数组的一个成员里（P2-5 直查替代全表扫描）
    store
        .upsert_wiki_page(
            &wiki_draft("topic/target", "topic", "命中页"),
            ContentPolicy::Always,
        )
        .unwrap();
    let target = store.get_wiki_page("topic/target").unwrap().unwrap();
    store
        .update_wiki_tags(&target.slug, &["work-item-id:abc".to_string()])
        .unwrap();
    let found = store
        .find_wiki_page_by_tag("work-item-id:abc")
        .unwrap()
        .unwrap();
    assert_eq!(found.slug, "topic/target");

    // 不存在的 tag 返回 None
    assert!(
        store
            .find_wiki_page_by_tag("work-item-id:nope")
            .unwrap()
            .is_none()
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn rename_wiki_page_moves_slug_relations_and_chat() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let draft = WikiPageDraft {
        slug: "topic/网站改版".to_string(),
        kind: "topic".to_string(),
        title: "网站改版".to_string(),
        summary: "网站改版（由人物关系确认时自动建档）".to_string(),
        content_md: "# 网站改版\n\n跟进付款相关事宜。".to_string(),
        tags: vec!["付款".to_string()],
        source_event_ids: vec![],
        status: "active".to_string(),
        reason: "test".to_string(),
        source_url: None,
    };
    store
        .upsert_wiki_page(&draft, ContentPolicy::Always)
        .unwrap();
    store
        .upsert_relation(&RelationDraft {
            from_slug: "person/张三".to_string(),
            from_kind: "person".to_string(),
            to_slug: "topic/网站改版".to_string(),
            to_kind: "topic".to_string(),
            relation: "跟进".to_string(),
            note: Some("处理付款事宜".to_string()),
            confidence: 3,
            source_conversation_id: Some("conv-1".to_string()),
            source_event_id: None,
        })
        .unwrap();
    store
        .create_wiki_chat_conversation("topic/网站改版", "[知识页] 网站改版")
        .unwrap();
    let before_revisions = store.list_wiki_revisions("topic/网站改版").unwrap().len();

    // 改名：标题 + slug 一起换，关系与会话迁移
    let outcome = store
        .rename_wiki_page("topic/网站改版", "测试项目 尾款", "项目真名更正")
        .unwrap();
    assert!(outcome.changed);
    assert_eq!(outcome.new_slug, "topic/测试项目-尾款");
    assert_eq!(outcome.relations_moved, 1);
    assert_eq!(outcome.chats_moved, 1);
    assert!(store.get_wiki_page("topic/网站改版").unwrap().is_none());
    let renamed = store.get_wiki_page("topic/测试项目-尾款").unwrap().unwrap();
    assert_eq!(renamed.title, "测试项目 尾款");
    assert!(
        renamed.summary.contains("测试项目 尾款"),
        "summary 里的旧名应被替换: {}",
        renamed.summary
    );
    // 关系引用已指向新 slug
    let rels = store.list_relations().unwrap();
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0].to_slug, "topic/测试项目-尾款");
    // 页内聊天会话已迁移
    assert!(
        store
            .find_wiki_chat_conversation("topic/测试项目-尾款")
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .find_wiki_chat_conversation("topic/网站改版")
            .unwrap()
            .is_none()
    );
    // 修订历史多了一条重命名记录
    assert_eq!(
        store
            .list_wiki_revisions("topic/测试项目-尾款")
            .unwrap()
            .len(),
        before_revisions + 1
    );

    // 标题没变：changed=false，零写操作
    let outcome = store
        .rename_wiki_page("topic/测试项目-尾款", "测试项目 尾款", "x")
        .unwrap();
    assert!(!outcome.changed);

    // 页面不存在：报错并提示可能已改名
    let err = store
        .rename_wiki_page("topic/网站改版", "x", "y")
        .unwrap_err();
    assert!(err.to_string().contains("知识页不存在"), "{err}");

    // 无前缀页（如来源页）：只改标题，slug 不动
    let src_draft = WikiPageDraft {
        slug: "tweet-123".to_string(),
        kind: "source".to_string(),
        title: "旧标题".to_string(),
        summary: "s".to_string(),
        content_md: "c".to_string(),
        tags: vec![],
        source_event_ids: vec![],
        status: "active".to_string(),
        reason: "test".to_string(),
        source_url: None,
    };
    store
        .upsert_wiki_page(&src_draft, ContentPolicy::Always)
        .unwrap();
    let outcome = store
        .rename_wiki_page("tweet-123", "新标题", "更正")
        .unwrap();
    assert!(outcome.changed);
    assert_eq!(outcome.new_slug, "tweet-123", "来源页 slug 应保持不变");
    assert_eq!(
        store.get_wiki_page("tweet-123").unwrap().unwrap().title,
        "新标题"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn find_wiki_page_by_title_matches_exact_ignoring_case_and_trim() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "person/李四".to_string(),
                kind: "person".to_string(),
                title: "李四".to_string(),
                summary: "简介".to_string(),
                content_md: "内容".to_string(),
                tags: vec![],
                source_event_ids: vec![],
                status: "active".to_string(),
                reason: "test".to_string(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    assert!(store.find_wiki_page_by_title(" 李四 ").unwrap().is_some());
    assert!(
        store
            .find_wiki_page_by_title("不存在的人")
            .unwrap()
            .is_none()
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn save_wiki_page_content_sets_human_edited_at_and_rejects_material() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .upsert_wiki_page(
            &wiki_draft("person/张三", "person", "AI 原始内容"),
            ContentPolicy::Always,
        )
        .unwrap();

    // 正常路径：保存正文 → human_edited_at 置位 + revision 原因带 [human]
    let saved = store
        .save_wiki_page_content("person/张三", "人类修改后的正文", "修正职位", None)
        .unwrap();
    assert!(saved.human_edited_at.is_some());
    assert_eq!(saved.content_md, "人类修改后的正文");
    let (_, revised_content, reason) = store
        .list_wiki_revisions("person/张三")
        .unwrap()
        .first()
        .cloned()
        .unwrap();
    assert_eq!(revised_content, "人类修改后的正文");
    assert!(
        reason.starts_with("[human]"),
        "reason 应带 [human] 前缀: {reason}"
    );
    let log = store.list_wiki_log(5).unwrap();
    assert!(log.iter().any(|(_, e)| e.contains("人工编辑正文")));

    // 空正文拒绝
    let empty = store
        .save_wiki_page_content("person/张三", "   ", "清空", None)
        .unwrap_err();
    assert!(empty.to_string().contains("\"code\":\"field_required\""));
    assert!(empty.to_string().contains("\"field\":\"content\""));

    // 素材页（source/note）只读拒绝
    store
        .upsert_wiki_page(
            &wiki_draft("tweet-1", "source", "素材内容"),
            ContentPolicy::Always,
        )
        .unwrap();
    let readonly = store
        .save_wiki_page_content("tweet-1", "改素材", "不该允许", None)
        .unwrap_err();
    assert!(
        readonly
            .to_string()
            .contains("\"code\":\"wiki_material_readonly\"")
    );
    store
        .upsert_wiki_page(
            &wiki_draft("note-x", "note", "笔记内容"),
            ContentPolicy::Always,
        )
        .unwrap();
    assert!(
        store
            .save_wiki_page_content("note-x", "改笔记", "不该允许", None)
            .is_err()
    );

    // 超过 64k 字符拒绝
    let big = "长".repeat(65537);
    assert!(
        store
            .save_wiki_page_content("person/张三", &big, "超大", None)
            .is_err()
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn save_wiki_page_content_optimistic_lock_rejects_stale_write() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .upsert_wiki_page(
            &wiki_draft("person/王五", "person", "AI 原始内容"),
            ContentPolicy::Always,
        )
        .unwrap();

    // 加载时刻的快照 updated_at
    let loaded_at = store
        .get_wiki_page("person/王五")
        .unwrap()
        .unwrap()
        .updated_at;

    // 模拟编辑期间后台 digest 写回：updated_at 变化（直接改库绕过守卫）
    store
        .connection
        .execute(
            "UPDATE wiki_pages SET content_md='digest 新内容', updated_at='2099-01-01T00:00:00Z' WHERE slug='person/王五'",
            [],
        )
        .unwrap();

    // 持旧快照保存 → 冲突拒绝，且不落库
    let err = store
        .save_wiki_page_content("person/王五", "人工修改", "修正", Some(&loaded_at))
        .unwrap_err();
    let contract: serde_json::Value =
        serde_json::from_str(err.to_string().strip_prefix("elsewhen-error-v1:").unwrap()).unwrap();
    assert_eq!(contract["code"], "wiki_page_changed");
    assert_eq!(contract["params"], serde_json::json!({}));
    assert!(format!("{err:#}").contains(&loaded_at));
    assert_eq!(
        store
            .get_wiki_page("person/王五")
            .unwrap()
            .unwrap()
            .content_md,
        "digest 新内容",
        "冲突时不得覆盖后台写入"
    );

    // 持当前快照保存 → 正常通过
    let current_at = store
        .get_wiki_page("person/王五")
        .unwrap()
        .unwrap()
        .updated_at;
    store
        .save_wiki_page_content("person/王五", "人工修改", "修正", Some(&current_at))
        .unwrap();
    assert_eq!(
        store
            .get_wiki_page("person/王五")
            .unwrap()
            .unwrap()
            .content_md,
        "人工修改"
    );

    // 模拟 Dart 往返格式漂移（毫秒精度 + "Z" 后缀）：同一时刻应判定一致。
    // 注意：上面保存成功后 updated_at 已变，需重读当前值再做格式变换。
    let fresh_at = store
        .get_wiki_page("person/王五")
        .unwrap()
        .unwrap()
        .updated_at;
    let reformatted = format!(
        "{}Z",
        chrono::DateTime::parse_from_rfc3339(&fresh_at)
            .unwrap()
            .to_utc()
            .format("%Y-%m-%dT%H:%M:%S%.3f")
    );
    store
        .save_wiki_page_content(
            "person/王五",
            "格式漂移仍应通过",
            "修正",
            Some(&reformatted),
        )
        .unwrap();

    // 不传 expected（None）→ 跳过校验（兼容旧调用方）
    store
        .save_wiki_page_content("person/王五", "无锁保存", "修正", None)
        .unwrap();

    let _ = std::fs::remove_file(path);
}

#[test]
fn wiki_pages_schema_exposes_human_edited_at_and_opinion() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    // v29 引入的这两列已固化进 migrations/01-baseline/up.sql，读取路径不得因缺列而报错
    let columns = {
        let mut statement = store
            .connection
            .prepare("PRAGMA table_info(wiki_pages)")
            .unwrap();
        statement
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    for expected in ["human_edited_at", "opinion"] {
        assert!(
            columns.iter().any(|name| name == expected),
            "wiki_pages 缺 {expected} 列，现有列：{columns:?}"
        );
    }
    store
        .upsert_wiki_page(
            &wiki_draft("topic/subject", "topic", "x"),
            ContentPolicy::Always,
        )
        .unwrap();
    let page = store.get_wiki_page("topic/subject").unwrap().unwrap();
    assert_eq!(page.human_edited_at, None);
    assert_eq!(page.opinion, None);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn list_wiki_page_metas_bounded_and_derivative_free() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();

    let page = |slug: &str, kind: &str, content: &str| {
        store
            .upsert_wiki_page(&wiki_draft(slug, kind, content), ContentPolicy::Always)
            .unwrap();
    };
    page("topic/old", "topic", "旧内容 long body");
    page("topic/new", "topic", "新内容 long body");
    page("topic/raw", "source", "素材页 long body");
    // 派生产物：meta 列表必须排除（同主列表语义）
    page("topic/derived", "topic", "派生 long body");
    store
        .connection
        .execute(
            "UPDATE wiki_pages SET area='derivative' WHERE slug='topic/derived'",
            [],
        )
        .unwrap();
    // 排序锚：updated_at 是文本 RFC3339，手动拉开差距
    store
        .connection
        .execute(
            "UPDATE wiki_pages SET updated_at='1999-01-01T00:00:00Z' WHERE slug='topic/old'",
            [],
        )
        .unwrap();
    store
        .connection
        .execute(
            "UPDATE wiki_pages SET updated_at='2999-01-01T00:00:00Z' WHERE slug='topic/new'",
            [],
        )
        .unwrap();

    // 排序：updated_at 倒序
    let metas = store.list_wiki_page_metas(500).unwrap();
    let slugs: Vec<_> = metas.iter().map(|m| m.slug.as_str()).collect();
    assert_eq!(
        slugs,
        vec!["topic/new", "topic/raw", "topic/old"],
        "updated_at 倒序、area=derivative 排除"
    );

    // LIMIT 生效
    assert_eq!(store.list_wiki_page_metas(1).unwrap().len(), 1);

    // 列表面没有正文（这是它存在的意义——列表不背全量 content_md 过 FFI）
    // meta struct 本身无 content_md 字段，这里锁定列清单不随列新增而回填正文：
    let mut stmt = store
        .connection
        .prepare(&format!(
            "SELECT {WIKI_PAGE_META_COLS} FROM wiki_pages WHERE slug='topic/new'"
        ))
        .unwrap();
    let count: i64 = stmt.column_count().try_into().unwrap();
    stmt.finalize().unwrap();
    assert_eq!(
        count, 15,
        "meta 列清单 = 15 列（无 content_md；含孵化 opinion）"
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn page_tldr_roundtrip_expiry_and_candidates() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let long = |title: &str| format!("# {title}\n{}", "转发规则与配额。".repeat(200));
    let put = |slug: &str, kind: &str, body: String| {
        store
            .upsert_wiki_page(&wiki_draft(slug, kind, &body), ContentPolicy::Always)
            .unwrap();
    };
    put("method/relay", "method", long("中继手册"));
    put("method/short", "method", "# 短页".into());
    put("source/incubating", "source", long("孵化中的素材"));
    store
        .create_wiki_page("memo", "我的记事本", &long("随手记"), &[])
        .unwrap();
    let hash = |slug: &str| {
        crate::storage::knowledge::content_hash(
            &store.get_wiki_page(slug).unwrap().unwrap().content_md,
        )
    };
    let candidates = || store.page_tldr_candidates(1000, 50, 0).unwrap();

    // 只有可参与的长页进候选：短页、孵化中的素材、记事本都不送模型。
    assert!(store.page_tldr("method/relay").unwrap().is_none());
    assert_eq!(candidates(), vec!["method/relay".to_string()]);

    let v1 = hash("method/relay");
    store
        .commit_page_tldr(
            "method/relay",
            &v1,
            "中继页讲转发规则与配额。",
            "full",
            Some("test-model"),
        )
        .unwrap();
    assert_eq!(
        store.page_tldr("method/relay").unwrap().unwrap(),
        "中继页讲转发规则与配额。"
    );
    assert!(candidates().is_empty());

    // 正文更新：旧摘要读不出来，并且重新成为候选（迁移 83 触发器删行）。
    put("method/relay", "method", long("中继手册 v2"));
    assert!(store.page_tldr("method/relay").unwrap().is_none());
    assert_eq!(candidates(), vec!["method/relay".to_string()]);

    // 生成期间页面被改过：旧正文的摘要不能盖上新版本的哈希。
    let stale = store.commit_page_tldr("method/relay", &v1, "旧摘要", "full", None);
    assert!(
        stale
            .unwrap_err()
            .to_string()
            .contains("page_tldr_version_changed")
    );
    let v2 = hash("method/relay");
    // 空摘要、超长摘要与未知生成范围被拒
    assert!(
        store
            .commit_page_tldr("method/relay", &v2, "   ", "full", None)
            .is_err()
    );
    assert!(
        store
            .commit_page_tldr("method/relay", &v2, &"长".repeat(601), "full", None)
            .is_err()
    );
    assert!(
        store
            .commit_page_tldr("method/relay", &v2, "摘要", "guess", None)
            .is_err()
    );

    // 失败退避：退避期内不是候选，并让整个回填冷却；正文再变则清账重来。
    let id = store.get_wiki_page("method/relay").unwrap().unwrap().id;
    let attempts = || -> i64 {
        store
            .connection
            .query_row(
                "SELECT attempts FROM page_tldr_backoff WHERE page_id=?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert!(!store.page_tldr_recently_failed(600).unwrap());
    store.record_page_tldr_failure(&id, &v2).unwrap();
    store.record_page_tldr_failure(&id, &v2).unwrap();
    assert_eq!(attempts(), 2);
    assert!(store.page_tldr_recently_failed(600).unwrap());
    assert!(candidates().is_empty());
    // 延后核对不算失败：次数不变。
    store.defer_page_tldr(&id, &v2).unwrap();
    assert_eq!(attempts(), 2);
    put("method/relay", "method", long("中继手册 v3"));
    assert_eq!(candidates(), vec!["method/relay".to_string()]);
    assert!(!store.page_tldr_recently_failed(600).unwrap());

    // 当前不可引用的页延后：不占候选名额，也不触发失败冷却。
    let v3 = hash("method/relay");
    store.defer_page_tldr(&id, &v3).unwrap();
    assert_eq!(attempts(), 0);
    assert!(candidates().is_empty());
    assert!(!store.page_tldr_recently_failed(600).unwrap());

    // 成功写入清掉退避记录。
    store
        .commit_page_tldr("method/relay", &v3, "新摘要", "head-outline", None)
        .unwrap();
    let rows: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM page_tldr_backoff", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 0);

    let _ = std::fs::remove_file(path);
}
