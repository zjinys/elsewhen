use super::*;
use crate::{
    config::AppConfig,
    storage::sync::{ReplicaIdentity, snapshot::SnapshotExporter},
    sync::{
        crypto::ContentKey,
        pairing::{PairingInvitation, PendingJoin},
    },
};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("elsewhen-datasets-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn original(&self) -> Store {
        let store = Store::open(&self.0.join("elsewhen.db")).unwrap();
        store
            .submit_input_as_event("personal original", "capture", None)
            .unwrap();
        store
            .save_ai_provider_config(
                None,
                "personal provider",
                "openai-compatible",
                "https://personal.invalid",
                "local-model",
                "PERSONAL_SYNTHETIC_KEY",
                0.3,
                None,
            )
            .unwrap();
        store
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn imported(catalog: &DatasetCatalog) -> (DatasetEntry, [u8; 32]) {
    let entry = catalog
        .create_staging(
            "shared-dataset",
            &uuid::Uuid::new_v4().to_string(),
            "joined dataset",
        )
        .unwrap();
    let source = Store::open(&catalog.root.join(format!("source-{}.db", entry.id))).unwrap();
    let target = Store::open(&catalog.path_for(&entry.id).unwrap()).unwrap();
    let master_a = ContentKey::generate().unwrap().to_bytes();
    let master_b = ContentKey::generate().unwrap().to_bytes();
    let va = source
        .create_sync_vault("source-install", &master_a, "owner", "test")
        .unwrap();
    let vb = target
        .create_sync_vault(&entry.installation_id, &master_b, "joiner", "test")
        .unwrap();
    source
        .submit_input_as_event("shared original", "capture", None)
        .unwrap();
    source
        .save_ai_provider_config(
            None,
            "shared provider",
            "openai-compatible",
            "https://shared.invalid",
            "shared-model",
            "SHARED_SYNTHETIC_KEY",
            0.7,
            None,
        )
        .unwrap();
    for (store, vault) in [(&source, &va), (&target, &vb)] {
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: entry.dataset_id.clone(),
                device_id: vault.device().device_id.clone(),
                stream_id: vault.device().stream_id.clone(),
                processor_device_id: va.device().device_id.clone(),
            })
            .unwrap();
    }
    source
        .initialize_sync_authority(va.device(), va.identity())
        .unwrap();
    let now = 1_800_000_000_000;
    let (invite, pending) =
        PairingInvitation::issue(&entry.dataset_id, va.device().clone(), va.identity(), now)
            .unwrap();
    let (join, request) =
        PendingJoin::begin(invite, vb.device().clone(), vb.identity(), now).unwrap();
    let (completion, response) = pending
        .receive(&request, now)
        .unwrap()
        .approve_for_dataset(va.identity(), 1, now, source.sync_roster_anchor().unwrap())
        .unwrap();
    let approved = join.finish(&response, 0, now).unwrap();
    target
        .install_sync_vault_pair(&entry.installation_id, &master_b, 1, &approved)
        .unwrap();
    target.pin_sync_authority_from_pair(&approved).unwrap();
    let confirmed = completion
        .confirm(&approved.completion_proof(vb.identity()).unwrap(), now)
        .unwrap();
    let roster = source
        .grant_sync_pair(1, &confirmed, va.identity())
        .unwrap();
    target
        .apply_signed_roster(&roster.encode().unwrap())
        .unwrap();
    let target = target.with_provider_master(master_b);
    target.enable_provider_key_sealing().unwrap();
    let mut export = SnapshotExporter::new(&source, &vb.device().device_id, "snapshot", 1).unwrap();
    target
        .begin_snapshot_import("snapshot", &va.device().device_id, 1)
        .unwrap();
    let mut index = 0;
    while let Some(bytes) = export
        .next_encrypted_chunk(&confirmed.send_key, va.identity())
        .unwrap()
    {
        target
            .stage_snapshot_chunk(
                "snapshot",
                index,
                &va.device().device_id,
                1,
                &bytes,
                &approved.receive_key,
                &va.identity().public_key(),
            )
            .unwrap();
        index += 1;
    }
    let manifest = export.finish(&confirmed.send_key, va.identity()).unwrap();
    assert!(
        target
            .publish_snapshot(
                "snapshot",
                &va.device().device_id,
                1,
                &manifest,
                &approved.receive_key,
                &va.identity().public_key()
            )
            .unwrap()
    );
    (entry, master_b)
}

