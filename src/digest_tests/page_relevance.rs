use super::super::*;
use super::drain;
use super::job_status;
use super::make_ready;
use super::record;
use super::temporary_database;
use super::*;

#[test]
fn every_event_is_enqueued_once_at_insert_and_open_never_duplicates() {
    let t = TempStore::new();
    t.store.insert_event(NewEvent::now("一")).unwrap();
    t.store.insert_event(NewEvent::now("二")).unwrap();
    // 无沉淀窗：任务应立即可领取
    let ready: i64 = t
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_digest_jobs WHERE available_at<=?1",
            [Utc::now().to_rfc3339()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(ready, 0, "刚落库的事件应留沉淀窗，不立即可领取");
    assert_eq!(
        t.store.digest_job_stats().unwrap().pending,
        2,
        "事件一落库就入队"
    );
    make_ready(&t.store);
    assert_eq!(
        t.store.digest_job_stats().unwrap().pending,
        2,
        "快进沉淀窗后仍在队列里"
    );

    // 重复开库不得重复入队，也不得把已入队的丢掉
    let reopened = Store::open(&t.path).unwrap();
    assert_eq!(reopened.digest_job_stats().unwrap().pending, 2);
    drop(reopened);
    let again = Store::open(&t.path).unwrap();
    assert_eq!(
        again.digest_job_stats().unwrap().pending,
        2,
        "重复 open 不重复入队"
    );
}

#[test]
fn no_provider_config_means_no_digest_provider() {
    let t = TempStore::new();
    record(&t.store, "晨跑");
    assert!(crate::wiki::digest_provider(&t.store).unwrap().is_none());
    assert_eq!(
        t.store.digest_job_stats().unwrap().pending,
        1,
        "事件照常入队等待"
    );
}

#[test]
fn relevance_scored_page_gets_fulltext_and_unrelated_one_does_not() {
    use crate::storage::EventRecord;
    use crate::wiki::select_context_pages_for_events;
    let t = TempStore::new();
    let seed = |slug: &str, title: &str, tags: &[&str], body: &str| {
        t.store
            .upsert_wiki_page(
                &WikiPageDraft {
                    slug: slug.into(),
                    kind: slug.split('/').next().unwrap().into(),
                    title: title.into(),
                    summary: title.into(),
                    content_md: body.into(),
                    tags: tags.iter().map(|s| s.to_string()).collect(),
                    source_event_ids: vec![],
                    status: "active".into(),
                    reason: "test".into(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )
            .unwrap();
    };
    // 低证据的散步页（旧逻辑按证据数/时间排序根本轮不到它）
    seed(
        "habit/walk-after-lunch",
        "午饭后散步习惯",
        &["散步"],
        "- 每天午饭后散步。",
    );
    std::thread::sleep(std::time::Duration::from_millis(2));
    // 高证据但不相关的页（旧逻辑会把它选进上下文）
    seed(
        "relationship/old-friend",
        "老朋友",
        &["朋友"],
        "- 很久没联系。",
    );
    let events = vec![EventRecord {
        id: "e1".into(),
        recorded_at: "2026-10-20T12:00:00Z".into(),
        raw_text: "今天午饭后走了四十分钟".into(),
    }];
    let pages = t.store.list_wiki_pages(None, None).unwrap();
    let picked = select_context_pages_for_events(&events, pages, &["habit", "relationship"], 8000);
    let slugs: Vec<&str> = picked.iter().map(|p| p.slug.as_str()).collect();
    assert!(
        slugs.contains(&"habit/walk-after-lunch"),
        "事件提到的低证据页必须进上下文：{slugs:?}"
    );
    assert!(
        !slugs.contains(&"relationship/old-friend"),
        "与事件无关的高证据页不再挤占上下文：{slugs:?}"
    );
}

#[test]
fn digest_prompt_includes_relevant_page_fulltext() {
    let t = TempStore::new();
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/walk-after-lunch".into(),
                kind: "habit".into(),
                title: "午饭后散步习惯".into(),
                summary: "坚持午饭后散步".into(),
                content_md: "- 每天午饭后散步三十分钟".into(),
                tags: vec!["散步".into()],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "test".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    record(&t.store, "今天午饭后走了四十分钟");
    make_ready(&t.store);
    let (provider, seen) = capture_prompt();
    let tick = process_digest_queue(&t.store, &provider, None).unwrap();
    assert!(matches!(tick, DigestTick::Processed { .. }));
    let prompt = seen.borrow();
    assert!(
        prompt.contains("每天午饭后散步三十分钟"),
        "散步页正文必须进提示词，模型才有机会 update 它"
    );
    assert!(
        prompt.contains("相关现有页面全文"),
        "提示词包含相关性选页的全文区块"
    );
}

#[test]
fn synonym_create_is_redirected_to_existing_page_update() {
    let t = TempStore::new();
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/walk-after-lunch".into(),
                kind: "habit".into(),
                title: "午饭后散步习惯".into(),
                summary: "坚持午饭后散步".into(),
                content_md: "- 每天午饭后散步三十分钟".into(),
                tags: vec!["散步".into()],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "test".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    record(&t.store, "今天午饭后散步了四十分钟");
    make_ready(&t.store);
    // 模型无视索引新建了同义 slug（slug 漂移，缺陷⑤）
    let provider = ScriptedProvider(|_| {
        Ok(r#"[{"op":"create","kind":"habit","slug":"habit/after-lunch-walk","title":"午饭后散步习惯","summary":"坚持午饭后散步","content":"- 每天午饭后散步四十分钟","tags":["散步"],"source_event_ids":["1"],"reason":"新事件"}]"#.into())
    });
    let tick = process_digest_queue(&t.store, &provider, None).unwrap();
    let DigestTick::Processed { outcome, .. } = tick else {
        panic!("预期消化成功：{tick:?}");
    };
    assert!(
        outcome.created.is_empty()
            && outcome
                .updated
                .contains(&"habit/walk-after-lunch".to_string()),
        "同义新建必须被改写为既有页更新：{outcome:?}"
    );
    assert!(
        t.store
            .get_wiki_page("habit/after-lunch-walk")
            .unwrap()
            .is_none(),
        "不得留下重复页"
    );
    let page = t
        .store
        .get_wiki_page("habit/walk-after-lunch")
        .unwrap()
        .unwrap();
    assert_eq!(page.evidence_count, 1, "证据并入既有页");
    assert!(page.content_md.contains("四十分钟"), "正文被更新");
}

#[test]
fn same_title_different_entity_is_not_redirected() {
    let t = TempStore::new();
    t.store
        .upsert_wiki_page(
            &WikiPageDraft {
                slug: "habit/walk-after-lunch".into(),
                kind: "habit".into(),
                title: "午饭后散步习惯".into(),
                summary: "坚持午饭后散步".into(),
                content_md: "- 每天午饭后散步三十分钟".into(),
                tags: vec!["散步".into()],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "test".into(),
                source_url: None,
            },
            ContentPolicy::Always,
        )
        .unwrap();
    record(&t.store, "晨跑");
    make_ready(&t.store);
    // kind 相同、标题不同、tag 不重合 → 不是同一个页，正常 create
    let provider = ScriptedProvider(|_| {
        Ok(r#"[{"op":"create","kind":"habit","slug":"habit/morning-run","title":"晨跑","summary":"坚持晨跑","content":"- 晨跑五公里","tags":["跑步"],"source_event_ids":["1"],"reason":"新习惯"}]"#.into())
    });
    let tick = process_digest_queue(&t.store, &provider, None).unwrap();
    let DigestTick::Processed { outcome, .. } = tick else {
        panic!("预期消化成功：{tick:?}");
    };
    assert!(
        outcome.created.contains(&"habit/morning-run".to_string()),
        "{outcome:?}"
    );
}
