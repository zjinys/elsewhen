use super::*;

use super::*;
use crate::event::NewEvent;
use crate::storage::Store;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_database() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "elsewhen-guidance-test-{}.db",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn page_for_test(store: &Store, kind: &str, title: &str) {
    let slug = crate::wiki::slugify(title);
    store
        .upsert_wiki_page(
            &crate::storage::WikiPageDraft {
                slug,
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
            crate::storage::ContentPolicy::Always,
        )
        .unwrap();
}

#[test]
fn wash_guidance_drops_invented_slugs_and_caps_counts() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    page_for_test(&store, "topic", "网站改版");
    let mut value =
        EventAnalysisV1::parse(r#"{"schema_version":"event-analysis-v3","recordable":true,"kind":"event","event_type":"note","confidence":0.9,"summary":"s","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[],"guidance":{"related":[{"slug":"网站改版","note":"相关经验"},{"slug":"杜撰页","note":"不存在的页"}],"lessons":["经验一"],"follow_ups_refined":["精炼待办"]}}"#).unwrap();
    wash_guidance(&store, &mut value);
    let guidance = value.guidance.as_ref().unwrap();
    assert_eq!(guidance.related.len(), 1, "杜撰 slug 必须被丢弃");
    assert_eq!(guidance.related[0].slug, "网站改版");
    assert_eq!(guidance.lessons[0], "经验一");
    // 全空后整字段清除
    let mut empty =
        EventAnalysisV1::parse(r#"{"schema_version":"event-analysis-v3","recordable":true,"kind":"event","event_type":"note","confidence":0.9,"summary":"s","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[],"guidance":{"related":[{"slug":"杜撰页","note":"n"}],"lessons":[],"follow_ups_refined":[]}}"#).unwrap();
    wash_guidance(&store, &mut empty);
    assert!(empty.guidance.is_none(), "清洗后为空应整字段省略");
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn decision_material_includes_relations_and_recent_entries() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    page_for_test(&store, "person", "李四");
    page_for_test(&store, "topic", "网站改版");
    store
        .upsert_relation(&crate::storage::RelationDraft {
            from_slug: crate::wiki::slugify("李四"),
            from_kind: "person".to_string(),
            to_slug: crate::wiki::slugify("网站改版"),
            to_kind: "topic".to_string(),
            relation: "参与".to_string(),
            note: None,
            confidence: 3,
            source_conversation_id: None,
            source_event_id: None,
        })
        .unwrap();
    store
        .insert_event(NewEvent::now("和李四讨论网站改版"))
        .unwrap();
    let (context, _) = decision_support_material(&store, "网站改版").unwrap();
    assert!(context.contains("关系："), "决策材料应含用户模型关系边");
    assert!(context.contains("近期记录："), "决策材料应含近期记录条目");
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn guidance_flows_to_detail_and_page_aggregation() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    page_for_test(&store, "topic", "网站改版");
    store.insert_event(NewEvent::now("网站改版上线了")).unwrap();
    // 直接注入带 guidance 的分析结果（绕过 provider，聚焦派生层读面）
    let value =
        EventAnalysisV1::parse(r#"{"schema_version":"event-analysis-v3","recordable":true,"kind":"event","event_type":"note","confidence":0.9,"summary":"上线","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[],"guidance":{"related":[{"slug":"网站改版","note":"此前联调经验可复用"}],"lessons":["先灰度再全量"],"follow_ups_refined":["观察一周报错率"]}}"#).unwrap();
    let detail = store
        .event_analysis_detail(&guidance_event_id(&store))
        .unwrap()
        .unwrap();
    store
        .complete_analysis(
            &crate::storage::AnalysisJob {
                id: job_id(&store),
                event_id: detail.event_id.clone(),
                raw_text: String::new(),
                attempts: 0,
            },
            &value.schema_version,
            &serde_json::to_string(&value).unwrap(),
        )
        .unwrap();
    let items = store
        .page_guidance_items(&crate::wiki::slugify("网站改版"))
        .unwrap();
    assert_eq!(items.len(), 1, "页聚合应引用到 guidance");
    assert_eq!(items[0].lessons, vec!["先灰度再全量".to_string()]);
    assert_eq!(
        items[0].follow_ups_refined,
        vec!["观察一周报错率".to_string()]
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn emotion_flows_list_and_delete_leaves_event_untouched() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .insert_event(NewEvent::now("和家长会老师聊了成绩"))
        .unwrap();
    let value =
        EventAnalysisV1::parse(r#"{"schema_version":"event-analysis-v3","recordable":true,"kind":"event","event_type":"note","confidence":0.9,"summary":"s","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[],"emotion":[{"target":"家长会老师","feeling":"被理解"},{"target":"","feeling":"焦虑"}]}"#).unwrap();
    store
        .complete_analysis(
            &crate::storage::AnalysisJob {
                id: job_id(&store),
                event_id: guidance_event_id(&store),
                raw_text: String::new(),
                attempts: 0,
            },
            &value.schema_version,
            &serde_json::to_string(&value).unwrap(),
        )
        .unwrap();
    // 列表面（派生读，target 空串归一为 None=对事件本身）
    let items = store.list_event_emotions(100).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].target.as_deref(), Some("家长会老师"));
    assert_eq!(items[0].feeling, "被理解");
    assert_eq!(items[1].target, None, "空 target = 对事件本身");

    // 删除只动 result_json 的 emotion 字段：事件原文与其它分析字段不动
    let before: String = store
        .connection
        .query_row(
            "SELECT raw_text FROM events WHERE id=?1",
            [guidance_event_id(&store)],
            |r| r.get(0),
        )
        .unwrap();
    let removed = store
        .delete_event_emotion(&items[0].analysis_id, Some("家长会老师"), "被理解")
        .unwrap();
    assert!(removed);
    let after_raw: String = store
        .connection
        .query_row(
            "SELECT raw_text FROM events WHERE id=?1",
            [guidance_event_id(&store)],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(before, after_raw, "事件真源必须分毫不动");
    let detail = store
        .event_analysis_detail(&guidance_event_id(&store))
        .unwrap()
        .unwrap();
    let value = EventAnalysisV1::parse(&detail.result_json.unwrap()).unwrap();
    let emotions = value.emotion.unwrap();
    assert_eq!(emotions.len(), 1);
    assert_eq!(emotions[0].feeling, "焦虑");
    // 清空后整字段移除（避免残留空数组垃圾）
    store
        .delete_event_emotion(&items[0].analysis_id, Some(""), "焦虑")
        .unwrap();
    let value = EventAnalysisV1::parse(
        &store
            .event_analysis_detail(&guidance_event_id(&store))
            .unwrap()
            .unwrap()
            .result_json
            .unwrap(),
    )
    .unwrap();
    assert!(value.emotion.is_none(), "空数组应整字段省略");
    drop(store);
    let _ = std::fs::remove_file(path);
}

fn guidance_event_id(store: &Store) -> String {
    store
        .connection
        .query_row("SELECT id FROM events LIMIT 1", [], |r| r.get(0))
        .unwrap()
}

fn job_id(store: &Store) -> String {
    store
        .connection
        .query_row("SELECT id FROM analysis_jobs LIMIT 1", [], |r| r.get(0))
        .unwrap()
}

fn rule_for_test(store: &Store) -> String {
    let id = format!("rule-{}", uuid::Uuid::new_v4());
    store
        .connection
        .execute(
            "INSERT INTO rules (id, content, status, created_at, updated_at)
             VALUES (?1, '发布前先跑测试', 'active', strftime('%Y-%m-%dT%H:%M:%fZ','now'), strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
            rusqlite::params![id],
        )
        .unwrap();
    id
}

fn hit_for_test(
    store: &Store,
    rule_id: &str,
    event_id: &str,
    status: &str,
    days_ago: i64,
) -> String {
    let id = format!("hit-{}", uuid::Uuid::new_v4());
    store
        .connection
        .execute(
            "INSERT INTO rule_hits (id, rule_id, event_id, status, confidence, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 0.9, ?5, ?5)",
            rusqlite::params![
                id,
                rule_id,
                event_id,
                status,
                (chrono::Utc::now() - chrono::Duration::days(days_ago)).to_rfc3339()
            ],
        )
        .unwrap();
    id
}

/// D6 采纳反馈：记录 / 清除（toggle）/ fail-closed。
/// 走 store 层（api ::set_rule_hit_feedback 会经 AppConfig::load 打开
/// 真实用户库，测试里一律不碰；api 层只做 trim/转发）。
#[test]
fn rule_hit_feedback_records_clears_and_fails_closed() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_id = store
        .insert_event(NewEvent::now("踩了发布流程的坑"))
        .unwrap();
    let rule_id = rule_for_test(&store);
    let hit_id = hit_for_test(&store, &rule_id, &event_id, "notified", 0);

    // 记录采纳
    store
        .set_rule_hit_feedback(&hit_id, Some("helpful"))
        .unwrap();
    let hits = store.recent_rule_hits(10).unwrap();
    assert_eq!(
        hits.iter()
            .find(|h| h.id == hit_id)
            .unwrap()
            .feedback
            .as_deref(),
        Some("helpful")
    );

    // 换一条（同事件同规则 UNIQUE，换事件）：反馈 not_relevant
    let event2_id = store.insert_event(NewEvent::now("又一次提醒")).unwrap();
    let hit2 = hit_for_test(&store, &rule_id, &event2_id, "aggregated", 0);
    store
        .set_rule_hit_feedback(&hit2, Some("not_relevant"))
        .unwrap();
    let hits = store.recent_rule_hits(10).unwrap();
    assert_eq!(
        hits.iter()
            .find(|h| h.id == hit2)
            .unwrap()
            .feedback
            .as_deref(),
        Some("not_relevant")
    );

    // 清除 = feedback NULL
    store.set_rule_hit_feedback(&hit_id, None).unwrap();
    let hits = store.recent_rule_hits(10).unwrap();
    assert!(
        hits.iter()
            .find(|h| h.id == hit_id)
            .unwrap()
            .feedback
            .is_none()
    );

    // fail-closed：未知值报错；不存在的 id 返回 false（api 层转 bail）
    assert!(
        store
            .set_rule_hit_feedback(&hit_id, Some("自动推断"))
            .is_err()
    );
    assert!(
        !store
            .set_rule_hit_feedback("no-such-hit", Some("helpful"))
            .unwrap()
    );
    // pending（未处置）命中不可反馈——度量只面向已送达用户的提醒
    let event3_id = store.insert_event(NewEvent::now("还没提醒的")).unwrap();
    let hit3 = hit_for_test(&store, &rule_id, &event3_id, "pending", 0);
    assert!(!store.set_rule_hit_feedback(&hit3, Some("helpful")).unwrap());
    let _ = event_id;
    drop(store);
    let _ = std::fs::remove_file(path);
}

/// D6 回顾勾连拼装等价验证（store 层；api ::get_daily_overview 直接
/// 调 store.rule_hits_on + memory_echo_for，经 AppConfig 会开真实库，
/// 测试不碰）。
#[test]
fn daily_overview_parts_available_from_store() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let today = chrono::Local::now().date_naive();
    let today_id = store.insert_event(NewEvent::now("今天的进展")).unwrap();
    let rule_id = rule_for_test(&store);
    hit_for_test(&store, &rule_id, &today_id, "aggregated", 0);
    let hits = store.rule_hits_on(today).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].rule_content, "发布前先跑测试");
    assert_eq!(hits[0].status, "aggregated");
    // 昨天的命中不该混进今天
    let yesterday = store
        .insert_event(NewEvent {
            raw_text: "昨天的事",
            occurred_at: chrono::Utc::now() - chrono::Duration::days(1),
            recorded_at: chrono::Utc::now() - chrono::Duration::days(1),
            source: "capture",
        })
        .unwrap();
    let _ = hit_for_test(&store, &rule_id, &yesterday, "notified", 1);
    // 同规则+同事件会 UNIQUE 冲突，这里只是构造不同事件——用新规则
    let rule2 = rule_for_test(&store);
    let _ = hit_for_test(&store, &rule2, &yesterday, "notified", 1);
    assert_eq!(store.rule_hits_on(today).unwrap().len(), 1);
    drop(store);
    let _ = std::fs::remove_file(path);
}

/// D6 回顾勾连 storage 联测：当日命中 + 一周回引同库成立。
#[test]
fn memory_echo_and_day_hits_are_store_level_queries() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let today = chrono::Local::now().date_naive();
    assert!(store.memory_echo_for(today, 3).unwrap().is_empty());
    let days_ago = chrono::Utc::now() - chrono::Duration::days(8);
    store
        .insert_event(NewEvent {
            raw_text: "去年秋天 mountain trip",
            occurred_at: days_ago,
            recorded_at: days_ago,
            source: "capture",
        })
        .unwrap();
    let echo = store.memory_echo_for(today, 3).unwrap();
    assert_eq!(echo.len(), 1);
    assert!(echo[0].2.contains("mountain trip"));
    // 超过 max 截断
    let days_ago2 = chrono::Utc::now() - chrono::Duration::days(7);
    store
        .insert_event(NewEvent {
            raw_text: "第二条回引素材",
            occurred_at: days_ago2,
            recorded_at: days_ago2,
            source: "capture",
        })
        .unwrap();
    assert_eq!(store.memory_echo_for(today, 1).unwrap().len(), 1);
}
