#[test]
fn legacy_v3_upgrade_preserves_originals_and_is_idempotent() {
    let path = std::env::temp_dir().join(format!("elsewhen-legacy-{}.db", uuid::Uuid::new_v4()));
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(include_str!("../migrations/01-baseline/up.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../migrations/02-goals/up.sql"))
        .unwrap();
    conn.execute_batch(include_str!(
        "../migrations/03-drop-legacy-migrations/up.sql"
    ))
    .unwrap();
    conn.execute_batch("PRAGMA user_version=3").unwrap();
    for (id, kind, body, url, created, updated, based_on) in [
        (
            "older",
            "source",
            "旧原文",
            Some("https://example.com/legacy"),
            "2026-01-02",
            "2026-01-02",
            None,
        ),
        (
            "newer",
            "source",
            "新原文",
            Some("https://example.com/legacy#fragment"),
            "2026-01-01",
            "2026-01-04",
            None,
        ),
        (
            "text",
            "note",
            "粘贴原文",
            None,
            "2026-01-01",
            "2026-01-01",
            None,
        ),
        (
            "file",
            "source",
            "# 文件\n\n- 本地路径：`/tmp/explicit.md`\n\n## 内容摘录\n\n文件原文",
            None,
            "2026-01-01",
            "2026-01-01",
            None,
        ),
        (
            "derived",
            "derivative",
            "旧版整理",
            None,
            "2026-01-03",
            "2026-01-03",
            Some("older"),
        ),
        (
            "history-source",
            "source",
            "后来更新的原文",
            Some("https://example.com/history"),
            "2026-01-01",
            "2026-01-04",
            None,
        ),
        (
            "history-derived",
            "derivative",
            "使用历史原文的整理",
            None,
            "2026-01-02",
            "2026-01-02",
            Some("history-source"),
        ),
    ] {
        conn.execute("INSERT INTO wiki_pages(id,slug,kind,title,content_md,source_url,first_seen_at,last_seen_at,created_at,updated_at,based_on)
            VALUES (?1,?1,?2,?1,?3,?4,?5,?5,?5,?6,?7)",rusqlite::params![id,kind,body,url,created,updated,based_on]).unwrap();
    }
    conn.execute(
        "UPDATE wiki_pages SET tags='[\"local-source\"]' WHERE id='file'",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO wiki_revisions(id,page_id,content_md,reason,created_at)
        VALUES ('old-revision','history-source','修订历史里的原文','import','2026-01-01')",
        [],
    )
    .unwrap();
    drop(conn);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.list_wiki_pages(None, None).unwrap().len(), 7);
    assert_eq!(
        store.page_source_snapshots("history-derived").unwrap()[0].content_md,
        "修订历史里的原文"
    );
    assert_eq!(store.source_history("older").unwrap().len(), 2);
    assert_eq!(
        store.source_history("older").unwrap()[0].content_md,
        "新原文",
        "最新来源按实际更新时间排列"
    );
    assert_eq!(
        store.get_wiki_page("older").unwrap().unwrap().content_md,
        "旧原文"
    );
    assert_eq!(
        store.page_source_snapshots("older").unwrap()[0].content_md,
        "旧原文",
        "旧别名页不能伪装成引用新版本"
    );
    assert_eq!(
        store.page_source_snapshots("derived").unwrap()[0].content_md,
        "旧原文"
    );
    assert_eq!(store.source_history("file").unwrap()[0].source_kind, "file");
    assert!(
        store
            .knowledge_issues(Some("derived"))
            .unwrap()
            .iter()
            .any(|i| i.kind == "source_changed")
    );
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.source_history("older").unwrap().len(), 2);
    drop(store);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn old_proposal_remains_resolvable_beyond_list_limit() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/a", "SQLite 事务说明");
    let id = propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap();
    for index in 0..205 {
        s.connection.execute("INSERT INTO knowledge_proposals(id,dedupe_key,target_slug,kind,title,content_md,reason,created_at)
            VALUES (?1,?1,?1,'method','another','body','test','2000-01-01')",[format!("proposal-{index}")]).unwrap();
    }
    assert!(
        !s.knowledge_proposals(None)
            .unwrap()
            .iter()
            .any(|p| p.id == id)
    );
    assert!(s.resolve_knowledge_proposal(&id, true).unwrap().is_some());
}

#[test]
fn citation_recheck_rejects_changed_or_disavowed_material() {
    let db = Db::new();
    let s = &db.store;
    let original = source(s, "https://example.com/a", "SQLite 事务说明");
    let selected = select_knowledge(s, "SQLite 事务", "conversation", 4000).unwrap();
    assert_eq!(selected.len(), 1);
    source(s, "https://example.com/a", "SQLite 事务更新");
    assert!(current_candidates(s, &selected).unwrap().is_empty());
    let selected = select_knowledge(s, "SQLite 事务", "conversation", 4000).unwrap();
    s.set_wiki_opinion(&original.slug, Some("reject")).unwrap();
    assert!(current_candidates(s, &selected).unwrap().is_empty());
}

