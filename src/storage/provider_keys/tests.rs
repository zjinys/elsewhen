use super::*;
use crate::{
    storage::sync::ReplicaIdentity,
    sync::{
        crypto::{ContentKey, DeviceIdentity, EnvelopeHeader},
        protocol::ProviderBundle,
    },
};

#[test]
fn stale_edit_basis_cannot_mix_a_new_key_with_an_old_destination() {
    let f = Fixture::new(true);
    let id = f.save("profile", "OLD_KEY");
    let shown = f.store.provider_edit_token(&id).unwrap();
    f.store
        .save_ai_provider_config(
            Some(&id),
            "profile",
            "openai-compatible",
            "https://new.invalid",
            "model",
            "NEW_KEY",
            0.5,
            None,
        )
        .unwrap();
    let current = f.store.provider_edit_token(&id).unwrap();
    assert_ne!(shown, current);
    let tx = f.store.begin_write_transaction().unwrap();
    assert!(
        f.store
            .validate_provider_edit_token(&id, Some(&shown))
            .is_err()
    );
    assert!(f.store.validate_provider_edit_token(&id, None).is_err());
    f.store
        .validate_provider_edit_token(&id, Some(&current))
        .unwrap();
    tx.unwrap().commit().unwrap();
    let actual = f.store.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(actual.base_url, "https://new.invalid");
    assert_eq!(actual.api_key, "NEW_KEY");
}

