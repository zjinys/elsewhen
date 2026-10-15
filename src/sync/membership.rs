//! Owner-signed cumulative membership. A transport key proves a paired channel,
//! not enrollment. The owner trust root is pinned by explicit Noise approval.

use super::{
    crypto::{DeviceIdentity, verify_membership_signature},
    pairing::PairingDevice,
    protocol::validate_id,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub(crate) const MAX_ROSTER_BYTES: usize = 256 * 1024;
pub(crate) const MAX_MEMBERS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RosterAnchor {
    pub dataset_id: String,
    pub owner: PairingDevice,
    pub revision: u64,
    pub digest: String,
}
impl RosterAnchor {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_id(&self.dataset_id)?;
        self.owner.validate()?;
        ensure!(
            (1..=i64::MAX as u64).contains(&self.revision),
            "sync_membership_revision_invalid"
        );
        ensure!(
            self.digest.len() == 64
                && self
                    .digest
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "sync_membership_digest_invalid"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RosterMember {
    pub device: PairingDevice,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProcessingYieldProof {
    pub dataset_id: String,
    pub from_device_id: String,
    pub to_device_id: String,
    pub roster_revision: u64,
    pub final_sequence: u64,
    pub signature: Vec<u8>,
}

impl ProcessingYieldProof {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_id(&self.dataset_id)?;
        validate_id(&self.from_device_id)?;
        validate_id(&self.to_device_id)?;
        ensure!(
            self.from_device_id != self.to_device_id,
            "sync_handoff_self_transfer_invalid"
        );
        ensure!(
            (1..=i64::MAX as u64).contains(&self.roster_revision),
            "sync_handoff_revision_invalid"
        );
        ensure!(self.signature.len() == 64, "sync_handoff_signature_invalid");
        Ok(())
    }

    pub(crate) fn sign(
        dataset_id: impl Into<String>,
        from_device_id: impl Into<String>,
        to_device_id: impl Into<String>,
        roster_revision: u64,
        final_sequence: u64,
        signer: &DeviceIdentity,
    ) -> Result<Self> {
        let dataset_id = dataset_id.into();
        let from_device_id = from_device_id.into();
        let to_device_id = to_device_id.into();
        let payload = Self::signed_payload(
            &dataset_id,
            &from_device_id,
            &to_device_id,
            roster_revision,
            final_sequence,
        )?;
        let signature = signer.sign_handoff(&payload)?;
        let proof = Self {
            dataset_id,
            from_device_id,
            to_device_id,
            roster_revision,
            final_sequence,
            signature,
        };
        proof.validate()?;
        Ok(proof)
    }

    pub(crate) fn verify(&self, public_key: &[u8; 32]) -> Result<()> {
        self.validate()?;
        let payload = Self::signed_payload(
            &self.dataset_id,
            &self.from_device_id,
            &self.to_device_id,
            self.roster_revision,
            self.final_sequence,
        )?;
        crate::sync::crypto::verify_handoff_signature(public_key, &payload, &self.signature)
    }

    fn signed_payload(
        dataset_id: &str,
        from_device_id: &str,
        to_device_id: &str,
        roster_revision: u64,
        final_sequence: u64,
    ) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct Raw<'a> {
            dataset_id: &'a str,
            from_device_id: &'a str,
            to_device_id: &'a str,
            roster_revision: u64,
            final_sequence: u64,
        }
        Ok(serde_json::to_vec(&Raw {
            dataset_id,
            from_device_id,
            to_device_id,
            roster_revision,
            final_sequence,
        })?)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MembershipRoster {
    pub version: u32,
    pub dataset_id: String,
    pub owner: PairingDevice,
    pub revision: u64,
    pub processor_device_id: String,
    pub members: Vec<RosterMember>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<ProcessingYieldProof>,
}
impl MembershipRoster {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "sync_membership_version_unsupported");
        validate_id(&self.dataset_id)?;
        self.owner.validate()?;
        validate_id(&self.processor_device_id)?;
        ensure!(
            (1..=i64::MAX as u64).contains(&self.revision),
            "sync_membership_revision_invalid"
        );
        ensure!(
            !self.members.is_empty() && self.members.len() <= MAX_MEMBERS,
            "sync_membership_limit"
        );
        let mut streams = std::collections::HashSet::new();
        let mut keys = std::collections::HashSet::new();
        let mut previous: Option<&str> = None;
        for member in &self.members {
            member.device.validate()?;
            ensure!(
                previous.is_none_or(|id| id < member.device.device_id.as_str())
                    && streams.insert(&member.device.stream_id)
                    && keys.insert(member.device.signing_key),
                "sync_membership_identity_conflict"
            );
            previous = Some(&member.device.device_id);
        }
        ensure!(
            self.members
                .iter()
                .any(|m| m.active && m.device == self.owner),
            "sync_membership_owner_missing"
        );
        ensure!(
            self.members
                .iter()
                .any(|m| m.active && m.device.device_id == self.processor_device_id),
            "sync_membership_processor_missing"
        );
        if let Some(handoff) = &self.handoff {
            handoff.validate()?;
            ensure!(
                handoff.dataset_id == self.dataset_id,
                "sync_handoff_dataset_mismatch"
            );
            ensure!(
                handoff.to_device_id == self.processor_device_id,
                "sync_handoff_target_mismatch"
            );
            ensure!(
                handoff.roster_revision == self.revision,
                "sync_handoff_revision_mismatch"
            );
            ensure!(
                self.members
                    .iter()
                    .any(|m| m.active && m.device.device_id == handoff.from_device_id),
                "sync_handoff_source_missing"
            );
            ensure!(
                self.members
                    .iter()
                    .any(|m| m.active && m.device.device_id == handoff.to_device_id),
                "sync_handoff_target_missing"
            );
        }
        Ok(())
    }

    pub(crate) fn digest(&self) -> Result<String> {
        self.validate()?;
        Ok(crate::storage::sync::hash(&serde_json::to_vec(self)?))
    }

    pub(crate) fn anchor(&self) -> Result<RosterAnchor> {
        Ok(RosterAnchor {
            dataset_id: self.dataset_id.clone(),
            owner: self.owner.clone(),
            revision: self.revision,
            digest: self.digest()?,
        })
    }

    pub(crate) fn validate_successor(&self, previous: &Self) -> Result<()> {
        self.validate()?;
        // 名字/平台不参与根身份（与 decode 一致）：改名产生的新版本允许通过。
        ensure!(
            self.dataset_id == previous.dataset_id && same_identity(&self.owner, &previous.owner),
            "sync_membership_root_changed"
        );
        ensure!(
            self.revision > previous.revision,
            "sync_membership_revision_stale"
        );
        if self.processor_device_id == previous.processor_device_id {
            ensure!(
                self.handoff.is_none(),
                "sync_handoff_unexpected_without_processor_change"
            );
        } else {
            let handoff = self
                .handoff
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("sync_processing_handoff_required"))?;
            ensure!(
                handoff.from_device_id == previous.processor_device_id,
                "sync_processing_handoff_required"
            );
            let old_processor = previous
                .members
                .iter()
                .find(|m| m.device.device_id == previous.processor_device_id && m.active)
                .ok_or_else(|| anyhow::anyhow!("sync_handoff_old_processor_inactive"))?;
            handoff.verify(&old_processor.device.signing_key)?;
        }
        for old in &previous.members {
            let new = self
                .members
                .iter()
                .find(|m| m.device.device_id == old.device.device_id)
                .ok_or_else(|| anyhow::anyhow!("sync_membership_history_removed"))?;
            ensure!(
                same_identity(&old.device, &new.device),
                "sync_membership_identity_changed"
            );
            ensure!(
                old.active || !new.active,
                "sync_membership_revoked_identity_reused"
            );
        }
        Ok(())
    }
}

