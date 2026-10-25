//! `snapshot-start` 的退役语义（review 2026-10-25 B1）。
//!
//! 这一组用例守的是「信令必须能退掉」：`snapshot-start` 不登记
//! `sync_snapshot_export_wires`，所以 `acknowledge_snapshot_export` 永远退不到它；
//! 在修复前它既不会因为工作完成而消失，也会把「已发布」当错误拒收 —— 结果是
//! 每分钟重投一次、被 drop、再重投的无限循环，而面板永久显示「等回执」。
//!
//! 因此这里断言两件事：
//! 1. **重复通知不是错误**（`applied` / `aborted` 都返回 `Ok`，且一行都不改）；
//! 2. **回执能真的把它退掉**（outbox 行变 `acknowledged`，该对端不再出现在状态表里）。

use super::*;

const BLOB_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaa";
const BLOB_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbb";

/// 造一份「b 推给本机」的快照起始状态。
fn begin(db: &Database, id: &str, blob_id: &str) -> anyhow::Result<()> {
    db.store.begin_blob_snapshot_import(id, "b", 1, blob_id)
}

fn state_of(db: &Database, id: &str) -> String {
    db.store
        .connection
        .query_row("SELECT state FROM sync_snapshots WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap()
}

fn blob_of(db: &Database, id: &str) -> Option<String> {
    db.store
        .connection
        .query_row("SELECT blob_id FROM sync_snapshots WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap()
}

/// 造一对已互相授权的库（本机是 `a`，对端是 `b`）。
fn paired() -> (Database, Database) {
    let local = Database::new("a");
    let remote = Database::new("b");
    pair(&local, &remote);
    (local, remote)
}

/// 已发布的快照再收到 start：必须当「已收口」，不能报错。
///
/// 报错这条路上游是 `record_wire_rejection`：同一个 `mail_id` 拒收三次就被 drop，
/// 发送端再发又是一封新 `mail_id`、计数归零 —— 这条路一旦打开就是无限循环。
#[test]
fn start_reannounce_after_publish_is_idempotent() {
    let (local, _remote) = paired();
    begin(&local, "snap", BLOB_A).unwrap();
    // 模拟发布完成：state 由导入路径置 applied，这里直接改列即可 ——
    // 本用例关心的是「已收口之后再来的 start」，与发布细节无关。
    local
        .store
        .connection
        .execute("UPDATE sync_snapshots SET state='applied' WHERE id='snap'", [])
        .unwrap();

    begin(&local, "snap", BLOB_A).expect("已发布的快照再收到 start 必须当成功");
    assert_eq!(state_of(&local, "snap"), "applied", "不得把已发布改回 staging");
    assert_eq!(blob_of(&local, "snap").as_deref(), Some(BLOB_A));
}

/// 已作废的快照再收到 start：同样当「已收口」。
///
/// 走不到这条早退分支的话，`begin_snapshot_import` 会对 `aborted` 报
/// `sync_snapshot_identity_conflict` —— 又是一个「永远拒收」的入口。
#[test]
fn start_reannounce_after_abort_is_idempotent() {
    let (local, _remote) = paired();
    begin(&local, "snap", BLOB_A).unwrap();
    local
        .store
        .connection
        .execute("UPDATE sync_snapshots SET state='aborted' WHERE id='snap'", [])
        .unwrap();

    begin(&local, "snap", BLOB_A).expect("已作废的快照再收到 start 必须当成功");
    assert_eq!(state_of(&local, "snap"), "aborted");
}

/// staging 阶段重复通知仍然要能过（上传方每拍重开 exporter 都会重发 start）。
#[test]
fn start_reannounce_while_staging_is_accepted() {
    let (local, _remote) = paired();
    begin(&local, "snap", BLOB_A).unwrap();
    begin(&local, "snap", BLOB_A).unwrap();
    assert_eq!(state_of(&local, "snap"), "staging");
    assert_eq!(blob_of(&local, "snap").as_deref(), Some(BLOB_A));
}

/// 同一 id 换一个 blob_id 是真冲突：blob 身份一旦定下就不许改，
/// 改成另一个 blob 等于把已收的分片丢掉（把幂等做成「宽容」会把这条也吞掉）。
#[test]
fn start_with_different_blob_id_still_conflicts() {
    let (local, _remote) = paired();
    begin(&local, "snap", BLOB_A).unwrap();

    let err = begin(&local, "snap", BLOB_B).unwrap_err().to_string();
    assert!(
        err.contains("sync_snapshot_blob_conflict"),
        "换 blob_id 必须报 sync_snapshot_blob_conflict，实际 {err}"
    );

    // 已发布的快照也不许换 blob_id。
    local
        .store
        .connection
        .execute("UPDATE sync_snapshots SET state='applied' WHERE id='snap'", [])
        .unwrap();
    let err = begin(&local, "snap", BLOB_B).unwrap_err().to_string();
    assert!(err.contains("sync_snapshot_blob_conflict"), "实际 {err}");
}

/// 非成员发来的 start 不许过（早退分支不能绕开授权）。
#[test]
fn start_from_non_member_is_rejected() {
    let (local, _remote) = paired();
    begin(&local, "snap", BLOB_A).unwrap();
    local
        .store
        .connection
        .execute("UPDATE sync_snapshots SET state='applied' WHERE id='snap'", [])
        .unwrap();

    let err = local
        .store
        .begin_blob_snapshot_import("snap", "intruder", 1, BLOB_A)
        .unwrap_err()
        .to_string();
    // 先把「身份不符」与「非成员」分开：这条路径上两者都该拒，
    // 但只有后者能证明早退分支补上了 `snapshot_source_authorized`。
    assert!(
        err.contains("sync_snapshot_identity_conflict")
            || err.contains("sync_snapshot_source_not_authorized"),
        "非成员发来的 start 必须被拒，实际 {err}"
    );
    let err = local
        .store
        .begin_blob_snapshot_import("snap", "b", 9, BLOB_A)
        .unwrap_err()
        .to_string();
    assert!(err.contains("sync_snapshot_identity_conflict"), "实际 {err}");
}

/// 回执真的能退掉 start 行 —— 这是 B1 的收口动作。
///
/// 断言链条：start 行在 outbox 里（`Uploading`）→ 收到回执后
/// `acknowledge_sync_wire` 把它变 `acknowledged` → 该对端不再出现在状态表里
/// （`deliver_statuses` 只统计 `state='pending'`，没有行 = 无事可做 = 空闲）。
#[test]
fn start_receipt_retires_outbox_row() {
    use crate::sync::engine::deliver::DeliverState;

    let (local, _remote) = paired();
    begin(&local, "snap", BLOB_A).unwrap();

    let message_id = "snapshot-start:snap";
    let payload_hash = "hash-of-start-plaintext";
    local
        .store
        .connection
        .execute(
            "INSERT INTO sync_wire_outbox(recipient_device_id,message_id,payload_hash,key_version,encrypted_envelope,attempts,available_at_ms,seq_hint) \
             VALUES('b',?1,?2,1,x'00',0,0,1)",
            params![message_id, payload_hash],
        )
        .unwrap();
    let now = 1_700_000_000_000_i64;
    let state_now = |db: &Database| {
        db.store
            .deliver_statuses(now)
            .unwrap()
            .into_iter()
            .find(|s| s.peer_device_id == "b")
            .map(|s| s.state)
    };
    assert_eq!(state_now(&local), Some(DeliverState::Uploading));

    // 接收端排回执（必须在事务里），发送端收到回执后走通用 ack 路径。
    let tx = local.store.begin_write_transaction().unwrap();
    local
        .store
        .queue_sync_wire_receipt("b", message_id, payload_hash)
        .unwrap();
    tx.unwrap().commit().unwrap();
    local
        .store
        .acknowledge_sync_wire("b", message_id, payload_hash)
        .unwrap();

    let state: String = local
        .store
        .connection
        .query_row(
            "SELECT state FROM sync_wire_outbox WHERE recipient_device_id='b' AND message_id=?1",
            [message_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "acknowledged", "start 行必须能被回执退掉");
    assert_eq!(
        state_now(&local),
        None,
        "退完就不该再有一行说「等回执」—— 这正是修复前永远为真的那句假话"
    );
}
