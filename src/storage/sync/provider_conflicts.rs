//! Provider sync conflict review (P4b).
//!
//! A remote provider bundle whose base does not match the local head is kept
//! as a `conflict` row in `sync_provider_versions`, ciphertext intact. Review
//! decrypts that envelope with the installation vault key and shows the local
//! and remote profile sets with API keys masked; the user then explicitly
//! keeps the local state or accepts the remote bundle. Nothing is merged
//! field-by-field and no URL is ever rewritten.

use anyhow::{Context, Result, ensure};
use rusqlite::OptionalExtension;

use super::Store;
use crate::sync::{
    crypto::{ContentKey, EncryptedEnvelope, EnvelopeHeader},
    protocol::ProviderBundle,
};

/// 冲突审阅中的单个 Provider 概要：Key 已掩码，绝不携带明文。
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ProviderConflictProfile {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub model: String,
    pub api_key_masked: String,
    pub api_key_present: bool,
    pub enabled: bool,
    pub is_active: bool,
    pub temperature: f64,
    pub max_tokens: Option<i64>,
    pub context_window: Option<i64>,
}

/// 一条待审阅的 Provider 同步冲突，绑定用户实际看到的本地/远端版本。
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ProviderConflictReview {
    pub revision_id: String,
    pub base_revision: Option<String>,
    pub sender_device_id: String,
    pub local_revision: String,
    pub remote_profiles: Vec<ProviderConflictProfile>,
    pub remote_active_id: Option<String>,
    pub remote_failover_order: Vec<String>,
    pub local_profiles: Vec<ProviderConflictProfile>,
    pub local_active_id: Option<String>,
}

/// 用户对一条 Provider 同步冲突的显式裁决。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderConflictResolution {
    KeepLocal,
    AcceptRemote,
}

struct ConflictRow {
    revision_id: String,
    base_revision: Option<String>,
    sender_device_id: String,
    encrypted_envelope: Vec<u8>,
}

fn masked_bundle(bundle: &ProviderBundle) -> Vec<ProviderConflictProfile> {
    bundle
        .profiles
        .iter()
        .map(|p| ProviderConflictProfile {
            id: p.id.clone(),
            name: p.name.clone(),
            provider_type: p.provider_type.clone(),
            base_url: p.base_url.clone(),
            model: p.model.clone(),
            api_key_masked: super::super::provider_keys::mask_api_key(&p.api_key),
            api_key_present: p.api_key_present,
            enabled: p.enabled,
            is_active: bundle.active_id.as_deref() == Some(p.id.as_str()),
            temperature: p.temperature,
            max_tokens: p.max_tokens,
            context_window: p.context_window,
        })
        .collect()
}

