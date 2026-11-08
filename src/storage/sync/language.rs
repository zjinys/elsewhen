//! Authenticated language policy deltas. Only the pinned owner changes policy;
//! a device can declare only the capability of its own installation stream.
use super::*;
use crate::storage::knowledge_language::{KnowledgeLanguagePolicy, LanguageCapability};

impl Store {
    pub(super) fn apply_sync_language_capability(
        &self,
        op: &SyncOperation,
        capability: &LanguageCapability,
    ) -> Result<Apply> {
        capability.validate()?;
        ensure!(
            capability.device_id == op.origin_device_id
                && capability.stream_id == op.origin_stream_id,
            "knowledge_language_capability_origin_mismatch"
        );
        if let Some(old) = self.language_capability(&capability.device_id, &capability.stream_id)? {
            if old.generation == capability.generation {
                ensure!(
                    old == *capability,
                    "knowledge_language_capability_equivocation"
                );
            }
        }
        self.put_language_capability(capability)?;
        Ok(Apply::Applied)
    }

    pub(super) fn apply_sync_knowledge_language(
        &self,
        op: &SyncOperation,
        policy: &KnowledgeLanguagePolicy,
        base: &Option<String>,
    ) -> Result<Apply> {
        policy.validate()?;
        let authority = self.sync_pinned_authority()?;
        ensure!(
            op.origin_device_id == authority.owner.device_id
                && policy.changed_by.as_deref() == Some(op.origin_device_id.as_str()),
            "knowledge_language_owner_required"
        );
        if policy.authority_revision > self.sync_roster_anchor()?.revision as i64 {
            return Ok(Apply::Waiting);
        }
        ensure!(
            policy.authority_revision > 0,
            "knowledge_language_authority_missing"
        );
        self.verify_knowledge_language_owner(policy)?;
        let old = self.knowledge_language_policy()?;
        let head = self.sync_object_head("knowledge_language", "dataset")?;
        if let Some(base) = base {
            let known:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM sync_known_revisions WHERE object_kind='knowledge_language' AND object_id='dataset' AND revision_id=?1)",[base],|r|r.get(0))?;
            if !known && head.as_ref().map(|h| &h.0) != Some(base) {
                return Ok(Apply::Waiting);
            }
        }
        if head.as_ref().map(|h| &h.0) != base.as_ref()
            || (policy.generation != old.generation + 1
                && !(head.is_none() && old.generation == 0 && policy.generation > 0))
        {
            return Ok(Apply::Conflict {
                kind: "knowledge_language",
                id: "dataset".into(),
                revision: head.map(|h| h.0),
                reason: "knowledge_language_generation_changed",
            });
        }
        self.write_knowledge_language_policy(policy)?;
        self.sync_set_head("knowledge_language", "dataset", &op.op_id, false)?;
        Ok(Apply::Applied)
    }
}
