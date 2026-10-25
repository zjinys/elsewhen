//! R4-6 传输方式落表：续传、回执、关闭只认 `sync_snapshot_exports` 行里的
//! `transport` / `blob_index` / `blob_id` / `blob_clock`，**不重读环境变量**。
//!
//! 为什么不能运行时嗅探：一份导出在途时可能跨进程重启、跨多个 tick，回执也可能
//! 几天后才到。发送端按 mail 推、接收端却按 blob 拉，或中途换传输方式，都会让
//! 「这份快照到底在哪」变成不可判定——快照是唯一会整体替换数据库的路径。

use super::*;
use crate::storage::sync::delivery::{BeginExport, SnapshotTransport};

/// 建一份「在途 blob 导出」：开建 + 记 blob 身份 + 推进到第 3 片。
fn blob_building(db: &Database, id: &str) {
    match db
        .store
        .begin_snapshot_export(id, "b", 1, SnapshotTransport::Blob)
        .unwrap()
    {
        BeginExport::Begun(transport) => assert_eq!(transport, SnapshotTransport::Blob),
        other => panic!("必须全新开建，实际 {other:?}"),
    }
    db.store
        .set_snapshot_blob_id(id, "0123456789abcdef01234567")
        .unwrap();
    db.store.mark_snapshot_blob_progress(id, 3).unwrap();
}

/// 续传读行：同一导出再次 begin 必须回到 `blob_index=3` / 原 blob_id，
/// 而不是从环境变量重新判断、从 0 重来。
#[test]
fn resume_follows_the_row_not_the_env() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    blob_building(&a, "checkpoint-x");

    match a
        .store
        .begin_snapshot_export("checkpoint-x", "b", 1, SnapshotTransport::Blob)
        .unwrap()
    {
        BeginExport::Resume {
            transport,
            blob_index,
            blob_id,
        } => {
            assert_eq!(transport, SnapshotTransport::Blob);
            assert_eq!(blob_index, 3, "必须从已传完的片数继续，不重传");
            assert_eq!(
                blob_id.as_deref(),
                Some("0123456789abcdef01234567"),
                "续传必须复用同一个 blob，不新开（旧的开新号会靠 TTL 收）"
            );
        }
        other => panic!("在途 blob 导出必须续传，实际 {other:?}"),
    }
}

/// 环境变量切回 mail：绝不允许把在途 blob 导出静默降级成 mail 续传 ——
/// 那等于说「之前传出去的分片可以不算数」。切回 mail 的方式是下一次快照
/// （或重新配对），本轮导出要么继续 blob、要么报错重开。
#[test]
fn env_flip_to_mail_cannot_resume_a_blob_export() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    blob_building(&a, "checkpoint-y");

    let error = a
        .store
        .begin_snapshot_export("checkpoint-y", "b", 1, SnapshotTransport::Mail)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("sync_snapshot_export_requires_new_request"),
        "传输方式冲突必须报错而不是静默切换：{error}"
    );
}

/// blob_clock 漂移被拒：源数据动了一格（logical_clock 前进），旧导出请求 id
/// 就不能再续传 —— 半新半旧的分片拼出来 manifest 摘要对不上，接收端会整份作废。
#[test]
fn blob_clock_drift_blocks_resume() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    blob_building(&a, "checkpoint-z");

    // 源数据前进一格：capture 推进 sync_replica.logical_clock。
    a.store
        .submit_input_as_event(&"drift after begin", "capture", None)
        .unwrap();

    let error = a
        .store
        .begin_snapshot_export("checkpoint-z", "b", 1, SnapshotTransport::Blob)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("sync_snapshot_export_requires_new_request"),
        "blob_clock 漂移必须拒绝续传：{error}"
    );
}

