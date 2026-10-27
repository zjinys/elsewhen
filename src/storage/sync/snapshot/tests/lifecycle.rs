use super::export;
use super::pair;
use super::stage;
use super::*;

#[test]
fn encrypted_snapshot_survives_restart_and_preserves_provider_and_concurrent_source_writes() {
    let a = Database::new();
    let b = Database::new();
    a.store
        .submit_input_as_event(&"large original ".repeat(70_000), "capture", None)
        .unwrap();
    let provider = a
        .store
        .save_ai_provider_config(
            None,
            "main",
            "openai-compatible",
            "https://synthetic.invalid/v1",
            "model",
            "SNAPSHOT_SYNTHETIC_KEY",
            0.5,
            Some(2048),
        )
        .unwrap();
    a.store
        .connection
        .execute(
            "UPDATE ai_provider_configs SET context_window=65536 WHERE id=?1",
            [&provider],
        )
        .unwrap();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    let conversation = a.store.create_conversation(Some("before"), None).unwrap();
    a.store
        .submit_conversation_input(&conversation, "before snapshot", None)
        .unwrap();
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let mut exporter = SnapshotExporter::new(&a.store, "b", &id, 1).unwrap();
    exporter.declare_blob(&format!("blob-{id}"));
    // A held source read view must not block unrelated capture (WAL).
    a.store
        .submit_input_as_event("after snapshot", "capture", None)
        .unwrap();
    let mut chunks = Vec::new();
    while let Some(chunk) = exporter.next_encrypted_chunk(&key, &signer).unwrap() {
        chunks.push(chunk)
    }
    assert!(chunks.len() > 1);
    let manifest = exporter.finish(&key, &signer).unwrap();
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    b.store
        .stage_snapshot_chunk(&id, 0, "a", 1, &chunks[0], &key, &signer.public_key())
        .unwrap();
    assert!(
        !b.store
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    assert_eq!(b.count("events"), 0);
    assert_eq!(b.count("ai_provider_configs"), 0);
    let reopened = Store::open(&b.store.path).unwrap();
    stage(&reopened, &id, "a", &chunks, &key, &signer.public_key());
    assert!(
        reopened
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    assert_eq!(b.count("events"), 2);
    assert_eq!(b.count("analysis_jobs"), 0);
    let restored = reopened.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(restored.api_key, "SNAPSHOT_SYNTHETIC_KEY");
    assert_eq!(restored.context_window, Some(65536));
    assert_eq!(restored.id, provider);
    assert!(
        reopened
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    assert_eq!(b.count("events"), 2);
    // Covered operations verify their hashes and regenerate receipts, not rows.
    for op in a.store.pending_sync_operations("b", 100).unwrap() {
        reopened
            .receive_sync_operation("a", &serde_json::to_vec(&op).unwrap())
            .unwrap();
    }
    assert_eq!(b.count("events"), 3);
    assert_eq!(b.count("messages"), 1);
    assert_eq!(reopened.pending_sync_receipts("a", 100).unwrap().len(), 3);
    assert_eq!(b.count("analysis_jobs"), 0);
    let stored: Vec<u8> = reopened
        .connection
        .query_row(
            "SELECT encrypted_chunk FROM sync_snapshot_chunks LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!String::from_utf8_lossy(&stored).contains("SNAPSHOT_SYNTHETIC_KEY"));
    drop(reopened);
}

#[test]
fn bad_manifest_rolls_back_every_domain_write_and_key() {
    let a = Database::new();
    let b = Database::new();
    a.store
        .submit_input_as_event("keep source", "capture", None)
        .unwrap();
    a.store
        .save_ai_provider_config(
            None,
            "main",
            "openai-compatible",
            "https://synthetic.invalid",
            "model",
            "KEY_IN_SOURCE",
            0.7,
            None,
        )
        .unwrap();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    let header = EnvelopeHeader::new("dataset", "a", "b", 1, format!("snapshot-end:{id}")).unwrap();
    let mut wrong: SnapshotManifest = serde_json::from_slice(
        &crypto::open(
            &EncryptedEnvelope::from_bytes(&manifest).unwrap(),
            &header,
            &key,
            &signer.public_key(),
        )
        .unwrap(),
    )
    .unwrap();
    wrong.digest = "0".repeat(64);
    let bad = crypto::seal(header, &key, &signer, &serde_json::to_vec(&wrong).unwrap())
        .unwrap()
        .to_bytes()
        .unwrap();
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &bad, &key, &signer.public_key())
            .is_err()
    );
    assert_eq!(b.count("events"), 0);
    assert_eq!(b.count("ai_provider_configs"), 0);
    assert_eq!(b.count("sync_checkpoint_operations"), 0);
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    assert_eq!(b.count("events"), 1);
}

#[test]
fn checkpointed_history_can_be_used_as_a_new_snapshot_source() {
    let a = Database::new();
    let b = Database::new();
    let c = Database::new();
    a.enable("a");
    b.enable("b");
    c.enable("c");
    pair(&a, &b);
    a.store
        .submit_input_as_event("one fact", "capture", None)
        .unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let key = ContentKey::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    b.store
        .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
        .unwrap();
    b.store.authorize_sync_member("c", "stream-c").unwrap();
    c.store.authorize_sync_member("b", "stream-b").unwrap();
    let id = Uuid::new_v4().to_string();
    let signer = DeviceIdentity::generate().unwrap();
    let (chunks, manifest) = export(&b.store, "c", &id, &key, &signer);
    c.store.begin_snapshot_import(&id, "b", 1).unwrap();
    stage(&c.store, &id, "b", &chunks, &key, &signer.public_key());
    assert!(
        c.store
            .publish_snapshot(&id, "b", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    assert_eq!(c.count("events"), 1);
    assert_eq!(c.count("sync_checkpoint_operations"), 1);
}

/// owner 的 GC 会删掉「已被全员确认」的历史前缀，但 `sync_frontiers` 的序号是
/// 单调不回退的 —— 于是「ops 行数」和「frontier 序号」在有 GC 的库上必然对不上。
///
/// 真机 2026-10-27 现场：owner 流 `be73b530` 的 frontier=241，库里只剩
/// 178..241 的 64 条（`sync_gc_state.gc_floor=177`）。接收端原先的校验是
/// `count == sequence`，于是**任何**快照都发布不了：手机停在 174/174、
/// `snapshot-end:` 永远 pending、信被记入拒收表后 drop。
///
/// 快照的完整性由 manifest 的 `bytes/records/digest` 兜底；frontier 校验要守的
/// 是「快照给出的操作是一段连续序号，且上界正好是声明的 frontier」，不是
/// 「行数等于序号」（行数只在从没 GC 过的库上才等于序号）。
#[test]
fn garbage_collected_history_still_publishes_a_snapshot() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    for i in 0..5 {
        a.store
            .submit_input_as_event(&format!("fact {i}"), "capture", None)
            .unwrap();
    }
    // 造出 `run_sync_gc` 之后的库形状：已确认的前缀被删、floor 落库、序号不回退。
    // 外键（`sync_outbox.op_id`）指着这些行，删父行前必须先清掉。
    a.store
        .connection
        .execute("DELETE FROM sync_outbox", [])
        .unwrap();
    let pruned = a
        .store
        .connection
        .execute(
            "DELETE FROM sync_operations
             WHERE local=1 AND state='applied' AND origin_stream_id='stream-a'
               AND origin_sequence <= 3",
            [],
        )
        .unwrap();
    a.store
        .connection
        .execute(
            "INSERT INTO sync_gc_state(origin_stream_id,gc_floor) VALUES('stream-a',3)",
            [],
        )
        .unwrap();
    assert_eq!(pruned, 3, "GC 应只删掉连续前缀");
    let remaining: i64 = a
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sync_operations WHERE origin_stream_id='stream-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let frontier: i64 = a
        .store
        .connection
        .query_row(
            "SELECT applied_sequence FROM sync_frontiers WHERE origin_stream_id='stream-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!((remaining, frontier), (2, 5), "这就是现场形状：64 vs 241");

    let signer = DeviceIdentity::generate().unwrap();
    let key = ContentKey::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap(),
        "历史被 GC 掉之后，快照仍必须能发布"
    );
    // 领域数据是全量的（GC 只删同步日志，不删领域行），同步日志只有剩下的那段。
    assert_eq!(b.count("events"), 5);
    // 快照里的操作在接收端落成 checkpoint 记录（历史原样搬运，不重放）。
    let logged: i64 = b
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM (
                 SELECT origin_sequence FROM sync_operations
                  WHERE origin_stream_id='stream-a' AND state IN ('applied','conflict')
                 UNION ALL
                 SELECT origin_sequence FROM sync_checkpoint_operations
                  WHERE origin_stream_id='stream-a')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(logged, 2, "只剩 GC 之后的那一段");
    let published_frontier: i64 = b
        .store
        .connection
        .query_row(
            "SELECT applied_sequence FROM sync_frontiers WHERE origin_stream_id='stream-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        published_frontier, 5,
        "接收端的 frontier 要对齐 manifest 的声明"
    );
}

/// frontier 校验放宽之后，仍然要挡住「声明得比数据高」的 manifest：接收端拿到的
/// 操作只有 1..N，manifest 却说 N+1，那说明快照少了一段。
#[test]
fn a_frontier_beyond_the_transferred_operations_is_rejected() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    a.store
        .submit_input_as_event("one fact", "capture", None)
        .unwrap();
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    let header = EnvelopeHeader::new("dataset", "a", "b", 1, format!("snapshot-end:{id}")).unwrap();
    let mut wrong: SnapshotManifest = serde_json::from_slice(
        &crypto::open(
            &EncryptedEnvelope::from_bytes(&manifest).unwrap(),
            &header,
            &key,
            &signer.public_key(),
        )
        .unwrap(),
    )
    .unwrap();
    let mut bumped = false;
    for (stream, sequence) in wrong.frontiers.iter_mut() {
        if stream == "stream-a" {
            *sequence += 1;
            bumped = true;
        }
    }
    assert!(bumped, "manifest 里应该有 stream-a 的 frontier");
    let bad = crypto::seal(header, &key, &signer, &serde_json::to_vec(&wrong).unwrap())
        .unwrap()
        .to_bytes()
        .unwrap();
    let error = b
        .store
        .publish_snapshot(&id, "a", 1, &bad, &key, &signer.public_key())
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("sync_snapshot_frontier_not_covered"),
        "unexpected error: {error:#}"
    );
    assert_eq!(b.count("events"), 0);
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    assert_eq!(b.count("events"), 1);
}

#[test]
fn completed_snapshot_identity_cannot_be_reused_for_other_content() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    b.store
        .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
        .unwrap();
    let expected =
        EnvelopeHeader::new("dataset", "a", "b", 1, format!("snapshot-end:{id}")).unwrap();
    let mut changed: SnapshotManifest = serde_json::from_slice(
        &crypto::open(
            &EncryptedEnvelope::from_bytes(&manifest).unwrap(),
            &expected,
            &key,
            &signer.public_key(),
        )
        .unwrap(),
    )
    .unwrap();
    changed.records += 1;
    let changed = crypto::seal(
        expected,
        &key,
        &signer,
        &serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &changed, &key, &signer.public_key())
            .is_err()
    );
    let chunk = SnapshotChunk {
        id: id.clone(),
        index: chunks.len(),
        data: STANDARD.encode(b"extra"),
    };
    let expected = EnvelopeHeader::new(
        "dataset",
        "a",
        "b",
        1,
        format!("snapshot:{id}:{}", chunks.len()),
    )
    .unwrap();
    let extra = crypto::seal(
        expected,
        &key,
        &signer,
        &serde_json::to_vec(&chunk).unwrap(),
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    assert!(
        b.store
            .stage_snapshot_chunk(
                &id,
                chunks.len(),
                "a",
                1,
                &extra,
                &key,
                &signer.public_key()
            )
            .is_err()
    );
}