struct Fixture {
    store: Store,
    dir: std::path::PathBuf,
    master: [u8; 32],
    signer: DeviceIdentity,
}
impl Fixture {
    fn new(sealed: bool) -> Self {
        let dir =
            std::env::temp_dir().join(format!("elsewhen-provider-sealed-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let master = ContentKey::generate().unwrap().to_bytes();
        let store = Store::open(&dir.join("test.db"))
            .unwrap()
            .with_provider_master(master);
        let vault = store
            .create_sync_vault("installation", &master, "device", "test")
            .unwrap();
        let signer = DeviceIdentity::from_pkcs8(&vault.identity().to_pkcs8()).unwrap();
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "dataset".into(),
                device_id: vault.device().device_id.clone(),
                stream_id: vault.device().stream_id.clone(),
                processor_device_id: vault.device().device_id.clone(),
            })
            .unwrap();
        if sealed {
            store.enable_provider_key_sealing().unwrap();
        }
        Self {
            store,
            dir,
            master,
            signer,
        }
    }
    fn save(&self, name: &str, key: &str) -> String {
        self.store
            .save_ai_provider_config(
                None,
                name,
                "openai-compatible",
                "https://synthetic.invalid/v1",
                "model",
                key,
                0.5,
                Some(1024),
            )
            .unwrap()
    }
    fn count(&self, table: &str) -> i64 {
        self.store
            .connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn sealing_preserves_logical_config_and_locked_metadata_but_never_returns_runtime_keys() {
    let f = Fixture::new(false);
    let id = f.save("profile", "SYNTHETIC_ORIGINAL_KEY");
    let before = f.store.export_provider_bundle("revision", None).unwrap();
    f.store.enable_provider_key_sealing().unwrap();
    let after = f.store.export_provider_bundle("revision", None).unwrap();
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(after).unwrap()
    );
    let raw: Option<String> = f
        .store
        .connection
        .query_row(
            "SELECT api_key FROM ai_provider_configs WHERE id=?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(raw.is_none());
    let ciphertext: Vec<u8> = f
        .store
        .connection
        .query_row(
            "SELECT ciphertext FROM provider_key_secrets WHERE config_id=?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!String::from_utf8_lossy(&ciphertext).contains("SYNTHETIC_ORIGINAL_KEY"));
    let locked = Store::open(&f.store.path).unwrap();
    assert_eq!(locked.list_ai_provider_configs().unwrap().len(), 1);
    assert_eq!(
        locked
            .active_ai_provider_metadata()
            .unwrap()
            .unwrap()
            .api_key,
        ""
    );
    assert!(locked.active_ai_provider_config().is_err());
    assert!(locked.list_ai_provider_configs_for_runtime().is_err());
    assert!(locked.export_provider_bundle("revision", None).is_err());
    assert_eq!(
        f.store
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        "SYNTHETIC_ORIGINAL_KEY"
    );
}

#[test]
fn settings_empty_key_keeps_secret_and_explicit_bundle_empty_or_missing_values_remain_distinct() {
    let f = Fixture::new(true);
    let id = f.save("profile", "ORIGINAL_KEY");
    f.store
        .save_ai_provider_config(
            Some(&id),
            "renamed",
            "openai-compatible",
            "https://changed.invalid/v1",
            "new-model",
            "",
            0.2,
            Some(2048),
        )
        .unwrap();
    let active = f.store.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(active.api_key, "ORIGINAL_KEY");
    assert_eq!(active.base_url, "https://changed.invalid/v1");
    {
        let tx = f.store.begin_write_transaction().unwrap();
        f.store.write_provider_key_on(&id, Some("")).unwrap();
        tx.unwrap().commit().unwrap();
    }
    assert_eq!(f.store.read_provider_key(&id).unwrap(), Some(String::new()));
    assert!(
        f.store
            .export_provider_bundle("empty", None)
            .unwrap()
            .profiles[0]
            .api_key_present
    );
    {
        let tx = f.store.begin_write_transaction().unwrap();
        f.store.write_provider_key_on(&id, None).unwrap();
        tx.unwrap().commit().unwrap();
    }
    assert!(f.store.active_ai_provider_config().unwrap().is_none());
    assert!(
        !f.store
            .export_provider_bundle("missing", None)
            .unwrap()
            .profiles[0]
            .api_key_present
    );
    assert_eq!(f.count("provider_key_secrets"), 0);
}

#[test]
fn wrong_key_tampering_and_plaintext_write_attempts_fail_closed() {
    let f = Fixture::new(true);
    let id = f.save("profile", "SYNTHETIC_KEY");
    let wrong = Store::open(&f.store.path)
        .unwrap()
        .with_provider_master([0; 32]);
    assert!(wrong.active_ai_provider_config().is_err());
    assert!(
        f.store
            .connection
            .execute(
                "UPDATE ai_provider_configs SET api_key='plaintext' WHERE id=?1",
                [&id]
            )
            .is_err()
    );
    assert!(
        f.store
            .connection
            .execute("DELETE FROM provider_key_policy", [])
            .is_err()
    );
    f.store
        .connection
        .execute(
            "UPDATE ai_provider_configs SET base_url='https://tampered.invalid' WHERE id=?1",
            [&id],
        )
        .unwrap();
    assert!(
        f.store.active_ai_provider_config().is_err(),
        "changed destination cannot silently receive the old key"
    );
}

#[test]
fn failed_migration_and_locked_edits_leave_original_rows_intact() {
    let f = Fixture::new(false);
    let first = f.save("first", "KEEP_ORIGINAL");
    f.save("too-large", &"x".repeat(MAX_KEY_BYTES + 1));
    assert!(f.store.enable_provider_key_sealing().is_err());
    assert!(!f.store.provider_keys_sealed().unwrap());
    assert_eq!(f.count("provider_key_secrets"), 0);
    assert_eq!(
        f.store.read_provider_key(&first).unwrap().as_deref(),
        Some("KEEP_ORIGINAL")
    );
    let protected = Fixture::new(true);
    let id = protected.save("protected", "KEEP_SECRET");
    let locked = Store::open(&protected.store.path).unwrap();
    assert!(
        locked
            .save_ai_provider_config(
                Some(&id),
                "changed",
                "openai-compatible",
                "https://other.invalid",
                "model",
                "replacement",
                0.1,
                None
            )
            .is_err()
    );
    let current = protected
        .store
        .active_ai_provider_config()
        .unwrap()
        .unwrap();
    assert_eq!(current.name, "protected");
    assert_eq!(current.api_key, "KEEP_SECRET");
}

#[test]
fn encrypted_provider_delivery_seals_before_binding_to_receiver_sql() {
    let a = Fixture::new(true);
    let b = Fixture::new(true);
    let id = a.save("source", "ONLY_IN_ENCRYPTED_STORAGE");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    a.store
        .save_ai_provider_config(
            Some(&id),
            "source",
            "openai-compatible",
            &url,
            "model",
            "",
            0.5,
            Some(1024),
        )
        .unwrap();
    let server = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        loop {
            let mut bytes = [0; 4096];
            let n = socket.read(&mut bytes).unwrap();
            assert!(n > 0 && request.len() + n <= 65536);
            request.extend_from_slice(&bytes[..n]);
            if let Some(end) = request.windows(4).position(|b| b == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                if request.len() >= end + 4 + length {
                    assert!(headers.contains("authorization: bearer only_in_encrypted_storage"));
                    break;
                }
            }
        }
        let body =
            r#"{"choices":[{"message":{"role":"assistant","content":"sealed key worked"}}]}"#;
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    let identity_a = a.store.sync_identity().unwrap().unwrap();
    let identity_b = b.store.sync_identity().unwrap().unwrap();
    b.store
        .authorize_sync_member(&identity_a.device_id, &identity_a.stream_id)
        .unwrap();
    let key = ContentKey::generate().unwrap();
    let bundle = a.store.export_provider_bundle("provider-v1", None).unwrap();
    let header = EnvelopeHeader::new(
        "dataset",
        &identity_a.device_id,
        &identity_b.device_id,
        1,
        "provider:provider-v1",
    )
    .unwrap();
    let wire = crate::sync::crypto::seal(
        header.clone(),
        &key,
        &a.signer,
        &serde_json::to_vec(&bundle).unwrap(),
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    b.store
        .apply_encrypted_provider_bundle(&wire, &header, &key, &a.signer.public_key())
        .unwrap();
    assert_eq!(
        b.store
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        "ONLY_IN_ENCRYPTED_STORAGE"
    );
    use crate::ai::provider::{AiProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider};
    let config = b.store.active_ai_provider_config().unwrap().unwrap();
    let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
        base_url: config.base_url,
        api_key: config.api_key,
        model: config.model,
        temperature: config.temperature as f32,
        max_tokens: config.max_tokens.map(|n| n as u32),
        context_window: config.context_window.map(|n| n as u32),
    })
    .unwrap();
    assert_eq!(
        provider.generate_reply(vec![]).unwrap().content,
        "sealed key worked"
    );
    server.join().unwrap();
    let plaintext: bool = b
        .store
        .connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM ai_provider_configs WHERE api_key IS NOT NULL)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!plaintext);
    b.store
        .connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&std::fs::read(&b.store.path).unwrap())
            .contains("ONLY_IN_ENCRYPTED_STORAGE")
    );
    let locked = Store::open(&b.store.path).unwrap();
    assert!(locked.active_ai_provider_config().is_err());
    let reopened = Store::open(&b.store.path)
        .unwrap()
        .with_provider_master(b.master);
    assert_eq!(
        reopened
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        "ONLY_IN_ENCRYPTED_STORAGE"
    );
}

