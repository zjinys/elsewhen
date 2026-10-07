//! Installation-local secrets. The wrapping key comes from the native keystore,
//! never SQLite, the relay, a Provider configuration, or a synchronized snapshot.

use anyhow::{Context, Result, ensure};
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};

use super::{
    crypto::{ContentKey, DeviceIdentity},
    pairing::{ApprovedPair, PairedRoute, PairingDevice},
    protocol::validate_id,
};

const MAX_VAULT_BYTES: usize = 512 * 1024;
const MAX_PEERS: usize = 256;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PeerMaterial {
    dataset_id: String,
    peer: PairingDevice,
    key_version: u64,
    send_key: [u8; 32],
    receive_key: [u8; 32],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    route: Option<PairedRoute>,
    revoked: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Material {
    version: u32,
    device: PairingDevice,
    signing_private_key: Vec<u8>,
    mailbox_owner: [u8; 32],
    peers: Vec<PeerMaterial>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pairings: Vec<super::pairing::journal::PairingRecord>,
}

pub(crate) struct LocalVault {
    pub installation_id: String,
    pub revision: i64,
    material: Material,
    identity: DeviceIdentity,
}

impl std::fmt::Debug for LocalVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalVault")
            .field("revision", &self.revision)
            .field("peers", &self.material.peers.len())
            .finish_non_exhaustive()
    }
}

impl LocalVault {
    pub(crate) fn create(installation_id: &str, name: &str, platform: &str) -> Result<Self> {
        validate_id(installation_id)?;
        let identity = DeviceIdentity::generate()?;
        let device = PairingDevice {
            device_id: uuid::Uuid::new_v4().to_string(),
            stream_id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            platform: platform.into(),
            signing_key: identity.public_key(),
        };
        device.validate()?;
        let mut mailbox_owner = [0; 32];
        SystemRandom::new()
            .fill(&mut mailbox_owner)
            .map_err(|_| anyhow::anyhow!("sync_vault_random_failed"))?;
        Ok(Self {
            installation_id: installation_id.into(),
            revision: 1,
            material: Material {
                version: 1,
                device,
                signing_private_key: identity.to_pkcs8(),
                mailbox_owner,
                peers: vec![],
                pairings: vec![],
            },
            identity,
        })
    }

    pub(crate) fn device(&self) -> &PairingDevice {
        &self.material.device
    }
    pub(crate) fn identity(&self) -> &DeviceIdentity {
        &self.identity
    }
    pub(crate) fn mailbox_owner(&self) -> &[u8; 32] {
        &self.material.mailbox_owner
    }

    pub(crate) fn peer_keys(
        &self,
        dataset: &str,
        device: &str,
    ) -> Result<(ContentKey, ContentKey, u64)> {
        let peer = self
            .material
            .peers
            .iter()
            .find(|p| p.dataset_id == dataset && p.peer.device_id == device)
            .context("sync_vault_peer_missing")?;
        ensure!(!peer.revoked, "sync_vault_peer_revoked");
        Ok((
            ContentKey::from_bytes(peer.send_key),
            ContentKey::from_bytes(peer.receive_key),
            peer.key_version,
        ))
    }

    pub(crate) fn peer_route(&self, dataset: &str, device: &str) -> Result<&PairedRoute> {
        let peer = self
            .material
            .peers
            .iter()
            .find(|p| p.dataset_id == dataset && p.peer.device_id == device)
            .context("sync_vault_peer_missing")?;
        ensure!(!peer.revoked, "sync_vault_peer_revoked");
        peer.route.as_ref().context("sync_vault_peer_route_missing")
    }

