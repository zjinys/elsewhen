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
    export.declare_blob("blob-snapshot-aaaaaaaa");
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
    let joined = Store::open(&switched.database_path)
        .unwrap()
        .with_provider_master(master);
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

/// 曾在真实机器上发生：某次失败的调用留下 0 字节 catalog，
/// 只读启动路径把它当成 schema 损坏直接拒化，整个应用起不来。
/// 空文件等价于“没有数据集”，应回落主库；真正有表的才报错。
#[test]
fn empty_catalog_file_at_startup_falls_back_to_main_database() {
    let workspace = Workspace::new();
    let _original = workspace.original();
    std::fs::write(workspace.0.join("sync-workspace.db"), b"").unwrap();
    assert!(selected_database_path(&workspace.0).unwrap().is_none());

    // 有内容但不符 schema 的文件仍然报错，不静默忽略。
    std::fs::write(workspace.0.join("sync-workspace.db"), b"not a database").unwrap();
    assert!(selected_database_path(&workspace.0).is_err());
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

#[test]
fn discard_staging_removes_row_and_folder() {
    let workspace = Workspace::new();
    let catalog = DatasetCatalog::open(&workspace.0).unwrap();
    let entry = catalog
        .create_staging("shared-dataset", "installation", "joined")
        .unwrap();
    let folder = workspace.0.join("datasets").join(&entry.id);
    assert!(folder.is_dir());

    catalog.discard_staging(&entry.id).unwrap();
    assert!(!folder.exists());
    assert!(catalog.list().unwrap().iter().all(|e| e.id != entry.id));
    // 幂等重试：目录行已删，再删报 unknown。
    assert!(catalog.discard_staging(&entry.id).is_err());
}

#[test]
fn discard_refuses_ready_datasets_and_unknown_ids() {
    let workspace = Workspace::new();
    let catalog = DatasetCatalog::open(&workspace.0).unwrap();
    let entry = catalog
        .create_staging("dataset", "installation", "staged")
        .unwrap();
    // ready 数据集永不可删：手工把状态推到 ready 模拟完成切换。
    catalog
        .connection
        .execute(
            "UPDATE datasets SET state='ready' WHERE id=?1",
            [entry.id.as_str()],
        )
        .unwrap();
    assert!(catalog.discard_staging(&entry.id).is_err());
    assert!(catalog.list().unwrap().iter().any(|e| e.id == entry.id));
    assert!(catalog.discard_staging("not-a-uuid").is_err());
    assert!(
        catalog
            .discard_staging(&uuid::Uuid::new_v4().to_string())
            .is_err()
    );
}

#[test]
fn discard_refuses_selected_entry() {
    let workspace = Workspace::new();
    let catalog = DatasetCatalog::open(&workspace.0).unwrap();
    let entry = catalog
        .create_staging("dataset", "installation", "staged")
        .unwrap();
    catalog
        .connection
        .execute(
            "UPDATE workspace SET selected_id=?1 WHERE singleton=1",
            [entry.id.as_str()],
        )
        .unwrap();
    assert!(catalog.discard_staging(&entry.id).is_err());
    assert!(catalog.list().unwrap().iter().any(|e| e.id == entry.id));
}

/// 造一个「已选中、且身份表齐全」的数据集工作区，返回 (workspace, 数据集库路径)。
fn selected_dataset() -> (Workspace, PathBuf) {
    let workspace = Workspace::new();
    let _original = workspace.original();
    let catalog = DatasetCatalog::open(&workspace.0).unwrap();
    let (entry, master) = imported(&catalog);
    catalog
        .select_import(&catalog.prepare_selection(&entry.id).unwrap(), &master, 2)
        .unwrap();
    let path = catalog.path_for(&entry.id).unwrap();
    assert_eq!(
        selected_database_path(&workspace.0).unwrap(),
        Some(path.clone())
    );
    (workspace, path)
}

/// 真机 2026-10-27 17:08 抓到：用户走完「清除本机同步身份」出口后，**下一次冷
/// 启动整机起不来** —— `Failed to initialize / Bad state: 初始化存储失败:
/// AnyhowException(Error resolving data directory: sync_selected_dataset_identity_changed)`。
///
/// 机制：出口合法地把 `sync_replica`/`sync_local_vault`/`sync_snapshots` 清空
/// （`storage/sync/reset.rs::REPLICATION_TABLES`），而启动校验把「三张表全空」
/// 当成「catalog 记的身份和文件里的身份不一致」。数据集本身业务表一格未动，
/// 必须仍然能打开 —— 否则出口只是把「局部坏状态」换成「整机不可用」。
#[test]
fn a_dataset_that_left_sync_still_opens_on_the_next_start() {
    let (workspace, path) = selected_dataset();

    // 故意走**真实出口**而不是手写 DELETE：手删只能证明 SQL 对不对，
    // 证明不了「出口之后的第一条启动路径」能不能走通。出口的准入闸门之一是
    // 「authority 必须为空」（有权威 = 健康身份，该走切换数据集），所以先把
    // 现场还原成 C6 那次真机的样子：authority 空、vault/replica 还在。
    {
        let store = Store::open(&path).unwrap();
        store
            .connection
            .execute_batch(
                "DELETE FROM sync_authority; DELETE FROM sync_authority_versions;",
            )
            .unwrap();
        store.clear_local_sync_identity().unwrap();
    }
    {
        let store = Store::open(&path).unwrap();
        let rows: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM sync_replica", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0, "出口应已清掉 replica");
        assert_eq!(
            store.list_events().unwrap().len(),
            1,
            "业务数据必须一格不少"
        );
    }
    assert_eq!(
        selected_database_path(&workspace.0).unwrap(),
        Some(path),
        "退出同步后的数据集必须能在冷启动时打开"
    );
}

/// 上一条的正向放行**不能**顺手放掉真正的身份不符：只清了一半（replica 还在、
/// vault 没了）依旧要报错，否则「表全空」和「表残缺」就没区别了。
#[test]
fn a_partially_cleared_identity_is_still_a_startup_error() {
    let (workspace, path) = selected_dataset();
    {
        let store = Store::open(&path).unwrap();
        store
            .connection
            .execute_batch("PRAGMA defer_foreign_keys = ON")
            .unwrap();
        let transaction = store.connection.unchecked_transaction().unwrap();
        store
            .connection
            .execute("DELETE FROM sync_local_vault", [])
            .unwrap();
        transaction.commit().unwrap();
    }
    let error = selected_database_path(&workspace.0).unwrap_err();
    assert!(
        format!("{error:#}").contains("sync_selected_dataset_identity_changed"),
        "半清状态必须继续报身份不符：{error:#}"
    );
}

/// 另外两条身份断言同样不能被「全空放行」带松：installation 被换、以及快照
/// 没落到 `applied`，都还是 `sync_selected_dataset_identity_changed`。
#[test]
fn a_changed_installation_or_unapplied_snapshot_is_still_a_startup_error() {
    let (workspace, path) = selected_dataset();
    {
        let store = Store::open(&path).unwrap();
        store
            .connection
            .execute("UPDATE sync_local_vault SET installation_id='another-installation'", [])
            .unwrap();
    }
    let error = selected_database_path(&workspace.0).unwrap_err();
    assert!(
        format!("{error:#}").contains("sync_selected_dataset_identity_changed"),
        "installation 被换必须报错：{error:#}"
    );

    let (workspace, path) = selected_dataset();
    {
        let store = Store::open(&path).unwrap();
        store
            .connection
            .execute("UPDATE sync_snapshots SET state='staging'", [])
            .unwrap();
    }
    let error = selected_database_path(&workspace.0).unwrap_err();
    assert!(
        format!("{error:#}").contains("sync_selected_dataset_identity_changed"),
        "快照未 applied 必须报错：{error:#}"
    );
}
