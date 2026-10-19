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
