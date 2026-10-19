//! Explicit-approval pairing over a short-lived out-of-band invitation.
//! Snow implements Noise_NNpsk0; Ed25519 additionally binds both advertised
//! long-term identities. No Provider secret is sent during the handshake.
//! Live Noise states are intentionally not Clone/serializable: App restart expires
//! an unapproved invitation. Post-approval recovery is encrypted in the local vault.

pub(crate) mod journal;
mod route;
pub(crate) use route::{InvitationRoute, PairedRoute, PairingDelivery};
use route::{make_paired_route, make_pairing_delivery, validate_join_route};

use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

use super::crypto::{
    self, ContentKey, DeviceIdentity, EncryptedEnvelope, EnvelopeHeader, verify_pairing_identity,
};
use super::protocol::validate_id;

const NOISE: &str = "Noise_NNpsk0_25519_ChaChaPoly_SHA256";
const INVITATION_PREFIX: &str = "elsewhen-pair-v1:";
/// 邀请有效期。曾经只有 60 秒，导致真人双设备配对两次都超时失败
/// （输码+确认+审批远不止 60s）。邀请码本身不授权——加入必须另行
/// 经过 owner 批准关守，且一次性消费，泄露窗口宽一点也不致命。
const TTL_MS: i64 = 24 * 60 * 60 * 1000;
const FRAME_LIMIT: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairingDevice {
    pub device_id: String,
    pub stream_id: String,
    pub name: String,
    pub platform: String,
    pub signing_key: [u8; 32],
}