pub(crate) fn same_identity(a: &PairingDevice, b: &PairingDevice) -> bool {
    a.device_id == b.device_id && a.stream_id == b.stream_id && a.signing_key == b.signing_key
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SignedRoster {
    pub roster: MembershipRoster,
    signature: Vec<u8>,
}
impl SignedRoster {
    pub(crate) fn sign(roster: MembershipRoster, owner: &DeviceIdentity) -> Result<Self> {
        roster.validate()?;
        ensure!(
            owner.public_key() == roster.owner.signing_key,
            "sync_membership_owner_key_mismatch"
        );
        let signature = owner.sign_membership(&serde_json::to_vec(&roster)?)?;
        let signed = Self { roster, signature };
        signed.encode()?;
        Ok(signed)
    }
    pub(crate) fn encode(&self) -> Result<Vec<u8>> {
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_ROSTER_BYTES,
            "sync_membership_payload_too_large"
        );
        Ok(bytes)
    }
    pub(crate) fn decode(bytes: &[u8], anchor: &RosterAnchor) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_ROSTER_BYTES,
            "sync_membership_payload_too_large"
        );
        anchor.validate()?;
        let signed: Self = serde_json::from_slice(bytes)
            .map_err(|_| anyhow::anyhow!("sync_membership_record_invalid"))?;
        signed.roster.validate()?;
        // 名字/平台不参与根身份：device_id/stream_id/signing_key 才是同一性
        // （same_identity）。改名只换显示名，不动根。
        ensure!(
            signed.roster.dataset_id == anchor.dataset_id
                && same_identity(&signed.roster.owner, &anchor.owner),
            "sync_membership_root_changed"
        );
        verify_membership_signature(
            &anchor.owner.signing_key,
            &serde_json::to_vec(&signed.roster)?,
            &signed.signature,
        )?;
        ensure!(
            signed.roster.revision >= anchor.revision,
            "sync_membership_before_pairing_anchor"
        );
        if signed.roster.revision == anchor.revision {
            ensure!(
                signed.roster.digest()? == anchor.digest,
                "sync_membership_anchor_mismatch"
            );
        }
        Ok(signed)
    }
}
