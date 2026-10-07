use super::*;
use crate::sync::{
    crypto::{self, ContentKey, EncryptedEnvelope, EnvelopeHeader},
    pairing::{PairingInvitation, PendingJoin},
};

#[test]
fn initial_snapshot_preserves_each_installations_own_vault() {
    use crate::storage::sync::{ReplicaIdentity, snapshot::SnapshotExporter};
    let a = Database::new();
    let b = Database::new();
    let key_a = ContentKey::generate().unwrap().to_bytes();
    let key_b = ContentKey::generate().unwrap().to_bytes();
    let vault_a = a.store.create_sync_vault("a", &key_a, "a", "test").unwrap();
    let vault_b = b.store.create_sync_vault("b", &key_b, "b", "test").unwrap();
    let before = b.ciphertext();
    for (db, vault) in [(&a, &vault_a), (&b, &vault_b)] {
        db.store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "dataset".into(),
                device_id: vault.device().device_id.clone(),
                stream_id: vault.device().stream_id.clone(),
                processor_device_id: vault_a.device().device_id.clone(),
            })
            .unwrap();
    }
    a.store
        .authorize_sync_member(&vault_b.device().device_id, &vault_b.device().stream_id)
        .unwrap();
    b.store
        .authorize_sync_member(&vault_a.device().device_id, &vault_a.device().stream_id)
        .unwrap();
    a.store
        .submit_input_as_event("shared fact", "capture", None)
        .unwrap();
    let content = ContentKey::generate().unwrap();
    let mut exporter =
        SnapshotExporter::new(&a.store, &vault_b.device().device_id, "vault-boundary", 1).unwrap();
    b.store
        .begin_snapshot_import("vault-boundary", &vault_a.device().device_id, 1)
        .unwrap();
    let mut index = 0;
    while let Some(chunk) = exporter
        .next_encrypted_chunk(&content, vault_a.identity())
        .unwrap()
    {
        b.store
            .stage_snapshot_chunk(
                "vault-boundary",
                index,
                &vault_a.device().device_id,
                1,
                &chunk,
                &content,
                &vault_a.device().signing_key,
            )
            .unwrap();
        index += 1;
    }
    let manifest = exporter.finish(&content, vault_a.identity()).unwrap();
    assert!(
        b.store
            .publish_snapshot(
                "vault-boundary",
                &vault_a.device().device_id,
                1,
                &manifest,
                &content,
                &vault_a.device().signing_key
            )
            .unwrap()
    );
    assert_eq!(b.ciphertext(), before);
    assert_eq!(
        b.store.load_sync_vault("b", &key_b, 1).unwrap().device(),
        vault_b.device()
    );
    assert!(b.store.load_sync_vault("a", &key_a, 1).is_err());
    assert_eq!(b.store.list_events().unwrap().len(), 1);
}

