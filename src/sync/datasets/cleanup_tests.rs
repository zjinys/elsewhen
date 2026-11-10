use super::tests::{Workspace, imported};
use super::*;

fn old_copy(catalog: &DatasetCatalog) -> DatasetEntry {
    let (entry, master) = imported(catalog);
    catalog
        .select_import(&catalog.prepare_selection(&entry.id).unwrap(), &master, 2)
        .unwrap();
    catalog.select_default(catalog.revision().unwrap()).unwrap();
    entry
}

#[test]
fn cleanup_old_copy_preserves_default_and_reserves_key_until_acknowledged() {
    let w = Workspace::new();
    let original = w.original();
    let catalog = DatasetCatalog::open(&w.0).unwrap();
    let entry = old_copy(&catalog);
    let current = w.0.join(DB_NAME);
    let p = catalog.cleanup_preview(&entry.id, &current).unwrap();
    assert!(p.bytes > 0);
    assert!(!p.pending);
    catalog
        .cleanup_copy(&p.entry_id, p.catalog_revision, &p.file_token, &current)
        .unwrap();
    assert!(!w.0.join("datasets").join(&entry.id).exists());
    assert!(catalog.list().unwrap().is_empty());
    assert_eq!(
        catalog.pending_cleanups().unwrap()[0].installation_id,
        entry.installation_id
    );
    assert!(
        catalog
            .create_staging("another-dataset", &entry.installation_id, "reserved")
            .is_err()
    );
    drop(catalog);
    let catalog = DatasetCatalog::open(&w.0).unwrap();
    // Simulate restart/key-store failure: no key acknowledgement was committed.
    let p = catalog.pending_cleanups().unwrap().remove(0);
    catalog.cleanup_copy(&p.entry_id, 0, "", &current).unwrap();
    catalog.finish_cleanup(&p.entry_id).unwrap();
    assert!(catalog.pending_cleanups().unwrap().is_empty());
    assert_eq!(
        original.list_events().unwrap()[0].raw_text,
        "personal original"
    );
}

#[test]
fn cleanup_refuses_selected_pinned_staging_stale_and_changed_files() {
    let w = Workspace::new();
    let _original = w.original();
    let catalog = DatasetCatalog::open(&w.0).unwrap();
    let (entry, master) = imported(&catalog);
    let current = w.0.join(DB_NAME);
    assert!(
        catalog.cleanup_preview(&entry.id, &current).is_err(),
        "staging is not an old ready copy"
    );
    catalog
        .select_import(&catalog.prepare_selection(&entry.id).unwrap(), &master, 2)
        .unwrap();
    assert!(
        catalog.cleanup_preview(&entry.id, &current).is_err(),
        "selected for next startup"
    );
    let active_path = catalog.path_for(&entry.id).unwrap();
    let lease = catalog.pin_selected_use(&active_path).unwrap();
    catalog.select_default(catalog.revision().unwrap()).unwrap();
    assert!(
        catalog.cleanup_preview(&entry.id, &current).is_err(),
        "another process still pins this copy"
    );
    drop(lease);
    assert!(
        catalog.cleanup_preview(&entry.id, &active_path).is_err(),
        "our current process path"
    );
    let p = catalog.cleanup_preview(&entry.id, &current).unwrap();
    catalog
        .create_staging("other", "other-install", "other")
        .unwrap();
    assert!(
        catalog
            .cleanup_copy(&p.entry_id, p.catalog_revision, &p.file_token, &current)
            .is_err(),
        "stale catalog revision"
    );
    let p = catalog.cleanup_preview(&entry.id, &current).unwrap();
    std::fs::write(active_path.parent().unwrap().join("extra"), b"new content").unwrap();
    assert!(
        catalog
            .cleanup_copy(&p.entry_id, p.catalog_revision, &p.file_token, &current)
            .is_err(),
        "files changed after preview"
    );
    assert!(active_path.is_file());
    assert!(catalog.pending_cleanups().unwrap().is_empty());
    assert!(
        catalog
            .cleanup_copy("../elsewhen.db", 0, "", &current)
            .is_err()
    );
    assert!(current.is_file());
}

#[test]
fn cleanup_commit_failure_and_interrupted_files_remain_retryable() {
    let w = Workspace::new();
    let _original = w.original();
    let catalog = DatasetCatalog::open(&w.0).unwrap();
    let entry = old_copy(&catalog);
    let current = w.0.join(DB_NAME);
    let p = catalog.cleanup_preview(&entry.id, &current).unwrap();
    catalog.connection.execute_batch("CREATE TRIGGER fail_cleanup BEFORE DELETE ON datasets BEGIN SELECT RAISE(ABORT,'simulated failure'); END;").unwrap();
    assert!(
        catalog
            .cleanup_copy(&p.entry_id, p.catalog_revision, &p.file_token, &current)
            .is_err()
    );
    assert!(catalog.path_for(&entry.id).unwrap().is_file());
    assert!(catalog.pending_cleanups().unwrap().is_empty());
    catalog
        .connection
        .execute_batch("DROP TRIGGER fail_cleanup;")
        .unwrap();
    // An interrupted removal has a durable intent but no completion marker.
    catalog
        .connection
        .execute(
            "INSERT INTO dataset_cleanup_jobs(entry_id,installation_id,name) VALUES(?1,?2,?3)",
            params![entry.id, entry.installation_id, entry.name],
        )
        .unwrap();
    catalog
        .connection
        .execute("DELETE FROM datasets WHERE id=?1", [&entry.id])
        .unwrap();
    assert!(catalog.finish_cleanup(&entry.id).is_err());
    drop(catalog);
    let catalog = DatasetCatalog::open(&w.0).unwrap();
    catalog.cleanup_copy(&entry.id, 0, "", &current).unwrap();
    catalog.finish_cleanup(&entry.id).unwrap();
    assert!(!w.0.join("datasets").join(&entry.id).exists());
    assert!(current.is_file());
}

