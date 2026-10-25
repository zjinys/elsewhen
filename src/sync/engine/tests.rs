use super::snapshot::stage_snapshot;
use super::*;
use crate::storage::sync::{BeginExport, ReplicaIdentity, SnapshotTransport};
use rusqlite::params;

mod fragments;

struct Database {
    store: Store,
    directory: std::path::PathBuf,
}
impl Database {
    fn new(device: &str) -> Self {
        let directory =
            std::env::temp_dir().join(format!("elsewhen-sync-engine-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let store = Store::open(&directory.join("test.db")).unwrap();
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "dataset".into(),
                device_id: device.into(),
                stream_id: format!("stream-{device}"),
                processor_device_id: "a".into(),
            })
            .unwrap();
        Self { store, directory }
    }
    fn wires(&self, peer: &str) -> Vec<(String, Vec<u8>)> {
        self.store.connection.prepare("SELECT message_id,encrypted_envelope FROM sync_wire_outbox WHERE recipient_device_id=?1 ORDER BY rowid")
            .unwrap().query_map([peer], |r| Ok((r.get(0)?,r.get(1)?))).unwrap()
            .collect::<rusqlite::Result<Vec<_>>>().unwrap()
    }
    fn count(&self, table: &str) -> i64 {
        self.store
            .connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
fn pair(a: &Database, b: &Database) {
    a.store.authorize_sync_member("b", "stream-b").unwrap();
    b.store.authorize_sync_member("a", "stream-a").unwrap();
}
fn accept(
    store: &Store,
    bytes: &[u8],
    peer: &str,
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> bool {
    accept_relay_payload(
        store,
        &STANDARD.encode(bytes),
        peer,
        1,
        key,
        &signer.public_key(),
    )
    .unwrap()
}

#[test]
fn snapshot_delivery_survives_manifest_first_restart_and_lost_application_receipt() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let backward_key = ContentKey::generate().unwrap();
    let backward_signer = DeviceIdentity::generate().unwrap();
    a.store
        .submit_input_as_event(&"multi-chunk original ".repeat(60_000), "capture", None)
        .unwrap();
    a.store
        .save_ai_provider_config(
            None,
            "provider",
            "openai-compatible",
            "http://127.0.0.1:9999/v1",
            "model",
            "SYNTHETIC_SNAPSHOT_SECRET",
            0.5,
            Some(1024),
        )
        .unwrap();
    stage_provider_updates(
        &a.store,
        &[EncryptionPeer {
            device_id: "b",
            key_version: 1,
            key: &key,
        }],
        &signer,
    )
    .unwrap();
    stage_snapshot(&a.store, "b", "initial", 1, &key, &signer, None).unwrap();
    let original_wires = a.wires("b");
    let chunks: Vec<_> = original_wires
        .iter()
        .filter(|(id, _)| id.starts_with("snapshot:"))
        .collect();
    assert!(chunks.len() > 4);
    assert!(original_wires.iter().all(|(_,bytes)| !String::from_utf8_lossy(bytes).contains("SYNTHETIC_SNAPSHOT_SECRET")));
    let manifest = &original_wires
        .iter()
        .find(|(id, _)| id == "snapshot-end:initial")
        .unwrap()
        .1;
    a.store
        .submit_input_as_event("after checkpoint", "capture", None)
        .unwrap();
    let reopened_source = Store::open(&a.store.path).unwrap();
    stage_snapshot(&reopened_source, "b", "initial", 1, &key, &signer, None).unwrap();
    assert_eq!(
        original_wires,
        a.wires("b"),
        "retry must not regenerate the snapshot from a newer source view"
    );
    b.store.begin_snapshot_import("initial", "a", 1).unwrap();
    assert!(accept(&b.store, manifest, "a", &key, &signer));
    assert_eq!(b.count("events"), 0);
    assert!(
        b.store
            .pending_sync_wire_receipts("a", 100)
            .unwrap()
            .is_empty()
    );
    // A provider update and an event arriving early must not contaminate staging.
    let provider = &original_wires
        .iter()
        .find(|(id, _)| id.starts_with("provider:"))
        .unwrap()
        .1;
    assert!(!accept(&b.store, provider, "a", &key, &signer));
    let op = a
        .store
        .pending_sync_operations("b", 100)
        .unwrap()
        .pop()
        .unwrap();
    let header =
        EnvelopeHeader::new("dataset", "a", "b", 1, operation_message_id(&op.op_id)).unwrap();
    let delta = crypto::seal(header, &key, &signer, &serde_json::to_vec(&op).unwrap())
        .unwrap()
        .to_bytes()
        .unwrap();
    assert!(!accept(&b.store, &delta, "a", &key, &signer));
    assert_eq!(b.count("sync_operations"), 0);
    assert!(accept(&b.store, &chunks[0].1, "a", &key, &signer));
    let reopened = Store::open(&b.store.path).unwrap();
    for (_, bytes) in chunks.iter().skip(1).rev() {
        assert!(accept(&reopened, bytes, "a", &key, &signer));
    }
    assert!(!b.store.snapshot_import_pending().unwrap());
    assert_eq!(b.count("events"), 1);
    assert_eq!(
        b.store
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        "SYNTHETIC_SNAPSHOT_SECRET"
    );
    assert_eq!(b.count("analysis_jobs"), 0);
    assert_eq!(b.count("chat_runs"), 0);
    assert!(accept(&b.store, provider, "a", &key, &signer));
    assert!(accept(&b.store, &delta, "a", &key, &signer));
    assert_eq!(b.count("events"), 2);
    // Simulate a successful receipt upload whose response/content was lost.
    stage_receipts(&b.store, "a", 1, &backward_key, &backward_signer).unwrap();
    b.store
        .mark_sync_wire_receipt_sent("a", "snapshot-end:initial")
        .unwrap();
    let pending = b.wires("a");
    let final_receipt_id = envelope_receipt_id("snapshot-end:initial");
    let original_receipt = &pending
        .iter()
        .find(|(id, _)| id == &final_receipt_id)
        .unwrap()
        .1;
    let receipt_hash = crate::storage::sync::hash(
        &crypto::open(
            &EncryptedEnvelope::from_bytes(original_receipt).unwrap(),
            &EnvelopeHeader::new("dataset", "b", "a", 1, &final_receipt_id).unwrap(),
            &backward_key,
            &backward_signer.public_key(),
        )
        .unwrap(),
    );
    b.store
        .acknowledge_sync_wire("a", &final_receipt_id, &receipt_hash)
        .unwrap();
    // Replayed manifest (or chunk) regenerates only the receipt, never the data.
    assert!(accept(&b.store, manifest, "a", &key, &signer));
    stage_receipts(&b.store, "a", 1, &backward_key, &backward_signer).unwrap();
    assert_eq!(b.count("events"), 2);
    assert!(
        b.store
            .due_sync_wire_envelopes("a", i64::MAX, 10)
            .unwrap()
            .iter()
            .any(|e| e.message_id == final_receipt_id)
    );
    assert!(accept(
        &a.store,
        original_receipt,
        "b",
        &backward_key,
        &backward_signer
    ));
    assert!(accept(
        &a.store,
        original_receipt,
        "b",
        &backward_key,
        &backward_signer
    ));
    let remaining: i64 = a.store.connection.query_row("SELECT COUNT(*) FROM sync_wire_outbox WHERE message_id LIKE 'snapshot%' AND state!='acknowledged'",[],|r|r.get(0)).unwrap();
    assert_eq!(remaining, 0);
    assert_eq!(
        a.store.pending_sync_operations("b", 100).unwrap().len(),
        2,
        "snapshot confirmation is not a per-operation GC watermark"
    );
}

/// 乱序到达：接收方是在「最后一片 chunk」上触发发布的，所以回执 subject 是那片
/// chunk 而不是 manifest。旧实现对 subject 不敏感、拿 manifest 的 hash 去比
/// chunk 的回执，必然 `sync_wire_ack_mismatch`：导出永远不会被确认，发送方永远
/// 重发整份快照，两端都停在「等待成员授权与初始同步」。
#[test]
fn snapshot_ack_accepts_the_chunk_that_triggered_publication() {
    let a = Database::new("a");
    a.store.authorize_sync_member("b", "stream-b").unwrap();
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    stage_snapshot(&a.store, "b", "ordered", 1, &key, &signer, None).unwrap();
    let (chunk, chunk_hash): (String, String) = a
        .store
        .connection
        .query_row(
            "SELECT message_id,payload_hash FROM sync_wire_outbox WHERE recipient_device_id='b' AND message_id LIKE 'snapshot:ordered:%' ORDER BY rowid LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let (manifest_hash, chunk_hash_actual): (String, String) = a
        .store
        .connection
        .query_row(
            "SELECT (SELECT payload_hash FROM sync_wire_outbox WHERE message_id='snapshot-end:ordered'), ?1",
            [&chunk_hash],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_ne!(
        manifest_hash, chunk_hash_actual,
        "前提：分片与 manifest 的哈希不同"
    );
    // chunk 回执：拿 manifest 的哈希去比是旧实现的自毁点。
    assert!(
        a.store
            .acknowledge_snapshot_export("b", "ordered", &chunk, &manifest_hash)
            .is_err(),
        "错配的哈希仍必须被拒"
    );
    a.store
        .acknowledge_snapshot_export("b", "ordered", &chunk, &chunk_hash)
        .unwrap();
    let state: String = a
        .store
        .connection
        .query_row("SELECT state FROM sync_snapshot_exports WHERE id='ordered'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(state, "acknowledged", "导出应被整条退休");
    let remaining: i64 = a
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sync_wire_outbox WHERE recipient_device_id='b' AND message_id LIKE 'snapshot%' AND state!='acknowledged'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(remaining, 0, "整份快照线（含 manifest）一起退休");
}

#[test]
fn partial_export_is_not_uploadable_and_new_request_is_required_after_restart() {
    let a = Database::new("a");
    a.store.authorize_sync_member("b", "stream-b").unwrap();
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    assert!(matches!(
        a.store
            .begin_snapshot_export("partial", "b", 1, SnapshotTransport::Mail)
            .unwrap(),
        BeginExport::Begun(_)
    ));
    let header = EnvelopeHeader::new("dataset", "a", "b", 1, "snapshot:partial:0").unwrap();
    a.store
        .stage_snapshot_export_payload("partial", &header, &key, &signer, b"incomplete", false)
        .unwrap();
    assert!(
        a.store
            .due_sync_wire_envelopes("b", i64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    let reopened = Store::open(&a.store.path).unwrap();
    assert!(stage_snapshot(&reopened, "b", "partial", 1, &key, &signer, None).is_err());
    reopened.abort_snapshot_export("partial").unwrap();
    assert!(
        reopened
            .due_sync_wire_envelopes("b", i64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    stage_snapshot(&reopened, "b", "replacement", 1, &key, &signer, None).unwrap();
    assert!(
        !reopened
            .due_sync_wire_envelopes("b", i64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    assert!(
        a.store
            .acknowledge_snapshot_export("b", "partial", "snapshot-end:partial", "wrong")
            .is_err()
    );
    assert!(
        a.store
            .acknowledge_snapshot_export("b", "replacement", "snapshot-end:replacement", "wrong")
            .is_err()
    );
    assert!(
        a.store
            .begin_snapshot_export("replacement", "b", 2, SnapshotTransport::Mail)
            .is_err()
    );
    a.store.revoke_sync_member("b").unwrap();
    assert!(
        a.store
            .due_sync_wire_envelopes("b", i64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    assert!(stage_snapshot(&a.store, "b", "replacement", 1, &key, &signer, None).is_err());
}

#[test]
fn unsolicited_snapshot_never_selects_or_overwrites_the_receiving_database() {
    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    stage_snapshot(&a.store, "b", "not-requested", 1, &key, &signer, None).unwrap();
    for (_, bytes) in a.wires("b") {
        assert!(
            accept_relay_payload(
                &b.store,
                &STANDARD.encode(bytes),
                "a",
                1,
                &key,
                &signer.public_key()
            )
            .is_err()
        );
    }
    assert_eq!(b.count("sync_snapshot_target"), 0);
    assert_eq!(b.count("sync_snapshot_chunks"), 0);
    b.store
        .submit_input_as_event("local stays", "capture", None)
        .unwrap();
    assert!(
        b.store
            .begin_snapshot_import("not-requested", "a", 1)
            .is_err()
    );
    assert_eq!(b.store.list_events().unwrap()[0].raw_text, "local stays");
}

#[test]
fn scan_cursor_is_durable_and_not_an_application_frontier() {
    let b = Database::new("b");
    assert_eq!(b.store.relay_scan_cursor("mailbox").unwrap(), "0");
    b.store
        .save_relay_scan_cursor("mailbox", "0000000000000003")
        .unwrap();
    let reopened = Store::open(&b.store.path).unwrap();
    assert_eq!(
        reopened.relay_scan_cursor("mailbox").unwrap(),
        "0000000000000003"
    );
    assert_eq!(b.count("sync_receipts"), 0);
    assert_eq!(
        b.store
            .connection
            .query_row(
                "SELECT applied_sequence FROM sync_frontiers WHERE origin_stream_id=?1",
                params!["stream-b"],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    reopened.save_relay_scan_cursor("mailbox", "0").unwrap();
    assert_eq!(b.store.relay_scan_cursor("mailbox").unwrap(), "0");
}

/// `snapshot-start` 收到回执后必须退役（review 2026-10-25 B1 的引擎级收口）。
///
/// 这条路以前是断的：start 分支只记下 blob 位置就 `return Ok(true)`，既不排回执，
/// 也不在白名单里 —— 于是 send 端那行 outbox 永远退不掉，`resend_spacing_ms` 一到
/// 又重投，接收端把「已发布」当错误拒收，三次后 drop，发送端再发是一封新
/// `mail_id`、计数归零，无限循环。
#[test]
fn snapshot_start_is_receipted_by_the_receiver_and_retired_by_the_sender() {
    use crate::sync::engine::deliver::DeliverState;

    let a = Database::new("a");
    let b = Database::new("b");
    pair(&a, &b);
    let forward = ContentKey::generate().unwrap();
    let signer_a = DeviceIdentity::generate().unwrap();
    let backward = ContentKey::generate().unwrap();
    let signer_b = DeviceIdentity::generate().unwrap();

    let message_id = "snapshot-start:starts-1";
    let header = EnvelopeHeader::new("dataset", "a", "b", 1, message_id).unwrap();
    let payload = serde_json::to_vec(&serde_json::json!({
        "version": 1,
        "blob_id": "aaaaaaaaaaaaaaaaaaaaaaaa",
    }))
    .unwrap();
    let start = crypto::seal(header, &forward, &signer_a, &payload)
        .unwrap()
        .to_bytes()
        .unwrap();

    // 首次到达：记下 blob 位置，并排一条回执。
    assert!(accept(&b.store, &start, "a", &forward, &signer_a));
    // 重投（发送端没收到回执）：仍必须成功 —— 通知不是数据，重复收到不是毒信。
    assert!(
        accept(&b.store, &start, "a", &forward, &signer_a),
        "重投 snapshot-start 必须当成功，否则会被 record_wire_rejection 三次后丢弃"
    );
    // 已发布的快照再收到 start 也一样。
    b.store
        .connection
        .execute(
            "UPDATE sync_snapshots SET state='applied' WHERE id='starts-1'",
            [],
        )
        .unwrap();
    assert!(
        accept(&b.store, &start, "a", &forward, &signer_a),
        "已发布的快照再收到 start 也必须当成功"
    );
    // 三次收到只该有一条回执（重复通知不制造新回执）。
    let receipts = b.store.pending_sync_wire_receipts("a", 16).unwrap();
    assert_eq!(receipts.len(), 1, "重复的 start 只该有一条回执");
    assert_eq!(receipts[0].message_id, message_id);

    // 发送端的 outbox 里确实有这一行，且现在是「等回执」。
    a.store
        .connection
        .execute(
            "INSERT INTO sync_wire_outbox(recipient_device_id,message_id,payload_hash,key_version,encrypted_envelope,attempts,available_at_ms,seq_hint,relay_id) \
             VALUES('b',?1,'hash',1,x'00',0,0,1,'mail-1')",
            params![message_id],
        )
        .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let awaiting = |db: &Database| {
        db.store
            .deliver_statuses(now)
            .unwrap()
            .into_iter()
            .find(|s| s.peer_device_id == "b")
            .map(|s| s.state)
    };
    assert_eq!(awaiting(&a), Some(DeliverState::AwaitingReceipt));

    // 回执回到发送端：走通用 `acknowledge_sync_wire`（`snapshot-end:` 才走导出 ack）。
    // 这里直接构造回执信封，绕开 relay —— 本用例要验的是白名单与分派，不是传输。
    let receipt_id = envelope_receipt_id(message_id);
    let receipt_header = EnvelopeHeader::new("dataset", "b", "a", 1, &receipt_id).unwrap();
    let receipt_payload = serde_json::to_vec(&crate::sync::protocol::EnvelopeReceipt {
        dataset_id: "dataset".into(),
        acknowledging_device_id: "b".into(),
        target_device_id: "a".into(),
        message_id: message_id.into(),
        payload_hash: "hash".into(),
    })
    .unwrap();
    let receipt = crypto::seal(receipt_header, &backward, &signer_b, &receipt_payload)
        .unwrap()
        .to_bytes()
        .unwrap();
    assert!(
        accept(&a.store, &receipt, "b", &backward, &signer_b),
        "snapshot-start 的回执必须被白名单放行"
    );
    assert_eq!(
        awaiting(&a),
        None,
        "回执到达后该行必须退掉 —— 这正是修复前永远为真的那句假话"
    );
}

/// 413 这类**结构性**失败必须真的落成 `blocked`（review 2026-10-25 M3）。
///
/// 改前 `is_blocking()` 是个永远为假的死分支：全仓没有任何一处写
/// `blocked_reason`，于是「两端上限不同步」这种重试一万次也不会好的错，
/// 每拍都重来一次 —— 正是 R5「3 次必表态」要掐掉的那种毒信循环。
///
/// 这里用一个只会回 `413 Payload Too Large` 的假中继把这条路走通：
/// `push_due` 失败 ⇒ `block_sync_wire(protocol)` ⇒ 面板 `Blocked{protocol}`
/// 且下次尝试在 10 分钟之后（不是 0 ms 之后）。
#[test]
fn protocol_failure_lands_as_blocked_with_a_ten_minute_delay() {
    use crate::sync::engine::deliver::DeliverState;

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buffer = [0u8; 4096];
            let _ = std::io::Read::read(&mut stream, &mut buffer);
            let _ = std::io::Write::write_all(
                &mut stream,
                b"HTTP/1.1 413 Payload Too Large\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
            );
        }
    });

    let a = Database::new("a");
    pair(&a, &Database::new("b"));
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    a.store
        .stage_sync_wire_envelope(
            &EnvelopeHeader::new("dataset", "a", "b", 1, "op:too-large").unwrap(),
            &key,
            &signer,
            b"payload",
        )
        .unwrap();

    let client = RelayClient::new(&url).unwrap();
    let now = 1_700_000_000_000_i64;
    let report = push_due(
        &a.store,
        &client,
        "b",
        &"ab".repeat(32),
        &"cd".repeat(32),
        &[7u8; 32],
        now,
    )
    .unwrap();
    assert_eq!(report.failed, 1);
    assert!(!report.quota_exhausted, "413 不是配额满，不该中止本拍其余信封");

    let (blocked, last_error, available): (Option<String>, String, i64) = a
        .store
        .connection
        .query_row(
            "SELECT blocked_reason,last_error,available_at_ms FROM sync_wire_outbox WHERE recipient_device_id='b' AND message_id='op:too-large'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(blocked.as_deref(), Some("protocol"), "阻塞必须真的写进表");
    assert_eq!(last_error, "protocol", "分类码要能被面板读出来");
    assert_eq!(
        available,
        now + 10 * 60_000,
        "结构性的错重试再快也不会成功：十分钟后才再看它"
    );

    let state = a
        .store
        .deliver_statuses(now)
        .unwrap()
        .into_iter()
        .find(|s| s.peer_device_id == "b")
        .map(|s| s.state);
    assert_eq!(
        state,
        Some(DeliverState::Blocked {
            reason: "protocol".into(),
            until_ms: now + 10 * 60_000,
        }),
        "面板要同时拿到原因与「到点会再试」的时刻"
    );
}

/// 汇总行报的是**密文字节数**，不是封数（二次复核 R2）。
///
/// 同一条日志契约里 `bytes` 在别处都是字节（`engine.rs` 的 push 行传
/// `encrypted_envelope.len()`），汇总行原来传 `report.uploaded`（封数）——
/// 格式上四个 `=` 成立，读的人却会以为「3 字节」。
#[test]
fn push_summary_reports_ciphertext_bytes_not_envelope_count() {
    // 字段本身：默认报告不谎报字节数。
    assert_eq!(PushReport::default().uploaded_bytes, 0);
    let report = PushReport {
        uploaded: 3,
        uploaded_bytes: 4096,
        failed: 1,
        quota_exhausted: false,
    };
    let line = push_summary_line("peer-b", &report);
    assert_eq!(
        line,
        "[wire] event=push-summary subject=peer-b bytes=4096 detail=envelopes 3 failed 1"
    );
    assert_eq!(
        line.matches('=').count(),
        4,
        "detail 里再带 `=` 就是伪字段（`wire_line` 会净化，这里锁住它）"
    );
}

/// 真跑一拍上传，确认 `uploaded_bytes` 累加的是密文长度。
#[test]
fn pushed_bytes_accumulate_the_real_ciphertext_length() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buffer = [0u8; 8192];
            let _ = std::io::Read::read(&mut stream, &mut buffer);
            let _ = std::io::Write::write_all(
                &mut stream,
                b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 25\r\nconnection: close\r\n\r\n{\"id\":\"0000000000000001\"}",
            );
        }
    });

    let a = Database::new("a");
    pair(&a, &Database::new("b"));
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    a.store
        .stage_sync_wire_envelope(
            &EnvelopeHeader::new("dataset", "a", "b", 1, "op:bytes").unwrap(),
            &key,
            &signer,
            b"payload",
        )
        .unwrap();
    let stored: i64 = a
        .store
        .connection
        .query_row(
            "SELECT LENGTH(encrypted_envelope) FROM sync_wire_outbox WHERE message_id='op:bytes'",
            [],
            |r| r.get(0),
        )
        .unwrap();

    let client = RelayClient::new(&url).unwrap();
    let report = push_due(
        &a.store,
        &client,
        "b",
        &"ab".repeat(32),
        &"cd".repeat(32),
        &[7u8; 32],
        1_700_000_000_000,
    )
    .unwrap();
    assert_eq!(report.uploaded, 1, "假中继回了 200，这一封该算送出");
    assert_eq!(
        report.uploaded_bytes as i64, stored,
        "上行字节数必须等于这封信的密文长度"
    );
    assert_ne!(
        report.uploaded_bytes, report.uploaded,
        "封数与字节数不能是同一个值（否则又是「计数冒充 bytes」）"
    );
}

/// blob 回收失败必须留到下一拍再关（review m4）。
///
/// 发布 + 回执在同一个事务里提交之后，这一行就不再满足「在途导入」的查询条件
/// （那是 `state='staging'`）。原来那一刻 `blob_close` 失败是直接 `?` 冒出去，
/// 于是**再没有任何人会去关它** —— relay 上那块空间只能等 30 分钟 TTL。
///
/// 假中继前两次回 500、之后回 200：失败要留下「还没回收」的事实，下一拍要真的
/// 关掉，关完之后不能再重复请求。顺便锁住关闭语义：已发布的用 `done`、
/// 作废的用 `cancel` —— relay 侧这两者含义不同（消费完成 / 撤回）。
#[test]
fn failed_blob_recycle_is_retried_on_the_next_sweep() {
    use crate::sync::engine::snapshot::{recycle_published_blobs, BlobDownload};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen = requests.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut raw = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let Ok(n) = std::io::Read::read(&mut stream, &mut buffer) else {
                    break;
                };
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&buffer[..n]);
                let text = String::from_utf8_lossy(&raw).to_string();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let length = head
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if body.len() >= length {
                        break;
                    }
                }
            }
            let n = {
                let mut guard = seen.lock().unwrap();
                guard.push(String::from_utf8_lossy(&raw).to_string());
                guard.len()
            };
            let response = if n <= 2 {
                "HTTP/1.1 500 Internal Server Error\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
            } else {
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 11\r\nconnection: close\r\n\r\n{\"freed\":0}"
            };
            let _ = std::io::Write::write_all(&mut stream, response.as_bytes());
        }
    });

    let db = Database::new("a");
    db.store
        .connection
        .execute(
            "INSERT INTO sync_snapshots(id,source_device_id,key_version,state,transport,blob_id)
               VALUES('snap-published','b',1,'applied','blob','blob-0123456789abcdef'),
                     ('snap-aborted','b',1,'aborted','blob','blob-fedcba9876543210')",
            [],
        )
        .unwrap();
    let client = RelayClient::new(&url).unwrap();
    let key = ContentKey::generate().unwrap();
    let public = DeviceIdentity::generate().unwrap().public_key();
    let download = BlobDownload {
        client: &client,
        mailbox: &"ab".repeat(32),
        owner_credential: &[7u8; 32],
        source: "b",
        key_version: 1,
        key: &key,
        public: &public,
    };
    let closed_at = |store: &Store, id: &str| -> Option<i64> {
        store
            .connection
            .query_row(
                "SELECT blob_closed_at FROM sync_snapshots WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap()
    };

    assert!(db.store.blob_recycle_pending().unwrap());
    assert_eq!(
        recycle_published_blobs(&db.store, &download).unwrap(),
        0,
        "close 失败不能记成已回收"
    );
    for id in ["snap-published", "snap-aborted"] {
        assert_eq!(closed_at(&db.store, id), None);
    }
    assert!(
        db.store.blob_recycle_pending().unwrap(),
        "失败的行必须还留在待回收清单里，否则它永远等不到下一拍"
    );

    assert_eq!(
        recycle_published_blobs(&db.store, &download).unwrap(),
        2,
        "下一拍要真的把两块都关掉"
    );
    for id in ["snap-published", "snap-aborted"] {
        assert!(closed_at(&db.store, id).is_some(), "关成功才落标记");
    }
    assert!(!db.store.blob_recycle_pending().unwrap());

    assert_eq!(recycle_published_blobs(&db.store, &download).unwrap(), 0);
    let bodies = requests.lock().unwrap().clone();
    assert_eq!(bodies.len(), 4, "每块 blob 只该被关一次（失败那次不算）");
    let statuses: Vec<&str> = bodies
        .iter()
        .map(|body| {
            if body.contains("\"status\":\"cancel\"") {
                "cancel"
            } else if body.contains("\"status\":\"done\"") {
                "done"
            } else {
                "unknown"
            }
        })
        .collect();
    assert_eq!(
        statuses,
        vec!["done", "cancel", "done", "cancel"],
        "已发布要 done、作废要 cancel；且失败的行仍在清单里等待重试"
    );
}