#[test]
fn snapshot_read_view_preserves_local_key_capability_and_receiver_sealing_policy() {
    use crate::{
        storage::sync::snapshot::SnapshotExporter,
        sync::engine::{self, EncryptionPeer},
    };
    let a = Fixture::new(true);
    let b = Fixture::new(true);
    a.save("source", "SNAPSHOT_AT_REST_KEY");
    let ia = a.store.sync_identity().unwrap().unwrap();
    let ib = b.store.sync_identity().unwrap().unwrap();
    a.store
        .authorize_sync_member(&ib.device_id, &ib.stream_id)
        .unwrap();
    b.store
        .authorize_sync_member(&ia.device_id, &ia.stream_id)
        .unwrap();
    let key = ContentKey::generate().unwrap();
    engine::stage_provider_updates(
        &a.store,
        &[EncryptionPeer {
            device_id: &ib.device_id,
            key_version: 1,
            key: &key,
        }],
        &a.signer,
    )
    .unwrap();
    let mut exporter =
        SnapshotExporter::new(&a.store, &ib.device_id, "sealed-snapshot", 1).unwrap();
    b.store
        .begin_snapshot_import("sealed-snapshot", &ia.device_id, 1)
        .unwrap();
    let mut index = 0;
    while let Some(bytes) = exporter.next_encrypted_chunk(&key, &a.signer).unwrap() {
        b.store
            .stage_snapshot_chunk(
                "sealed-snapshot",
                index,
                &ia.device_id,
                1,
                &bytes,
                &key,
                &a.signer.public_key(),
            )
            .unwrap();
        index += 1;
    }
    let manifest = exporter.finish(&key, &a.signer).unwrap();
    assert!(
        b.store
            .publish_snapshot(
                "sealed-snapshot",
                &ia.device_id,
                1,
                &manifest,
                &key,
                &a.signer.public_key()
            )
            .unwrap()
    );
    let bundle: ProviderBundle = b.store.current_provider_bundle().unwrap();
    assert_eq!(bundle.profiles[0].api_key, "SNAPSHOT_AT_REST_KEY");
    assert_eq!(b.count("provider_key_secrets"), 1);
    assert!(b.store.provider_keys_sealed().unwrap());
}

