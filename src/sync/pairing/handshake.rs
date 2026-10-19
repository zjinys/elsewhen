//! Noise 握手与三角色状态机：PendingJoin/PendingInvitation/PendingApproval 推进及帧读写。

use super::*;

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

    pub(crate) fn begin_inner(
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
        anchor: crate::sync::membership::RosterAnchor,
    ) -> Result<(PendingCompletion, Vec<u8>)> {
        self.approve_with_authority(identity, key_version, now_ms, Some(anchor))
    }

    pub(crate) fn approve_with_authority(
        mut self,
        identity: &DeviceIdentity,
        key_version: u64,
        now_ms: i64,
        authority: Option<crate::sync::membership::RosterAnchor>,
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
