//! providers 存储层内嵌测试平移。

use super::*;

use super::*;
use crate::storage::sync::ReplicaIdentity;
use crate::sync::crypto::{DeviceIdentity, seal};
use uuid::Uuid;

struct Db {
    store: Store,
    dir: std::path::PathBuf,
}
impl Db {
    fn new(device: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("elsewhen-sync-provider-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "dataset".into(),
                device_id: device.into(),
                stream_id: format!("stream-{device}"),
                processor_device_id: "a".into(),
            })
            .unwrap();
        store.authorize_sync_member("a", "stream-a").unwrap();
        Self { store, dir }
    }
}
impl Drop for Db {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn configured() -> Db {
    let db = Db::new("a");
    db.store
        .save_ai_provider_config(
            None,
            "first",
            "openai-compatible",
            "http://127.0.0.1:9999",
            "model-1",
            "SYNTHETIC_KEY_FIRST",
            0.2,
            Some(2048),
        )
        .unwrap();
    let second = db
        .store
        .save_ai_provider_config(
            None,
            "second",
            "openai-compatible",
            "https://synthetic.invalid/v1",
            "model-2",
            "SYNTHETIC_KEY_SECOND",
            0.7,
            None,
        )
        .unwrap();
    db.store
        .connection
        .execute(
            "UPDATE ai_provider_configs SET context_window=65536 WHERE id=?1",
            [&second],
        )
        .unwrap();
    db.store.set_active_ai_provider_config(&second).unwrap();
    db
}
fn encode(
    bundle: &ProviderBundle,
    signer: &DeviceIdentity,
    key: &ContentKey,
) -> (EnvelopeHeader, Vec<u8>) {
    let header = EnvelopeHeader::new(
        "dataset",
        "a",
        "b",
        1,
        format!("provider:{}", bundle.revision_id),
    )
    .unwrap();
    let bytes = seal(
        header.clone(),
        key,
        signer,
        &serde_json::to_vec(bundle).unwrap(),
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    (header, bytes)
}

#[test]
fn missing_key_remains_distinct_from_an_explicit_empty_key() {
    let a = configured();
    let b = Db::new("b");
    let signer = DeviceIdentity::generate().unwrap();
    let key = ContentKey::generate().unwrap();
    a.store
        .connection
        .execute(
            "UPDATE ai_provider_configs SET api_key=NULL WHERE is_active=1",
            [],
        )
        .unwrap();
    let bundle = a
        .store
        .export_provider_bundle("nullable-key-revision", None)
        .unwrap();
    assert!(
        !bundle
            .profiles
            .iter()
            .find(|p| Some(&p.id) == bundle.active_id.as_ref())
            .unwrap()
            .api_key_present
    );
    let (header, bytes) = encode(&bundle, &signer, &key);
    b.store
        .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
        .unwrap();
    assert!(a.store.active_ai_provider_config().unwrap().is_none());
    assert!(b.store.active_ai_provider_config().unwrap().is_none());
    assert_eq!(
        serde_json::to_value(
            b.store
                .export_provider_bundle("nullable-key-revision", None)
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(bundle).unwrap()
    );
}

#[test]
fn new_device_receives_real_keys_active_profile_and_parameters() {
    let a = configured();
    let b = Db::new("b");
    let signer = DeviceIdentity::generate().unwrap();
    let key = ContentKey::generate().unwrap();
    let bundle = a.store.export_provider_bundle("revision-1", None).unwrap();
    let (header, bytes) = encode(&bundle, &signer, &key);
    assert_eq!(
        b.store
            .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
            .unwrap(),
        ProviderApplyResult::Applied
    );
    let actual = b.store.export_provider_bundle("revision-1", None).unwrap();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(&bundle).unwrap()
    );
    let active = b.store.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(active.api_key, "SYNTHETIC_KEY_SECOND");
    assert_eq!(active.context_window, Some(65536));
    assert!(
        b.store
            .list_ai_provider_configs()
            .unwrap()
            .iter()
            .all(|p| p.api_key_source == "database")
    );
    let stored: Vec<u8> = b
        .store
        .connection
        .query_row(
            "SELECT encrypted_envelope FROM sync_provider_versions",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!String::from_utf8_lossy(&stored).contains("SYNTHETIC_KEY"));
    assert!(!format!("{bundle:?}").contains("SYNTHETIC_KEY"));
    assert!(!format!("{:?}", bundle.profiles[0]).contains("SYNTHETIC_KEY"));
}

#[test]
fn key_rotation_profile_deletion_and_explicit_empty_key_replace_exactly() {
    let a = configured();
    let b = Db::new("b");
    let signer = DeviceIdentity::generate().unwrap();
    let key = ContentKey::generate().unwrap();
    let mut bundle = a.store.export_provider_bundle("revision-1", None).unwrap();
    let (header, bytes) = encode(&bundle, &signer, &key);
    b.store
        .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
        .unwrap();
    bundle.base_revision = Some("revision-1".into());
    bundle.revision_id = "revision-2".into();
    bundle.profiles.truncate(1);
    bundle.profiles[0].api_key = "rotated-synthetic".into();
    bundle.active_id = Some(bundle.profiles[0].id.clone());
    bundle.failover_order = vec![bundle.profiles[0].id.clone()];
    let (header, bytes) = encode(&bundle, &signer, &key);
    assert_eq!(
        b.store
            .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
            .unwrap(),
        ProviderApplyResult::Applied
    );
    assert_eq!(b.store.list_ai_provider_configs().unwrap().len(), 1);
    assert_eq!(
        b.store
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        "rotated-synthetic"
    );
    bundle.base_revision = Some("revision-2".into());
    bundle.revision_id = "revision-3".into();
    bundle.profiles[0].api_key.clear();
    let (header, bytes) = encode(&bundle, &signer, &key);
    b.store
        .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
        .unwrap();
    assert_eq!(
        b.store
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        ""
    );
}

#[test]
fn replay_compares_payload_not_random_ciphertext_and_rejects_changed_revision() {
    let a = configured();
    let b = Db::new("b");
    let signer = DeviceIdentity::generate().unwrap();
    let key = ContentKey::generate().unwrap();
    let mut bundle = a.store.export_provider_bundle("revision-1", None).unwrap();
    let (header, bytes) = encode(&bundle, &signer, &key);
    b.store
        .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
        .unwrap();
    let (header, other) = encode(&bundle, &signer, &key);
    assert_ne!(other, bytes);
    assert_eq!(
        b.store
            .apply_encrypted_provider_bundle(&other, &header, &key, &signer.public_key())
            .unwrap(),
        ProviderApplyResult::Duplicate
    );
    bundle.profiles[0].api_key = "different-synthetic".into();
    let (header, bytes) = encode(&bundle, &signer, &key);
    assert!(
        b.store
            .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
            .is_err()
    );
}

#[test]
fn local_vault_and_concurrent_versions_are_never_silently_overwritten() {
    let a = configured();
    let b = Db::new("b");
    let signer = DeviceIdentity::generate().unwrap();
    let key = ContentKey::generate().unwrap();
    b.store
        .save_ai_provider_config(
            None,
            "local",
            "openai-compatible",
            "https://local.invalid",
            "local-model",
            "local-key",
            0.7,
            None,
        )
        .unwrap();
    let bundle = a.store.export_provider_bundle("revision-1", None).unwrap();
    let (header, bytes) = encode(&bundle, &signer, &key);
    assert_eq!(
        b.store
            .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
            .unwrap(),
        ProviderApplyResult::Conflict
    );
    assert_eq!(
        b.store
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        "local-key"
    );
    let conflict: String = b
        .store
        .connection
        .query_row("SELECT state FROM sync_provider_versions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(conflict, "conflict");
}

#[test]
fn local_edit_invalidates_import_base_before_remote_update() {
    let a = configured();
    let b = Db::new("b");
    let signer = DeviceIdentity::generate().unwrap();
    let key = ContentKey::generate().unwrap();
    let mut bundle = a.store.export_provider_bundle("revision-1", None).unwrap();
    let (header, bytes) = encode(&bundle, &signer, &key);
    b.store
        .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
        .unwrap();
    let active = b.store.active_ai_provider_config().unwrap().unwrap();
    b.store
        .save_ai_provider_config(
            Some(&active.id),
            &active.name,
            &active.provider_type,
            &active.base_url,
            &active.model,
            "LOCAL_ROTATION",
            active.temperature,
            active.max_tokens,
        )
        .unwrap();
    bundle.base_revision = Some("revision-1".into());
    bundle.revision_id = "revision-2".into();
    let (header, bytes) = encode(&bundle, &signer, &key);
    assert_eq!(
        b.store
            .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
            .unwrap(),
        ProviderApplyResult::Conflict
    );
    assert_eq!(
        b.store
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        "LOCAL_ROTATION"
    );
}

#[test]
fn invalid_bundle_and_revoked_sender_leave_no_partial_configuration() {
    let a = configured();
    let b = Db::new("b");
    let signer = DeviceIdentity::generate().unwrap();
    let key = ContentKey::generate().unwrap();
    let bundle = a.store.export_provider_bundle("revision-1", None).unwrap();
    let mut invalid = bundle.clone();
    invalid.active_id = Some("missing".into());
    let (header, bytes) = encode(&invalid, &signer, &key);
    assert!(
        b.store
            .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
            .is_err()
    );
    invalid = bundle.clone();
    invalid.profiles[1].name = invalid.profiles[0].name.clone();
    let (header, bytes) = encode(&invalid, &signer, &key);
    assert!(
        b.store
            .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
            .is_err()
    );
    b.store.revoke_sync_member("a").unwrap();
    let (header, bytes) = encode(&bundle, &signer, &key);
    assert!(
        b.store
            .apply_encrypted_provider_bundle(&bytes, &header, &key, &signer.public_key())
            .is_err()
    );
    assert!(b.store.list_ai_provider_configs().unwrap().is_empty());
    let count: i64 = b
        .store
        .connection
        .query_row("SELECT count(*) FROM sync_provider_versions", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

/// active 的双语义分野：显式激活进同步域（dirty 记账），运行时 failover
/// 落点不进。这是同步冲突审阅能否收敛的分水岭——两端对话行为各自改
/// active 会让 provider bundle 永久分叉（详见 set_active_ai_provider_config_for_sync）。
#[test]
fn runtime_failover_activates_without_entering_sync_domain() {
    let a = configured();
    let ids: Vec<String> = a
        .store
        .list_ai_provider_configs()
        .unwrap()
        .into_iter()
        .map(|row| row.id)
        .collect();
    let (first, second) = (&ids[0], &ids[1]);

    // 显式激活（用户意图）：is_active 生效且 dirty 记账。
    a.store
        .set_active_ai_provider_config_for_sync(first, true)
        .unwrap();
    let dirty_before: String = a
        .store
        .connection
        .query_row(
            "SELECT revision_id FROM sync_provider_dirty WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .unwrap();

    // 运行时 failover 落点：切换生效，但不进同步域。
    a.store
        .set_active_ai_provider_config_for_sync(second, false)
        .unwrap();
    let active: Option<String> = a
        .store
        .connection
        .query_row(
            "SELECT id FROM ai_provider_configs WHERE is_active=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(active.as_deref(), Some(second.as_str()));
    let dirty_after: Option<String> = a
        .store
        .connection
        .query_row(
            "SELECT revision_id FROM sync_provider_dirty WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        dirty_after.as_deref(),
        Some(dirty_before.as_str()),
        "failover 落点不能改写同步 dirty 的 revision"
    );
}
