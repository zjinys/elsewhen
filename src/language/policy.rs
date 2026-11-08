//! Serializable knowledge-language policy and application capability records.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactLanguage {
    pub output_locale: String,
    pub policy_version: i64,
    pub policy_generation: i64,
    pub dependency: String,
}

impl ArtifactLanguage {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            crate::storage::prompts::normalize(&self.output_locale)? == self.output_locale
                && (1..=1_000_000).contains(&self.policy_version)
                && self.policy_generation >= 0
                && self.dependency.len() == 64
                && self.dependency.bytes().all(|b| b.is_ascii_hexdigit()),
            "knowledge_artifact_language_invalid"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeLanguagePolicy {
    pub output_locale: String,
    pub policy_version: i64,
    pub generation: i64,
    pub authority_revision: i64,
    pub changed_by: Option<String>,
    pub dataset_id: Option<String>,
    pub owner_signature: Option<String>,
    pub updated_at: String,
}
impl KnowledgeLanguagePolicy {
    pub(crate) fn validate(&self) -> Result<()> {
        let locale = crate::storage::prompts::normalize(&self.output_locale)?;
        ensure!(
            locale == self.output_locale
                && (1..=1_000_000).contains(&self.policy_version)
                && self.generation >= 0
                && self.authority_revision >= 0,
            "knowledge_language_policy_invalid"
        );
        ensure!(
            !self.updated_at.is_empty() && self.updated_at.len() <= 64,
            "knowledge_language_time_invalid"
        );
        if let Some(device) = &self.changed_by {
            crate::sync::protocol::validate_id(device)?;
        }
        if let Some(dataset) = &self.dataset_id {
            crate::sync::protocol::validate_id(dataset)?;
        }
        ensure!(
            self.owner_signature.as_ref().is_none_or(|s| s.len() <= 128),
            "knowledge_language_signature_invalid"
        );
        Ok(())
    }

    pub(crate) fn signed_payload(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&(
            "elsewhen.knowledge-language.v1",
            &self.dataset_id,
            &self.output_locale,
            self.policy_version,
            self.generation,
            self.authority_revision,
            &self.changed_by,
            &self.updated_at,
        ))?)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageCapability {
    pub device_id: String,
    pub stream_id: String,
    pub generation: i64,
    pub roster_revision: i64,
    pub engine_version: i64,
    pub max_policy_version: i64,
    pub locales: Vec<String>,
    pub updated_at: String,
}
impl LanguageCapability {
    pub(crate) fn validate(&self) -> Result<()> {
        crate::sync::protocol::validate_id(&self.device_id)?;
        crate::sync::protocol::validate_id(&self.stream_id)?;
        ensure!(
            self.generation > 0
                && self.roster_revision >= 0
                && self.engine_version > 0
                && self.max_policy_version > 0
                && self.locales.len() <= 32
                && !self.locales.is_empty()
                && !self.updated_at.is_empty()
                && self.updated_at.len() <= 64,
            "knowledge_language_capability_invalid"
        );
        let mut unique = std::collections::HashSet::new();
        for locale in &self.locales {
            ensure!(
                crate::storage::prompts::normalize(locale)? == *locale && unique.insert(locale),
                "knowledge_language_capability_invalid"
            );
        }
        Ok(())
    }
}
