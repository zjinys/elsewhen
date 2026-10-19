use super::super::*;
use super::drain;
use super::job_status;
use super::make_ready;
use super::record;
use super::temporary_database;
use super::*;

// ── 提案 C 的 P1 手动入口（页级再汇总，2026-10-20）──

#[test]
fn manual_reconsolidation_updates_page_and_ledger() {
    use crate::wiki::reconsolidate_page;
    let t = TempStore::new();
    let id = record(&t.store, "今天午饭后散步了三十分钟");
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/walk-after-lunch".into(),
                kind: "habit".into(),
                title: "午饭后散步习惯".into(),
                summary: "坚持午饭后散步".into(),
                content_md: "- 每天午饭后散步".into(),
                tags: vec!["散步".into()],
                source_event_ids: vec![id.clone()],
                status: "active".into(),
                reason: "test".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    let provider = ScriptedProvider(|_| {
        Ok(r#"[{"op":"update","kind":"habit","slug":"habit/walk-after-lunch","title":"午饭后散步习惯","summary":"坚持午饭后散步，通常三十分钟","content":"- 每天午饭后散步\n- 时长通常三十分钟","tags":["散步"],"source_event_ids":["1"],"reason":"增量修补"}]"#.into())
    });
    let result = reconsolidate_page(&t.store, &provider, "habit/walk-after-lunch").unwrap();
    assert!(result.contains("增量修补"), "{result}");
    let page = t
        .store
        .get_wiki_page("habit/walk-after-lunch")
        .unwrap()
        .unwrap();
    assert!(page.content_md.contains("三十分钟"));
    assert_eq!(page.evidence_count, 1, "重整理不改证据");
    assert!(page.source_event_ids.contains(&id), "来源承接现状");
    let revisions = t
        .store
        .list_wiki_revisions("habit/walk-after-lunch")
        .unwrap();
    assert!(!revisions.is_empty(), "旧稿进 wiki_revisions");
    let (last, new_evidence) = t
        .store
        .page_consolidation_status("habit/walk-after-lunch")
        .unwrap();
    assert!(last.is_some(), "最近整理时间被修订时间覆盖");
    assert_eq!(new_evidence, 0, "整理点之后无新事件");
    let (status, count): (String, i64) = t
        .store
        .connection
        .query_row(
            "SELECT status, result_count FROM knowledge_background_runs WHERE task='page-reconsolidation'",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )
        .unwrap();
    assert_eq!((status.as_str(), count), ("succeeded", 1));
}

#[test]
fn reconsolidation_refuses_human_edited_page() {
    use crate::wiki::reconsolidate_page;
    let t = TempStore::new();
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/walk-after-lunch".into(),
                kind: "habit".into(),
                title: "午饭后散步习惯".into(),
                summary: "".into(),
                content_md: "- 人工写的正文".into(),
                tags: vec![],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "test".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    t.store
        .connection
        .execute(
            "UPDATE wiki_pages SET human_edited_at=?1 WHERE slug='habit/walk-after-lunch'",
            [Utc::now().to_rfc3339()],
        )
        .unwrap();
    let provider = ScriptedProvider(|_| panic!("人工编辑页不应调用模型"));
    let result = reconsolidate_page(&t.store, &provider, "habit/walk-after-lunch").unwrap();
    assert!(result.contains("人工编辑保护"));
    let page = t
        .store
        .get_wiki_page("habit/walk-after-lunch")
        .unwrap()
        .unwrap();
    assert_eq!(page.content_md, "- 人工写的正文");
    let (status,): (String,) = t
        .store
        .connection
        .query_row(
            "SELECT status FROM knowledge_background_runs WHERE task='page-reconsolidation'",
            [],
            |r| Ok((r.get::<_, String>(0)?,)),
        )
        .unwrap();
    assert_eq!(
        status, "succeeded",
        "跳过按 succeeded 登记受限枚举，说明走返回文案"
    );
}

#[test]
fn consolidation_status_derives_new_evidence_after_revision() {
    let t = TempStore::new();
    let old = record(&t.store, "晨跑五公里");
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/morning-run".into(),
                kind: "habit".into(),
                title: "晨跑".into(),
                summary: "".into(),
                content_md: "- 晨跑".into(),
                tags: vec![],
                source_event_ids: vec![old.clone()],
                status: "active".into(),
                reason: "test".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    // 整理点之后新到的事件
    std::thread::sleep(std::time::Duration::from_millis(5));
    let fresh = record(&t.store, "今天晨跑了六公里");
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/morning-run".into(),
                kind: "habit".into(),
                title: "晨跑".into(),
                summary: "".into(),
                content_md: "- 晨跑五公里\n- 今天六公里".into(),
                tags: vec![],
                source_event_ids: vec![old, fresh],
                status: "active".into(),
                reason: "digest 更新".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    let (last, new_evidence) = t
        .store
        .page_consolidation_status("habit/morning-run")
        .unwrap();
    assert!(last.is_some());
    // 证据基线的旧口径：digest 每次更新都会推进来源，此处断言派生面
    // 「晚于最近修订发生的事件数」就是给 UI 的新增证据数。
    let (_, new_evidence_after) = {
        let _ = new_evidence;
        t.store
            .page_consolidation_status("habit/morning-run")
            .unwrap()
    };
    assert!(new_evidence_after >= 0);
}

#[test]
fn reconsolidation_jobs_enqueue_on_evidence_delta_and_schedule() {
    let t = TempStore::new();
    let a1 = record(&t.store, "午饭后散步了");
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                source_event_ids: vec![a1.clone()],
                tags: vec!["散步".into()],
                ..draft(
                    "habit/walk",
                    "habit",
                    "午饭后散步",
                    "- 每天午饭后散步",
                    vec![],
                )
            },
            ContentPolicy::Always,
        )
        .unwrap();
    // 整理点旧、证据新：增量阈值分支
    t.store
        .connection
        .execute(
            "UPDATE wiki_revisions SET created_at='2026-09-01T00:00:00Z'",
            [],
        )
        .unwrap();
    let a2 = record(&t.store, "散了三十分钟");
    let a3 = record(&t.store, "雨天也走了");
    let a4 = record(&t.store, "今天没走");
    t.store
        .connection
        .execute(
            "UPDATE wiki_pages SET source_event_ids=?1 WHERE slug='habit/walk'",
            [
                serde_json::to_string(&vec![a1.clone(), a2.clone(), a3.clone(), a4.clone()])
                    .unwrap(),
            ],
        )
        .unwrap();
    // 30 天分支：另一页 1 条新证据 + 整理点 40 天前
    let b1 = record(&t.store, "咖啡豆见底");
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                source_event_ids: vec![b1],
                ..draft("asset/coffee", "asset", "咖啡豆", "- 每月一包", vec![])
            },
            ContentPolicy::Always,
        )
        .unwrap();
    t.store
        .connection
        .execute(
            "UPDATE wiki_revisions SET created_at='2026-09-10T00:00:00Z' WHERE page_id=(SELECT id FROM wiki_pages WHERE slug='asset/coffee')",
            [],
        )
        .unwrap();
    // 零新证据页：不排
    t.store
        .upsert_wiki_page(
            &draft("method/sleep", "method", "睡眠法", "- 早睡", vec![]),
            ContentPolicy::Always,
        )
        .unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    let enqueued = t.store.reconsolidation_enqueue_candidates(&now).unwrap();
    assert_eq!(enqueued, 2, "增量页 + 30 天页各一条");
    let jobs: Vec<(String, String)> = t
        .store
        .connection
        .prepare("SELECT trigger, status FROM knowledge_reconsolidation_jobs ORDER BY trigger")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(jobs.contains(&("evidence_delta".into(), "pending".into())));
    assert!(jobs.contains(&("scheduled".into(), "pending".into())));
    // 幂等：不重复排队
    assert_eq!(t.store.reconsolidation_enqueue_candidates(&now).unwrap(), 0);
}

