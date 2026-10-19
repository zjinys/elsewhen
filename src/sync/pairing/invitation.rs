//! 邀请信封：PairingInvitation 的构造/编码/展示与共享上下文。

use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InvitationContext {
    pub(crate) version: u32,
    pub(crate) invitation_id: String,
    pub(crate) dataset_id: String,
    pub(crate) issuer: PairingDevice,
    pub(crate) expires_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) route: Option<InvitationRoute>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairingInvitation {
    pub(crate) context: InvitationContext,
    pub(crate) secret: [u8; 32],
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

    pub(crate) fn issue_inner(
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

    pub(crate) fn validate(&self, now_ms: i64) -> Result<()> {
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

    pub(crate) fn handshake(&self, initiator: bool, now_ms: i64) -> Result<snow::HandshakeState> {
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