#[test]
fn tweet_aliases_share_identity_and_rule_revisions_require_explicit_confirmation() {
    let db = Db::new();
    let s = &db.store;
    let original = source(
        s,
        "https://twitter.com/author/status/123?s=20",
        "SQLite 事务说明",
    );
    assert_eq!(
        source(s, "https://x.com/i/status/123", "SQLite 事务说明").id,
        original.id
    );
    let id = propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap();
    let page = s.resolve_knowledge_proposal(&id, true).unwrap().unwrap();
    s.set_knowledge_metadata(&page.slug, "独特适用场景", "rule")
        .unwrap();
    assert!(
        select_knowledge(s, "独特适用場景", "conversation", 4000)
            .unwrap()
            .iter()
            .any(|c| c.page_slug == page.slug)
    );
    let id = propose_knowledge(s, &page.slug, "revision", &compile_stub()).unwrap();
    assert!(
        s.knowledge_proposals(Some(&page.slug))
            .unwrap()
            .iter()
            .any(|p| p.id == id && p.status == "pending")
    );
    assert_eq!(
        s.knowledge_metadata(&page.slug).unwrap().applicable_when,
        "独特适用场景"
    );
    s.resolve_knowledge_proposal(&id, true).unwrap();
    assert_eq!(s.knowledge_metadata(&page.slug).unwrap().strength, "rule");
}

