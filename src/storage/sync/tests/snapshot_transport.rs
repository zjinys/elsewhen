//! 快照传输的续传语义：续传点只认 `sync_snapshot_exports` 行里的
//! `blob_index` / `blob_id` / `blob_clock`。
//!
//! 为什么必须落表而不是「拍脑袋从 0 重来」：一份导出在途时会跨进程重启、跨多个
//! tick，回执也可能几天后才到。快照是唯一会整体替换数据库的路径，重传整份 121 MB
//! 既慢又会在 relay 上留下整份副本。
//!
//! 曾经这里还有一组「mail ↔ blob 不许互相续传」的用例；mail 分片传输删除后
//! （2026-10-26 C5-A）那组用例失去了对象 —— `transport` 列只能写 `'blob'`，
//! 由迁移 64 的触发器保证（见 `storage::new_migrations_tests`）。

use super::*;
use crate::storage::sync::delivery::BeginExport;

/// 建一份「在途 blob 导出」：开建 + 记 blob 身份 + 推进到第 3 片。
fn blob_building(db: &Database, id: &str) {
    match db.store.begin_snapshot_export(id, "b", 1).unwrap() {
        BeginExport::Begun => {}
        other => panic!("必须全新开建，实际 {other:?}"),
    }
    db.store
        .set_snapshot_blob_id(id, "0123456789abcdef01234567")
        .unwrap();
    db.store.mark_snapshot_blob_progress(id, 3).unwrap();
}

/// 续传读行：同一导出再次 begin 必须回到 `blob_index=3` / 原 blob_id，
/// 而不是从 0 重来。
#[test]
fn resume_follows_the_row() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    blob_building(&a, "checkpoint-x");

    match a
        .store
        .begin_snapshot_export("checkpoint-x", "b", 1)
        .unwrap()
    {
        BeginExport::Resume {
            blob_index,
            blob_id,
        } => {
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
        .begin_snapshot_export("checkpoint-z", "b", 1)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("sync_snapshot_export_requires_new_request"),
        "blob_clock 漂移必须拒绝续传：{error}"
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
fn aborted_export_resumes_under_same_id() {
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
        .begin_snapshot_export("checkpoint-abort", "b", 1)
        .unwrap()
    {
        BeginExport::Resume {
            blob_index,
            blob_id,
        } => {
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