/// mail 传输本来就没有「同 id 续传」：中断的 mail 导出必须换新请求 id（旧语义，
/// 防重放/防半份）。这条守住「只有 blob 允许续传」的分界线。
#[test]
fn mail_export_never_resumes_under_same_id() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    match a
        .store
        .begin_snapshot_export("checkpoint-m", "b", 1, SnapshotTransport::Mail)
        .unwrap()
    {
        BeginExport::Begun(transport) => assert_eq!(transport, SnapshotTransport::Mail),
        other => panic!("必须全新开建，实际 {other:?}"),
    }
    let error = a
        .store
        .begin_snapshot_export("checkpoint-m", "b", 1, SnapshotTransport::Mail)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("sync_snapshot_export_requires_new_request"),
        "mail 导出同 id 二次 begin 必须要求新请求：{error}"
    );
}

/// abort 之后同一个 id 必须还能续传（review 2026-10-25 M1）。
///
/// 修前这里是死锁：`stage_snapshot_inner` 任何错误都 `abort_snapshot_export`，
/// 而 `claim_sync_resnapshot` 的决策是「resume the SAME request id」——
/// abort 后就再也没法用同一个 id begin，每拍都炸 `requires_new_request`，
/// 从 `network.rs` 的 `?` 一路撞穿整拍。`blob_index`/`blob_id`/`blob_clock`
/// 三列白设计，一次网络抖动就能让重快照永久卡死。
///
/// 为什么放行是安全的：`abort_snapshot_export` 只作用于 `building`，所以
/// `aborted` 恒等于「建过但没建成」；三列在 abort 时未被触碰，续传点最终由
/// `blob_open` 与 relay 事实对齐（blob 没了就从 0 重传），不会跳过任何片。
#[test]
fn aborted_blob_export_resumes_under_same_id() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    blob_building(&a, "checkpoint-abort");
    a.store.abort_snapshot_export("checkpoint-abort").unwrap();
    let aborted: String = a
        .store
        .connection
        .query_row(
            "SELECT state FROM sync_snapshot_exports WHERE id='checkpoint-abort'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(aborted, "aborted");

    match a
        .store
        .begin_snapshot_export("checkpoint-abort", "b", 1, SnapshotTransport::Blob)
        .unwrap()
    {
        BeginExport::Resume {
            transport,
            blob_index,
            blob_id,
        } => {
            assert_eq!(transport, SnapshotTransport::Blob);
            assert_eq!(blob_index, 3, "abort 不能抹掉已传片数");
            assert_eq!(blob_id.as_deref(), Some("0123456789abcdef01234567"));
        }
        other => panic!("aborted 的 blob 导出必须能同 id 续传，实际 {other:?}"),
    }
    // 续传后状态摆回 building：否则再出错时 abort 变成空操作、发布也过不去。
    let resumed: String = a
        .store
        .connection
        .query_row(
            "SELECT state FROM sync_snapshot_exports WHERE id='checkpoint-abort'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(resumed, "building");
}

/// 放行 `aborted` 只针对 blob：mail 的中断导出仍必须换新请求 id。
///
/// mail 没有逐片 durable 的进度（`blob_index` 恒 0），"续传"只能是重发整份，
/// 那就不是续传而是新请求 —— 旧语义必须原样保留。
#[test]
fn aborted_mail_export_still_requires_new_request() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    match a
        .store
        .begin_snapshot_export("checkpoint-abort-mail", "b", 1, SnapshotTransport::Mail)
        .unwrap()
    {
        BeginExport::Begun(transport) => assert_eq!(transport, SnapshotTransport::Mail),
        other => panic!("必须全新开建，实际 {other:?}"),
    }
    a.store
        .abort_snapshot_export("checkpoint-abort-mail")
        .unwrap();
    let error = a
        .store
        .begin_snapshot_export("checkpoint-abort-mail", "b", 1, SnapshotTransport::Mail)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("sync_snapshot_export_requires_new_request"),
        "abort 过的 mail 导出仍必须要求新请求：{error}"
    );
}
