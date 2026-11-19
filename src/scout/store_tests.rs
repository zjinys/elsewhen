//! scout 镜像存储测试：设置持久化、幂等 upsert 不覆盖已读态、过滤查询、
//! 已读操作、水位。全部走临时库，不碰个人数据。

use super::*;
use crate::scout::ScoutFinding;

fn store() -> Store {
    Store::open(&std::env::temp_dir().join(format!("scout-mirror-{}.db", uuid::Uuid::new_v4())))
        .unwrap()
}

fn finding(id: i64, kind: &str, item: &str) -> ScoutFinding {
    ScoutFinding {
        id,
        source_id: "gumroad-notion".into(),
        snapshot_id: 100 + id,
        kind: kind.into(),
        item_key: item.into(),
        payload_json: r#"{"name":"item","price_cents":1999}"#.into(),
        delta_json: None,
        detected_at: "2026-11-15T13:30:00Z".into(),
    }
}

#[test]
fn settings_roundtrip_and_validation() {
    let s = store();
    assert!(s.scout_settings().unwrap().is_none());

    s.save_scout_settings(&ScoutSettings {
        base_url: "https://elsewhen-scout.example.workers.dev/".into(),
        token_secret: Some("scout_prod".into()),
        poll_interval_minutes: 15,
        enabled: true,
        last_polled_at: None,
    })
    .unwrap();
    let cfg = s.scout_settings().unwrap().unwrap();
    // 尾部斜杠被规范化掉。
    assert_eq!(cfg.base_url, "https://elsewhen-scout.example.workers.dev");
    assert_eq!(cfg.token_secret.as_deref(), Some("scout_prod"));

    // 间隔下限 5。
    assert!(
        s.save_scout_settings(&ScoutSettings {
            base_url: "https://x.dev".into(),
            token_secret: None,
            poll_interval_minutes: 4,
            enabled: true,
            last_polled_at: None,
        })
        .is_err()
    );
}

fn ensure_settings(s: &Store) {
    s.save_scout_settings(&ScoutSettings {
        base_url: "https://x.dev".into(),
        token_secret: None,
        poll_interval_minutes: 15,
        enabled: true,
        last_polled_at: None,
    })
    .unwrap();
}

#[test]
fn upsert_is_idempotent_and_preserves_read_state() {
    let s = store();
    ensure_settings(&s);

    assert_eq!(
        s.upsert_findings(&[finding(1, "new", "a"), finding(2, "new", "b")])
            .unwrap(),
        2
    );
    assert_eq!(s.scout_unread_count().unwrap(), 2);

    // 已读 id=1 后再次拉到同一 id（服务端 payload 更新）：read_at 不被冲掉。
    s.mark_findings_read(&[1]).unwrap();
    assert_eq!(s.scout_unread_count().unwrap(), 1);
    let updated = finding(1, "changed", "a");
    assert_eq!(s.upsert_findings(&[updated]).unwrap(), 0);
    let row = s
        .list_scout_findings(None, None, false, 10, 0)
        .unwrap()
        .into_iter()
        .find(|r| r.id == 1)
        .unwrap();
    assert!(row.read_at.is_some(), "已读态不得被 upsert 覆盖");
    assert_eq!(row.kind, "changed", "payload 字段应更新");
}

#[test]
fn list_filters_and_pagination() {
    let s = store();
    s.upsert_findings(&[
        finding(1, "new", "a"),
        finding(2, "changed", "a"),
        finding(3, "gone", "b"),
    ])
    .unwrap();

    assert_eq!(
        s.list_scout_findings(Some("new"), None, false, 10, 0)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        s.list_scout_findings(None, Some("gumroad-notion"), false, 10, 0)
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        s.list_scout_findings(None, None, false, 2, 0)
            .unwrap()
            .len(),
        2
    );
    // id 倒序：第一页是 3、2。
    assert_eq!(
        s.list_scout_findings(None, None, false, 2, 0).unwrap()[0].id,
        3
    );
    assert!(
        s.list_scout_findings(Some("bogus"), None, false, 10, 0)
            .is_err()
    );
}

