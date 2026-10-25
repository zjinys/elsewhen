//! 「必表态」计数与审计（R5-2）。
//!
//! 这些断言锁的是**判定口径**，不是实现细节：
//! - 同一封邮件每被扫一次拒绝就 +1，跨进程重启（重新开库）仍然累加；
//! - 表态之后（`dropped_at` 非空）不再参与阈值判定，但行留着做审计；
//! - 接受成功就整行消失（它已经 ack，不该留下任何计数）；
//! - 超过 retention 的旧行被清掉（治「为治无限增长又添一张无限增长的表」）。

use super::*;

fn rejection_count(store: &Store, mailbox: &str, mail_id: &str) -> Option<(i64, Option<i64>)> {
    store
        .connection
        .query_row(
            "SELECT attempts, dropped_at FROM sync_wire_rejections WHERE mail_id=?1 AND mailbox_id=?2",
            rusqlite::params![mail_id, mailbox],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .unwrap()
}

#[test]
fn attempts_accumulate_across_reopen_and_dropping_keeps_the_audit_row() {
    let dir = std::env::temp_dir().join(format!("elsewhen-rejections-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.db");
    let now = 1_700_000_000_000_i64;
    {
        let store = Store::open(&path).unwrap();
        // 第一次拒绝：新建行。
        assert_eq!(
            store
                .record_wire_rejection(
                    "mb-owner",
                    "mb-sender",
                    "0000000000000042",
                    "snapshot-start:abc",
                    "sync_relay_payload_invalid",
                    now,
                )
                .unwrap(),
            1
        );
        // 第二次：累加。
        assert_eq!(
            store
                .record_wire_rejection(
                    "mb-owner",
                    "mb-sender",
                    "0000000000000042",
                    "",
                    "sync_relay_payload_invalid",
                    now + 1_000,
                )
                .unwrap(),
            2
        );
    }
    // 重新开库：计数必须还在（放内存的话每次重启都从头数，阈值永远到不了）。
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store
            .record_wire_rejection(
                "mb-owner",
                "mb-sender",
                "0000000000000042",
                "",
                "sync_relay_payload_invalid",
                now + 2_000,
            )
            .unwrap(),
        3
    );
    // message_id 只有第一次记得住：后续拒绝拿不到信封头（读不出明文），
    // 不能把审计里的业务 id 洗成空串。
    let (attempts, dropped_at) = rejection_count(&store, "mb-owner", "0000000000000042").unwrap();
    assert_eq!(attempts, 3);
    assert_eq!(dropped_at, None, "还没表态，不能被当成已经放弃");

    let store = Store::open(&path).unwrap();
    store
        .mark_wire_rejection_dropped("mb-owner", "0000000000000042", now + 3_000)
        .unwrap();
    // 幂等：重复表态不覆盖首次时间（审计要的是「什么时候放弃的」）。
    store
        .mark_wire_rejection_dropped("mb-owner", "0000000000000042", now + 9_000)
        .unwrap();
    let (_, dropped_at) = rejection_count(&store, "mb-owner", "0000000000000042").unwrap();
    assert_eq!(dropped_at, Some(now + 3_000));

    let store = Store::open(&path).unwrap();
    // 接受成功 = 整行消失。
    store
        .forget_wire_rejection("mb-owner", "0000000000000042")
        .unwrap();
    assert_eq!(rejection_count(&store, "mb-owner", "0000000000000042"), None);
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn prune_only_removes_rows_older_than_the_relay_ttl() {
    let db = Database::new("a");
    let now = 1_700_000_000_000_i64;
    let day = 24 * 60 * 60 * 1000_i64;
    for (id, seen) in [("1", now - 8 * day), ("2", now - 1 * day)] {
        db.store
            .record_wire_rejection("mb", "mb2", id, "", "sync_relay_payload_invalid", seen)
            .unwrap();
    }
    db.store.prune_wire_rejections(now).unwrap();
    assert_eq!(
        rejection_count(&db.store, "mb", "1"),
        None,
        "超过 7 天 TTL 的审计行清掉"
    );
    assert_eq!(
        rejection_count(&db.store, "mb", "2").map(|(a, _)| a),
        Some(1),
        "还在 TTL 内的行必须留着继续计数"
    );
}

/// 拒收计数必须按**信箱**分别算（review m5）。
///
/// relay 的信箱是一个信箱一个 SQLite 文件，`mail_id` 只是那个文件里的 rowid：
/// 重新配对换到新信箱之后，新信箱的第 1 封和旧信箱的第 1 封是同一个字符串。
/// 主键只有 `mail_id` 时，新信箱里一封普通信件会继承旧信箱那封坏信的拒绝次数，
/// 数到三次就被 drop —— 一次重新配对足以让好信被当成毒信丢掉。
#[test]
fn rejection_counters_are_scoped_to_the_mailbox() {
    let db = Database::new("a");
    let now = 1_700_000_000_000_i64;
    for round in 0..3 {
        db.store
            .record_wire_rejection(
                "mb-old",
                "mb-sender",
                "1",
                "",
                "sync_relay_payload_invalid",
                now + round,
            )
            .unwrap();
    }
    // 换信箱后新信箱的第 1 封：计数从 1 重新开始。
    assert_eq!(
        db.store
            .record_wire_rejection(
                "mb-new",
                "mb-sender",
                "1",
                "",
                "sync_relay_payload_invalid",
                now,
            )
            .unwrap(),
        1,
        "新信箱的第一封不能继承旧信箱的拒收次数"
    );
    // 旧信箱的审计行原样保留（它是「谁在什么时候放弃的」的唯一记录）。
    assert_eq!(
        rejection_count(&db.store, "mb-old", "1").map(|(a, _)| a),
        Some(3)
    );
    // 表态与遗忘同样只作用于自己那个信箱。
    db.store
        .mark_wire_rejection_dropped("mb-old", "1", now + 10)
        .unwrap();
    assert_eq!(
        rejection_count(&db.store, "mb-old", "1").unwrap().1,
        Some(now + 10)
    );
    assert_eq!(
        rejection_count(&db.store, "mb-new", "1").unwrap().1,
        None,
        "旧信箱的表态不能把新信箱的信标记成已放弃"
    );
    db.store.forget_wire_rejection("mb-new", "1").unwrap();
    assert_eq!(rejection_count(&db.store, "mb-new", "1"), None);
    assert_eq!(
        rejection_count(&db.store, "mb-old", "1").map(|(a, _)| a),
        Some(3),
        "清掉新信箱的行不能删掉旧信箱的审计"
    );
}

/// 已经表态放弃的行是**审计**，后续扫描不得改写它（review m5 后半）。
///
/// 否则「几次之后放弃」这个事实会被下一次扫描悄悄抹掉：计数复活、`last_error`
/// 被换成新值，而「已经 drop 过」的结论还在（`dropped_at` 非空），
/// 于是审计里的原因和次数与实际发生的事对不上。
#[test]
fn dropped_rows_are_audit_only_and_no_longer_updated() {
    let db = Database::new("a");
    let now = 1_700_000_000_000_i64;
    db.store
        .record_wire_rejection("mb", "mb2", "7", "snapshot-start:abc", "first", now)
        .unwrap();
    db.store
        .mark_wire_rejection_dropped("mb", "7", now + 1_000)
        .unwrap();
    let attempts = db
        .store
        .record_wire_rejection("mb", "mb2", "7", "", "second", now + 2_000)
        .unwrap();
    assert_eq!(attempts, 1, "表态之后计数不再变化");
    let (attempts, dropped_at): (i64, Option<i64>) = db
        .store
        .connection
        .query_row(
            "SELECT attempts, dropped_at FROM sync_wire_rejections WHERE mail_id='7' AND mailbox_id='mb'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((attempts, dropped_at), (1, Some(now + 1_000)));
    let error: String = db
        .store
        .connection
        .query_row(
            "SELECT last_error FROM sync_wire_rejections WHERE mail_id='7' AND mailbox_id='mb'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(error, "first", "放弃时记下的原因不能被后续扫描改写");
    let message_id: String = db
        .store
        .connection
        .query_row(
            "SELECT message_id FROM sync_wire_rejections WHERE mail_id='7' AND mailbox_id='mb'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(message_id, "snapshot-start:abc");
}