    /// Persisting approved key material does not authorize a dataset member or
    /// create a relay grant. The coordinator must still complete those steps.
    pub(crate) fn install_pair(&mut self, pair: &ApprovedPair) -> Result<bool> {
        ensure!(
            pair.local_device() == self.device(),
            "sync_vault_pair_identity_mismatch"
        );
        validate_id(&pair.dataset_id)?;
        pair.peer.validate()?;
        if let Some(route) = &pair.route {
            route.validate()?;
            ensure!(
                route.local_mailbox
                    == super::transport_relay::RelayClient::mailbox_key(self.mailbox_owner()),
                "sync_vault_local_mailbox_mismatch"
            );
        }
        ensure!(
            pair.peer.device_id != self.device().device_id
                && pair.peer.stream_id != self.device().stream_id,
            "sync_vault_pair_identity_mismatch"
        );
        ensure!(
            (1..=i64::MAX as u64).contains(&pair.key_version),
            "sync_vault_peer_version_invalid"
        );
        let send_key = pair.send_key.to_bytes();
        let receive_key = pair.receive_key.to_bytes();
        ensure!(
            send_key != receive_key,
            "sync_vault_direction_keys_must_differ"
        );
        let position = self.material.peers.iter().position(|p| {
            p.dataset_id == pair.dataset_id && p.peer.device_id == pair.peer.device_id
        });
        if let Some(position) = position {
            let old = &self.material.peers[position];
            ensure!(
                old.peer.stream_id == pair.peer.stream_id
                    && old.peer.signing_key == pair.peer.signing_key,
                "sync_vault_peer_identity_changed"
            );
            ensure!(
                pair.key_version >= old.key_version,
                "sync_vault_peer_version_stale"
            );
            if pair.key_version == old.key_version {
                ensure!(
                    !old.revoked
                        && old.send_key == send_key
                        && old.receive_key == receive_key
                        && old.peer == pair.peer
                        && old.route == pair.route,
                    "sync_vault_peer_version_conflict"
                );
                return Ok(false);
            }
        } else {
            ensure!(
                self.material.peers.len() < MAX_PEERS,
                "sync_vault_peer_limit"
            );
        }
        let peer = PeerMaterial {
            dataset_id: pair.dataset_id.clone(),
            peer: pair.peer.clone(),
            key_version: pair.key_version,
            send_key,
            receive_key,
            route: pair.route.clone(),
            revoked: false,
        };
        if let Some(position) = position {
            self.material.peers[position] = peer;
        } else {
            self.material.peers.push(peer);
        }
        Ok(true)
    }

    pub(crate) fn pairing(
        &self,
        invitation: &str,
    ) -> Result<&super::pairing::journal::PairingRecord> {
        self.material
            .pairings
            .iter()
            .find(|p| p.invitation_id == invitation)
            .context("sync_pairing_record_missing")
    }

    pub(crate) fn pairing_statuses(
        &self,
        now_ms: i64,
    ) -> Vec<super::pairing::journal::PairingStatus> {
        self.material
            .pairings
            .iter()
            .map(|record| record.status(now_ms))
            .collect()
    }

    pub(crate) fn pairing_mut(
        &mut self,
        invitation: &str,
    ) -> Result<&mut super::pairing::journal::PairingRecord> {
        self.material
            .pairings
            .iter_mut()
            .find(|p| p.invitation_id == invitation)
            .context("sync_pairing_record_missing")
    }

    pub(crate) fn next_pairing_epoch(&self, peer: &str) -> Result<u64> {
        self.material
            .pairings
            .iter()
            .filter(|p| p.peer.device_id == peer)
            .map(|p| p.key_version)
            .chain(
                self.material
                    .peers
                    .iter()
                    .filter(|p| p.peer.device_id == peer)
                    .map(|p| p.key_version),
            )
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .filter(|v| *v <= i64::MAX as u64)
            .context("sync_vault_peer_version_exhausted")
    }

    pub(crate) fn add_pairing(
        &mut self,
        record: super::pairing::journal::PairingRecord,
        now_ms: i64,
    ) -> Result<()> {
        use super::pairing::journal::{MAX_PAIRING_RECORDS, MAX_PENDING_PAIRINGS};
        ensure!(
            now_ms >= 0 && now_ms < record.expires_at_ms,
            "pairing_confirmation_expired"
        );
        record.validate_local(
            self.device(),
            &super::transport_relay::RelayClient::mailbox_key(self.mailbox_owner()),
        )?;
        ensure!(
            self.material.pairings.len() < MAX_PAIRING_RECORDS,
            "sync_pairing_history_limit"
        );
        ensure!(
            !self
                .material
                .pairings
                .iter()
                .any(|p| p.invitation_id == record.invitation_id),
            "sync_pairing_invitation_already_consumed"
        );
        for old in &mut self.material.pairings {
            old.expire(now_ms);
        }
        ensure!(
            self.material
                .pairings
                .iter()
                .filter(|p| p.pending())
                .count()
                < MAX_PENDING_PAIRINGS,
            "sync_pairing_pending_limit"
        );
        ensure!(
            !self
                .material
                .pairings
                .iter()
                .any(|p| p.pending() && p.peer.device_id == record.peer.device_id),
            "sync_pairing_peer_already_pending"
        );
        self.material.pairings.push(record);
        Ok(())
    }

    pub(crate) fn revoke_peer(&mut self, dataset: &str, device: &str) -> Result<bool> {
        let peer = self
            .material
            .peers
            .iter_mut()
            .find(|p| p.dataset_id == dataset && p.peer.device_id == device)
            .context("sync_vault_peer_missing")?;
        let changed = !peer.revoked;
        peer.revoked = true;
        Ok(changed)
    }

