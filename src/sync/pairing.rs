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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InvitationContext {
    version: u32,
    invitation_id: String,
    dataset_id: String,
    issuer: PairingDevice,
    expires_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    route: Option<InvitationRoute>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairingInvitation {
    context: InvitationContext,
    secret: [u8; 32],
}

impl std::fmt::Debug for PairingInvitation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PairingInvitation([REDACTED])")
    }
}

impl PairingInvitation {
    pub(crate) fn issue(
        dataset_id: &str,
        issuer: PairingDevice,
        identity: &DeviceIdentity,
        now_ms: i64,
    ) -> Result<(Self, PendingInvitation)> {
        Self::issue_inner(dataset_id, issuer, identity, now_ms, None)
    }

    pub(crate) fn issue_for_relay(
        dataset_id: &str,
        issuer: PairingDevice,
        identity: &DeviceIdentity,
        now_ms: i64,
        route: InvitationRoute,
    ) -> Result<(Self, PendingInvitation)> {
        route.validate()?;
        Self::issue_inner(dataset_id, issuer, identity, now_ms, Some(route))
    }

    fn issue_inner(
        dataset_id: &str,
        issuer: PairingDevice,
        identity: &DeviceIdentity,
        now_ms: i64,
        route: Option<InvitationRoute>,
    ) -> Result<(Self, PendingInvitation)> {
        validate_id(dataset_id)?;
        issuer.validate()?;
        ensure!(
            issuer.signing_key == identity.public_key(),
            "pairing_issuer_key_mismatch"
        );
        let mut secret = [0; 32];
        SystemRandom::new()
            .fill(&mut secret)
            .map_err(|_| anyhow::anyhow!("pairing_random_failed"))?;
        let invitation = Self {
            context: InvitationContext {
                version: 1,
                invitation_id: uuid::Uuid::new_v4().to_string(),
                dataset_id: dataset_id.to_string(),
                issuer,
                route,
                expires_at_ms: now_ms
                    .checked_add(TTL_MS)
                    .ok_or_else(|| anyhow::anyhow!("pairing_time_invalid"))?,
            },
            secret,
        };
        let noise = invitation.handshake(false, now_ms)?;
        Ok((invitation.clone(), PendingInvitation { invitation, noise }))
    }

    pub(crate) fn dataset_id(&self) -> &str {
        &self.context.dataset_id
    }
    pub(crate) fn issuer(&self) -> &PairingDevice {
        &self.context.issuer
    }
    pub(crate) fn invitation_id(&self) -> &str {
        &self.context.invitation_id
    }
    pub(crate) fn expires_at_ms(&self) -> i64 {
        self.context.expires_at_ms
    }
    pub(crate) fn route(&self) -> Option<&InvitationRoute> {
        self.context.route.as_ref()
    }
    pub(crate) fn relay_credential(&self) -> Result<[u8; 32]> {
        ensure!(self.context.route.is_some(), "pairing_relay_route_missing");
        let mut bytes = b"elsewhen.pairing.relay.v1\0".to_vec();
        bytes.extend(serde_json::to_vec(&self.context)?);
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &self.secret);
        Ok(ring::hmac::sign(&key, &bytes)
            .as_ref()
            .try_into()
            .expect("HMAC-SHA256 length"))
    }

    /// Secret-bearing output intended only for an explicit QR/copy invitation UI.
    pub(crate) fn encode(&self) -> Result<String> {
        let bytes = serde_json::to_vec(self)?;
        ensure!(bytes.len() <= 3072, "pairing_invitation_too_large");
        Ok(format!(
            "{INVITATION_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(bytes)
        ))
    }

    pub(crate) fn decode(encoded: &str, now_ms: i64) -> Result<Self> {
        ensure!(
            encoded.len() <= 4096 + INVITATION_PREFIX.len(),
            "pairing_invitation_too_large"
        );
        let encoded = encoded
            .strip_prefix(INVITATION_PREFIX)
            .ok_or_else(|| anyhow::anyhow!("pairing_invitation_invalid"))?;
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| anyhow::anyhow!("pairing_invitation_invalid"))?;
        let invitation: Self = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("pairing_invitation_invalid"))?;
        invitation.validate(now_ms)?;
        Ok(invitation)
    }

    fn validate(&self, now_ms: i64) -> Result<()> {
        ensure!(self.context.version == 1, "pairing_version_unsupported");
        validate_id(&self.context.dataset_id)?;
        validate_id(&self.context.invitation_id)?;
        self.context.issuer.validate()?;
        if let Some(route) = &self.context.route {
            route.validate()?;
        }
        ensure!(
            now_ms >= 0
                && self.context.expires_at_ms > now_ms
                && self.context.expires_at_ms.saturating_sub(now_ms) <= TTL_MS,
            "pairing_invitation_expired_or_clock_invalid"
        );
        Ok(())
    }

    fn handshake(&self, initiator: bool, now_ms: i64) -> Result<snow::HandshakeState> {
        self.validate(now_ms)?;
        let prologue = serde_json::to_vec(&self.context)?;
        let builder = snow::Builder::new(NOISE.parse()?)
            .psk(0, &self.secret)?
            .prologue(&prologue)?;
        Ok(if initiator {
            builder.build_initiator()?
        } else {
            builder.build_responder()?
        })
    }
}