/// 复制 = 字段 + Key 全量克隆到新 id；不改变激活态；撞名自动递增。
/// sealed/明文两种存储态都要成立：sealed 下 Key 经解密后按新 id 重新密封。
#[test]
fn duplicate_clones_fields_and_key_without_activation_or_colliding_names() {
    for sealed in [false, true] {
        let f = Fixture::new(sealed);
        let id = f.save("主用 GPT", "SECRET_KEY");
        f.store
            .connection
            .execute(
                "UPDATE ai_provider_configs SET context_window=65536 WHERE id=?1",
                [&id],
            )
            .unwrap();

        let first = f.store.duplicate_ai_provider_config(&id).unwrap();
        let second = f.store.duplicate_ai_provider_config(&id).unwrap();

        let configs = f.store.list_ai_provider_configs().unwrap();
        assert_eq!(configs.len(), 3);
        let dup = configs.iter().find(|c| c.id == first).unwrap();
        assert_eq!(dup.name, "主用 GPT 副本");
        assert_eq!(
            f.store.read_provider_key(&dup.id).unwrap().as_deref(),
            Some("SECRET_KEY")
        );
        assert_eq!(dup.base_url, "https://synthetic.invalid/v1");
        assert_eq!(dup.context_window, Some(65536));
        assert!(!dup.is_active);
        let dup2 = configs.iter().find(|c| c.id == second).unwrap();
        assert_eq!(dup2.name, "主用 GPT 副本 2");
        assert_eq!(
            f.store.read_provider_key(&dup2.id).unwrap().as_deref(),
            Some("SECRET_KEY")
        );

        // 原配置保持激活，复制不打扰运行中的对话。
        let active = f.store.active_ai_provider_config().unwrap().unwrap();
        assert_eq!(active.id, id);
        if sealed {
            assert_eq!(f.count("provider_key_secrets"), 3);
        }
    }
}

/// 复制不存在的配置直接报错，不产生空壳行。
#[test]
fn duplicate_missing_config_fails_without_side_effects() {
    let f = Fixture::new(true);
    assert!(f.store.duplicate_ai_provider_config("missing").is_err());
    assert_eq!(f.count("ai_provider_configs"), 0);
}