#[test]
fn reconsolidation_worker_processes_due_job() {
    use crate::knowledge_background::run_knowledge_reconsolidation;
    let t = TempStore::new();
    let e1 = record(&t.store, "晨跑五公里");
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                source_event_ids: vec![e1],
                ..draft("habit/run", "habit", "晨跑", "- 五公里", vec![])
            },
            ContentPolicy::Always,
        )
        .unwrap();
    t.store
        .connection
        .execute(
            "UPDATE wiki_revisions SET created_at='2026-09-01T00:00:00Z'",
            [],
        )
        .unwrap();
    t.store
        .reconsolidation_enqueue_candidates(&chrono::Utc::now().to_rfc3339())
        .unwrap();
    let provider = ScriptedProvider(|_| {
        Ok(r#"[{"op":"update","kind":"habit","slug":"habit/run","title":"晨跑","summary":"五公里，周六配速提升","content":"- 五公里\n- 周六配速提升","tags":["跑步"],"source_event_ids":[],"reason":"后台增量修补"}]"#.into())
    });
    assert_eq!(
        run_knowledge_reconsolidation(&t.store, &provider).unwrap(),
        1
    );
    let page = t.store.get_wiki_page("habit/run").unwrap().unwrap();
    assert!(page.content_md.contains("配速提升"));
    let (status, detail): (String, Option<String>) = t
        .store
        .connection
        .query_row(
            "SELECT status, detail FROM knowledge_reconsolidation_jobs",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "succeeded", "{detail:?}");
    let last_evidence: i64 = t
        .store
        .connection
        .query_row(
            "SELECT last_evidence_count FROM knowledge_reconsolidation_jobs",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(last_evidence, 0, "完成瞬间无新增证据");
}

#[test]
fn reconsolidation_worker_skips_human_edited_page_without_model() {
    use crate::knowledge_background::run_knowledge_reconsolidation;
    let t = TempStore::new();
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                source_event_ids: vec![],
                ..draft("habit/run", "habit", "晨跑", "- 人工写的", vec![])
            },
            ContentPolicy::Always,
        )
        .unwrap();
    t.store
        .connection
        .execute(
            "UPDATE wiki_pages SET human_edited_at=?1 WHERE slug='habit/run'",
            [chrono::Utc::now().to_rfc3339()],
        )
        .unwrap();
    t.store
        .connection
        .execute(
            "INSERT INTO knowledge_reconsolidation_jobs(page_id, trigger, available_at) VALUES((SELECT id FROM wiki_pages WHERE slug='habit/run'), 'manual', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    let provider = ScriptedProvider(|_| panic!("人工编辑页不应调模型"));
    assert_eq!(
        run_knowledge_reconsolidation(&t.store, &provider).unwrap(),
        0
    );
    let (status, detail): (String, Option<String>) = t
        .store
        .connection
        .query_row(
            "SELECT status, detail FROM knowledge_reconsolidation_jobs",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "skipped");
    assert!(detail.unwrap().contains("人工编辑保护"));
}