#[test]
fn mark_read_all_and_clear() {
    let s = store();
    ensure_settings(&s);
    s.upsert_findings(&[finding(1, "new", "a"), finding(2, "new", "b")])
        .unwrap();

    // 空切片 = 全部已读。
    assert_eq!(s.mark_findings_read(&[]).unwrap(), 2);
    assert_eq!(s.scout_unread_count().unwrap(), 0);

    s.clear_scout_mirror().unwrap();
    assert_eq!(
        s.list_scout_findings(None, None, false, 10, 0)
            .unwrap()
            .len(),
        0
    );
    // 设置行保留。
    assert!(s.scout_settings().unwrap().is_some());
}

#[test]
fn unread_by_source_and_watermark() {
    let s = store();
    ensure_settings(&s);
    let mut f2 = finding(2, "new", "b");
    f2.source_id = "github-dart".into();
    s.upsert_findings(&[finding(1, "new", "a"), f2]).unwrap();
    let by_source = s.scout_unread_by_source().unwrap();
    assert_eq!(
        by_source,
        vec![("github-dart".into(), 1), ("gumroad-notion".into(), 1)]
    );

    s.touch_scout_polled_at("2026-11-15T14:00:00Z").unwrap();
    assert_eq!(
        s.scout_settings()
            .unwrap()
            .unwrap()
            .last_polled_at
            .as_deref(),
        Some("2026-11-15T14:00:00Z")
    );
}

#[test]
fn source_summaries_aggregate_counts_and_latest() {
    let s = store();
    ensure_settings(&s);
    let mut a = finding(1, "new", "a");
    a.source_id = "github-dart".into();
    a.detected_at = "2026-11-16T10:00:00Z".into();
    let mut b = finding(2, "changed", "b");
    b.source_id = "github-dart".into();
    b.detected_at = "2026-11-16T12:00:00Z".into();
    let mut c = finding(3, "gone", "c");
    c.source_id = "cws-watchlist".into();
    c.detected_at = "2026-11-15T08:00:00Z".into();
    s.upsert_findings(&[a, b, c]).unwrap();

    // 标 c 已读：摘要计数含已读，unread 单独给。
    s.mark_findings_read(&[3]).unwrap();

    let sums = s.scout_source_summaries().unwrap();
    assert_eq!(sums.len(), 2);
    // 未读多的源排前面。
    assert_eq!(sums[0].source_id, "github-dart");
    assert_eq!(sums[0].unread, 2);
    assert_eq!(sums[0].new_count, 1);
    assert_eq!(sums[0].changed_count, 1);
    assert_eq!(sums[0].gone_count, 0);
    assert_eq!(sums[0].total, 2);
    assert_eq!(
        sums[0].latest_detected_at.as_deref(),
        Some("2026-11-16T12:00:00Z")
    );

    assert_eq!(sums[1].source_id, "cws-watchlist");
    assert_eq!(sums[1].unread, 0);
    assert_eq!(sums[1].gone_count, 1);
    assert_eq!(sums[1].total, 1);
}

#[test]
fn source_summaries_empty_mirror() {
    let s = store();
    ensure_settings(&s);
    assert!(s.scout_source_summaries().unwrap().is_empty());
}

#[test]
fn activity_orders_by_detection_time_with_stable_ties() {
    let s = store();
    let mut latest = finding(1, "new", "latest");
    latest.detected_at = "2026-11-17T10:00:00Z".into();
    s.upsert_findings(&[
        latest,
        finding(2, "changed", "older"),
        finding(3, "gone", "tie"),
    ])
    .unwrap();
    let first = s.list_scout_findings(None, None, false, 2, 0).unwrap();
    assert_eq!(first.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 3]);
    assert_eq!(
        s.list_scout_findings(None, None, false, 2, 2).unwrap()[0].id,
        2
    );
}
