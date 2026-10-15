//! Ciphertext-only persistence for the installation vault. These tables are
//! deliberately absent from every sync operation and snapshot query.

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use crate::storage::Store;
use crate::sync::{
    membership::{RosterAnchor, SignedRoster},
    pairing::{ApprovedPair, PairingDevice},
    vault::LocalVault,
};

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

    /// Join preparation must never adopt a database containing independent data.
    pub(crate) fn create_sync_join_vault(
        &self,
        installation_id: &str,
        master: &[u8; 32],
        name: &str,
        platform: &str,
    ) -> Result<LocalVault> {
        let tx = self.begin_write_transaction()?;
        self.ensure_empty_snapshot_target()?;
        let vault = self.create_sync_vault(installation_id, master, name, platform)?;
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

    /// owner 本机改名：本机 vault 显示名、签名 roster、pinned anchor 三处
    /// 原子更新；广播交给 membership tick。仅 owner 本机（签名权在那里）。
    /// `target` 可为 owner 自己或任意成员（给新设备改个可读名字）。
    pub(crate) fn rename_sync_vault_device(
        &self,
        installation_id: &str,
        master: &[u8; 32],
        expected_revision: i64,
        target_device: &str,
        new_name: &str,
    ) -> Result<u64> {
        let tx = self.begin_write_transaction()?;
        let mut vault = self.load_sync_vault(installation_id, master, expected_revision)?;
        ensure!(
            vault.revision == expected_revision,
            "sync_vault_revision_changed"
        );
        let local_device_id = vault.device().device_id.clone();
        let current = self
            .current_signed_roster()?
            .context("sync_authority_not_ready")?;
        ensure!(
            current.roster.owner.device_id == local_device_id,
            "sync_rename_requires_owner"
        );
        ensure!(
            current
                .roster
                .members
                .iter()
                .any(|m| m.active && m.device.device_id == target_device),
            "sync_device_unknown"
        );
        let trimmed = new_name.trim().to_string();
        if target_device == local_device_id {
            vault.rename_device(&trimmed)?;
            self.save_sync_vault(&mut vault, master)?;
        }
        let mut roster = current.roster.clone();
        roster.revision = roster
            .revision
            .checked_add(1)
            .context("sync_membership_revision_exhausted")?;
        for member in &mut roster.members {
            if member.device.device_id == target_device {
                member.device.name = trimmed.clone();
            }
        }
        if roster.owner.device_id == target_device {
            roster.owner.name = trimmed.clone();
        }
        let renames_owner = roster.owner.device_id == target_device;
        let signed = SignedRoster::sign(roster, vault.identity())?;
        // pin 的锚里 owner 名参与完整比较：同步放宽后的 decode 只看加密身份，
        // 但锚本身也要反映新名，免得本机后续读回时展示陈旧名字。
        if renames_owner {
            let anchor_json: String = self.connection.query_row(
                "SELECT anchor FROM sync_authority WHERE singleton=1",
                [],
                |r| r.get(0),
            )?;
            let mut anchor: RosterAnchor = serde_json::from_str(&anchor_json)
                .map_err(|_| anyhow::anyhow!("sync_authority_record_invalid"))?;
            anchor.owner.name = trimmed.clone();
            self.connection.execute(
                "UPDATE sync_authority SET anchor=?1 WHERE singleton=1",
                [serde_json::to_string(&anchor)?],
            )?;
            // local_device 与 vault.device 同源，一并改名保持一致。
            let local_json: String = self.connection.query_row(
                "SELECT local_device FROM sync_authority WHERE singleton=1",
                [],
                |r| r.get(0),
            )?;
            let mut local: PairingDevice = serde_json::from_str(&local_json)
                .map_err(|_| anyhow::anyhow!("sync_authority_record_invalid"))?;
            local.name = trimmed.clone();
            self.connection.execute(
                "UPDATE sync_authority SET local_device=?1 WHERE singleton=1",
                [serde_json::to_string(&local)?],
            )?;
        }
        self.apply_signed_roster(&signed.encode()?)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(signed.roster.revision)
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