pub(crate) struct PendingInvitation {
    invitation: PairingInvitation,
    noise: snow::HandshakeState,
}
pub(crate) struct PendingJoin {
    invitation: PairingInvitation,
    joining: PairingDevice,
    mailbox: Option<String>,
    noise: snow::HandshakeState,
}
pub(crate) struct PendingApproval {
    invitation: PairingInvitation,
    joining: PairingDevice,
    mailbox: Option<String>,
    noise: snow::HandshakeState,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JoinClaims {
    invitation_id: String,
    dataset_id: String,
    issuer_device_id: String,
    joining: PairingDevice,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mailbox: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApprovedClaims {
    invitation_id: String,
    dataset_id: String,
    issuer: PairingDevice,
    joining: PairingDevice,
    key_version: u64,
    issuer_to_joiner: [u8; 32],
    joiner_to_issuer: [u8; 32],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    authority: Option<super::membership::RosterAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mailbox: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Signed<T> {
    claims: T,
    signature: Vec<u8>,
}

pub(crate) struct ApprovedPair {
    pub dataset_id: String,
    pub peer: PairingDevice,
    local_device: PairingDevice,
    invitation_id: String,
    expires_at_ms: i64,
    pub key_version: u64,
    pub send_key: ContentKey,
    pub receive_key: ContentKey,
    pub authority: Option<super::membership::RosterAnchor>,
    pub route: Option<PairedRoute>,
    delivery: Option<PairingDelivery>,
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
    pair: ApprovedPair,
    expires_at_ms: i64,
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

impl PendingJoin {
    pub(crate) fn begin(
        invitation: PairingInvitation,
        joining: PairingDevice,
        identity: &DeviceIdentity,
        now_ms: i64,
    ) -> Result<(Self, Vec<u8>)> {
        Self::begin_inner(invitation, joining, identity, now_ms, None)
    }

    pub(crate) fn begin_for_relay(
        invitation: PairingInvitation,
        joining: PairingDevice,
        identity: &DeviceIdentity,
        now_ms: i64,
        mailbox: String,
    ) -> Result<(Self, Vec<u8>)> {
        Self::begin_inner(invitation, joining, identity, now_ms, Some(mailbox))
    }

    fn begin_inner(
        invitation: PairingInvitation,
        joining: PairingDevice,
        identity: &DeviceIdentity,
        now_ms: i64,
        mailbox: Option<String>,
    ) -> Result<(Self, Vec<u8>)> {
        joining.validate()?;
        invitation.validate(now_ms)?;
        validate_join_route(&invitation, mailbox.as_deref())?;
        ensure!(
            joining.signing_key == identity.public_key(),
            "pairing_joiner_key_mismatch"
        );
        ensure!(
            joining.device_id != invitation.context.issuer.device_id
                && joining.stream_id != invitation.context.issuer.stream_id,
            "pairing_identity_collision"
        );
        let claims = JoinClaims {
            invitation_id: invitation.context.invitation_id.clone(),
            dataset_id: invitation.context.dataset_id.clone(),
            issuer_device_id: invitation.context.issuer.device_id.clone(),
            joining: joining.clone(),
            mailbox: mailbox.clone(),
        };
        let signature = identity.sign_pairing("join", &serde_json::to_vec(&claims)?)?;
        let mut noise = invitation.handshake(true, now_ms)?;
        let request = write_frame(
            &mut noise,
            &serde_json::to_vec(&Signed { claims, signature })?,
        )?;
        Ok((
            Self {
                invitation,
                joining,
                mailbox,
                noise,
            },
            request,
        ))
    }

    pub(crate) fn finish(
        mut self,
        response: &[u8],
        minimum_key_version: u64,
        now_ms: i64,
    ) -> Result<ApprovedPair> {
        self.invitation.validate(now_ms)?;
        let payload = read_frame(&mut self.noise, response)?;
        ensure!(
            self.noise.is_handshake_finished(),
            "pairing_handshake_incomplete"
        );
        let signed: Signed<ApprovedClaims> = serde_json::from_slice(&payload)
            .map_err(|_| anyhow::anyhow!("pairing_approval_invalid"))?;
        verify_pairing_identity(
            &self.invitation.context.issuer.signing_key,
            "approve",
            &serde_json::to_vec(&signed.claims)?,
            &signed.signature,
        )?;
        let claims = signed.claims;
        validate_join_route(&self.invitation, claims.mailbox.as_deref())?;
        ensure!(
            claims.invitation_id == self.invitation.context.invitation_id
                && claims.dataset_id == self.invitation.context.dataset_id
                && claims.issuer == self.invitation.context.issuer
                && claims.joining == self.joining
                && claims.mailbox == self.mailbox
                && claims.key_version > minimum_key_version
                && claims.key_version <= i64::MAX as u64,
            "pairing_approval_context_mismatch"
        );
        ensure!(
            claims.issuer_to_joiner != claims.joiner_to_issuer,
            "pairing_direction_keys_must_differ"
        );
        if let Some(anchor) = &claims.authority {
            anchor.validate()?;
            ensure!(
                anchor.dataset_id == claims.dataset_id && anchor.owner == claims.issuer,
                "pairing_authority_context_mismatch"
            );
        }
        let paired_route = make_paired_route(&self.invitation, self.mailbox.as_deref(), false)?;
        let delivery = make_pairing_delivery(&self.invitation, self.mailbox.as_deref(), false)?;
        Ok(ApprovedPair {
            dataset_id: claims.dataset_id,
            peer: claims.issuer,
            local_device: claims.joining,
            invitation_id: claims.invitation_id,
            expires_at_ms: self.invitation.context.expires_at_ms,
            key_version: claims.key_version,
            send_key: ContentKey::from_bytes(claims.joiner_to_issuer),
            receive_key: ContentKey::from_bytes(claims.issuer_to_joiner),
            authority: claims.authority,
            route: paired_route,
            delivery,
            bootstrap: self.invitation.route().cloned(),
        })
    }
}

impl PendingInvitation {
    /// Consumes the single-use server state. Receiving a valid request produces
    /// only a preview, not a member grant or content keys.
    pub(crate) fn receive(mut self, request: &[u8], now_ms: i64) -> Result<PendingApproval> {
        self.invitation.validate(now_ms)?;
        let payload = read_frame(&mut self.noise, request)?;
        let signed: Signed<JoinClaims> = serde_json::from_slice(&payload)
            .map_err(|_| anyhow::anyhow!("pairing_request_invalid"))?;
        signed.claims.joining.validate()?;
        verify_pairing_identity(
            &signed.claims.joining.signing_key,
            "join",
            &serde_json::to_vec(&signed.claims)?,
            &signed.signature,
        )?;
        let claims = signed.claims;
        validate_join_route(&self.invitation, claims.mailbox.as_deref())?;
        ensure!(
            claims.invitation_id == self.invitation.context.invitation_id
                && claims.dataset_id == self.invitation.context.dataset_id
                && claims.issuer_device_id == self.invitation.context.issuer.device_id,
            "pairing_request_context_mismatch"
        );
        ensure!(
            claims.joining.device_id != self.invitation.context.issuer.device_id
                && claims.joining.stream_id != self.invitation.context.issuer.stream_id,
            "pairing_identity_collision"
        );
        Ok(PendingApproval {
            invitation: self.invitation,
            joining: claims.joining,
            mailbox: claims.mailbox,
            noise: self.noise,
        })
    }
}

impl PendingApproval {
    pub(crate) fn preview(&self) -> &PairingDevice {
        &self.joining
    }
    pub(crate) fn reject(self) {}

    /// Called only by an explicit approval action for this displayed request.
    /// The coordinator reserves key_version durably above the peer's previous
    /// epoch before approving; it must not reset to one on reconnection.
    /// Pair installation and relay grant publication use a recoverable coordinator;
    /// the issuer activates membership only after PendingCompletion::confirm.
    /// This primitive itself performs no database writes.
    pub(crate) fn approve(
        self,
        identity: &DeviceIdentity,
        key_version: u64,
        now_ms: i64,
    ) -> Result<(PendingCompletion, Vec<u8>)> {
        self.approve_with_authority(identity, key_version, now_ms, None)
    }

    pub(crate) fn approve_for_dataset(
        self,
        identity: &DeviceIdentity,
        key_version: u64,
        now_ms: i64,
        anchor: super::membership::RosterAnchor,
    ) -> Result<(PendingCompletion, Vec<u8>)> {
        self.approve_with_authority(identity, key_version, now_ms, Some(anchor))
    }

    fn approve_with_authority(
        mut self,
        identity: &DeviceIdentity,
        key_version: u64,
        now_ms: i64,
        authority: Option<super::membership::RosterAnchor>,
    ) -> Result<(PendingCompletion, Vec<u8>)> {
        self.invitation.validate(now_ms)?;
        ensure!(
            self.invitation.route().is_none() || authority.is_some(),
            "pairing_relay_authority_required"
        );
        let paired_route = make_paired_route(&self.invitation, self.mailbox.as_deref(), true)?;
        let delivery = make_pairing_delivery(&self.invitation, self.mailbox.as_deref(), true)?;
        if let Some(anchor) = &authority {
            anchor.validate()?;
            ensure!(
                anchor.dataset_id == self.invitation.context.dataset_id
                    && anchor.owner == self.invitation.context.issuer,
                "pairing_authority_context_mismatch"
            );
        }
        ensure!(
            (1..=i64::MAX as u64).contains(&key_version),
            "pairing_key_version_invalid"
        );
        ensure!(
            identity.public_key() == self.invitation.context.issuer.signing_key,
            "pairing_issuer_key_mismatch"
        );
        let outgoing = ContentKey::generate()?;
        let incoming = ContentKey::generate()?;
        let claims = ApprovedClaims {
            invitation_id: self.invitation.context.invitation_id.clone(),
            dataset_id: self.invitation.context.dataset_id.clone(),
            issuer: self.invitation.context.issuer.clone(),
            joining: self.joining.clone(),
            key_version,
            issuer_to_joiner: outgoing.to_bytes(),
            joiner_to_issuer: incoming.to_bytes(),
            authority: authority.clone(),
            mailbox: self.mailbox.clone(),
        };
        let signature = identity.sign_pairing("approve", &serde_json::to_vec(&claims)?)?;
        let response = write_frame(
            &mut self.noise,
            &serde_json::to_vec(&Signed { claims, signature })?,
        )?;
        ensure!(
            self.noise.is_handshake_finished(),
            "pairing_handshake_incomplete"
        );
        Ok((
            PendingCompletion {
                pair: ApprovedPair {
                    dataset_id: self.invitation.context.dataset_id,
                    peer: self.joining,
                    local_device: self.invitation.context.issuer,
                    invitation_id: self.invitation.context.invitation_id,
                    expires_at_ms: self.invitation.context.expires_at_ms,
                    key_version,
                    send_key: outgoing,
                    receive_key: incoming,
                    authority,
                    route: paired_route,
                    delivery,
                    bootstrap: self.invitation.context.route,
                },
                expires_at_ms: self.invitation.context.expires_at_ms,
            },
            response,
        ))
    }
}

fn read_frame(noise: &mut snow::HandshakeState, frame: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        !frame.is_empty() && frame.len() <= FRAME_LIMIT,
        "pairing_frame_size_invalid"
    );
    let mut payload = vec![0; FRAME_LIMIT];
    let size = noise
        .read_message(frame, &mut payload)
        .map_err(|_| anyhow::anyhow!("pairing_handshake_authentication_failed"))?;
    payload.truncate(size);
    Ok(payload)
}
fn write_frame(noise: &mut snow::HandshakeState, payload: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        payload.len() <= FRAME_LIMIT - 256,
        "pairing_payload_too_large"
    );
    let mut frame = vec![0; FRAME_LIMIT];
    let size = noise
        .write_message(payload, &mut frame)
        .map_err(|_| anyhow::anyhow!("pairing_handshake_write_failed"))?;
    frame.truncate(size);
    Ok(frame)
}

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