#[cfg(unix)]
#[test]
fn cleanup_refuses_symlink_children_and_recovery_folder_replacement() {
    use std::os::unix::fs::symlink;
    let w = Workspace::new();
    let _original = w.original();
    let catalog = DatasetCatalog::open(&w.0).unwrap();
    let entry = old_copy(&catalog);
    let current = w.0.join(DB_NAME);
    let folder = w.0.join("datasets").join(&entry.id);
    symlink(&current, folder.join("unexpected-link")).unwrap();
    assert!(catalog.cleanup_preview(&entry.id, &current).is_err());
    std::fs::remove_file(folder.join("unexpected-link")).unwrap();
    catalog
        .connection
        .execute(
            "INSERT INTO dataset_cleanup_jobs(entry_id,installation_id,name) VALUES(?1,?2,?3)",
            params![entry.id, entry.installation_id, entry.name],
        )
        .unwrap();
    catalog
        .connection
        .execute("DELETE FROM datasets WHERE id=?1", [&entry.id])
        .unwrap();
    let saved = w.0.join("saved");
    std::fs::rename(&folder, &saved).unwrap();
    symlink(&saved, &folder).unwrap();
    assert!(catalog.cleanup_copy(&entry.id, 0, "", &current).is_err());
    assert!(catalog.finish_cleanup(&entry.id).is_err());
    assert!(saved.join(DB_NAME).is_file());
    assert!(current.is_file());
}

#[test]
fn cleanup_api_requires_workspace_exclusion_and_keeps_default_events() {
    let _env = crate::config::test_env_guard();
    crate::config::unpin();
    let w = Workspace::new();
    let _original = w.original();
    let config = crate::config::AppConfig::pin(Some(w.0.to_str().unwrap())).unwrap();
    let catalog = DatasetCatalog::open(&w.0).unwrap();
    let entry = old_copy(&catalog);
    let p = crate::api::sync::list_sync_dataset_cleanup()
        .unwrap()
        .remove(0);
    let guard = crate::sync::runtime::initialization_guard(&config)
        .unwrap()
        .unwrap();
    let result = crate::api::sync::cleanup_sync_dataset_copy(
        entry.id.clone(),
        p.catalog_revision,
        p.file_token.clone(),
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("sync_dataset_cleanup_requires_lock")
    );
    assert!(catalog.path_for(&entry.id).unwrap().is_file());
    drop(guard);
    let job = crate::api::sync::cleanup_sync_dataset_copy(
        entry.id.clone(),
        p.catalog_revision,
        p.file_token,
    )
    .unwrap();
    assert_eq!(job.installation_id, entry.installation_id);
    assert!(crate::api::sync::list_sync_dataset_cleanup().unwrap()[0].pending);
    assert_eq!(_original.list_events().unwrap().len(), 1);
    crate::config::unpin();
}

#[test]
fn cleanup_catalog_upgrades_v1_without_selecting_or_deleting_a_copy() {
    let w = Workspace::new();
    let _original = w.original();
    let catalog = DatasetCatalog::open(&w.0).unwrap();
    let entry = old_copy(&catalog);
    catalog
        .connection
        .execute_batch("DROP TABLE dataset_cleanup_jobs; PRAGMA user_version=1;")
        .unwrap();
    drop(catalog);
    assert!(selected_database_path(&w.0).unwrap().is_none());
    let catalog = DatasetCatalog::open(&w.0).unwrap();
    assert!(catalog.pending_cleanups().unwrap().is_empty());
    assert!(catalog.path_for(&entry.id).unwrap().is_file());
    assert_eq!(catalog.list().unwrap()[0].id, entry.id);
}

#[test]
fn cleanup_cannot_remove_a_copy_pinned_before_next_start_selection_changed() {
    let _env = crate::config::test_env_guard();
    crate::config::unpin();
    let w = Workspace::new();
    let _original = w.original();
    let catalog = DatasetCatalog::open(&w.0).unwrap();
    let (entry, master) = imported(&catalog);
    catalog
        .select_import(&catalog.prepare_selection(&entry.id).unwrap(), &master, 2)
        .unwrap();
    let config = crate::config::AppConfig::pin(Some(w.0.to_str().unwrap())).unwrap();
    assert_eq!(config.database_path, catalog.path_for(&entry.id).unwrap());
    catalog.select_default(catalog.revision().unwrap()).unwrap();
    assert!(
        catalog
            .cleanup_preview(&entry.id, &w.0.join(DB_NAME))
            .is_err(),
        "the pinned process lease outlives the next-start selection"
    );
    crate::config::unpin();
    assert!(
        catalog
            .cleanup_preview(&entry.id, &w.0.join(DB_NAME))
            .is_ok()
    );
}