#[test]
fn selected_dataset_changes_only_after_restart_and_personal_data_is_preserved() {
    let _guard = crate::config::test_env_guard();
    crate::config::unpin();
    let workspace = Workspace::new();
    let original = workspace.original();
    let pinned = AppConfig::pin(Some(workspace.0.to_str().unwrap())).unwrap();
    let catalog = DatasetCatalog::open(&workspace.0).unwrap();
    let (entry, master) = imported(&catalog);
    let selection = catalog.prepare_selection(&entry.id).unwrap();
    catalog.select_import(&selection, &master, 2).unwrap();
    assert_eq!(
        AppConfig::load().unwrap().database_path,
        pinned.database_path
    );
    Store::open(&AppConfig::load().unwrap().database_path)
        .unwrap()
        .submit_input_as_event("recorded before restart", "capture", None)
        .unwrap();
    assert_eq!(original.list_events().unwrap().len(), 2);
    assert_eq!(
        original
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        "PERSONAL_SYNTHETIC_KEY"
    );
    crate::config::unpin();
    let switched = AppConfig::pin(Some(workspace.0.to_str().unwrap())).unwrap();
    assert_eq!(switched.data_dir, workspace.0);
    assert_eq!(switched.database_path, catalog.path_for(&entry.id).unwrap());
    let joined = Store::open(&switched.database_path).unwrap().with_provider_master(master);
    assert_eq!(joined.list_events().unwrap().len(), 1);
    assert_eq!(joined.list_events().unwrap()[0].raw_text, "shared original");
    assert_eq!(
        joined.active_ai_provider_config().unwrap().unwrap().api_key,
        "SHARED_SYNTHETIC_KEY"
    );
    catalog.select_default(catalog.revision().unwrap()).unwrap();
    assert_eq!(
        AppConfig::load().unwrap().database_path,
        switched.database_path
    );
    crate::config::unpin();
    assert_eq!(
        AppConfig::pin(Some(workspace.0.to_str().unwrap()))
            .unwrap()
            .database_path,
        pinned.database_path
    );
    crate::config::unpin();
}

#[test]
fn incomplete_wrong_key_and_stale_previews_cannot_change_selection() {
    let workspace = Workspace::new();
    let _original = workspace.original();
    let catalog = DatasetCatalog::open(&workspace.0).unwrap();
    let empty = catalog
        .create_staging("dataset", "unused-installation", "empty")
        .unwrap();
    assert!(catalog.prepare_selection(&empty.id).is_err());
    assert!(selected_database_path(&workspace.0).unwrap().is_none());
    let (entry, master) = imported(&catalog);
    let preview = catalog.prepare_selection(&entry.id).unwrap();
    assert!(catalog.select_import(&preview, &[0; 32], 2).is_err());
    assert!(catalog.select_import(&preview, &master, 3).is_err());
    assert!(selected_database_path(&workspace.0).unwrap().is_none());
    assert!(
        !catalog
            .list()
            .unwrap()
            .iter()
            .find(|e| e.id == entry.id)
            .unwrap()
            .ready
    );
    catalog
        .create_staging("another", "different-installation", "another")
        .unwrap();
    assert!(catalog.select_import(&preview, &master, 2).is_err());
    let fresh = catalog.prepare_selection(&entry.id).unwrap();
    catalog.select_import(&fresh, &master, 2).unwrap();
    assert!(catalog.select_default(fresh.catalog_revision).is_err());
}

