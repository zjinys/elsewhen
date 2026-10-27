//! blob 数据面的端到端验证：发送端把快照分片直接 PUT 到 relay 的 blob 上，
//! 接收端边拉边删（逐窗 ack），manifest 最后才到。
//!
//! 这一层验证的正是「窗口流控」，信箱路径永远试不出来：
//! - relay 上的 `blob_bytes` 峰值必须是**一窗 + 少量余量**，而不是整份快照；
//! - close 之后 relay 上一条不剩（残留 0）；
//! - 分片密文一律不进 outbox，`sync_wire_outbox` 里最多只有一条信令。

use super::key;
use crate::storage::Store;
use crate::storage::sync::ReplicaIdentity;
use crate::sync::{
    crypto::{ContentKey, DeviceIdentity},
    engine,
    engine::snapshot::{BlobDownload, BlobUpload},
    transport_relay::{RelayClient, RelaySource, WINDOW_CHUNKS},
};

/// relay 自己的 blob 账（`/status` 是 owner 签名操作）。峰值断言只能看 relay 的
/// 实际占盘，不能看客户端「打算传多少」。
fn blob_ledger(client: &RelayClient, owner: &[u8; 32], mailbox: &str) -> (i64, i64) {
    let status = client.status(mailbox, owner).unwrap();
    (status.blob_bytes, status.blobs)
}

