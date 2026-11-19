//! Scout 情报的今天栏卡片：每日一张合并卡、夜间静默、无未读不出卡
//! （提案 2026-11-15 §4.2）。卡片只是「提示去看」，无台账闭环动作。

use super::*;
use chrono::{Local, TimeZone};

fn at(offset_days: i64, hour: u32, minute: u32) -> chrono::DateTime<Local> {
    let date = (Local::now() + chrono::Duration::days(offset_days)).date_naive();
    Local
        .from_local_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
        .single()
        .unwrap()
}

fn scout_finding(store: &Store, id: i64, source: &str, kind: &str) {
    store
        .upsert_findings(&[crate::scout::ScoutFinding {
            id,
            source_id: source.to_string(),
            item_key: format!("item/{id}"),
            kind: kind.to_string(),
            payload_json: "{}".to_string(),
            delta_json: None,
            snapshot_id: 1,
            detected_at: "2026-11-16T08:00:00Z".to_string(),
        }])
        .unwrap();
}

fn card_content(store: &Store, message_id: &str) -> String {
    store
        .connection
        .query_row(
            "SELECT content FROM messages WHERE id=?1",
            [message_id],
            |r| r.get(0),
        )
        .unwrap()
}

fn scout_store() -> Store {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store.create_conversation(Some("主对话流"), None).unwrap();
    store
        .save_scout_settings(&crate::scout::ScoutSettings {
            base_url: "https://elsewhen-scout.example.workers.dev".into(),
            token_secret: None,
            poll_interval_minutes: 15,
            enabled: true,
            last_polled_at: None,
        })
        .unwrap();
    store
}

fn signals(store: &Store, now: chrono::DateTime<Local>, count: usize, kind: &str) {
    let items=(0..count).map(|i|serde_json::json!({"product_id":format!("p{i}"),"platform":"gumroad","title":format!("Product {i}"),"payload":{"currency_code":"USD"},"reasons":[{"kind":kind,"complete":true,"source_id":"s","metric":"price_cents","from":1000,"to":2000,"from_at":(now-chrono::Duration::hours(1)).to_rfc3339(),"to_at":now.to_rfc3339(),"snapshot_ids":[1,i+2],"valid_until":(now+chrono::Duration::days(3)).to_rfc3339()}]})).collect::<Vec<_>>();
    store
        .cache_scout_products(
            "/v1/products?view=signals",
            "fixture",
            &serde_json::json!({"instance_id":"fixture","as_of":now.to_rfc3339(),"items":items})
                .to_string(),
        )
        .unwrap();
}

#[test]
fn scout_brief_daily_gate_quiet_hours_and_evidence_deduplication() {
    let store = scout_store();
    let now = at(0, 9, 0);
    signals(&store, now, 2, "price_change");
    assert!(
        store
            .tick_scout_brief_card_at(at(0, 23, 30))
            .unwrap()
            .is_empty()
    );
    let ids = store.tick_scout_brief_card_at(now).unwrap();
    assert_eq!(ids.len(), 1);
    let body = card_content(&store, &ids[0]);
    assert!(body.starts_with("scout-brief-v2\n"));
    assert!(!body.contains("未读"));
    assert!(!body.contains("1530"));
    assert!(
        store
            .tick_scout_brief_card_at(at(0, 15, 0))
            .unwrap()
            .is_empty()
    );
    // A freshly fetched page with identical evidence is still not a new alert.
    signals(&store, at(1, 9, 0), 2, "price_change");
    assert!(
        store
            .tick_scout_brief_card_at(at(1, 9, 0))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn scout_brief_caps_products_and_ignores_raw_volume() {
    let store = scout_store();
    let now = at(0, 9, 0);
    for id in 0..1500 {
        scout_finding(&store, id, "github-dart", "new");
    }
    assert!(store.tick_scout_brief_card_at(now).unwrap().is_empty());
    signals(&store, now, 10, "new_scope");
    assert!(store.tick_scout_brief_card_at(now).unwrap().is_empty());
    signals(&store, now, 10, "price_change");
    let ids = store.tick_scout_brief_card_at(now).unwrap();
    let body = card_content(&store, &ids[0]);
    let data: serde_json::Value =
        serde_json::from_str(body.strip_prefix("scout-brief-v2\n").unwrap()).unwrap();
    assert_eq!(data["items"].as_array().unwrap().len(), 3);
    assert_eq!(data["items"][0]["title"], "Product 0");
}

#[test]
fn scout_brief_silent_for_stale_incomplete_or_expired_evidence() {
    let store = scout_store();
    let now = at(0, 9, 0);
    signals(&store, now - chrono::Duration::days(2), 1, "price_change");
    assert!(store.tick_scout_brief_card_at(now).unwrap().is_empty());
    signals(&store, now, 1, "price_change");
    let cache: serde_json::Value =
        serde_json::from_str(&store.cached_scout_products().unwrap()).unwrap();
    let mut page = cache["/v1/products?view=signals"].clone();
    page["items"][0]["reasons"][0]["complete"] = serde_json::json!(false);
    store
        .cache_scout_products("/v1/products?view=signals", "fixture", &page.to_string())
        .unwrap();
    assert!(store.tick_scout_brief_card_at(now).unwrap().is_empty());
    page["items"][0]["reasons"][0]["complete"] = serde_json::json!(true);
    page["items"][0]["reasons"][0]["valid_until"] =
        serde_json::json!((now - chrono::Duration::hours(1)).to_rfc3339());
    store
        .cache_scout_products("/v1/products?view=signals", "fixture", &page.to_string())
        .unwrap();
    assert!(store.tick_scout_brief_card_at(now).unwrap().is_empty());
}

#[test]
fn scout_brief_legacy_daily_card_does_not_block_highlights() {
    let store = scout_store();
    let now = at(0, 9, 0);
    let conversation = store
        .connection
        .query_row("SELECT id FROM conversations LIMIT 1", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap();
    store
        .insert_message_with_kind(
            &conversation,
            "assistant",
            "🛰 情报速览｜1530 条未读情报",
            None,
            None,
            "scout_brief",
            Some(&format!("scout_brief:{}", now.format("%Y-%m-%d"))),
        )
        .unwrap();
    signals(&store, now, 1, "price_change");
    let ids = store.tick_scout_brief_card_at(now).unwrap();
    assert_eq!(ids.len(), 1);
    assert!(card_content(&store, &ids[0]).starts_with("scout-brief-v2\n"));
    signals(&store, now, 3, "price_change");
    assert_eq!(store.tick_scout_brief_card_at(now).unwrap(), ids);
    let count:i64=store.connection.query_row("SELECT COUNT(*) FROM messages WHERE kind='scout_brief' AND content LIKE 'scout-brief-v2%'",[],|r|r.get(0)).unwrap();
    assert_eq!(count, 1);
}
