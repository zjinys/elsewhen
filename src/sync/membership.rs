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
pub(crate) struct MembershipRoster {
    pub version: u32,
    pub dataset_id: String,
    pub owner: PairingDevice,
    pub revision: u64,
    pub processor_device_id: String,
    pub members: Vec<RosterMember>,
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
        ensure!(
            self.dataset_id == previous.dataset_id && self.owner == previous.owner,
            "sync_membership_root_changed"
        );
        ensure!(
            self.revision > previous.revision,
            "sync_membership_revision_stale"
        );
        ensure!(
            self.processor_device_id == previous.processor_device_id,
            "sync_processing_handoff_required"
        );
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
        ensure!(
            signed.roster.dataset_id == anchor.dataset_id && signed.roster.owner == anchor.owner,
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