/// 发送端那份导出的状态（`building` / `ready` / `acknowledged` / `aborted`）。
fn export_state(store: &Store, id: &str) -> String {
    store
        .connection
        .query_row(
            "SELECT state FROM sync_snapshot_exports WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
}

pub(super) fn roundtrip(client: &RelayClient, directory: &std::path::Path) {
    let source = Store::open(&directory.join("blob-snapshot-source.db")).unwrap();
    let target = Store::open(&directory.join("blob-snapshot-target.db")).unwrap();
    for (store, device) in [(&source, "source"), (&target, "target")] {
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "blob-snapshot-dataset".into(),
                device_id: device.into(),
                stream_id: format!("stream-{device}"),
                processor_device_id: "source".into(),
            })
            .unwrap();
    }
    source
        .authorize_sync_member("target", "stream-target")
        .unwrap();
    target
        .authorize_sync_member("source", "stream-source")
        .unwrap();
    let signer_a = DeviceIdentity::generate().unwrap();
    let content_ab = ContentKey::generate().unwrap();
    // 正文规模默认 1 条 9.5 MB 事件（73 片，远超一窗），窗口流控才会真的被触发。
    // R4 验收要跑事故同规模（174 片 ≈ 122 MB 密文）时放大：
    // `ELSEWHEN_BLOB_SNAPSHOT_EVENTS=3 ELSEWHEN_BLOB_SNAPSHOT_UNITS=400000`。
    // 单条记录上限 `MAX_RECORD_BYTES`=20 MB，所以只能加条数，不能把一条撑爆；
    // 默认值也不许调大 —— 分片全落盘，日常回归没必要写几百 MB。
    let units: usize = std::env::var("ELSEWHEN_BLOB_SNAPSHOT_UNITS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(500_000);
    let events: usize = std::env::var("ELSEWHEN_BLOB_SNAPSHOT_EVENTS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    for index in 0..events {
        let body = format!("{index}|{}", "blob snapshot body ".repeat(units));
        source
            .submit_input_as_event(&body, "capture", None)
            .unwrap();
    }

    let source_owner = key();
    let target_owner = key();
    let source_mailbox = client.create_mailbox(&source_owner).unwrap();
    let target_mailbox = client.create_mailbox(&target_owner).unwrap();
    let forward = key();
    let expiry = chrono::Utc::now().timestamp_millis() + 3_600_000;
    client
        .register_sender(
            &target_mailbox,
            &target_owner,
            &source_mailbox,
            &forward,
            expiry,
            0,
        )
        .unwrap();
    let upload = BlobUpload {
        sink: Box::new(crate::sync::transport_relay::RelaySink {
            client: client.clone(),
            mailbox: target_mailbox.clone(),
            sender_mb: source_mailbox.clone(),
            credential: forward,
        }),
        signal: Some(engine::snapshot::SignalSender {
            client: client.clone(),
            mailbox: target_mailbox.clone(),
            sender_mb: source_mailbox.clone(),
            credential: forward,
        }),
        recipient: "target".into(),
        key_version: 1,
        // 一遍「发送一拍 / 接收一拍」是串行跑的，等满 20 s 会把测试拖成分钟级。
        window_wait_ms: 300,
    };
    let source_public = signer_a.public_key();
    let source_peer = [engine::ReceivingPeer {
        device_id: "source",
        mailbox_id: &source_mailbox,
        key_version: 1,
        key: &content_ab,
        public_key: &source_public,
    }];
    let baseline = blob_ledger(client, &target_owner, &target_mailbox);
    let now = chrono::Utc::now().timestamp_millis();
    let mut peak = 0;
    let mut rounds = 0;
    for tick in 0..40 {
        rounds = tick + 1;
        // 发送端一拍：续传（第一次是全新上传）到窗口满或传完为止。
        engine::snapshot::stage_snapshot(
            &source,
            "target",
            "blob-checkpoint",
            1,
            &content_ab,
            &signer_a,
            &upload,
        )
        .unwrap();
        // outbox 里永远只该有「信令」（snapshot-start / manifest）：分片密文一律
        // 走 blob，任何一条 `snapshot:{id}:{index}` 出现在信件队列里都是回归。
        assert!(
            source
                .due_sync_wire_envelopes("target", i64::MAX, 16)
                .unwrap()
                .iter()
                .all(|wire| !wire.message_id.starts_with("snapshot:")),
            "blob 传输下分片密文不得进 outbox"
        );
        engine::push_due(
            &source,
            client,
            "target",
            &target_mailbox,
            &source_mailbox,
            &forward,
            now + tick * 60_000,
        )
        .unwrap();
        // 峰值采样点必须紧跟上传：接收端是「收一窗就 ack 一窗」，等 import 之后
        // 再读 relay 的账就永远是 0（数据已经被删了）。
        let in_flight = blob_ledger(client, &target_owner, &target_mailbox);
        peak = peak.max(in_flight.0 - baseline.0);
        let pulled = engine::pull_page(
            &target,
            client,
            &target_mailbox,
            &target_owner,
            &source_peer,
        )
        .unwrap();
        assert_eq!(pulled.rejected, 0, "快照信令不得被拒");
        assert!(!pulled.acknowledgement_failed);
        if target.snapshot_import_pending().unwrap() {
            let source = RelaySource {
                client: client.clone(),
                mailbox: target_mailbox.clone(),
                owner_credential: target_owner,
            };
            let download = BlobDownload {
                source: &source,
                from: "source",
                key_version: 1,
                key: &content_ab,
                public: &source_public,
            };
            engine::snapshot::import_blob_snapshots(&target, &download, 4).unwrap();
        }
        if !target.snapshot_import_pending().unwrap() {
            break;
        }
    }
    assert!(
        !target.snapshot_import_pending().unwrap(),
        "blob 传输必须完成整份快照导入（跑了 {rounds} 拍）"
    );
    eprintln!(
        "[blob] acceptance rounds={rounds} events={events} units={units} peak={peak} baseline={}",
        baseline.0
    );
    // `snapshot-start` 幂等：这份导出跨了 40 拍、每拍都重开一次 exporter，
    // outbox 里的「分片在哪个 blob 上」永远只该有一条 —— 每拍新增一行等于每拍
    // 刷一次信令，接收端会被重复的 start 淹掉，也让信箱账目失真。
    let starts: i64 = source
        .connection
        .query_row(
            "SELECT count(*) FROM sync_wire_outbox WHERE recipient_device_id='target' AND message_id LIKE 'snapshot-start:%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(starts, 1, "snapshot-start 必须是幂等的一条");
    // 峰值：relay 上同时只该有一窗还加一片。一片 256 KiB 明文封成信封约 683 KiB
    // （实测），加上发送端没被 ack 的那一片，取 WINDOW_CHUNKS + 1 片 × 1 MiB
    // 的硬上限 —— 断言的意义是「与片数无关，只与窗口大小有关」。
    assert!(peak > 0, "必须真的经过 blob 数据面");
    assert!(
        peak <= ((WINDOW_CHUNKS + 1) * 1024 * 1024) as i64,
        "relay 峰值必须是窗口量级而不是整份快照，实际 {peak} 字节"
    );
    // 发布成功即整块回收：relay 上的 blob 账必须回到基线（残留 0）。
    // R4-5 回执语义核对：blob 路径下有**两条**回执 —— `snapshot-start:{id}`（通知收口）
    // 与 `snapshot-end:{id}`（manifest 收口）。分片走 blob，没有逐片回执可言。
    // `snapshot-start` 的那条是 B1 修复（review 2026-10-25）的核心：它不登记
    // `sync_snapshot_export_wires`，因此 `acknowledge_snapshot_export` 永远退不到它，
    // 只能靠自己那条回执走通用 `acknowledge_sync_wire`。
    let receipts = target.pending_sync_wire_receipts("source", 16).unwrap();
    assert_eq!(receipts.len(), 2, "start 与 end 各一条回执");
    assert_eq!(receipts[0].message_id, "snapshot-end:blob-checkpoint");
    assert_eq!(
        receipts[1].message_id, "snapshot-start:blob-checkpoint",
        "已发布的快照也必须给 start 回执，否则发送端永远重投"
    );
    for receipt in &receipts {
        assert_eq!(
            receipt.acknowledging_device_id, "target",
            "回执必须由接收方签收（blob close 不是回执）"
        );
    }
    // 回执回到发送端之前，导出停在 `ready`（分片传完 + manifest 已进信箱），
    // 还不能是 `acknowledged`：blob 已经 close，只有接收方的回执能把这行收尾
    // （`acknowledge_snapshot_export` 顺便种下接收方的起始 watermark）。
    assert_eq!(export_state(&source, "blob-checkpoint"), "ready");

    let (bytes, blobs) = blob_ledger(client, &target_owner, &target_mailbox);
    assert_eq!(bytes - baseline.0, 0, "close 之后 blob_bytes 必须归零");
    assert_eq!(blobs - baseline.1, 0, "close 之后不许留下 blob 行");
    assert_eq!(target.list_events().unwrap().len(), events);
}

/// 缺口必须 `break`（停在这一拍等发送端补片），不能报错、也不能跳着拼。
///
/// 造法：relay 的 blob 上只有 20..29 片（前面的片已被回收/被 ack 掉），本地一片
/// 都没有 ⇒ 窗口 `from` 落在 20 而本地期望 0。这正是「不跳着放」的边界：宁可
/// 这一拍什么都不做，也不能把 20..29 当成 0..9 落盘——错位的片拼出来的快照
/// 摘要对不上，接收端会整份作废，而发送端还在等一个永远不会来的 ack。
pub(super) fn gap_import_breaks_without_error(client: &RelayClient, directory: &std::path::Path) {
    let target = Store::open(&directory.join("gap-target.db")).unwrap();
    let source = Store::open(&directory.join("gap-source.db")).unwrap();
    for (store, device) in [(&target, "target"), (&source, "source")] {
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "gap-dataset".into(),
                device_id: device.into(),
                stream_id: format!("stream-{device}"),
                processor_device_id: "source".into(),
            })
            .unwrap();
    }
    source
        .authorize_sync_member("target", "stream-target")
        .unwrap();
    target
        .authorize_sync_member("source", "stream-source")
        .unwrap();

    let owner = key();
    let mailbox = client.create_mailbox(&owner).unwrap();
    let sender_mb = client.create_mailbox(&key()).unwrap();
    let forward = key();
    let expiry = chrono::Utc::now().timestamp_millis() + 3_600_000;
    client
        .register_sender(&mailbox, &owner, &sender_mb, &forward, expiry, 0)
        .unwrap();
    let blob_id = crate::sync::transport_relay::random_hex(24);
    assert!(
        client
            .blob_open(&mailbox, &sender_mb, &forward, &blob_id)
            .unwrap()
            .created
    );
    // 只放尾巴：0..19 缺失。
    for index in 20..30 {
        client
            .blob_put(
                &mailbox,
                &sender_mb,
                &forward,
                &blob_id,
                index,
                b"gap-frame",
            )
            .unwrap();
    }
    target
        .begin_blob_snapshot_import("gap-checkpoint", "source", 1, &blob_id)
        .unwrap();
    let content = ContentKey::generate().unwrap();
    let public = DeviceIdentity::generate().unwrap().public_key();
    let download_source = RelaySource {
        client: client.clone(),
        mailbox: mailbox.clone(),
        owner_credential: owner,
    };
    let download = BlobDownload {
        source: &download_source,
        from: "source",
        key_version: 1,
        key: &content,
        public: &public,
    };
    assert!(
        !engine::snapshot::import_blob_snapshots(&target, &download, 4).unwrap(),
        "缺口不得发布快照"
    );
    let staged: i64 = target
        .connection
        .query_row(
            "SELECT count(*) FROM sync_snapshot_chunks WHERE snapshot_id='gap-checkpoint'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(staged, 0, "缺口上不许落盘任何一片");
    assert!(
        target.snapshot_import_pending().unwrap(),
        "导入必须保持待续状态，等发送端补片"
    );
    // 缺口这一拍也没有 ack 掉任何东西：20..29 仍在 relay 上原样等着。
    let state = client
        .blob_state(&mailbox, &blob_id, &owner, None)
        .unwrap()
        .unwrap();
    assert_eq!(state.present.len(), 10);
    assert_eq!(state.highest, Some(29));
    client
        .blob_close(&mailbox, &owner, &blob_id, "done", None)
        .unwrap();
}