    fn aad(installation: &str, revision: i64) -> Result<Vec<u8>> {
        validate_id(installation)?;
        ensure!(revision > 0, "sync_vault_revision_invalid");
        let mut aad = b"elsewhen-sync-local-vault-v1\0".to_vec();
        aad.extend_from_slice(&(installation.len() as u16).to_be_bytes());
        aad.extend_from_slice(installation.as_bytes());
        aad.extend_from_slice(&revision.to_be_bytes());
        Ok(aad)
    }

    pub(crate) fn seal(&self, master: &[u8; 32]) -> Result<Vec<u8>> {
        let aad = Self::aad(&self.installation_id, self.revision)?;
        let mut body = serde_json::to_vec(&self.material)
            .map_err(|_| anyhow::anyhow!("sync_vault_encode_failed"))?;
        ensure!(body.len() + 29 <= MAX_VAULT_BYTES, "sync_vault_too_large");
        let mut nonce = [0; 12];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| anyhow::anyhow!("sync_vault_random_failed"))?;
        let key = aead::LessSafeKey::new(
            aead::UnboundKey::new(&aead::CHACHA20_POLY1305, master)
                .map_err(|_| anyhow::anyhow!("sync_vault_key_invalid"))?,
        );
        key.seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(&aad),
            &mut body,
        )
        .map_err(|_| anyhow::anyhow!("sync_vault_seal_failed"))?;
        let mut sealed = vec![1];
        sealed.extend_from_slice(&nonce);
        sealed.extend(body);
        Ok(sealed)
    }

    pub(crate) fn open(
        installation: &str,
        revision: i64,
        bytes: &[u8],
        master: &[u8; 32],
    ) -> Result<Self> {
        let aad = Self::aad(installation, revision)?;
        ensure!(
            (29..=MAX_VAULT_BYTES).contains(&bytes.len()) && bytes[0] == 1,
            "sync_vault_record_invalid"
        );
        let key = aead::LessSafeKey::new(
            aead::UnboundKey::new(&aead::CHACHA20_POLY1305, master)
                .map_err(|_| anyhow::anyhow!("sync_vault_key_invalid"))?,
        );
        let mut body = bytes[13..].to_vec();
        let plaintext = key
            .open_in_place(
                aead::Nonce::try_assume_unique_for_key(&bytes[1..13])
                    .map_err(|_| anyhow::anyhow!("sync_vault_record_invalid"))?,
                aead::Aad::from(&aad),
                &mut body,
            )
            .map_err(|_| anyhow::anyhow!("sync_vault_authentication_failed"))?;
        let material: Material = serde_json::from_slice(plaintext)
            .map_err(|_| anyhow::anyhow!("sync_vault_record_invalid"))?;
        ensure!(
            material.version == 1 && material.peers.len() <= MAX_PEERS,
            "sync_vault_record_invalid"
        );
        material.device.validate()?;
        let identity = DeviceIdentity::from_pkcs8(&material.signing_private_key)
            .map_err(|_| anyhow::anyhow!("sync_vault_identity_invalid"))?;
        ensure!(
            identity.public_key() == material.device.signing_key,
            "sync_vault_identity_invalid"
        );
        let mut seen = std::collections::HashSet::new();
        for peer in &material.peers {
            validate_id(&peer.dataset_id)?;
            peer.peer.validate()?;
            if let Some(route) = &peer.route {
                route.validate()?;
                ensure!(
                    route.local_mailbox
                        == super::transport_relay::RelayClient::mailbox_key(
                            &material.mailbox_owner
                        ),
                    "sync_vault_local_mailbox_mismatch"
                );
            }
            ensure!(
                seen.insert((&peer.dataset_id, &peer.peer.device_id))
                    && peer.peer.device_id != material.device.device_id
                    && peer.peer.stream_id != material.device.stream_id
                    && (1..=i64::MAX as u64).contains(&peer.key_version)
                    && peer.send_key != peer.receive_key,
                "sync_vault_peer_invalid"
            );
        }
        ensure!(
            material.pairings.len() <= super::pairing::journal::MAX_PAIRING_RECORDS,
            "sync_pairing_history_limit"
        );
        let mut invitations = std::collections::HashSet::new();
        let mailbox = super::transport_relay::RelayClient::mailbox_key(&material.mailbox_owner);
        for record in &material.pairings {
            record.validate_local(&material.device, &mailbox)?;
            ensure!(
                invitations.insert(&record.invitation_id),
                "sync_pairing_record_invalid"
            );
        }
        Ok(Self {
            installation_id: installation.into(),
            revision,
            material,
            identity,
        })
    }
}
