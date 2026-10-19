//! 协议凭据结构：Pending*、Join/ApprovedClaims、Signed、ApprovedPair 与完成体。

use super::*;

pub(crate) struct PendingInvitation {
    pub(crate) invitation: PairingInvitation,
    pub(crate) noise: snow::HandshakeState,
}
pub(crate) struct PendingJoin {
    pub(crate) invitation: PairingInvitation,
    pub(crate) joining: PairingDevice,
    pub(crate) mailbox: Option<String>,
    pub(crate) noise: snow::HandshakeState,
}
pub(crate) struct PendingApproval {
    pub(crate) invitation: PairingInvitation,
    pub(crate) joining: PairingDevice,
    pub(crate) mailbox: Option<String>,
    pub(crate) noise: snow::HandshakeState,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JoinClaims {
    pub(crate) invitation_id: String,
    pub(crate) dataset_id: String,
    pub(crate) issuer_device_id: String,
    pub(crate) joining: PairingDevice,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mailbox: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApprovedClaims {
    pub(crate) invitation_id: String,
    pub(crate) dataset_id: String,
    pub(crate) issuer: PairingDevice,
    pub(crate) joining: PairingDevice,
    pub(crate) key_version: u64,
    pub(crate) issuer_to_joiner: [u8; 32],
    pub(crate) joiner_to_issuer: [u8; 32],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) authority: Option<crate::sync::membership::RosterAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mailbox: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Signed<T> {
    pub(crate) claims: T,
    pub(crate) signature: Vec<u8>,
}

pub(crate) struct ApprovedPair {
    pub dataset_id: String,
    pub peer: PairingDevice,
    pub(crate) local_device: PairingDevice,
    pub(crate) invitation_id: String,
    pub(crate) expires_at_ms: i64,
    pub key_version: u64,
    pub send_key: ContentKey,
    pub receive_key: ContentKey,
    pub authority: Option<crate::sync::membership::RosterAnchor>,
    pub route: Option<PairedRoute>,
    pub(crate) delivery: Option<PairingDelivery>,
    pub(crate) bootstrap: Option<InvitationRoute>,
}
impl std::fmt::Debug for ApprovedPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApprovedPair")
            .field("peer", &self.peer.device_id)
            .finish_non_exhaustive()
    }
}

pub(crate) struct PendingCompletion {
    pub(crate) pair: ApprovedPair,
    pub(crate) expires_at_ms: i64,
}

impl ApprovedPair {
    pub(crate) fn invitation_id(&self) -> &str {
        &self.invitation_id
    }

    pub(crate) fn local_device(&self) -> &PairingDevice {
        &self.local_device
    }

    /// The joiner sends this only after its keys/identity have been installed
    /// durably. Until it arrives the issuer must not activate the new member.
    pub(crate) fn completion_proof(&self, identity: &DeviceIdentity) -> Result<Vec<u8>> {
        ensure!(
            identity.public_key() == self.local_device.signing_key,
            "pairing_joiner_key_mismatch"
        );
        let header = EnvelopeHeader::new(
            &self.dataset_id,
            &self.local_device.device_id,
            &self.peer.device_id,
            self.key_version,
            format!("pair-complete:{}", self.invitation_id),
        )?;
        crypto::seal(header, &self.send_key, identity, b"pairing-installed-v1")?.to_bytes()
    }
}

impl PendingCompletion {
    pub(crate) fn confirm(self, bytes: &[u8], now_ms: i64) -> Result<ApprovedPair> {
        ensure!(
            now_ms >= 0 && now_ms < self.expires_at_ms,
            "pairing_confirmation_expired"
        );
        ensure!(bytes.len() <= FRAME_LIMIT, "pairing_frame_size_invalid");
        let expected = EnvelopeHeader::new(
            &self.pair.dataset_id,
            &self.pair.peer.device_id,
            &self.pair.local_device.device_id,
            self.pair.key_version,
            format!("pair-complete:{}", self.pair.invitation_id),
        )?;
        let envelope = EncryptedEnvelope::from_bytes(bytes)?;
        ensure!(
            crypto::open(
                &envelope,
                &expected,
                &self.pair.receive_key,
                &self.pair.peer.signing_key
            )? == b"pairing-installed-v1",
            "pairing_confirmation_invalid"
        );
        Ok(self.pair)
    }
}
