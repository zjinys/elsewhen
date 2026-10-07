//! Ciphertext-only persistence for the installation vault. These tables are
//! deliberately absent from every sync operation and snapshot query.

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use crate::storage::Store;
use crate::sync::{pairing::ApprovedPair, vault::LocalVault};

impl Store {
    pub(crate) fn create_sync_vault(
        &self,
        installation_id: &str,
        master: &[u8; 32],
        name: &str,
        platform: &str,
    ) -> Result<LocalVault> {
        let tx = self.begin_write_transaction()?;
        let existing: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_local_vault)",
            [],
            |r| r.get(0),
        )?;
        ensure!(!existing, "sync_vault_already_exists");
        ensure!(
            self.sync_identity()?.is_none(),
            "sync_vault_missing_for_existing_replica"
        );
        let vault = LocalVault::create(installation_id, name, platform)?;
        self.connection.execute("INSERT INTO sync_local_vault(singleton,installation_id,revision,encrypted_secrets) VALUES(1,?1,?2,?3)",
            params![vault.installation_id,vault.revision,vault.seal(master)?])?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(vault)
    }

    /// The coordinator can supply its separately protected revision floor. It
    /// must not infer that floor from a potentially restored database itself.
    pub(crate) fn load_sync_vault(
        &self,
        installation_id: &str,
        master: &[u8; 32],
        minimum_revision: i64,
    ) -> Result<LocalVault> {
        ensure!(minimum_revision > 0, "sync_vault_revision_invalid");
        let record: Option<(String,i64,Vec<u8>)> = self.connection.query_row(
            "SELECT installation_id,revision,encrypted_secrets FROM sync_local_vault WHERE singleton=1",[],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
        ).optional()?;
        let (installation, revision, encrypted) = record.context("sync_vault_missing")?;
        ensure!(
            installation == installation_id,
            "sync_vault_wrong_installation"
        );
        ensure!(
            revision >= minimum_revision,
            "sync_vault_restored_revision_requires_repair"
        );
        let vault = LocalVault::open(installation_id, revision, &encrypted, master)?;
        if let Some(identity) = self.sync_identity()? {
            ensure!(
                identity.device_id == vault.device().device_id
                    && identity.stream_id == vault.device().stream_id,
                "sync_vault_replica_identity_mismatch"
            );
        }
        Ok(vault)
    }

    pub(crate) fn install_sync_vault_pair(
        &self,
        installation_id: &str,
        master: &[u8; 32],
        expected_revision: i64,
        pair: &ApprovedPair,
    ) -> Result<i64> {
        let tx = self.begin_write_transaction()?;
        let mut vault = self.load_sync_vault(installation_id, master, expected_revision)?;
        ensure!(
            vault.revision == expected_revision,
            "sync_vault_revision_changed"
        );
        if vault.install_pair(pair)? {
            self.save_sync_vault(&mut vault, master)?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(vault.revision)
    }

    pub(crate) fn revoke_sync_vault_peer(
        &self,
        installation_id: &str,
        master: &[u8; 32],
        expected_revision: i64,
        dataset: &str,
        device: &str,
    ) -> Result<i64> {
        let tx = self.begin_write_transaction()?;
        let mut vault = self.load_sync_vault(installation_id, master, expected_revision)?;
        ensure!(
            vault.revision == expected_revision,
            "sync_vault_revision_changed"
        );
        if vault.revoke_peer(dataset, device)? {
            self.save_sync_vault(&mut vault, master)?;
        }
        if self
            .sync_identity()?
            .is_some_and(|identity| identity.dataset_id == dataset)
        {
            self.revoke_sync_member(device)?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(vault.revision)
    }

    pub(super) fn save_sync_vault(&self, vault: &mut LocalVault, master: &[u8; 32]) -> Result<()> {
        ensure!(
            !self.connection.is_autocommit(),
            "sync_requires_domain_transaction"
        );
        let old = vault.revision;
        vault.revision = old
            .checked_add(1)
            .context("sync_vault_revision_exhausted")?;
        let changed = self.connection.execute(
            "UPDATE sync_local_vault SET revision=?1,encrypted_secrets=?2 WHERE singleton=1 AND installation_id=?3 AND revision=?4",
            params![vault.revision,vault.seal(master)?,vault.installation_id,old],
        )?;
        ensure!(changed == 1, "sync_vault_revision_changed");
        Ok(())
    }
}

#[cfg(test)]
mod tests;
