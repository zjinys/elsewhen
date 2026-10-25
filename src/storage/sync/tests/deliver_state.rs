//! `deliver` 状态机（R6-1）与错误分类（R6-2）。
//!
//! 断言的重点是**推导口径**：状态只从 outbox 现有列算出来，不新增 state 列，
//! 因此这里逐列构造场景、逐状态断言 —— 一旦有人往表里塞第二个真相，
//! 「backoff 与时间戳自相矛盾」这种事故会立刻在这组用例里现形。

use super::*;

/// 直接插一行 pending 信封：状态机只看列，不看信封内容，所以这里不需要真密钥。
fn stage_pending(db: &Database, peer: &str, message_id: &str) {
    db.store
        .connection
        .execute(
            "INSERT INTO sync_wire_outbox(recipient_device_id,message_id,payload_hash,key_version,encrypted_envelope,attempts,available_at_ms,seq_hint) \
             VALUES(?1,?2,'hash',1,x'00',0,0,1)",
            params![peer, message_id],
        )
        .unwrap();
}

fn state_of(db: &Database, peer: &str, now_ms: i64) -> Option<crate::sync::engine::deliver::DeliverState> {
    db.store
        .deliver_statuses(now_ms)
        .unwrap()
        .into_iter()
        .find(|s| s.peer_device_id == peer)
        .map(|s| s.state)
}

#[test]
fn state_is_derived_from_outbox_columns_not_stored_twice() {
    use crate::sync::engine::deliver::{DeliverState, DeliveryFailure};
    let db = Database::new("a");
    let peer = "b";
    let now = 1_700_000_000_000_i64;

    // 无事可做 = 没有行（不是「有一行说 idle」）。
    assert!(db.store.deliver_statuses(now).unwrap().is_empty());

    stage_pending(&db, peer, "delta-1");
    assert_eq!(state_of(&db, peer, now), Some(DeliverState::Uploading));

    // 上了 relay（relay_id 非空）= 进入等回执，不再是上传中。
    db.store
        .record_sync_wire_attempt(peer, "delta-1", now, Some("0000000000000009"), None)
        .unwrap();
    let awaiting = db.store.deliver_statuses(now).unwrap();
    assert_eq!(awaiting[0].state, DeliverState::AwaitingReceipt);
    assert_eq!(awaiting[0].pending, 1);
    assert_eq!(awaiting[0].awaiting_receipt, 1);

    // 网络失败：退避中，原因可回放（这是 `last_error` 存在的原因）。
    db.store
        .record_sync_wire_attempt(peer, "delta-1", now, None, Some(DeliveryFailure::Transport))
        .unwrap();
    let until_ms = match state_of(&db, peer, now) {
        Some(DeliverState::Backoff { reason, until_ms }) => {
            assert_eq!(reason, "transport");
            assert!(until_ms > now, "退避必须指向未来，否则这一拍就会立刻重试");
            until_ms
        }
        other => panic!("期望 backoff，实际 {other:?}"),
    };
    // 时间到了就自动回到可发送：状态不是存下来的，因此不需要任何人去改它。
    assert_eq!(
        state_of(&db, peer, until_ms + 1),
        Some(DeliverState::AwaitingReceipt),
        "退避结束应回到等回执（这封信早已上过 relay）"
    );

    // 配额：blocked（要人看），且 blocked 优先于 backoff —— 同时成立时让人看到原因。
    db.store
        .block_sync_wire(peer, "delta-1", "relay_quota", now + 15_000)
        .unwrap();
    assert_eq!(
        state_of(&db, peer, now),
        Some(DeliverState::Blocked {
            reason: "relay_quota".into(),
            until_ms: now + 15_000,
        }),
        "阻塞必须给出「到点会再试」的时刻，不能只说原因"
    );

    // 回执落库（行被确认）= 回到无事可做。
    db.store
        .connection
        .execute(
            "UPDATE sync_wire_outbox SET state='acknowledged' WHERE recipient_device_id=?1",
            [peer],
        )
        .unwrap();
    assert!(db.store.deliver_statuses(now).unwrap().is_empty());
}

#[test]
fn one_row_per_peer_with_the_worst_state_winning() {
    use crate::sync::engine::deliver::{DeliverState, DeliveryFailure};
    let db = Database::new("a");
    let now = 1_700_000_000_000_i64;
    stage_pending(&db, "b", "delta-1");
    stage_pending(&db, "b", "delta-2");
    stage_pending(&db, "c", "delta-3");
    db.store
        .record_sync_wire_attempt("b", "delta-1", now, Some("0000000000000001"), None)
        .unwrap();
    db.store
        .record_sync_wire_attempt("b", "delta-2", now, None, Some(DeliveryFailure::Transport))
        .unwrap();
    let statuses = db.store.deliver_statuses(now).unwrap();
    // 每个对端一行：面板要的是「这个设备现在怎么样」，不是逐封的信箱视图。
    assert_eq!(statuses.len(), 2);
    let b = statuses.iter().find(|s| s.peer_device_id == "b").unwrap();
    assert_eq!(b.pending, 2);
    assert_eq!(b.awaiting_receipt, 1);
    match &b.state {
        DeliverState::Backoff { .. } => {}
        other => panic!("有封在退避就整体显示 backoff，实际 {other:?}"),
    }
    assert_eq!(
        statuses
            .iter()
            .find(|s| s.peer_device_id == "c")
            .unwrap()
            .state,
        DeliverState::Uploading
    );
}