#[test]
fn catalog_commit_failure_retains_original_selection_and_the_completed_import() {
    let workspace = Workspace::new();
    let original = workspace.original();
    let catalog = DatasetCatalog::open(&workspace.0).unwrap();
    let (entry, master) = imported(&catalog);
    let preview = catalog.prepare_selection(&entry.id).unwrap();
    catalog.connection.execute_batch("CREATE TRIGGER reject_selection BEFORE UPDATE ON workspace BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    assert!(catalog.select_import(&preview, &master, 2).is_err());
    assert!(selected_database_path(&workspace.0).unwrap().is_none());
    assert!(
        !catalog
            .list()
            .unwrap()
            .iter()
            .find(|e| e.id == entry.id)
            .unwrap()
            .ready
    );
    assert_eq!(original.list_events().unwrap().len(), 1);
    assert_eq!(
        Store::open(&catalog.path_for(&entry.id).unwrap())
            .unwrap()
            .list_events()
            .unwrap()
            .len(),
        1
    );
    catalog
        .connection
        .execute_batch("DROP TRIGGER reject_selection")
        .unwrap();
    catalog.select_import(&preview, &master, 2).unwrap();
}

#[test]
fn startup_never_falls_back_when_selected_data_is_missing_or_wrong() {
    let workspace = Workspace::new();
    let original = workspace.original();
    let catalog = DatasetCatalog::open(&workspace.0).unwrap();
    let (entry, master) = imported(&catalog);
    catalog
        .select_import(&catalog.prepare_selection(&entry.id).unwrap(), &master, 2)
        .unwrap();
    let path = catalog.path_for(&entry.id).unwrap();
    {
        let store = Store::open(&path).unwrap();
        store
            .connection
            .execute("UPDATE sync_replica SET dataset_id='another-dataset'", [])
            .unwrap();
    }
    assert!(selected_database_path(&workspace.0).is_err());
    std::fs::remove_file(&path).unwrap();
    assert!(selected_database_path(&workspace.0).is_err());
    catalog.select_default(catalog.revision().unwrap()).unwrap();
    assert!(selected_database_path(&workspace.0).unwrap().is_none());
    assert_eq!(
        original
            .active_ai_provider_config()
            .unwrap()
            .unwrap()
            .api_key,
        "PERSONAL_SYNTHETIC_KEY"
    );
}

#[test]
fn catalog_limits_names_ids_installations_and_refuses_unrelated_files() {
    let workspace = Workspace::new();
    assert!(selected_database_path(&workspace.0).unwrap().is_none());
    let catalog = DatasetCatalog::open(&workspace.0).unwrap();
    assert!(
        catalog
            .create_staging("../escape", "install", "bad")
            .is_err()
    );
    assert!(
        catalog
            .create_staging("dataset", "install", "bad\nname")
            .is_err()
    );
    catalog
        .create_staging("dataset", "install", "valid")
        .unwrap();
    assert!(
        catalog
            .create_staging("dataset", "install", "duplicate")
            .is_err()
    );
    assert!(catalog.path_for("../escape").is_err());
    for i in 1..32 {
        catalog
            .create_staging("dataset", &format!("install-{i}"), "bounded")
            .unwrap();
    }
    assert!(
        catalog
            .create_staging("dataset", "overflow", "overflow")
            .is_err()
    );
    assert_eq!(catalog.list().unwrap().len(), 32);
    let other = Workspace::new();
    std::fs::write(other.0.join(CATALOG), b"not a catalog").unwrap();
    assert!(DatasetCatalog::open(&other.0).is_err());
    assert!(selected_database_path(&other.0).is_err());
    assert_eq!(
        std::fs::read(other.0.join(CATALOG)).unwrap(),
        b"not a catalog"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_catalog_and_dataset_files_are_not_followed() {
    use std::os::unix::fs::symlink;
    let workspace = Workspace::new();
    let outside = Workspace::new();
    symlink(outside.0.join("absent.db"), workspace.0.join(CATALOG)).unwrap();
    assert!(DatasetCatalog::open(&workspace.0).is_err());
    assert!(selected_database_path(&workspace.0).is_err());
    assert!(!outside.0.join("absent.db").exists());
    std::fs::remove_file(workspace.0.join(CATALOG)).unwrap();
    let catalog = DatasetCatalog::open(&workspace.0).unwrap();
    let entry = catalog
        .create_staging("dataset", "install", "valid")
        .unwrap();
    let path = catalog.path_for(&entry.id).unwrap();
    symlink(outside.0.join("absent.db"), &path).unwrap();
    assert!(catalog.path_for(&entry.id).is_err());
    assert!(!outside.0.join("absent.db").exists());
}