impl PairingDevice {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_id(&self.device_id)?;
        validate_id(&self.stream_id)?;
        ensure!(
            !self.name.trim().is_empty()
                && self.name.len() <= 256
                && !self.name.chars().any(char::is_control),
            "pairing_device_name_invalid"
        );
        ensure!(
            !self.platform.is_empty()
                && self.platform.len() <= 32
                && self
                    .platform
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "pairing_platform_invalid"
        );
        Ok(())
    }

    pub(crate) fn fingerprint(&self) -> String {
        ring::digest::digest(&ring::digest::SHA256, &self.signing_key)
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

mod claims;
mod handshake;
mod invitation;

pub(crate) use claims::{ApprovedClaims, JoinClaims, Signed};
pub(crate) use claims::{
    ApprovedPair, PendingApproval, PendingCompletion, PendingInvitation, PendingJoin,
};
#[cfg(test)]
pub(crate) use invitation::InvitationContext;
pub(crate) use invitation::PairingInvitation;
#[cfg(test)]
mod route_tests;

#[cfg(test)]
mod tests {
    use super::*;
    fn device(id: &str, identity: &DeviceIdentity) -> PairingDevice {
        PairingDevice {
            device_id: id.into(),
            stream_id: format!("stream-{id}"),
            name: format!("Device {id}"),
            platform: "test".into(),
            signing_key: identity.public_key(),
        }
    }
    #[test]
    fn pairing_cannot_roll_back_a_previously_installed_key_epoch() {
        let a = DeviceIdentity::generate().unwrap();
        let b = DeviceIdentity::generate().unwrap();
        let now = 1_800_000_000_000;
        let (invitation, server) =
            PairingInvitation::issue("dataset", device("a", &a), &a, now).unwrap();
        let (join, request) = PendingJoin::begin(invitation, device("b", &b), &b, now).unwrap();
        let (_, reply) = server
            .receive(&request, now)
            .unwrap()
            .approve(&a, 2, now)
            .unwrap();
        assert!(join.finish(&reply, 2, now).is_err());
        let (invitation, server) =
            PairingInvitation::issue("dataset", device("a", &a), &a, now).unwrap();
        let (join, request) = PendingJoin::begin(invitation, device("b", &b), &b, now).unwrap();
        let (pending, reply) = server
            .receive(&request, now)
            .unwrap()
            .approve(&a, 3, now)
            .unwrap();
        let installed = join.finish(&reply, 2, now).unwrap();
        assert_eq!(
            pending
                .confirm(&installed.completion_proof(&b).unwrap(), now)
                .unwrap()
                .key_version,
            3
        );
    }

    #[test]
    fn issuer_requires_a_fresh_authenticated_installation_confirmation() {
        let a = DeviceIdentity::generate().unwrap();
        let b = DeviceIdentity::generate().unwrap();
        let now = 1_800_000_000_000;
        for expired in [false, true] {
            let (invitation, server) =
                PairingInvitation::issue("dataset", device("a", &a), &a, now).unwrap();
            let (join, request) = PendingJoin::begin(invitation, device("b", &b), &b, now).unwrap();
            let (pending, response) = server
                .receive(&request, now)
                .unwrap()
                .approve(&a, 1, now)
                .unwrap();
            let approved = join.finish(&response, 0, now).unwrap();
            let proof = approved.completion_proof(&b).unwrap();
            if expired {
                assert!(pending.confirm(&proof, now + TTL_MS).is_err());
            } else {
                assert!(pending.confirm(b"not a confirmation", now).is_err());
            }
        }
    }

    #[test]
    fn explicit_approval_binds_both_identities_and_directional_keys() {
        let a = DeviceIdentity::generate().unwrap();
        let b = DeviceIdentity::generate().unwrap();
        let now = 1_800_000_000_000;
        let (invitation, server) =
            PairingInvitation::issue("dataset", device("a", &a), &a, now).unwrap();
        let encoded = invitation.encode().unwrap();
        let invitation = PairingInvitation::decode(&encoded, now + 1).unwrap();
        assert_eq!(format!("{invitation:?}"), "PairingInvitation([REDACTED])");
        let (join, request) = PendingJoin::begin(invitation, device("b", &b), &b, now + 2).unwrap();
        let pending = server.receive(&request, now + 3).unwrap();
        assert_eq!(pending.preview().device_id, "b");
        assert_eq!(
            pending.preview().fingerprint(),
            device("b", &b).fingerprint()
        );
        let (pending_completion, response) = pending.approve(&a, 1, now + 4).unwrap();
        let approved_b = join.finish(&response, 0, now + 5).unwrap();
        let proof = approved_b.completion_proof(&b).unwrap();
        let approved_a = pending_completion.confirm(&proof, now + 6).unwrap();
        assert_eq!(approved_a.dataset_id, "dataset");
        assert_eq!(approved_b.peer.signing_key, a.public_key());
        assert_eq!(approved_a.peer.signing_key, b.public_key());
        assert_eq!(approved_a.key_version, 1);
        assert_eq!(
            approved_a.send_key.to_bytes(),
            approved_b.receive_key.to_bytes()
        );
        assert_eq!(
            approved_a.receive_key.to_bytes(),
            approved_b.send_key.to_bytes()
        );
        assert_ne!(
            approved_a.send_key.to_bytes(),
            approved_a.receive_key.to_bytes()
        );
        assert!(
            !response
                .windows(32)
                .any(|w| w == approved_a.send_key.to_bytes())
        );
    }
    #[test]
    fn wrong_psk_or_tampered_request_cannot_reach_approval() {
        let a = DeviceIdentity::generate().unwrap();
        let b = DeviceIdentity::generate().unwrap();
        let now = 1_800_000_000_000;
        let (mut invitation, server) =
            PairingInvitation::issue("dataset", device("a", &a), &a, now).unwrap();
        invitation.secret[0] ^= 1;
        let (_, request) = PendingJoin::begin(invitation, device("b", &b), &b, now).unwrap();
        assert!(server.receive(&request, now).is_err());
        let (invitation, server) =
            PairingInvitation::issue("dataset", device("a", &a), &a, now).unwrap();
        let (_, mut request) = PendingJoin::begin(invitation, device("b", &b), &b, now).unwrap();
        request[20] ^= 1;
        assert!(server.receive(&request, now).is_err());
    }
    #[test]
    fn expired_invitation_and_delayed_approval_fail_closed() {
        let a = DeviceIdentity::generate().unwrap();
        let b = DeviceIdentity::generate().unwrap();
        let now = 1_800_000_000_000;
        let (invitation, server) =
            PairingInvitation::issue("dataset", device("a", &a), &a, now).unwrap();
        assert!(PairingInvitation::decode(&invitation.encode().unwrap(), now + TTL_MS).is_err());
        let (_, request) = PendingJoin::begin(invitation, device("b", &b), &b, now).unwrap();
        let pending = server.receive(&request, now).unwrap();
        assert!(pending.approve(&a, 1, now + TTL_MS).is_err());
    }
    #[test]
    fn approval_cannot_substitute_the_issuer_key_or_joiner_identity() {
        let a = DeviceIdentity::generate().unwrap();
        let b = DeviceIdentity::generate().unwrap();
        let stranger = DeviceIdentity::generate().unwrap();
        let now = 1_800_000_000_000;
        let (invitation, server) =
            PairingInvitation::issue("dataset", device("a", &a), &a, now).unwrap();
        assert!(PendingJoin::begin(invitation.clone(), device("b", &stranger), &b, now).is_err());
        let (_, request) = PendingJoin::begin(invitation, device("b", &b), &b, now).unwrap();
        assert!(
            server
                .receive(&request, now)
                .unwrap()
                .approve(&stranger, 1, now)
                .is_err()
        );
    }
    #[test]
    fn tampered_reply_and_wrong_invitation_context_are_rejected() {
        let a = DeviceIdentity::generate().unwrap();
        let b = DeviceIdentity::generate().unwrap();
        let now = 1_800_000_000_000;
        let (invitation, server) =
            PairingInvitation::issue("dataset", device("a", &a), &a, now).unwrap();
        let (join, request) = PendingJoin::begin(invitation, device("b", &b), &b, now).unwrap();
        let (_, mut reply) = server
            .receive(&request, now)
            .unwrap()
            .approve(&a, 1, now)
            .unwrap();
        reply[20] ^= 1;
        assert!(join.finish(&reply, 0, now).is_err());
        let (mut invitation, server) =
            PairingInvitation::issue("dataset", device("a", &a), &a, now).unwrap();
        invitation.context.dataset_id = "other-dataset".into();
        let (_, request) = PendingJoin::begin(invitation, device("b", &b), &b, now).unwrap();
        assert!(server.receive(&request, now).is_err());
    }
    #[test]
    fn rejecting_preview_produces_no_keys_and_oversized_inputs_are_bounded() {
        let a = DeviceIdentity::generate().unwrap();
        let b = DeviceIdentity::generate().unwrap();
        let now = 1_800_000_000_000;
        let (invitation, server) =
            PairingInvitation::issue("dataset", device("a", &a), &a, now).unwrap();
        let (_, request) = PendingJoin::begin(invitation, device("b", &b), &b, now).unwrap();
        server.receive(&request, now).unwrap().reject();
        assert!(PairingInvitation::decode(&"x".repeat(5000), now).is_err());
        let (_, server) = PairingInvitation::issue("dataset", device("a", &a), &a, now).unwrap();
        assert!(server.receive(&vec![0; FRAME_LIMIT + 1], now).is_err());
    }
}