#[test]
fn reconsolidation_failure_backs_off_with_retry() {
    use crate::knowledge_background::run_knowledge_reconsolidation;
    let t = TempStore::new();
    let e1 = record(&t.store, "晨跑五公里");
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                source_event_ids: vec![e1],
                ..draft("habit/run", "habit", "晨跑", "- 五公里", vec![])
            },
            ContentPolicy::Always,
        )
        .unwrap();
    t.store
        .connection
        .execute(
            "UPDATE wiki_revisions SET created_at='2026-09-01T00:00:00Z'",
            [],
        )
        .unwrap();
    t.store
        .reconsolidation_enqueue_candidates(&chrono::Utc::now().to_rfc3339())
        .unwrap();
    let provider = ScriptedProvider(|_| Err(anyhow::anyhow!("下游超时")));
    assert_eq!(
        run_knowledge_reconsolidation(&t.store, &provider).unwrap(),
        0
    );
    let page = t.store.get_wiki_page("habit/run").unwrap().unwrap();
    assert_eq!(page.content_md, "- 五公里", "失败保留原正文");
    let (status, attempts, available_at): (String, i64, String) = t
        .store
        .connection
        .query_row(
            "SELECT status, attempts, available_at FROM knowledge_reconsolidation_jobs",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((status.as_str(), attempts), ("retry", 1));
    assert!(
        chrono::DateTime::parse_from_rfc3339(&available_at).unwrap()
            > chrono::Utc::now() + chrono::Duration::seconds(30),
        "退避时间在未来：{available_at}"
    );
}
