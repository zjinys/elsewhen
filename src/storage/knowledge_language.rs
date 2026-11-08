//! Dataset knowledge-language policy and authenticated application capability.
//! Templates, UI locale and local response preferences never enter this domain.
use super::Store;
pub(crate) use crate::language::policy::{KnowledgeLanguagePolicy, LanguageCapability};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use rusqlite::{OptionalExtension, params};

pub(crate) const POLICY_VERSION: i64 = 1;
pub(crate) const ENGINE_VERSION: i64 = 1;

pub(crate) fn supported(locale: &str, version: i64) -> bool {
    version == POLICY_VERSION && matches!(locale, "zh-Hans" | "en")
}

pub(crate) fn register_connection_capability(connection: &rusqlite::Connection) -> Result<()> {
    connection.create_scalar_function(
        "elsewhen_language_policy_supported",
        2,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8
            | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| Ok(supported(&ctx.get::<String>(0)?, ctx.get::<i64>(1)?)),
    )?;
    Ok(())
}

impl Store {
    /// A settings preview never treats relay capabilities as application
    /// support. The write path repeats these checks inside its transaction.
    pub(crate) fn knowledge_language_edit_blocker(&self) -> Result<Option<&'static str>> {
        if !self.is_sync_owner_device()? {
            return Ok(Some("owner_required"));
        }
        let Some(identity) = self.sync_identity()? else {
            return Ok(None);
        };
        if !self.has_sync_authority()? {
            return Ok(Some("authority_pending"));
        }
        let members = self
            .connection
            .prepare("SELECT device_id FROM sync_members WHERE active=1 AND device_id<>?1")?
            .query_map([identity.device_id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for device in members {
            if self
                .require_language_processor_capability(&device, "en", POLICY_VERSION)
                .is_err()
            {
                return Ok(Some("capability_required"));
            }
        }
        Ok(None)
    }

    /// Local provenance points to the exact immutable manifest. It is never
    /// inferred from whichever preference happens to be current at completion.
    pub(crate) fn record_language_artifact(
        &self,
        kind: &str,
        id: &str,
        bundle: &str,
        keys: &[&str],
    ) -> Result<()> {
        self.record_language_artifact_with_output(kind, id, bundle, keys, None)
    }

    pub(crate) fn record_language_artifact_with_output(
        &self,
        kind: &str,
        id: &str,
        bundle: &str,
        keys: &[&str],
        output: Option<&str>,
    ) -> Result<()> {
        let _scope = self.use_prompt_bundle(bundle);
        let language = self.run_language()?;
        let dependency = self.prompt_dependency(keys)?;
        let output = output.unwrap_or(&language.knowledge_locale);
        self.connection.execute(
            "INSERT OR IGNORE INTO knowledge_language_artifacts VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                kind,
                id,
                output,
                language.knowledge_policy_version,
                language.knowledge_policy_generation,
                dependency,
                bundle,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        self.put_artifact_language(
            kind,
            id,
            &crate::language::policy::ArtifactLanguage {
                output_locale: output.into(),
                policy_version: language.knowledge_policy_version,
                policy_generation: language.knowledge_policy_generation,
                dependency,
            },
        )?;
        Ok(())
    }

    pub(crate) fn artifact_language(
        &self,
        kind: &str,
        id: &str,
    ) -> Result<Option<crate::language::policy::ArtifactLanguage>> {
        self.connection.query_row("SELECT metadata_json FROM knowledge_artifact_languages WHERE artifact_kind=?1 AND artifact_id=?2", params![kind,id], |r| r.get::<_,String>(0)).optional()?
            .map(|s| Ok(serde_json::from_str(&s)?)).transpose()
    }

    pub(crate) fn put_artifact_language(
        &self,
        kind: &str,
        id: &str,
        metadata: &crate::language::policy::ArtifactLanguage,
    ) -> Result<()> {
        metadata.validate()?;
        if let Some(existing) = self.artifact_language(kind, id)? {
            ensure!(
                existing == *metadata,
                "knowledge_artifact_language_conflict"
            );
        } else {
            self.connection.execute(
                "INSERT INTO knowledge_artifact_languages VALUES(?1,?2,?3)",
                params![kind, id, serde_json::to_string(metadata)?],
            )?;
        }
        Ok(())
    }

    pub(crate) fn analysis_prompt_bundle(&self, job: &str) -> Result<String> {
        let saved = self.connection.query_row(
            "SELECT prompt_bundle_id FROM analysis_jobs WHERE id=?1",
            [job],
            |r| r.get::<_, Option<String>>(0),
        )?;
        if let Some(saved) = saved {
            return Ok(saved);
        }
        // Legacy in-flight jobs and direct import callers predate claim-time
        // pinning. Establish their first manifest once; never replace it later.
        let generation: i64 = self.connection.query_row(
            "SELECT prompt_generation FROM analysis_jobs WHERE id=?1",
            [job],
            |r| r.get(0),
        )?;
        let bundle =
            self.capture_prompt_bundle("background_task", &format!("analysis:{job}:{generation}"))?;
        self.connection.execute(
            "UPDATE analysis_jobs SET prompt_bundle_id=?2 WHERE id=?1 AND prompt_bundle_id IS NULL",
            params![job, bundle],
        )?;
        Ok(bundle)
    }

    pub(crate) fn digest_prompt_bundle(&self, run: &str) -> Result<String> {
        self.connection
            .query_row(
                "SELECT prompt_bundle_id FROM knowledge_digest_runs WHERE id=?1",
                [run],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub(crate) fn knowledge_language_policy(&self) -> Result<KnowledgeLanguagePolicy> {
        Ok(self.connection.query_row(
            "SELECT output_locale,policy_version,generation,authority_revision,changed_by,updated_at,dataset_id,owner_signature FROM dataset_content_language WHERE singleton=1",
            [], |r| Ok(KnowledgeLanguagePolicy { output_locale:r.get(0)?, policy_version:r.get(1)?, generation:r.get(2)?, authority_revision:r.get(3)?, changed_by:r.get(4)?, updated_at:r.get(5)?, dataset_id:r.get(6)?, owner_signature:r.get(7)? }),
        )?)
    }

    pub(crate) fn language_capability(
        &self,
        device: &str,
        stream: &str,
    ) -> Result<Option<LanguageCapability>> {
        self.connection.query_row(
            "SELECT generation,engine_version,max_policy_version,locales_json,updated_at,roster_revision FROM sync_language_capabilities WHERE device_id=?1 AND stream_id=?2",
            params![device,stream], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,i64>(5)?)),
        ).optional()?.map(|(generation,engine_version,max_policy_version,locales,updated_at,roster_revision)| Ok(LanguageCapability {device_id:device.into(),stream_id:stream.into(),generation,roster_revision,engine_version,max_policy_version,locales:serde_json::from_str(&locales)?,updated_at})).transpose()
    }

    pub(crate) fn put_language_capability(&self, capability: &LanguageCapability) -> Result<()> {
        capability.validate()?;
        self.connection.execute(
            "INSERT INTO sync_language_capabilities(device_id,stream_id,generation,engine_version,max_policy_version,locales_json,updated_at,roster_revision) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(device_id,stream_id) DO UPDATE SET generation=excluded.generation,roster_revision=excluded.roster_revision,engine_version=excluded.engine_version,max_policy_version=excluded.max_policy_version,locales_json=excluded.locales_json,updated_at=excluded.updated_at WHERE excluded.generation>sync_language_capabilities.generation",
            params![capability.device_id,capability.stream_id,capability.generation,capability.engine_version,capability.max_policy_version,serde_json::to_string(&capability.locales)?,capability.updated_at,capability.roster_revision],
        )?;
        Ok(())
    }

    pub(crate) fn announce_language_capability(&self) -> Result<()> {
        let Some(identity) = self.sync_identity()? else {
            return Ok(());
        };
        self.require_local_sync_membership()?;
        let tx = self.begin_write_transaction()?;
        let previous = self.language_capability(&identity.device_id, &identity.stream_id)?;
        let roster_revision = self.sync_roster_anchor()?.revision as i64;
        let locales = vec!["zh-Hans".to_string(), "en".to_string()];
        if previous.as_ref().is_some_and(|c| {
            c.roster_revision == roster_revision
                && c.engine_version == ENGINE_VERSION
                && c.max_policy_version == POLICY_VERSION
                && c.locales == locales
        }) {
            return Ok(());
        }
        let capability = LanguageCapability {
            device_id: identity.device_id,
            stream_id: identity.stream_id,
            generation: self.replica_next_sequence()?,
            roster_revision,
            engine_version: ENGINE_VERSION,
            max_policy_version: POLICY_VERSION,
            locales,
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        self.put_language_capability(&capability)?;
        self.append_sync_mutation(
            crate::sync::protocol::SyncMutation::LanguageCapabilityDeclared { capability },
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    pub(crate) fn require_language_processor_capability(
        &self,
        device: &str,
        locale: &str,
        version: i64,
    ) -> Result<()> {
        if locale == "zh-Hans" && version == 1 {
            return Ok(());
        }
        let stream: String = self
            .connection
            .query_row(
                "SELECT stream_id FROM sync_members WHERE device_id=?1 AND active=1",
                [device],
                |r| r.get(0),
            )
            .optional()?
            .context("knowledge_language_processor_inactive")?;
        let capability = self
            .language_capability(device, &stream)?
            .context("knowledge_language_capability_missing")?;
        ensure!(
            capability.roster_revision == self.sync_roster_anchor()?.revision as i64,
            "knowledge_language_capability_stale"
        );
        ensure!(
            capability.engine_version >= ENGINE_VERSION
                && capability.max_policy_version >= version
                && capability.locales.iter().any(|l| l == locale),
            "knowledge_language_processor_incompatible"
        );
        Ok(())
    }

    pub(crate) fn set_knowledge_language(
        &self,
        locale: &str,
        expected: i64,
    ) -> Result<KnowledgeLanguagePolicy> {
        self.set_knowledge_language_with_signer(locale, expected, None)
    }

    pub(crate) fn set_knowledge_language_with_signer(
        &self,
        locale: &str,
        expected: i64,
        signer: Option<&crate::sync::crypto::DeviceIdentity>,
    ) -> Result<KnowledgeLanguagePolicy> {
        ensure!(
            supported(locale, POLICY_VERSION),
            "knowledge_language_unsupported"
        );
        let tx = self.begin_write_transaction()?;
        ensure!(
            self.is_sync_owner_device()?,
            "knowledge_language_owner_required"
        );
        let old = self.knowledge_language_policy()?;
        ensure!(old.generation == expected, "knowledge_language_changed");
        let mut policy = old.clone();
        if let Some(identity) = self.sync_identity()? {
            self.announce_language_capability()?;
            let members = self
                .connection
                .prepare("SELECT device_id FROM sync_members WHERE active=1")?
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for device in members {
                self.require_language_processor_capability(&device, locale, POLICY_VERSION)?;
            }
            policy.authority_revision = self.sync_roster_anchor()?.revision as i64;
            policy.changed_by = Some(identity.device_id);
            policy.dataset_id = Some(identity.dataset_id);
            let signer = signer.context("knowledge_language_owner_signer_required")?;
            self.require_sync_signer(signer)?;
            ensure!(
                signer.public_key() == self.sync_pinned_authority()?.owner.signing_key,
                "knowledge_language_owner_required"
            );
        }
        if old.output_locale == locale
            && old.policy_version == POLICY_VERSION
            && (self.sync_identity()?.is_none() || old.owner_signature.is_some())
        {
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(old);
        }
        policy.output_locale = locale.into();
        policy.policy_version = POLICY_VERSION;
        policy.generation = old
            .generation
            .checked_add(1)
            .context("knowledge_language_generation_exhausted")?;
        policy.updated_at = chrono::Utc::now().to_rfc3339();
        let base_revision = self
            .sync_object_head("knowledge_language", "dataset")?
            .map(|h| h.0);
        if let Some(signer) = signer {
            policy.owner_signature =
                Some(STANDARD.encode(signer.sign_membership(&policy.signed_payload()?)?));
        }
        self.write_knowledge_language_policy(&policy)?;
        if let Some(op) = self.append_sync_mutation(
            crate::sync::protocol::SyncMutation::KnowledgeLanguageChanged {
                policy: policy.clone(),
                base_revision,
            },
        )? {
            self.sync_set_head("knowledge_language", "dataset", &op, false)?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(policy)
    }

    pub(crate) fn write_knowledge_language_policy(
        &self,
        policy: &KnowledgeLanguagePolicy,
    ) -> Result<()> {
        policy.validate()?;
        self.connection.execute("UPDATE dataset_content_language SET output_locale=?1,policy_version=?2,generation=?3,authority_revision=?4,changed_by=?5,updated_at=?6,dataset_id=?7,owner_signature=?8 WHERE singleton=1",params![policy.output_locale,policy.policy_version,policy.generation,policy.authority_revision,policy.changed_by,policy.updated_at,policy.dataset_id,policy.owner_signature])?;
        Ok(())
    }

    pub(crate) fn verify_knowledge_language_owner(
        &self,
        policy: &KnowledgeLanguagePolicy,
    ) -> Result<()> {
        policy.validate()?;
        let authority = self.sync_pinned_authority()?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        ensure!(
            policy.dataset_id.as_deref() == Some(identity.dataset_id.as_str())
                && policy.changed_by.as_deref() == Some(authority.owner.device_id.as_str())
                && policy.authority_revision > 0
                && policy.authority_revision <= self.sync_roster_anchor()?.revision as i64,
            "knowledge_language_owner_required"
        );
        let signature = STANDARD.decode(
            policy
                .owner_signature
                .as_ref()
                .context("knowledge_language_owner_proof_missing")?,
        )?;
        crate::sync::crypto::verify_membership_signature(
            &authority.owner.signing_key,
            &policy.signed_payload()?,
            &signature,
        )?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "knowledge_language_tests.rs"]
mod tests;