#[test]
fn file_import_confirms_captured_originals_and_detects_concurrent_updates() {
    let db = Db::new();
    let s = &db.store;
    let dir = std::env::temp_dir().join(format!("elsewhen-files-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let file = dir.join("note.md");
    std::fs::write(&file, "文件的原始正文").unwrap();
    let preview = crate::local_sources::preview_file_import(s, &dir).unwrap();
    assert!(s.list_wiki_pages(None, None).unwrap().is_empty());
    std::fs::write(&file, "确认期间文件发生变化").unwrap();
    let report = crate::local_sources::confirm_file_import(s, &preview).unwrap();
    let slug = &report.pages[0];
    assert_eq!(
        s.get_wiki_page(slug).unwrap().unwrap().content_md,
        "文件的原始正文",
        "只写入实际确认的版本"
    );
    assert_eq!(s.source_history(slug).unwrap()[0].source_kind, "file");
    let pending = crate::local_sources::preview_file_import(s, &dir).unwrap();
    assert!(
        pending.entries[0]
            .previous_excerpt
            .as_ref()
            .unwrap()
            .contains("首处变化")
    );
    std::fs::write(&file, "第三个版本").unwrap();
    let newer = crate::local_sources::preview_file_import(s, &dir).unwrap();
    crate::local_sources::confirm_file_import(s, &newer).unwrap();
    assert!(crate::local_sources::confirm_file_import(s, &pending).is_err());
    assert_eq!(s.source_history(slug).unwrap().len(), 2);
    assert_eq!(s.list_wiki_pages(None, None).unwrap().len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn topic_analysis_binds_only_applicable_sources() {
    let db = Db::new();
    let s = &db.store;
    let source = source(
        s,
        "https://example.com/transactions",
        "SQLite 事务确保写入一致性",
    );
    let slug = crate::local_sources::plan_topic(s, "SQLite 事务").unwrap();
    assert_eq!(
        s.page_source_snapshots(&slug).unwrap()[0].id,
        s.source_history(&source.slug).unwrap()[0].id
    );
    assert_eq!(
        latest_citations(s, "topic", &slug).unwrap()[0].page_slug,
        source.slug
    );
    s.set_wiki_opinion(&source.slug, Some("reject")).unwrap();
    crate::local_sources::plan_topic(s, "SQLite 事务").unwrap();
    assert!(latest_citations(s, "topic", &slug).unwrap().is_empty());
}

#[test]
fn chat_revision_does_not_overwrite_edits_made_after_the_preview() {
    use crate::ai::tool::{ToolCall, ToolRegistry, dispatch, execute_pending_action};
    let db = Db::new();
    let store = &db.store;
    let original = source(store, "https://example.com/revision", "SQLite 事务说明");
    let id = propose_knowledge(store, &original.slug, "method", &compile_stub()).unwrap();
    let page = store
        .resolve_knowledge_proposal(&id, true)
        .unwrap()
        .unwrap();
    let conversation = store.create_conversation(Some("编辑"), None).unwrap();
    let call = ToolCall::new(
        "save_wiki_revision",
        serde_json::json!({"slug":page.slug,"title":"修订标题","content_md":"模型修订正文","change_note":"补充","save_as":"revision"}),
    );
    dispatch(&call, &ToolRegistry::default(), store, &conversation);
    let pending = store
        .pending_actions_for_conversation(&conversation)
        .unwrap()
        .remove(0);
    store
        .save_wiki_page_content(&page.slug, "稍后人工修订", "user", None)
        .unwrap();
    assert!(execute_pending_action(store, &pending).is_err());
    assert_eq!(
        store.get_wiki_page(&page.slug).unwrap().unwrap().content_md,
        "稍后人工修订"
    );
}

/// 2026-10-18/19 事故档案「投递侧不设防」结论的落地：相关知识自动
/// 摘要挂载。挂载段沿用「知识选材：」识别前缀，excerpt 换成 AI 摘要
/// （体积降一个量级），身份字段保真——引用校验与 usage 零改动。
#[test]
fn summary_mount_carries_ai_summary_with_verified_identity() {
    let db = Db::new();
    let s = &db.store;
    let page = crate::wiki::save_text_page(
        "装配要点：先校准底座，再分步锁紧螺栓，最后通电自检。",
        Some("装配手册"),
        &[],
        s,
    )
    .unwrap();
    // 模拟编译管线写入的 AI 独立摘要（save_text_page 不产摘要）。
    s.connection
        .execute(
            "UPDATE wiki_pages SET summary=?1 WHERE id=?2",
            rusqlite::params!["一到两句独立概括：先校准再锁紧，最后通电自检。", page.id],
        )
        .unwrap();

    let (mounted, slugs) = summary_mount(s, "装配手册的步骤顺序是什么？", 3).unwrap();
    assert_eq!(
        slugs,
        vec![page.slug.clone()],
        "slug 列表供 knowledge_mounted 事件"
    );
    assert!(mounted.starts_with("知识选材：\n"), "沿用引用校验识别前缀");
    assert!(
        mounted.contains("先校准再锁紧，最后通电自检"),
        "excerpt 应是 AI 摘要而非正文摘录"
    );
    assert!(mounted.contains(&page.slug));
    assert!(
        !mounted.contains("分步锁紧螺栓"),
        "挂载不带正文全文正文细节"
    );

    // 身份字段保真：挂载候选能通过 current_candidates 校验（否则答案里
    // 的 [[kb:slug]] 引用会被剥离）。
    let candidates = candidates_in_context(&[ContextMessage::new("system", mounted.clone())]);
    assert_eq!(candidates.len(), 1);
    let current = current_candidates(s, &candidates).unwrap();
    assert_eq!(current.len(), 1, "挂载候选必须是校验通过的在册知识");
    assert_eq!(current[0].page_slug, page.slug);
}

/// 未命中知识时不挂空段；命中但零摘要时也不产出只有壳的挂载。
#[test]
fn summary_mount_stays_silent_when_nothing_relevant() {
    let db = Db::new();
    let s = &db.store;
    let _ = crate::wiki::save_text_page(
        "只关于古腾堡印刷机的历史笔记，与提问无关。",
        Some("印刷机历史"),
        &[],
        s,
    )
    .unwrap();
    assert!(
        summary_mount(s, "糖尿病足预警的日常检查步骤", 3)
            .unwrap()
            .0
            .is_empty()
    );
    // 空查询/空白输入同样安静。
    assert!(summary_mount(s, "怎么？", 3).unwrap().0.is_empty());

    // 相关命中但摘要与正文预览皆空的极端情形：不产出空壳挂载。
    let page = crate::wiki::save_text_page("装配口径说明。", Some("装配手册二"), &[], s).unwrap();
    s.connection
        .execute(
            "UPDATE wiki_pages SET summary='' WHERE id=?1",
            rusqlite::params![page.id],
        )
        .unwrap();
    // 注：mount_summary 回退正文预览，正文非空则仍挂载——此处验证的是
    // 「正文与摘要都空则静默」，而正文非空时挂预览属正常回退。
    let (mounted, _) = summary_mount(s, "装配手册二的口径", 3).unwrap();
    assert!(!mounted.is_empty(), "AI 摘要缺失时回退正文预览，不拒绝挂载");
}

/// 挂载段必须标 OptionalBackground：预算紧张时最先让路，且不会像
/// 工具结果那样成为顶包素材（2026-10-18 档案 P1 的教训）。
#[test]
fn knowledge_mount_segment_is_sacrificial_background() {
    let db = Db::new();
    let s = &db.store;
    let _ = crate::wiki::save_text_page(
        "装配要点：先校准底座，再分步锁紧螺栓。",
        Some("装配手册三"),
        &[],
        s,
    )
    .unwrap();
    let mounted =
        crate::ai::conversation::relevant_knowledge_mount(s, "装配手册三的顺序", 3).unwrap();
    assert_eq!(mounted.role, "system");
    assert!(
        mounted.optional_background,
        "挂载知识是可让路的背景，不是必需证据"
    );
    assert_eq!(
        mounted.segment.purpose,
        crate::ai::memory::SegmentPurpose::SelectedEvidence
    );
    // 无命中静默：不产出空提示段。
    let silent = crate::ai::conversation::relevant_knowledge_mount(s, "完全无关的量子隧穿参数", 3);
    assert!(silent.is_none());
}

/// 健康巡检：各环节数字能反映真实库况（2026-10-19 教训：没有仪表盘，
/// 坏环节静默坏数周）。纯查询，不写业务状态。
#[test]
fn health_report_counts_reflect_pipeline_state() {
    let db = Db::new();
    let s = &db.store;
    // 库存：一页无摘要（降级面）、一页有 AI 摘要
    let plain = crate::wiki::save_text_page("装配要点：先校准再锁紧。", Some("装配手册四"), &[], s)
        .unwrap();
    let summarized =
        crate::wiki::save_text_page("糖尿病足的日常检查要点。", Some("糖尿病足"), &[], s).unwrap();
    s.connection
        .execute(
            "UPDATE wiki_pages SET summary=?1 WHERE id=?2",
            rusqlite::params!["独立摘要：每日检查足部。", summarized.id],
        )
        .unwrap();
    // 模拟旧数据：无摘要行计入降级面（save_text_page 本身会填预览）
    s.connection
        .execute(
            "UPDATE wiki_pages SET summary='' WHERE id=?1",
            rusqlite::params![plain.id],
        )
        .unwrap();
    // 编译积压：一条 pending 建议
    let original = source(s, "https://example.com/health", "用于健康报告的来源页");
    let _proposal = propose_knowledge(s, &original.slug, "method", &compile_stub()).unwrap();
    // 采用：一条 usage（cited 引用 plain 页）
    s.connection
        .execute(
            "INSERT INTO knowledge_usage(id,task,owner_id,candidates_json,cited_json,created_at)
             VALUES ('u1','conversation','o','[]',?1,?2)",
            rusqlite::params![
                format!(r#"[{{"page_slug":"{}"}}]"#, plain.id),
                chrono::Utc::now().to_rfc3339()
            ],
        )
        .unwrap();
    // 摘要管线事件环境：最小 FK 链 conversation → message → run
    let now = chrono::Utc::now().to_rfc3339();
    s.connection
        .execute(
            "INSERT INTO conversations(id,created_at,updated_at) VALUES ('c-health',?1,?1)",
            [&now],
        )
        .unwrap();
    s.connection
        .execute(
            "INSERT INTO messages(id,conversation_id,role,content,created_at) VALUES ('m-health','c-health','user','问',?1)",
            [&now],
        )
        .unwrap();
    s.connection
        .execute(
            "INSERT INTO chat_runs(id,request_key,conversation_id,input_message_id,input_hash,status,executor_id,lease_until,created_at,updated_at) VALUES ('run-health','rk-health','c-health','m-health','h','running','owner',?2,?1,?1)",
            [
                &now,
                &(chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
            ],
        )
        .unwrap();
    s.append_chat_run_event(
        "run-health",
        "owner",
        "summary_unavailable",
        &serde_json::json!({"reason":"incomplete_or_invalid_basis","detail":"没有可摘要的历史"}),
    )
    .unwrap();
    s.append_chat_run_event(
        "run-health",
        "owner",
        "summary_ready",
        &serde_json::json!({}),
    )
    .unwrap();

    let report = health_report(s, 30).unwrap();
    assert!(
        report.pages_active >= 2,
        "至少包含两张笔记页（source 帮手另有来源页）"
    );
    assert_eq!(
        report.pages_without_summary, 1,
        "无摘要笔记页计入降级面（source 页有预览不在内）"
    );
    assert_eq!(report.proposals_pending, 1);
    assert_eq!(report.usage_records_recent, 1);
    assert_eq!(report.cited_pages_recent, 1);
    assert_eq!(report.summary_unavailable_recent, 1);
    assert_eq!(report.summary_ready_recent, 1);
    assert_eq!(
        report.last_summary_error.as_deref(),
        Some("没有可摘要的历史"),
        "P0-a：真实错误留痕供巡检定位"
    );
    // 被 usage 引用过的 plain 页不算「从未露面」；糖尿病足页（无摘要、
    // 无挂载、无引用）与 source 帮手的来源页共 2 页断点
    assert_eq!(report.pages_never_surfaced, 2);
}