/// abort 之后同一个 id 必须还能续传并真的发布（真 relay，review 2026-10-25 M1）。
///
/// 造法：第一次上传故意指向一个**连不上**的地址 —— 这正是事故里的样子（网络抖、
/// relay 重启），`blob_open` 连不上 ⇒ `stage_snapshot_inner` 把导出收成 `aborted`。
/// 第二次换真 client、同一个 id 再 stage：必须续传（不是 `requires_new_request`），
/// 真的把分片传上去并发布。
///
/// 修前这条路是死锁：`claim_sync_resnapshot` 对 aborted 的决策是「resume the SAME
/// request id」，而 `begin_snapshot_export` 只认 `building` —— 于是每拍都炸，
/// `?` 一路撞穿整拍，重快照永久卡死。
pub(super) fn aborted_export_resumes_and_publishes(
    client: &RelayClient,
    directory: &std::path::Path,
) {
    let source = Store::open(&directory.join("abort-source.db")).unwrap();
    let target = Store::open(&directory.join("abort-target.db")).unwrap();
    for (store, device) in [(&source, "source"), (&target, "target")] {
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "abort-dataset".into(),
                device_id: device.into(),
                stream_id: format!("stream-{device}"),
                processor_device_id: "source".into(),
            })
            .unwrap();
    }
    source
        .authorize_sync_member("target", "stream-target")
        .unwrap();
    target
        .authorize_sync_member("source", "stream-source")
        .unwrap();
    source
        .submit_input_as_event("aborted export resumes", "capture", None)
        .unwrap();
    let signer_a = DeviceIdentity::generate().unwrap();
    let content_ab = ContentKey::generate().unwrap();
    let source_owner = key();
    let target_owner = key();
    let source_mailbox = client.create_mailbox(&source_owner).unwrap();
    let target_mailbox = client.create_mailbox(&target_owner).unwrap();
    let forward = key();
    let expiry = chrono::Utc::now().timestamp_millis() + 3_600_000;
    client
        .register_sender(
            &target_mailbox,
            &target_owner,
            &source_mailbox,
            &forward,
            expiry,
            0,
        )
        .unwrap();

    // 第一次：指向一个必然连不上的地址（端口 1 上没人监听）。
    let unreachable = RelayClient::new("http://127.0.0.1:1").unwrap();
    let broken = BlobUpload {
        sink: Box::new(crate::sync::transport_relay::RelaySink {
            client: unreachable.clone(),
            mailbox: target_mailbox.clone(),
            sender_mb: source_mailbox.clone(),
            credential: forward,
        }),
        signal: None,
        recipient: "target".into(),
        key_version: 1,
        window_wait_ms: 200,
    };
    assert!(
        engine::snapshot::stage_snapshot(
            &source,
            "target",
            "abort-checkpoint",
            1,
            &content_ab,
            &signer_a,
            &broken,
        )
        .is_err(),
        "连不上 relay 就该失败（这一步是造 abort，不是在被测路径上）"
    );
    assert_eq!(
        export_state(&source, "abort-checkpoint"),
        "aborted",
        "上传失败必须把导出收成 aborted"
    );
    let recorded_blob: Option<String> = source
        .connection
        .query_row(
            "SELECT blob_id FROM sync_snapshot_exports WHERE id='abort-checkpoint'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        recorded_blob.is_some(),
        "blob 身份必须在失败前就落表，否则续传只能开新号"
    );

    // 第二次：同一个 id，真的 relay。必须续传并发布。
    let upload = BlobUpload {
        sink: Box::new(crate::sync::transport_relay::RelaySink {
            client: client.clone(),
            mailbox: target_mailbox.clone(),
            sender_mb: source_mailbox.clone(),
            credential: forward,
        }),
        signal: Some(engine::snapshot::SignalSender {
            client: client.clone(),
            mailbox: target_mailbox.clone(),
            sender_mb: source_mailbox.clone(),
            credential: forward,
        }),
        recipient: "target".into(),
        key_version: 1,
        window_wait_ms: 200,
    };
    let source_public = signer_a.public_key();
    let source_peer = [engine::ReceivingPeer {
        device_id: "source",
        mailbox_id: &source_mailbox,
        key_version: 1,
        key: &content_ab,
        public_key: &source_public,
    }];
    let now = chrono::Utc::now().timestamp_millis();
    let mut rounds = 0;
    for tick in 0..40 {
        rounds = tick + 1;
        engine::snapshot::stage_snapshot(
            &source,
            "target",
            "abort-checkpoint",
            1,
            &content_ab,
            &signer_a,
            &upload,
        )
        .unwrap();
        engine::push_due(
            &source,
            client,
            "target",
            &target_mailbox,
            &source_mailbox,
            &forward,
            now + tick * 60_000,
        )
        .unwrap();
        let pulled = engine::pull_page(
            &target,
            client,
            &target_mailbox,
            &target_owner,
            &source_peer,
        )
        .unwrap();
        assert_eq!(pulled.rejected, 0);
        if target.snapshot_import_pending().unwrap() {
            let source = RelaySource {
                client: client.clone(),
                mailbox: target_mailbox.clone(),
                owner_credential: target_owner,
            };
            let download = BlobDownload {
                source: &source,
                from: "source",
                key_version: 1,
                key: &content_ab,
                public: &source_public,
            };
            engine::snapshot::import_blob_snapshots(&target, &download, 4).unwrap();
        }
        if !target.snapshot_import_pending().unwrap() {
            break;
        }
    }
    assert!(
        !target.snapshot_import_pending().unwrap(),
        "abort 之后必须能续传并完成导入（跑了 {rounds} 拍）"
    );
    assert_eq!(
        export_state(&source, "abort-checkpoint"),
        "ready",
        "续传成功后导出必须回到 ready 等回执"
    );
    assert_eq!(target.list_events().unwrap().len(), 1);
    // start 信令仍是幂等的一条：abort 再续传不许刷出第二条。
    let starts: i64 = source
        .connection
        .query_row(
            "SELECT count(*) FROM sync_wire_outbox WHERE recipient_device_id='target' AND message_id LIKE 'snapshot-start:abort-checkpoint'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(starts, 1, "续传不得多刷一条 snapshot-start");
}