/// 成功之后必须清掉 `last_error`（review 2026-10-25 m1）。
///
/// `last_error` 是 backoff 判定的**半个条件**（另一半是 `available_at_ms > now`）。
/// 成功路径以前写的是 `CASE WHEN ?5='' THEN last_error ELSE ?5 END` —— 成功传空串，
/// 于是旧分类码永远留着；下一次失败之前的那段「已送出、等回执」会被算成
/// `Backoff{旧原因}`，面板显示「正在重试（上次的原因）」，而其实什么也没在重试。
#[test]
fn last_error_clears_on_success() {
    use crate::sync::engine::deliver::{DeliverState, DeliveryFailure};
    let db = Database::new("a");
    let peer = "b";
    let now = 1_700_000_000_000_i64;

    stage_pending(&db, peer, "delta-1");
    db.store
        .record_sync_wire_attempt(peer, "delta-1", now, None, Some(DeliveryFailure::Transport))
        .unwrap();
    assert!(matches!(
        state_of(&db, peer, now),
        Some(DeliverState::Backoff { .. })
    ));

    // 成功：上 relay、无失败。
    db.store
        .record_sync_wire_attempt(peer, "delta-1", now, Some("0000000000000002"), None)
        .unwrap();
    let last_error: String = db
        .store
        .connection
        .query_row(
            "SELECT last_error FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id='delta-1'",
            [peer],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(last_error, "", "成功必须清空 last_error");
    assert_eq!(
        state_of(&db, peer, now),
        Some(DeliverState::AwaitingReceipt),
        "刚送出、还没到点重发，不是退避"
    );
}

/// 重发间隔的上限是 5 分钟，不是 60 秒（review 2026-10-25 B1 的兜底）。
///
/// 成功路径的唯一含义就是「这封信已在中继上」；正常退役靠对端回执，
/// 重发只兜「响应丢了」。60 s 对一条本就不该重发的信令太密。
#[test]
fn resend_spacing_is_capped_at_five_minutes() {
    use crate::sync::engine::deliver::resend_spacing_ms;
    assert_eq!(resend_spacing_ms(0), 1_000);
    assert_eq!(resend_spacing_ms(3), 8_000);
    assert_eq!(resend_spacing_ms(20), 300_000, "上限必须是 5 分钟");
}

/// 配额阻塞之后又撞上别的错：面板必须报**最近一次**事实，不能一直挂着旧账。
///
/// 改前 `blocked_reason` 只在成功时清除，于是一旦 507 过一次，面板就永远说
/// 「中继信箱已满」——哪怕配额早就回落、当下真正的病是网络（m2）。
#[test]
fn quota_then_transport_shows_transport_not_quota() {
    use crate::sync::engine::deliver::{DeliverState, DeliveryFailure};
    let db = Database::new("a");
    let now = 1_700_000_000_000_i64;
    stage_pending(&db, "b", "delta-1");
    db.store
        .block_sync_wire("b", "delta-1", "relay_quota", now + 15_000)
        .unwrap();
    assert!(matches!(
        state_of(&db, "b", now),
        Some(DeliverState::Blocked { .. })
    ));

    // 15 秒后真去试，这次撞的是 5xx：新事实覆盖旧原因。
    db.store
        .record_sync_wire_attempt("b", "delta-1", now + 15_001, None, Some(DeliveryFailure::Transport))
        .unwrap();
    match state_of(&db, "b", now + 15_002) {
        Some(DeliverState::Backoff { reason, .. }) => assert_eq!(reason, "transport"),
        other => panic!("最近一次失败是网络错，面板必须报 transport，实际 {other:?}"),
    }
}

/// 重封一封已被阻塞的信：旧的阻塞原因属于**上一份 payload**，必须清掉。
///
/// 不清的后果很具体：换了内容重封、新内容其实发得出去，面板却还在说
/// 「中继信箱已满」；而 `deliver_statuses` 的 blocked 分支优先于一切，
/// 这一行会把整个对端显示成一直卡住（m3）。
#[test]
fn restage_clears_blocked_reason() {
    use crate::sync::crypto::{ContentKey, DeviceIdentity, EnvelopeHeader};
    use crate::sync::engine::deliver::DeliverState;

    let db = Database::new("a");
    db.store.authorize_sync_member("b", "stream-b").unwrap();
    let now = 1_700_000_000_000_i64;
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let stage = |payload: &[u8]| {
        db.store
            .stage_sync_wire_envelope(
                &EnvelopeHeader::new("test-dataset", "a", "b", 1, "op:restage").unwrap(),
                &key,
                &signer,
                payload,
            )
            .unwrap();
    };
    stage(b"first payload");
    db.store
        .block_sync_wire("b", "op:restage", "relay_quota", now + 15_000)
        .unwrap();
    assert!(matches!(
        state_of(&db, "b", now),
        Some(DeliverState::Blocked { .. })
    ));

    stage(b"second payload");
    assert_eq!(
        state_of(&db, "b", now),
        Some(DeliverState::Uploading),
        "重封之后这封信根本没上过 relay，不该再显示成阻塞"
    );
    let (blocked, last_error): (Option<String>, String) = db
        .store
        .connection
        .query_row(
            "SELECT blocked_reason,last_error FROM sync_wire_outbox WHERE recipient_device_id='b' AND message_id='op:restage'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(blocked, None, "旧的阻塞原因必须清空");
    assert_eq!(last_error, "", "旧的分类码同样属于上一份内容");
}
