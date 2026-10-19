use super::export;
use super::pair;
use super::stage;
use super::*;

#[test]
fn newer_request_fences_late_chunks_and_existing_user_data_is_never_replaced() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let old = Uuid::new_v4().to_string();
    let new = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &old, &key, &signer);
    b.store.begin_snapshot_import(&old, "a", 1).unwrap();
    b.store.begin_snapshot_import(&new, "a", 1).unwrap();
    assert!(
        b.store
            .stage_snapshot_chunk(&old, 0, "a", 1, &chunks[0], &key, &signer.public_key())
            .is_err()
    );
    assert!(
        b.store
            .publish_snapshot(&old, "a", 1, &manifest, &key, &signer.public_key())
            .is_err()
    );
    assert!(b.store.begin_snapshot_import(&old, "a", 1).is_err());
    b.store
        .submit_input_as_event("local data must stay", "capture", None)
        .unwrap();
    assert!(
        b.store
            .begin_snapshot_import(&Uuid::new_v4().to_string(), "a", 1)
            .is_err()
    );
    assert_eq!(
        b.store.list_events().unwrap()[0].raw_text,
        "local data must stay"
    );
}

#[test]
fn provider_dirty_revision_must_be_durably_prepared_before_checkpoint() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    a.store
        .save_ai_provider_config(
            None,
            "main",
            "openai-compatible",
            "https://synthetic.invalid",
            "model",
            "synthetic",
            0.7,
            None,
        )
        .unwrap();
    let id = Uuid::new_v4().to_string();
    assert!(SnapshotExporter::new(&a.store, "b", &id, 1).is_err());
    let bundle = a.store.current_provider_bundle().unwrap();
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    assert!(
        a.store
            .mark_provider_revision_prepared(&bundle.revision_id)
            .is_err()
    );
    engine::stage_provider_bundle(&a.store, &bundle, "b", 1, &key, &signer).unwrap();
    a.store
        .mark_provider_revision_prepared(&bundle.revision_id)
        .unwrap();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    let header = EnvelopeHeader::new(
        "dataset",
        "a",
        "b",
        1,
        format!("provider:{}", bundle.revision_id),
    )
    .unwrap();
    let old = crypto::seal(
        header.clone(),
        &key,
        &signer,
        &serde_json::to_vec(&bundle).unwrap(),
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    assert_eq!(
        b.store
            .apply_encrypted_provider_bundle(&old, &header, &key, &signer.public_key())
            .unwrap(),
        crate::storage::sync::providers::ProviderApplyResult::Duplicate
    );
}

#[test]
fn snapshot_uses_hash_checkpoints_instead_of_resending_deleted_chat_bodies() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    let conversation = a
        .store
        .create_conversation(Some("removed title sentinel"), None)
        .unwrap();
    a.store
        .send_message(&conversation, "assistant", "removed reply sentinel", None)
        .unwrap();
    a.store
        .set_conversation_archived(&conversation, true)
        .unwrap();
    a.store.delete_archived_conversation(&conversation).unwrap();
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    let mut stream = Vec::new();
    for (index, bytes) in chunks.iter().enumerate() {
        let expected =
            EnvelopeHeader::new("dataset", "a", "b", 1, format!("snapshot:{id}:{index}")).unwrap();
        let chunk: SnapshotChunk = serde_json::from_slice(
            &crypto::open(
                &EncryptedEnvelope::from_bytes(bytes).unwrap(),
                &expected,
                &key,
                &signer.public_key(),
            )
            .unwrap(),
        )
        .unwrap();
        stream.extend(STANDARD.decode(chunk.data).unwrap());
    }
    assert!(!String::from_utf8_lossy(&stream).contains("removed reply sentinel"));
    assert!(!String::from_utf8_lossy(&stream).contains("removed title sentinel"));
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    assert_eq!(b.count("messages"), 0);
    assert_eq!(b.count("sync_checkpoint_operations"), 4);
    for operation in a.store.pending_sync_operations("b", 100).unwrap() {
        b.store
            .receive_sync_operation("a", &serde_json::to_vec(&operation).unwrap())
            .unwrap();
    }
    assert_eq!(b.count("messages"), 0);
}
