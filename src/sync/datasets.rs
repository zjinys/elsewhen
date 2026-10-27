//! Installation-local dataset catalog. Selection is for the next process start;
//! no open Store or pinned AppConfig is redirected. Staging uses final, unique
//! directories so publishing never renames an open SQLite database/WAL pair.

use super::protocol::validate_id;
use crate::storage::Store;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use std::path::{Path, PathBuf};

const CATALOG: &str = "sync-workspace.db";
const DB_NAME: &str = "elsewhen.db";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DatasetEntry {
    pub id: String,
    pub dataset_id: String,
    pub installation_id: String,
    pub name: String,
    pub ready: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct DatasetSelection {
    pub entry: DatasetEntry,
    pub catalog_revision: i64,
    pub snapshot_id: String,
    pub vault_revision: i64,
}

pub(crate) struct DatasetCatalog {
    root: PathBuf,
    connection: Connection,
}

fn checked_file(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path).context("sync_dataset_file_missing")?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "sync_dataset_file_invalid"
    );
    Ok(())
}
fn directory(path: &Path, create: bool) -> Result<()> {
    if create {
        match std::fs::create_dir(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    let metadata = std::fs::symlink_metadata(path).context("sync_dataset_directory_missing")?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "sync_dataset_directory_invalid"
    );
    #[cfg(unix)]
    if create {
        crate::config::set_unix_permissions(path, 0o700)?;
    }
    Ok(())
}
fn data_path(root: &Path, id: &str) -> Result<PathBuf> {
    uuid::Uuid::parse_str(id).map_err(|_| anyhow::anyhow!("sync_dataset_id_invalid"))?;
    directory(&root.join("datasets"), false)?;
    let folder = root.join("datasets").join(id);
    directory(&folder, false)?;
    let path = folder.join(DB_NAME);
    match std::fs::symlink_metadata(&path) {
        Ok(_) => checked_file(&path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    for suffix in ["-wal", "-shm"] {
        let side = folder.join(format!("{DB_NAME}{suffix}"));
        if std::fs::symlink_metadata(&side).is_ok() {
            checked_file(&side)?;
        }
    }
    Ok(path)
}

fn check_schema(connection: &Connection) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(version == 1, "sync_dataset_catalog_version_invalid");
    Ok(())
}
fn entry_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DatasetEntry> {
    Ok(DatasetEntry {
        id: row.get(0)?,
        dataset_id: row.get(1)?,
        installation_id: row.get(2)?,
        name: row.get(3)?,
        ready: row.get::<_, String>(4)? == "ready",
    })
}

impl DatasetCatalog {
    pub(crate) fn open(root: &Path) -> Result<Self> {
        directory(root, false)?;
        let path = root.join(CATALOG);
        let exists = std::fs::symlink_metadata(&path).is_ok();
        if exists {
            checked_file(&path)?;
        }
        let mut connection = Connection::open(&path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        {
            let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if version == 0 {
                let populated: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%')",
                    [],
                    |r| r.get(0),
                )?;
                ensure!(!populated, "sync_dataset_catalog_version_invalid");
                tx.execute_batch("CREATE TABLE datasets(id TEXT PRIMARY KEY,dataset_id TEXT NOT NULL,installation_id TEXT NOT NULL UNIQUE,name TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN ('staging','ready')),published_snapshot TEXT);
                    CREATE TABLE workspace(singleton INTEGER PRIMARY KEY CHECK(singleton=1),revision INTEGER NOT NULL CHECK(revision>0),selected_id TEXT REFERENCES datasets(id));
                    INSERT INTO workspace(singleton,revision,selected_id) VALUES(1,1,NULL);
                    PRAGMA user_version=1;")?;
            } else {
                check_schema(&tx)?;
            }
            tx.commit()?;
        }
        #[cfg(unix)]
        crate::config::set_unix_permissions(&path, 0o600)?;
        Ok(Self {
            root: root.to_path_buf(),
            connection,
        })
    }

    pub(crate) fn create_staging(
        &self,
        dataset_id: &str,
        installation_id: &str,
        name: &str,
    ) -> Result<DatasetEntry> {
        validate_id(dataset_id)?;
        validate_id(installation_id)?;
        ensure!(
            !name.trim().is_empty() && name.len() <= 256 && !name.chars().any(char::is_control),
            "sync_dataset_name_invalid"
        );
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM datasets", [], |r| r.get(0))?;
        ensure!(count < 32, "sync_dataset_count_limit");
        let duplicate: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM datasets WHERE installation_id=?1)",
            [installation_id],
            |r| r.get(0),
        )?;
        ensure!(!duplicate, "sync_dataset_installation_already_reserved");
        directory(&self.root.join("datasets"), true)?;
        let id = uuid::Uuid::new_v4().to_string();
        let folder = self.root.join("datasets").join(&id);
        // create_dir, not create_dir_all: never adopt an existing staging folder.
        std::fs::create_dir(&folder)?;
        directory(&folder, true)?;
        tx.execute("INSERT INTO datasets(id,dataset_id,installation_id,name,state) VALUES(?1,?2,?3,?4,'staging')",params![id,dataset_id,installation_id,name])?;
        tx.execute(
            "UPDATE workspace SET revision=revision+1 WHERE singleton=1",
            [],
        )?;
        tx.commit()?;
        Ok(DatasetEntry {
            id,
            dataset_id: dataset_id.into(),
            installation_id: installation_id.into(),
            name: name.into(),
            ready: false,
        })
    }

    pub(crate) fn list(&self) -> Result<Vec<DatasetEntry>> {
        Ok(self.connection.prepare("SELECT id,dataset_id,installation_id,name,state FROM datasets ORDER BY rowid LIMIT 32")?
            .query_map([],entry_row)?.collect::<rusqlite::Result<_>>()?)
    }

    /// 删除一个未完成的加入暂存项：目录先删（失败可重试，孤儿目录无害），
    /// 再删目录行并推进 catalog 修订。fail-closed：只允许 staging；已就绪
    /// 的数据集与当前选中项永不可删。
    pub(crate) fn discard_staging(&self, entry_id: &str) -> Result<()> {
        uuid::Uuid::parse_str(entry_id).map_err(|_| anyhow::anyhow!("sync_dataset_id_invalid"))?;
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let state: Option<String> = tx
            .query_row("SELECT state FROM datasets WHERE id=?1", [entry_id], |r| {
                r.get(0)
            })
            .optional()?;
        let state = state.context("sync_dataset_unknown")?;
        ensure!(state == "staging", "sync_dataset_not_staging");
        let selected: Option<String> = tx
            .query_row(
                "SELECT selected_id FROM workspace WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if selected.as_deref() == Some(entry_id) {
            anyhow::bail!("sync_dataset_not_staging");
        }
        let folder = self.root.join("datasets").join(entry_id);
        match std::fs::symlink_metadata(&folder) {
            Ok(meta) => {
                // 只删目录，绝不跟符号链接。错误的我们不动。
                ensure!(meta.is_dir(), "sync_dataset_invalid_staging_folder");
                std::fs::remove_dir_all(&folder)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        tx.execute(
            "DELETE FROM datasets WHERE id=?1 AND state='staging'",
            [entry_id],
        )?;
        tx.execute(
            "UPDATE workspace SET revision=revision+1 WHERE singleton=1",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn path_for(&self, id: &str) -> Result<PathBuf> {
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM datasets WHERE id=?1)",
            [id],
            |r| r.get(0),
        )?;
        ensure!(exists, "sync_dataset_unknown");
        data_path(&self.root, id)
    }
    /// Read-only: the currently selected entry, if any. A committed selection
    /// can never point at a staging entry, so this needs no state filtering.
    pub(crate) fn selected_id(&self) -> Result<Option<String>> {
        Ok(self.connection.query_row(
            "SELECT selected_id FROM workspace WHERE singleton=1",
            [],
            |r| r.get(0),
        )?)
    }

    pub(crate) fn revision(&self) -> Result<i64> {
        Ok(self.connection.query_row(
            "SELECT revision FROM workspace WHERE singleton=1",
            [],
            |r| r.get(0),
        )?)
    }

    pub(crate) fn prepare_selection(&self, id: &str) -> Result<DatasetSelection> {
        let tx = self.connection.unchecked_transaction()?;
        let entry = self.connection.query_row(
            "SELECT id,dataset_id,installation_id,name,state FROM datasets WHERE id=?1",
            [id],
            entry_row,
        )?;
        let path = self.path_for(id)?;
        checked_file(&path)?;
        let store = Store::open(&path)?;
        let state = store.begin_write_transaction()?;
        ensure!(
            store.has_sync_authority()?,
            "sync_dataset_authority_missing"
        );
        store.require_local_sync_membership()?;
        store.require_background_quiescent_for_sync()?;
        let identity = store.sync_identity()?.context("sync_not_initialized")?;
        ensure!(
            identity.dataset_id == entry.dataset_id,
            "sync_dataset_identity_mismatch"
        );
        let snapshot_id:String=store.connection.query_row("SELECT s.id FROM sync_snapshots s JOIN sync_snapshot_target t ON t.snapshot_id=s.id WHERE s.state='applied'",[],|r|r.get(0))?;
        let (installation, vault_revision) = store.local_vault_basis()?;
        ensure!(
            installation == entry.installation_id,
            "sync_dataset_installation_mismatch"
        );
        let catalog_revision = self.revision()?;
        if let Some(state) = state {
            state.commit()?;
        }
        tx.commit()?;
        Ok(DatasetSelection {
            entry,
            catalog_revision,
            snapshot_id,
            vault_revision,
        })
    }

    /// Pins the exact preview. Master material comes from protected native storage;
    /// catalog/target commit order cannot expose a partially applied snapshot.
    pub(crate) fn select_import(
        &self,
        selection: &DatasetSelection,
        master: &[u8; 32],
        minimum_vault_revision: i64,
    ) -> Result<()> {
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        ensure!(
            self.revision()? == selection.catalog_revision,
            "sync_dataset_selection_changed"
        );
        let entry = self.connection.query_row(
            "SELECT id,dataset_id,installation_id,name,state FROM datasets WHERE id=?1",
            [&selection.entry.id],
            entry_row,
        )?;
        ensure!(entry == selection.entry, "sync_dataset_selection_changed");
        let path = self.path_for(&entry.id)?;
        checked_file(&path)?;
        let store = Store::open(&path)?;
        let target = store.begin_write_transaction()?;
        ensure!(
            store.has_sync_authority()?,
            "sync_dataset_authority_missing"
        );
        store.require_local_sync_membership()?;
        store.require_background_quiescent_for_sync()?;
        let identity = store.sync_identity()?.context("sync_not_initialized")?;
        ensure!(
            identity.dataset_id == entry.dataset_id,
            "sync_dataset_identity_mismatch"
        );
        let valid:bool=store.connection.query_row("SELECT EXISTS(SELECT 1 FROM sync_snapshot_target t JOIN sync_snapshots s ON s.id=t.snapshot_id WHERE s.id=?1 AND s.state='applied')",[&selection.snapshot_id],|r|r.get(0))?;
        ensure!(valid, "sync_dataset_snapshot_changed");
        let vault =
            store.load_sync_vault(&entry.installation_id, master, minimum_vault_revision)?;
        ensure!(
            vault.revision == selection.vault_revision,
            "sync_dataset_vault_changed"
        );
        store.require_sync_signer(vault.identity())?;
        tx.execute(
            "UPDATE datasets SET state='ready',published_snapshot=?2 WHERE id=?1",
            params![entry.id, selection.snapshot_id],
        )?;
        tx.execute(
            "UPDATE workspace SET selected_id=?1,revision=revision+1 WHERE singleton=1",
            [&entry.id],
        )?;
        tx.commit()?;
        if let Some(target) = target {
            target.commit()?;
        }
        Ok(())
    }

    pub(crate) fn select_default(&self, expected_revision: i64) -> Result<()> {
        let original = self.root.join(DB_NAME);
        if std::fs::symlink_metadata(&original).is_ok() {
            checked_file(&original)?;
        } else {
            checked_file(&self.root.join("events.db"))?;
        }
        let tx =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        ensure!(
            self.revision()? == expected_revision,
            "sync_dataset_selection_changed"
        );
        tx.execute(
            "UPDATE workspace SET selected_id=NULL,revision=revision+1 WHERE singleton=1",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }
}

/// Startup-only read. Missing catalog means legacy/default; malformed, missing or
/// unfinished selected datasets are errors, never a silent fallback to another DB.
pub(crate) fn selected_database_path(root: &Path) -> Result<Option<PathBuf>> {
    let catalog = root.join(CATALOG);
    match std::fs::symlink_metadata(&catalog) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => checked_file(&catalog)?,
    }
    let connection = Connection::open_with_flags(&catalog, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    // 0 字节 / 未初始化的 catalog 不是损坏：曾在某次失败的调用里被
    // SQLite 创建后未及建表（进程中途被杀）。完整入口 `open()` 能把它
    // 初始化到 version=1，但本函数是只读启动路径，不能写；当作
    // “没有数据集”回落主库。有表的才严格校验 schema。
    let populated: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%')",
        [],
        |r| r.get(0),
    )?;
    if !populated {
        return Ok(None);
    }
    check_schema(&connection)?;
    let transaction = connection.unchecked_transaction()?;
    let selected: Option<String> = connection.query_row(
        "SELECT selected_id FROM workspace WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    let Some(id) = selected else {
        return Ok(None);
    };
    let (state, dataset, installation, snapshot): (String, String, String, Option<String>) =
        connection.query_row(
            "SELECT state,dataset_id,installation_id,published_snapshot FROM datasets WHERE id=?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
    ensure!(state == "ready", "sync_selected_dataset_not_ready");
    let snapshot = snapshot.context("sync_selected_dataset_not_ready")?;
    let path = data_path(root, &id)?;
    checked_file(&path)?;
    let target = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let valid:bool=target.query_row("SELECT EXISTS(SELECT 1 FROM sync_replica WHERE dataset_id=?1) AND EXISTS(SELECT 1 FROM sync_local_vault WHERE installation_id=?2) AND EXISTS(SELECT 1 FROM sync_snapshots s JOIN sync_snapshot_target t ON t.snapshot_id=s.id WHERE s.id=?3 AND s.state='applied')",params![dataset,installation,snapshot],|r|r.get(0))?;
    ensure!(valid, "sync_selected_dataset_identity_changed");
    transaction.commit()?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests;