struct Database {
    store: Store,
    path: std::path::PathBuf,
}
impl Database {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("elsewhen-vault-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self {
            store: Store::open(&path.join("test.db")).unwrap(),
            path,
        }
    }
    fn ciphertext(&self) -> Vec<u8> {
        self.store
            .connection
            .query_row("SELECT encrypted_secrets FROM sync_local_vault", [], |r| {
                r.get(0)
            })
            .unwrap()
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn pair(a: &LocalVault, b: &LocalVault, version: u64) -> (ApprovedPair, ApprovedPair) {
    let now = 1_800_000_000_000;
    let (invitation, pending) =
        PairingInvitation::issue("dataset", a.device().clone(), a.identity(), now).unwrap();
    let (join, request) =
        PendingJoin::begin(invitation, b.device().clone(), b.identity(), now).unwrap();
    let (completion, response) = pending
        .receive(&request, now)
        .unwrap()
        .approve(a.identity(), version, now)
        .unwrap();
    let b_pair = join.finish(&response, version - 1, now).unwrap();
    let a_pair = completion
        .confirm(&b_pair.completion_proof(b.identity()).unwrap(), now)
        .unwrap();
    (a_pair, b_pair)
}

#[test]
fn device_signing_identity_and_directional_keys_survive_encrypted_persistence() {
    let a = Database::new();
    let b = Database::new();
    let a_master = ContentKey::generate().unwrap().to_bytes();
    let b_master = ContentKey::generate().unwrap().to_bytes();
    let a_vault = a
        .store
        .create_sync_vault("install-a", &a_master, "device-a", "linux")
        .unwrap();
    let b_vault = b
        .store
        .create_sync_vault("install-b", &b_master, "device-b", "android")
        .unwrap();
    let (a_pair, b_pair) = pair(&a_vault, &b_vault, 1);
    assert_eq!(
        a.store
            .install_sync_vault_pair("install-a", &a_master, 1, &a_pair)
            .unwrap(),
        2
    );
    assert_eq!(
        b.store
            .install_sync_vault_pair("install-b", &b_master, 1, &b_pair)
            .unwrap(),
        2
    );
    let reopened = Store::open(&a.store.path).unwrap();
    let a_loaded = reopened.load_sync_vault("install-a", &a_master, 2).unwrap();
    let b_loaded = b.store.load_sync_vault("install-b", &b_master, 2).unwrap();
    assert_eq!(a_loaded.device(), a_vault.device());
    assert_eq!(
        a_loaded.identity().public_key(),
        a_vault.identity().public_key()
    );
    assert_eq!(a_loaded.mailbox_owner(), a_vault.mailbox_owner());
    let (send, _, version) = a_loaded
        .peer_keys("dataset", &b_loaded.device().device_id)
        .unwrap();
    let (_, receive, _) = b_loaded
        .peer_keys("dataset", &a_loaded.device().device_id)
        .unwrap();
    let header = EnvelopeHeader::new(
        "dataset",
        &a_loaded.device().device_id,
        &b_loaded.device().device_id,
        version,
        "vault-roundtrip",
    )
    .unwrap();
    let envelope =
        crypto::seal(header.clone(), &send, a_loaded.identity(), b"after restart").unwrap();
    assert_eq!(
        crypto::open(
            &envelope,
            &header,
            &receive,
            &a_loaded.identity().public_key()
        )
        .unwrap(),
        b"after restart"
    );
    let bytes = a.ciphertext();
    for secret in [
        a_master.to_vec(),
        a_loaded.identity().to_pkcs8(),
        send.to_bytes().to_vec(),
        a_loaded.mailbox_owner().to_vec(),
    ] {
        assert!(!bytes.windows(secret.len()).any(|part| part == secret));
    }
    assert!(!format!("{a_loaded:?}").contains(&a_loaded.device().device_id));
    assert!(
        a.store.sync_identity().unwrap().is_none(),
        "key installation does not activate a dataset or relay grant"
    );
    assert!(
        EncryptedEnvelope::from_bytes(&bytes).is_err(),
        "local vault is not a transferable content envelope"
    );
}

#[test]
fn local_vault_authenticates_installation_revision_and_key_without_replacing_corrupt_data() {
    let db = Database::new();
    let key = ContentKey::generate().unwrap().to_bytes();
    let vault = db
        .store
        .create_sync_vault("installation", &key, "local", "test")
        .unwrap();
    let original = db.ciphertext();
    assert!(
        db.store
            .create_sync_vault("installation", &key, "replacement", "test")
            .is_err()
    );
    assert!(db.store.load_sync_vault("other", &key, 1).is_err());
    assert!(
        db.store
            .load_sync_vault("installation", &[0; 32], 1)
            .is_err()
    );
    assert!(db.store.load_sync_vault("installation", &key, 2).is_err());
    assert!(LocalVault::open("other", 1, &original, &key).is_err());
    assert!(LocalVault::open("installation", 2, &original, &key).is_err());
    let mut bad = original.clone();
    bad[20] ^= 1;
    assert!(LocalVault::open("installation", 1, &bad, &key).is_err());
    assert!(LocalVault::open("installation", 1, &[], &key).is_err());
    assert!(LocalVault::open("installation", 1, &vec![0; 600 * 1024], &key).is_err());
    assert_eq!(db.ciphertext(), original);
    assert_ne!(
        vault.seal(&key).unwrap(),
        original,
        "rewrites use fresh nonces"
    );
}

#[test]
fn pair_version_and_vault_revision_checks_fence_replay_concurrent_updates_and_revocation() {
    let a = Database::new();
    let b = Database::new();
    let key = ContentKey::generate().unwrap().to_bytes();
    let a_vault = a.store.create_sync_vault("a", &key, "a", "test").unwrap();
    let b_vault = b.store.create_sync_vault("b", &key, "b", "test").unwrap();
    let (first, _) = pair(&a_vault, &b_vault, 1);
    assert_eq!(
        a.store
            .install_sync_vault_pair("a", &key, 1, &first)
            .unwrap(),
        2
    );
    let stored = a.ciphertext();
    assert!(
        a.store
            .install_sync_vault_pair("a", &key, 1, &first)
            .is_err()
    );
    assert_eq!(
        a.store
            .install_sync_vault_pair("a", &key, 2, &first)
            .unwrap(),
        2
    );
    assert_eq!(a.ciphertext(), stored);
    let (changed_same_version, _) = pair(&a_vault, &b_vault, 1);
    assert!(
        a.store
            .install_sync_vault_pair("a", &key, 2, &changed_same_version)
            .is_err()
    );
    assert_eq!(
        a.store
            .revoke_sync_vault_peer("a", &key, 2, "dataset", &b_vault.device().device_id)
            .unwrap(),
        3
    );
    assert!(
        a.store
            .load_sync_vault("a", &key, 3)
            .unwrap()
            .peer_keys("dataset", &b_vault.device().device_id)
            .is_err()
    );
    assert!(
        a.store
            .install_sync_vault_pair("a", &key, 3, &first)
            .is_err()
    );
    let (renewed, _) = pair(&a_vault, &b_vault, 2);
    assert_eq!(
        a.store
            .install_sync_vault_pair("a", &key, 3, &renewed)
            .unwrap(),
        4
    );
    assert_eq!(
        a.store
            .load_sync_vault("a", &key, 4)
            .unwrap()
            .peer_keys("dataset", &b_vault.device().device_id)
            .unwrap()
            .2,
        2
    );
    assert!(
        a.store
            .install_sync_vault_pair("a", &key, 4, &first)
            .is_err()
    );
    assert!(
        b.store
            .install_sync_vault_pair("b", &key, 1, &first)
            .is_err()
    );
}

#[test]
fn failed_vault_update_rolls_back_and_missing_identity_is_not_regenerated() {
    let a = Database::new();
    let b = Database::new();
    let key = ContentKey::generate().unwrap().to_bytes();
    assert!(a.store.load_sync_vault("a", &key, 1).is_err());
    let a_vault = a.store.create_sync_vault("a", &key, "a", "test").unwrap();
    let b_vault = b.store.create_sync_vault("b", &key, "b", "test").unwrap();
    let (pair, _) = pair(&a_vault, &b_vault, 1);
    let before = a.ciphertext();
    a.store.connection.execute_batch("CREATE TRIGGER vault_failure BEFORE UPDATE ON sync_local_vault BEGIN SELECT RAISE(ABORT,'synthetic vault failure'); END;").unwrap();
    assert!(
        a.store
            .install_sync_vault_pair("a", &key, 1, &pair)
            .is_err()
    );
    assert_eq!(a.ciphertext(), before);
    assert_eq!(a.store.load_sync_vault("a", &key, 1).unwrap().revision, 1);
    let c = Database::new();
    c.store
        .initialize_sync_replica(&crate::storage::sync::ReplicaIdentity {
            dataset_id: "dataset".into(),
            device_id: "legacy".into(),
            stream_id: "legacy-stream".into(),
            processor_device_id: "legacy".into(),
        })
        .unwrap();
    assert!(
        c.store
            .create_sync_vault("c", &key, "replacement", "test")
            .is_err()
    );
    assert!(c.store.load_sync_vault("c", &key, 1).is_err());
}
