//! Explicit local-copy removal. The durable intent survives filesystem/native
//! key-store failures; an installation ID stays reserved until both are removed.
use super::*;
use std::fs::{File, OpenOptions};

#[derive(Clone, Debug)]
pub(crate) struct CleanupPreview {
    pub entry_id: String,
    pub installation_id: String,
    pub name: String,
    pub bytes: u64,
    pub catalog_revision: i64,
    pub file_token: String,
    pub pending: bool,
}

/// Lives for the entire pinned process, even after selecting another dataset
/// for next startup. Lock files live outside the folder being removed (Windows).
pub(crate) struct DatasetUseLease(File);
impl DatasetUseLease {
    fn acquire(root: &Path, id: &str, exclusive: bool) -> Result<Self> {
        uuid::Uuid::parse_str(id).map_err(|_| anyhow::anyhow!("sync_dataset_id_invalid"))?;
        let path = root.join(format!("dataset-use-{id}.lock"));
        if std::fs::symlink_metadata(&path).is_ok() {
            checked_file(&path)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        #[cfg(unix)]
        crate::config::set_unix_permissions(&path, 0o600)?;
        if exclusive {
            file.try_lock()
                .map_err(|_| anyhow::anyhow!("sync_dataset_in_use"))?;
        } else {
            file.try_lock_shared()
                .map_err(|_| anyhow::anyhow!("sync_dataset_in_use"))?;
        }
        Ok(Self(file))
    }
}
impl Drop for DatasetUseLease {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

// Reject symlinks anywhere in the deletion tree. Bind the confirmation to the
// displayed file inventory, including SQLite WAL and any unexpected contents.
fn inventory(folder: &Path) -> Result<(u64, String)> {
    fn walk(path: &Path, base: &Path, items: &mut Vec<String>, bytes: &mut u64) -> Result<()> {
        let meta = std::fs::symlink_metadata(path)?;
        ensure!(!meta.file_type().is_symlink(), "sync_dataset_file_invalid");
        let name = path.strip_prefix(base)?.to_string_lossy();
        if meta.is_dir() {
            items.push(format!("d:{name}"));
            for child in std::fs::read_dir(path)? {
                walk(&child?.path(), base, items, bytes)?;
            }
        } else {
            ensure!(meta.is_file(), "sync_dataset_file_invalid");
            *bytes = bytes
                .checked_add(meta.len())
                .context("sync_dataset_size_invalid")?;
            items.push(format!("f:{name}:{}:{:?}", meta.len(), meta.modified()?));
        }
        Ok(())
    }
    let mut items = vec![];
    let mut bytes = 0;
    walk(folder, folder, &mut items, &mut bytes)?;
    items.sort();
    Ok((
        bytes,
        crate::storage::sync::hash(items.join("\n").as_bytes()),
    ))
}

impl DatasetCatalog {
    /// Serializes startup against catalog cleanup, then pins a shared OS lease.
    pub(crate) fn pin_selected_use(&self, database: &Path) -> Result<Option<DatasetUseLease>> {
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let selected = self.selected_id()?;
        let lease = if let Some(id) = selected {
            ensure!(
                self.path_for(&id)? == database,
                "sync_dataset_selection_changed"
            );
            Some(DatasetUseLease::acquire(&self.root, &id, false)?)
        } else {
            ensure!(
                database == self.root.join(DB_NAME),
                "sync_dataset_selection_changed"
            );
            None
        };
        tx.commit()?;
        Ok(lease)
    }

    pub(crate) fn cleanup_preview(
        &self,
        id: &str,
        current_database: &Path,
    ) -> Result<CleanupPreview> {
        let tx = self.connection.unchecked_transaction()?;
        let preview = self.cleanup_preview_inner(id, current_database)?;
        tx.commit()?;
        Ok(preview)
    }