impl Store {
    fn provider_conflict_row(&self, revision_id: &str) -> Result<ConflictRow> {
        self.connection
            .query_row(
                "SELECT revision_id,base_revision,sender_device_id,encrypted_envelope
                 FROM sync_provider_versions WHERE revision_id=?1 AND state='conflict'",
                [revision_id],
                |r| {
                    Ok(ConflictRow {
                        revision_id: r.get(0)?,
                        base_revision: r.get(1)?,
                        sender_device_id: r.get(2)?,
                        encrypted_envelope: r.get(3)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("provider_conflict_not_pending"))
    }

    /// Decrypt a conflicted remote envelope with explicitly supplied material.
    /// Production passes the vault-resolved receive key and roster signing key;
    /// tests without a LocalVault inject the symmetric counterpart. The sealed
    /// bytes must re-verify against the exact header recorded at staging time,
    /// so a tampered ciphertext cannot be reinterpreted as the same revision.
    fn open_provider_conflict_with(
        &self,
        row: &ConflictRow,
        dataset_id: &str,
        local_device_id: &str,
        key_version: u64,
        key: &ContentKey,
        trusted_sender_public_key: &[u8; 32],
    ) -> Result<ProviderBundle> {
        let header = EnvelopeHeader::new(
            dataset_id,
            &row.sender_device_id,
            local_device_id,
            key_version,
            format!("provider:{}", row.revision_id),
        )?;
        let envelope = EncryptedEnvelope::from_bytes(&row.encrypted_envelope)?;
        let bundle = ProviderBundle::decode(&crate::sync::crypto::open(
            &envelope,
            &header,
            key,
            trusted_sender_public_key,
        )?)?;
        ensure!(
            bundle.revision_id == row.revision_id && bundle.base_revision == row.base_revision,
            "provider_conflict_revision_mismatch"
        );
        Ok(bundle)
    }

    /// Resolve vault-held decryption material for a conflicted envelope:
    /// sender identity comes from the signed roster, content key from the
    /// installation vault. Fails closed while the runtime key is locked.
    fn provider_conflict_key_material(
        &self,
        sender_device_id: &str,
    ) -> Result<ConflictKeyMaterial> {
        let identity = self
            .sync_identity()?
            .context("provider_conflict_requires_sync")?;
        ensure!(
            identity.device_id != sender_device_id,
            "provider_conflict_sender_unknown"
        );
        self.require_local_sync_membership()?;
        // 与 apply_encrypted_provider_bundle 使用同一成员判定：只有
        // sync_member_keys 与 sync_members 双侧均 active 的发送者可被信任。
        let signer: Option<Vec<u8>> = self.connection.query_row(
            "SELECT k.signing_key FROM sync_member_keys k JOIN sync_members m ON m.device_id=k.device_id AND m.stream_id=k.stream_id WHERE k.device_id=?1 AND k.active=1 AND m.active=1",
            [sender_device_id],
            |r| r.get(0),
        ).optional()?;
        let signer: [u8; 32] = signer
            .and_then(|bytes| bytes.as_slice().try_into().ok())
            .ok_or_else(|| anyhow::anyhow!("provider_conflict_sender_unknown"))?;
        let (installation, revision): (String, i64) = self.connection.query_row(
            "SELECT installation_id,revision FROM sync_local_vault WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        // 冲突信封用的是 vault 包裹密钥，不是 Provider 密封主密钥；
        // 运行时会话持有的正是这把 vault 主密钥。
        let master = crate::sync::runtime::provider_master(&self.path, revision)
            .context("provider_key_locked（请先在设置→数据中解锁本机同步密钥）")?;
        let vault = self.load_sync_vault(&installation, &master, revision)?;
        ensure!(
            vault.revision == revision,
            "provider_conflict_vault_changed"
        );
        let (_, receive, key_version) = vault.peer_keys(&identity.dataset_id, sender_device_id)?;
        Ok(ConflictKeyMaterial {
            dataset_id: identity.dataset_id,
            local_device_id: identity.device_id,
            key_version,
            key: receive,
            signer,
        })
    }

    /// 列出待审阅的 Provider 同步冲突（Key 已掩码）。同步密钥锁定时失败关闭。
    pub fn list_sync_provider_conflicts(&self) -> Result<Vec<ProviderConflictReview>> {
        // Sync setup always enables key sealing (see enable_provider_key_sealing),
        // but fail closed rather than assume it: conflict review must never
        // imply an unsealed store can safely accept a remote key bundle.
        ensure!(
            self.provider_keys_sealed()?,
            "provider_conflict_requires_sealing"
        );
        let rows: Vec<ConflictRow> = {
            let mut statement = self.connection.prepare(
                "SELECT revision_id,base_revision,sender_device_id,encrypted_envelope
                 FROM sync_provider_versions WHERE state='conflict' ORDER BY revision_id",
            )?;
            statement
                .query_map([], |r| {
                    Ok(ConflictRow {
                        revision_id: r.get(0)?,
                        base_revision: r.get(1)?,
                        sender_device_id: r.get(2)?,
                        encrypted_envelope: r.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let local_revision = self.sync_provider_head_revision()?;
        let mut reviews = Vec::with_capacity(rows.len());
        for row in rows {
            let material = self.provider_conflict_key_material(&row.sender_device_id)?;
            let remote = self.open_provider_conflict_with(
                &row,
                &material.dataset_id,
                &material.local_device_id,
                material.key_version,
                &material.key,
                &material.signer,
            )?;
            let local = self.export_provider_bundle(&row.revision_id, None)?;
            reviews.push(ProviderConflictReview {
                revision_id: row.revision_id.clone(),
                base_revision: row.base_revision.clone(),
                sender_device_id: row.sender_device_id.clone(),
                local_revision: local_revision.clone(),
                remote_profiles: masked_bundle(&remote),
                remote_active_id: remote.active_id.clone(),
                remote_failover_order: remote.failover_order.clone(),
                local_profiles: masked_bundle(&local),
                local_active_id: local.active_id.clone(),
            });
        }
        Ok(reviews)
    }

    /// 显式裁决一条 Provider 同步冲突：保留本地或接受远端整包。
    /// 两个期望版本都必须与用户审阅时看到的版本一致，否则失败关闭。
    pub fn resolve_sync_provider_conflict(
        &self,
        revision_id: &str,
        expected_local_revision: &str,
        expected_remote_revision: &str,
        resolution: ProviderConflictResolution,
    ) -> Result<()> {
        ensure!(
            self.provider_keys_sealed()?,
            "provider_conflict_requires_sealing"
        );
        let tx = self.begin_write_transaction()?;
        crate::sync::protocol::validate_id(revision_id)?;
        crate::sync::protocol::validate_id(expected_local_revision)?;
        ensure!(
            revision_id == expected_remote_revision,
            "provider_conflict_revision_mismatch（冲突内容已变化，请重新审阅）"
        );
        let row = self.provider_conflict_row(revision_id)?;
        ensure!(
            self.sync_provider_head_revision()? == expected_local_revision,
            "provider_conflict_local_changed（本地配置已变化，请重新审阅）"
        );
        match resolution {
            ProviderConflictResolution::KeepLocal => {
                self.connection.execute(
                    "UPDATE sync_provider_versions SET state='dismissed' WHERE revision_id=?1 AND state='conflict'",
                    [revision_id],
                )?;
                // Re-advertise the current local state so the remote device
                // converges onto the revision the user chose to keep.
                self.sync_mark_provider_dirty()?;
            }
            ProviderConflictResolution::AcceptRemote => {
                let material = self.provider_conflict_key_material(&row.sender_device_id)?;
                let bundle = self.open_provider_conflict_with(
                    &row,
                    &material.dataset_id,
                    &material.local_device_id,
                    material.key_version,
                    &material.key,
                    &material.signer,
                )?;
                self.replace_provider_bundle_on(&bundle)?;
                self.connection.execute(
                    "UPDATE sync_provider_versions SET state='applied' WHERE revision_id=?1 AND state='conflict'",
                    [revision_id],
                )?;
                self.connection.execute(
                    "INSERT INTO sync_provider_state(singleton,revision_id) VALUES(1,?1)
                     ON CONFLICT(singleton) DO UPDATE SET revision_id=excluded.revision_id",
                    [revision_id],
                )?;
                self.connection
                    .execute("DELETE FROM sync_provider_dirty", [])?;
            }
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
}

struct ConflictKeyMaterial {
    dataset_id: String,
    local_device_id: String,
    key_version: u64,
    key: ContentKey,
    signer: [u8; 32],
}

#[cfg(test)]
impl Store {
    /// Review helper for stores without a LocalVault: the caller injects the
    /// symmetric content key and the sender's signing key directly.
    pub(crate) fn list_sync_provider_conflicts_with(
        &self,
        key_version: u64,
        key: &ContentKey,
        signer: &[u8; 32],
    ) -> Result<Vec<ProviderConflictReview>> {
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let rows: Vec<ConflictRow> = {
            let mut statement = self.connection.prepare(
                "SELECT revision_id,base_revision,sender_device_id,encrypted_envelope
                 FROM sync_provider_versions WHERE state='conflict' ORDER BY revision_id",
            )?;
            statement
                .query_map([], |r| {
                    Ok(ConflictRow {
                        revision_id: r.get(0)?,
                        base_revision: r.get(1)?,
                        sender_device_id: r.get(2)?,
                        encrypted_envelope: r.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let local_revision = self.sync_provider_head_revision()?;
        let mut reviews = Vec::with_capacity(rows.len());
        for row in rows {
            let remote = self.open_provider_conflict_with(
                &row,
                &identity.dataset_id,
                &identity.device_id,
                key_version,
                key,
                signer,
            )?;
            let local = self.export_provider_bundle(&row.revision_id, None)?;
            reviews.push(ProviderConflictReview {
                revision_id: row.revision_id.clone(),
                base_revision: row.base_revision.clone(),
                sender_device_id: row.sender_device_id.clone(),
                local_revision: local_revision.clone(),
                remote_profiles: masked_bundle(&remote),
                remote_active_id: remote.active_id.clone(),
                remote_failover_order: remote.failover_order.clone(),
                local_profiles: masked_bundle(&local),
                local_active_id: local.active_id.clone(),
            });
        }
        Ok(reviews)
    }

    /// Resolution with injected material, mirroring resolve_sync_provider_conflict.
    pub(crate) fn resolve_sync_provider_conflict_with(
        &self,
        revision_id: &str,
        expected_local_revision: &str,
        expected_remote_revision: &str,
        resolution: ProviderConflictResolution,
        key_version: u64,
        key: &ContentKey,
        signer: &[u8; 32],
    ) -> Result<()> {
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let tx = self.begin_write_transaction()?;
        ensure!(
            revision_id == expected_remote_revision,
            "provider_conflict_revision_mismatch"
        );
        let row = self.provider_conflict_row(revision_id)?;
        ensure!(
            self.sync_provider_head_revision()? == expected_local_revision,
            "provider_conflict_local_changed"
        );
        match resolution {
            ProviderConflictResolution::KeepLocal => {
                self.connection.execute(
                    "UPDATE sync_provider_versions SET state='dismissed' WHERE revision_id=?1 AND state='conflict'",
                    [revision_id],
                )?;
                self.sync_mark_provider_dirty()?;
            }
            ProviderConflictResolution::AcceptRemote => {
                let bundle = self.open_provider_conflict_with(
                    &row,
                    &identity.dataset_id,
                    &identity.device_id,
                    key_version,
                    key,
                    signer,
                )?;
                self.replace_provider_bundle_on(&bundle)?;
                self.connection.execute(
                    "UPDATE sync_provider_versions SET state='applied' WHERE revision_id=?1 AND state='conflict'",
                    [revision_id],
                )?;
                self.connection.execute(
                    "INSERT INTO sync_provider_state(singleton,revision_id) VALUES(1,?1)
                     ON CONFLICT(singleton) DO UPDATE SET revision_id=excluded.revision_id",
                    [revision_id],
                )?;
                self.connection
                    .execute("DELETE FROM sync_provider_dirty", [])?;
            }
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
