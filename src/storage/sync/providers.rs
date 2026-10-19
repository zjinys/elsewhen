//! Full Provider configuration replication. Only authenticated ciphertext is
//! retained in synchronization history; settings DTOs remain redacted.

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use crate::storage::Store;
use crate::sync::crypto::{self, ContentKey, EncryptedEnvelope, EnvelopeHeader};
use crate::sync::protocol::{ProviderBundle, ProviderProfile};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderApplyResult {
    Applied,
    Duplicate,
    Conflict,
}

impl Store {
    pub(crate) fn sync_mark_provider_dirty(&self) -> Result<()> {
        if self.sync_identity()?.is_none() {
            return Ok(());
        }
        ensure!(
            !self.connection.is_autocommit(),
            "sync_requires_domain_transaction"
        );
        let head: Option<String> = self
            .connection
            .query_row(
                "SELECT revision_id FROM sync_provider_state WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let revision = uuid::Uuid::new_v4().to_string();
        let prior_base: Option<Option<String>> = self
            .connection
            .query_row(
                "SELECT base_revision FROM sync_provider_dirty WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let base = prior_base.unwrap_or(head.clone());
        self.connection.execute("INSERT INTO sync_provider_dirty(singleton,base_revision,revision_id) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET revision_id=excluded.revision_id", params![head, revision])?;
        self.connection.execute("INSERT INTO sync_provider_state(singleton,revision_id,base_revision) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET revision_id=excluded.revision_id,base_revision=excluded.base_revision", params![revision,base])?;
        Ok(())
    }

    /// Current local provider revision (the head users must bind conflict
    /// resolutions to). The singleton row always exists once sync was
    /// initialized; reading it keeps one canonical source for conflict review.
    pub(crate) fn sync_provider_head_revision(&self) -> Result<String> {
        self.connection
            .query_row(
                "SELECT revision_id FROM sync_provider_state WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub(crate) fn ensure_sync_provider_baseline(&self) -> Result<()> {
        ensure!(self.sync_identity()?.is_some(), "sync_not_initialized");
        self.connection.execute("INSERT OR IGNORE INTO sync_provider_state(singleton,revision_id,base_revision) VALUES(1,?1,NULL)",[uuid::Uuid::new_v4().to_string()])?;
        Ok(())
    }

    pub(crate) fn current_provider_bundle(&self) -> Result<ProviderBundle> {
        let transaction = if self.connection.is_autocommit() {
            Some(self.connection.unchecked_transaction()?)
        } else {
            None
        };
        let (revision, base): (String, Option<String>) = self.connection.query_row(
            "SELECT revision_id,base_revision FROM sync_provider_state WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let bundle = self.export_provider_bundle(&revision, base)?;
        if let Some(tx) = transaction {
            tx.commit()?;
        }
        Ok(bundle)
    }

    /// Freeze the coalesced local revision only after every current peer has an
    /// encrypted durable delivery. Later edits then use this published base.
    pub(crate) fn mark_provider_revision_prepared(&self, expected_revision: &str) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let bundle = self.current_provider_bundle()?;
        ensure!(
            bundle.revision_id == expected_revision,
            "sync_provider_revision_changed"
        );
        let digest = super::hash(&serde_json::to_vec(&bundle)?);
        let message_id = format!("provider:{expected_revision}");
        // 同 revision 的重封密文不同（nonce 随机），但对方已 ack 过同 id 信封
        // 即视为已送达；只要求 hash 精确匹配会永远清不掉 dirty（重发事故）。
        let missing:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM sync_members m WHERE m.active=1 AND m.device_id!=?1 AND NOT EXISTS(SELECT 1 FROM sync_wire_outbox w WHERE w.recipient_device_id=m.device_id AND w.message_id=?2 AND (w.payload_hash=?3 OR w.state='acknowledged')))",params![identity.device_id,message_id,digest],|r|r.get(0))?;
        ensure!(!missing, "sync_provider_delivery_not_prepared");
        self.connection.execute(
            "DELETE FROM sync_provider_dirty WHERE revision_id=?1",
            [expected_revision],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// Internal secret-bearing export, never exposed as a settings query.
    pub(crate) fn export_provider_bundle(
        &self,
        revision_id: &str,
        base_revision: Option<String>,
    ) -> Result<ProviderBundle> {
        let tx = if self.connection.is_autocommit() {
            Some(self.connection.unchecked_transaction()?)
        } else {
            None
        };
        let mut query=self.connection.prepare("SELECT id,name,provider_type,base_url,model,'',enabled,created_at,updated_at,temperature,max_tokens,context_window,api_key IS NOT NULL FROM ai_provider_configs ORDER BY created_at,id LIMIT 65")?;
        let mut profiles = query
            .query_map([], |r| {
                Ok(ProviderProfile {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    provider_type: r.get(2)?,
                    base_url: r.get(3)?,
                    model: r.get(4)?,
                    api_key: r.get(5)?,
                    api_key_present: r.get(12)?,
                    enabled: r.get(6)?,
                    created_at: r.get(7)?,
                    updated_at: r.get(8)?,
                    temperature: r.get(9)?,
                    max_tokens: r.get(10)?,
                    context_window: r.get(11)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for profile in &mut profiles {
            let key = self.read_provider_key(&profile.id)?;
            profile.api_key_present = key.is_some();
            profile.api_key = key.unwrap_or_default();
        }
        let active_id = self
            .connection
            .query_row(
                "SELECT id FROM ai_provider_configs WHERE is_active=1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let failover_order = self.connection.prepare("SELECT id FROM ai_provider_configs ORDER BY is_active DESC,COALESCE((SELECT position FROM ai_provider_sync_order WHERE config_id=ai_provider_configs.id),9223372036854775807),created_at,id LIMIT 65")?
            .query_map([],|r|r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
        let bundle = ProviderBundle {
            revision_id: revision_id.to_string(),
            base_revision,
            profiles,
            active_id,
            failover_order,
        };
        bundle.validate()?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(bundle)
    }

    /// The pairing/transport layer supplies already-trusted keys and expected
    /// context. An incoming header cannot grant itself membership or ownership.
    pub(crate) fn apply_encrypted_provider_bundle(
        &self,
        bytes: &[u8],
        expected: &EnvelopeHeader,
        key: &ContentKey,
        trusted_sender: &[u8; 32],
    ) -> Result<ProviderApplyResult> {
        self.require_sync_transport_key(&expected.sender_device_id, trusted_sender, false)?;
        let envelope = EncryptedEnvelope::from_bytes(bytes)?;
        let plaintext = crypto::open(&envelope, expected, key, trusted_sender)?;
        let bundle = ProviderBundle::decode(&plaintext)?;
        ensure!(
            expected.message_id == format!("provider:{}", bundle.revision_id),
            "sync_provider_identity_mismatch"
        );
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        ensure!(
            expected.dataset_id == identity.dataset_id
                && expected.recipient_device_id == identity.device_id,
            "sync_provider_wrong_recipient"
        );
        let tx = self.begin_write_transaction()?;
        let member: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
            [&expected.sender_device_id],
            |r| r.get(0),
        )?;
        ensure!(member, "sync_member_not_authorized");
        let canonical = serde_json::to_vec(&bundle)
            .map_err(|_| anyhow::anyhow!("sync_provider_bundle_invalid"))?;
        let digest = super::hash(&canonical);
        let existing: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT content_hash,state FROM sync_provider_versions WHERE revision_id=?1 UNION ALL SELECT content_hash,'applied' FROM sync_checkpoint_providers WHERE revision_id=?1",
                [&bundle.revision_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((hash, state)) = existing {
            ensure!(hash == digest, "sync_provider_revision_content_conflict");
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(if state == "conflict" {
                ProviderApplyResult::Conflict
            } else {
                ProviderApplyResult::Duplicate
            });
        }
        let head: Option<String> = self
            .connection
            .query_row(
                "SELECT revision_id FROM sync_provider_state WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let has_profiles: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM ai_provider_configs)",
            [],
            |r| r.get(0),
        )?;
        // A new member joins an empty dataset. Never overwrite an independent
        // local vault merely because the sender calls its payload a snapshot.
        let conflict = head != bundle.base_revision || (head.is_none() && has_profiles);
        self.connection.execute("INSERT INTO sync_provider_versions(revision_id,base_revision,sender_device_id,content_hash,encrypted_envelope,state) VALUES(?1,?2,?3,?4,?5,?6)",params![bundle.revision_id,bundle.base_revision,expected.sender_device_id,digest,bytes,if conflict {"conflict"} else {"applied"}])?;
        if conflict {
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(ProviderApplyResult::Conflict);
        }
        self.replace_provider_bundle_on(&bundle)?;
        self.connection.execute("INSERT INTO sync_provider_state(singleton,revision_id,base_revision) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET revision_id=excluded.revision_id,base_revision=excluded.base_revision",params![bundle.revision_id,bundle.base_revision])?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(ProviderApplyResult::Applied)
    }

    pub(super) fn replace_provider_bundle_on(&self, bundle: &ProviderBundle) -> Result<()> {
        if self.has_sync_authority()? && bundle.profiles.iter().any(|p| p.api_key_present) {
            ensure!(
                self.provider_keys_sealed()?,
                "provider_key_sealing_required"
            );
        }
        ensure!(
            !self.connection.is_autocommit(),
            "sync_requires_domain_transaction"
        );
        // Full snapshot semantics: empty key is an actual empty value here,
        // never the settings form's "leave the old key unchanged" sentinel.
        self.connection
            .execute("DELETE FROM ai_provider_configs", [])?;
        for profile in &bundle.profiles {
            self.connection.execute("INSERT INTO ai_provider_configs(id,name,provider_type,base_url,model,api_key_source,api_key,enabled,is_active,created_at,updated_at,temperature,max_tokens,context_window) VALUES(?1,?2,?3,?4,?5,'database',?6,?7,?8,?9,?10,?11,?12,?13)",
                params![profile.id,profile.name,profile.provider_type,profile.base_url,profile.model,Option::<&str>::None,profile.enabled,bundle.active_id.as_deref()==Some(profile.id.as_str()),profile.created_at,profile.updated_at,profile.temperature,profile.max_tokens,profile.context_window])
                .map_err(|_|anyhow::anyhow!("sync_provider_apply_failed"))?;
            self.write_provider_key_on(
                &profile.id,
                profile.api_key_present.then_some(profile.api_key.as_str()),
            )?;
        }
        for (position, id) in bundle.failover_order.iter().enumerate() {
            self.connection.execute(
                "INSERT INTO ai_provider_sync_order(config_id,position) VALUES(?1,?2)",
                params![id, position as i64],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "providers_tests.rs"]
mod tests;