    fn cleanup_preview_inner(&self, id: &str, current_database: &Path) -> Result<CleanupPreview> {
        let entry = self
            .connection
            .query_row(
                "SELECT id,dataset_id,installation_id,name,state FROM datasets WHERE id=?1",
                [id],
                entry_row,
            )
            .optional()?
            .context("sync_dataset_unknown")?;
        ensure!(entry.ready, "sync_dataset_cleanup_not_ready");
        ensure!(
            self.selected_id()?.as_deref() != Some(id),
            "sync_dataset_cleanup_selected"
        );
        let path = self.path_for(id)?;
        ensure!(path != current_database, "sync_dataset_in_use");
        let _lease = DatasetUseLease::acquire(&self.root, id, true)?;
        let (bytes, file_token) =
            inventory(path.parent().context("sync_dataset_directory_invalid")?)?;
        let preview = CleanupPreview {
            entry_id: entry.id,
            installation_id: entry.installation_id,
            name: entry.name,
            bytes,
            catalog_revision: self.revision()?,
            file_token,
            pending: false,
        };
        Ok(preview)
    }

    pub(crate) fn pending_cleanups(&self) -> Result<Vec<CleanupPreview>> {
        Ok(self
            .connection
            .prepare(
                "SELECT entry_id,installation_id,name FROM dataset_cleanup_jobs ORDER BY rowid",
            )?
            .query_map([], |r| {
                Ok(CleanupPreview {
                    entry_id: r.get(0)?,
                    installation_id: r.get(1)?,
                    name: r.get(2)?,
                    bytes: 0,
                    catalog_revision: 0,
                    file_token: String::new(),
                    pending: true,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub(crate) fn cleanup_copy(
        &self,
        id: &str,
        revision: i64,
        token: &str,
        current_database: &Path,
    ) -> Result<CleanupPreview> {
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let prior = self
            .pending_cleanups()?
            .into_iter()
            .find(|p| p.entry_id == id);
        if prior.is_none() {
            let preview = self.cleanup_preview_inner(id, current_database)?;
            ensure!(
                preview.catalog_revision == revision && preview.file_token == token,
                "sync_dataset_cleanup_changed"
            );
            tx.execute(
                "INSERT INTO dataset_cleanup_jobs(entry_id,installation_id,name) VALUES(?1,?2,?3)",
                params![id, preview.installation_id, preview.name],
            )?;
            tx.execute("DELETE FROM datasets WHERE id=?1", [id])?;
            tx.execute(
                "UPDATE workspace SET revision=revision+1 WHERE singleton=1",
                [],
            )?;
        }
        // Commit the intent before touching files. A crash cannot turn a
        // partially removed ready database into a selectable empty database.
        tx.commit()?;
        let preview = self
            .pending_cleanups()?
            .into_iter()
            .find(|p| p.entry_id == id)
            .context("sync_dataset_cleanup_unknown")?;
        uuid::Uuid::parse_str(id).map_err(|_| anyhow::anyhow!("sync_dataset_id_invalid"))?;
        directory(&self.root.join("datasets"), false)?;
        let folder = self.root.join("datasets").join(id);
        ensure!(
            folder.join(DB_NAME) != current_database,
            "sync_dataset_in_use"
        );
        let _lease = DatasetUseLease::acquire(&self.root, id, true)?;
        match std::fs::symlink_metadata(&folder) {
            Ok(_) => {
                directory(&folder, false)?;
                inventory(&folder)?;
                std::fs::remove_dir_all(&folder)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        self.connection.execute(
            "UPDATE dataset_cleanup_jobs SET files_removed=1 WHERE entry_id=?1",
            [id],
        )?;
        Ok(preview)
    }

    /// Called only after native storage confirms deletion of this job's own
    /// installation key/revision. Missing acknowledgements remain retryable.
    pub(crate) fn finish_cleanup(&self, id: &str) -> Result<()> {
        let removed = self.connection.execute(
            "DELETE FROM dataset_cleanup_jobs WHERE entry_id=?1 AND files_removed=1",
            [id],
        )?;
        ensure!(removed == 1, "sync_dataset_cleanup_not_finished");
        Ok(())
    }
}
