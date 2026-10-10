//! Post-approval recovery only. Live Noise states and invitation PSKs are never
//! serialized. These records may be persisted only inside the local AEAD vault.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::{ApprovedPair, FRAME_LIMIT, PairingDevice, PendingCompletion};
use crate::sync::{crypto::ContentKey, membership::RosterAnchor, protocol::validate_id};

pub(crate) const MAX_PAIRING_RECORDS: usize = 256;
pub(crate) const MAX_PENDING_PAIRINGS: usize = 8;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPair {
    dataset_id: String,
    local_device: PairingDevice,
    send_key: [u8; 32],
    receive_key: [u8; 32],
    authority: RosterAnchor,
    route: super::PairedRoute,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "state", deny_unknown_fields)]
enum Progress {
    AwaitingInstallation { pair: StoredPair, response: Vec<u8> },
    AwaitingMembership { proof: Vec<u8> },
    Authorized { proof_hash: String },
    Joined,
    Expired,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairingRecord {
    pub invitation_id: String,
    pub peer: PairingDevice,
    pub key_version: u64,
    pub expires_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bootstrap: Option<super::InvitationRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    delivery: Option<super::PairingDelivery>,
    progress: Progress,
}

/// Recovery inventory contains no credentials, Noise frames or private keys.
#[derive(Debug)]
pub(crate) struct PairingStatus {
    pub invitation_id: String,
    pub peer_device_id: String,
    pub peer_name: String,
    pub state: &'static str,
    pub expires_at_ms: i64,
}

impl PairingRecord {
    pub(crate) fn status(&self, now_ms: i64) -> PairingStatus {
        let state = match self.progress {
            Progress::AwaitingInstallation { .. } if now_ms >= self.expires_at_ms => "expired",
            Progress::AwaitingInstallation { .. } => "awaiting_installation",
            Progress::AwaitingMembership { .. } => "awaiting_membership",
            Progress::Authorized { .. } => "authorized",
            Progress::Joined => "joined",
            Progress::Expired => "expired",
        };
        PairingStatus {
            invitation_id: self.invitation_id.clone(),
            peer_device_id: self.peer.device_id.clone(),
            peer_name: self.peer.name.clone(),
            state,
            expires_at_ms: self.expires_at_ms,
        }
    }

    pub(crate) fn owner(pending: PendingCompletion, response: Vec<u8>) -> Result<Self> {
        let pair = pending.pair;
        let authority = pair.authority.context("sync_pairing_has_no_authority")?;
        let route = pair.route.context("pairing_relay_route_missing")?;
        ensure!(
            authority.owner == pair.local_device,
            "sync_membership_owner_required"
        );
        let record = Self {
            invitation_id: pair.invitation_id,
            peer: pair.peer,
            key_version: pair.key_version,
            expires_at_ms: pending.expires_at_ms,
            delivery: pair.delivery,
            bootstrap: pair.bootstrap,
            progress: Progress::AwaitingInstallation {
                pair: StoredPair {
                    dataset_id: pair.dataset_id,
                    local_device: pair.local_device,
                    send_key: pair.send_key.to_bytes(),
                    receive_key: pair.receive_key.to_bytes(),
                    authority,
                    route,
                },
                response,
            },
        };
        record.validate()?;
        Ok(record)
    }

    pub(crate) fn joiner(pair: &ApprovedPair, proof: Vec<u8>) -> Result<Self> {
        let anchor = pair
            .authority
            .as_ref()
            .context("sync_pairing_has_no_authority")?;
        ensure!(anchor.owner == pair.peer, "sync_pairing_authority_mismatch");
        let route = pair.route.as_ref().context("pairing_relay_route_missing")?;
        route.validate()?;
        if let Some(delivery) = &pair.delivery {
            ensure!(
                delivery.relay_url == route.relay_url
                    && delivery.recipient_mailbox == route.peer_mailbox
                    && delivery.sender_mailbox != route.local_mailbox,
                "pairing_delivery_invalid"
            );
        }
        let record = Self {
            invitation_id: pair.invitation_id.clone(),
            peer: pair.peer.clone(),
            key_version: pair.key_version,
            expires_at_ms: pair.expires_at_ms,
            delivery: pair.delivery.clone(),
            bootstrap: pair.bootstrap.clone(),
            progress: Progress::AwaitingMembership { proof },
        };
        record.validate()?;
        Ok(record)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        validate_id(&self.invitation_id)?;
        self.peer.validate()?;
        if let Some(route) = &self.bootstrap {
            route.validate()?;
            if let Progress::AwaitingInstallation { pair, .. } = &self.progress {
                ensure!(
                    route.relay_url == pair.route.relay_url
                        && route.issuer_mailbox == pair.route.local_mailbox
                        && route.request_sender != pair.route.local_mailbox
                        && route.request_sender != pair.route.peer_mailbox,
                    "sync_pairing_bootstrap_mismatch"
                );
            }
            if let (Progress::AwaitingMembership { .. }, Some(delivery)) =
                (&self.progress, &self.delivery)
            {
                ensure!(
                    route.relay_url == delivery.relay_url
                        && route.issuer_mailbox == delivery.recipient_mailbox
                        && route.request_sender == delivery.sender_mailbox,
                    "sync_pairing_bootstrap_mismatch"
                );
            }
        }
        ensure!(
            (1..=i64::MAX as u64).contains(&self.key_version) && self.expires_at_ms > 0,
            "sync_pairing_record_invalid"
        );
        if let Some(delivery) = &self.delivery {
            delivery.validate()?;
            ensure!(
                self.pending() && delivery.expires_at_ms == self.expires_at_ms,
                "pairing_delivery_invalid"
            );
            if let Progress::AwaitingInstallation { pair, .. } = &self.progress {
                ensure!(
                    delivery.relay_url == pair.route.relay_url
                        && delivery.sender_mailbox == pair.route.local_mailbox
                        && delivery.recipient_mailbox == pair.route.peer_mailbox,
                    "pairing_delivery_invalid"
                );
            }
        }
        let frame = match &self.progress {
            Progress::AwaitingInstallation { pair, response } => {
                validate_id(&pair.dataset_id)?;
                pair.local_device.validate()?;
                pair.authority.validate()?;
                pair.route.validate()?;
                ensure!(
                    pair.authority.dataset_id == pair.dataset_id
                        && pair.authority.owner == pair.local_device
                        && pair.local_device.device_id != self.peer.device_id
                        && pair.local_device.stream_id != self.peer.stream_id
                        && pair.send_key != pair.receive_key,
                    "sync_pairing_record_invalid"
                );
                Some(response)
            }
            Progress::AwaitingMembership { proof } => Some(proof),
            Progress::Authorized { proof_hash } => {
                ensure!(
                    proof_hash.len() == 64 && proof_hash.bytes().all(|b| b.is_ascii_hexdigit()),
                    "sync_pairing_record_invalid"
                );
                None
            }
            Progress::Joined | Progress::Expired => None,
        };
        if let Some(frame) = frame {
            ensure!(
                !frame.is_empty() && frame.len() <= FRAME_LIMIT,
                "pairing_frame_size_invalid"
            );
        }
        Ok(())
    }

    pub(crate) fn validate_local(&self, local: &PairingDevice, mailbox: &str) -> Result<()> {
        self.validate()?;
        ensure!(
            self.peer.device_id != local.device_id && self.peer.stream_id != local.stream_id,
            "sync_pairing_record_invalid"
        );
        if matches!(self.progress, Progress::AwaitingMembership { .. }) {
            if let Some(delivery) = &self.delivery {
                ensure!(
                    delivery.sender_mailbox != mailbox && delivery.recipient_mailbox != mailbox,
                    "pairing_delivery_invalid"
                );
            }
        }
        if let Progress::AwaitingInstallation { pair, .. } = &self.progress {
            ensure!(
                pair.local_device == *local && pair.route.local_mailbox == mailbox,
                "sync_pairing_record_invalid"
            );
        }
        Ok(())
    }

    pub(crate) fn pending(&self) -> bool {
        matches!(
            self.progress,
            Progress::AwaitingInstallation { .. } | Progress::AwaitingMembership { .. }
        )
    }

    pub(crate) fn expire(&mut self, now_ms: i64) {
        if self.pending() && self.expires_at_ms <= now_ms {
            self.progress = Progress::Expired;
            self.delivery = None;
        }
    }

    pub(crate) fn delivery(&self, now_ms: i64) -> Result<super::PairingDelivery> {
        self.frame(now_ms)?;
        self.delivery
            .clone()
            .context("sync_pairing_delivery_missing_requires_new_invitation")
    }

    pub(crate) fn frame(&self, now_ms: i64) -> Result<&[u8]> {
        ensure!(
            now_ms >= 0 && now_ms < self.expires_at_ms,
            "pairing_confirmation_expired"
        );
        match &self.progress {
            Progress::AwaitingInstallation { response, .. } => Ok(response),
            Progress::AwaitingMembership { proof } => Ok(proof),
            _ => anyhow::bail!("sync_pairing_no_pending_frame"),
        }
    }

    pub(crate) fn confirmed_pair(&self, proof: &[u8], now_ms: i64) -> Result<ApprovedPair> {
        let Progress::AwaitingInstallation { pair, .. } = &self.progress else {
            anyhow::bail!("sync_pairing_not_awaiting_installation");
        };
        PendingCompletion {
            pair: ApprovedPair {
                dataset_id: pair.dataset_id.clone(),
                peer: self.peer.clone(),
                local_device: pair.local_device.clone(),
                invitation_id: self.invitation_id.clone(),
                expires_at_ms: self.expires_at_ms,
                key_version: self.key_version,
                send_key: ContentKey::from_bytes(pair.send_key),
                receive_key: ContentKey::from_bytes(pair.receive_key),
                authority: Some(pair.authority.clone()),
                route: Some(pair.route.clone()),
                delivery: None,
                bootstrap: self.bootstrap.clone(),
            },
            expires_at_ms: self.expires_at_ms,
        }
        .confirm(proof, now_ms)
    }

    pub(crate) fn authorized(&self, proof: &[u8]) -> Result<bool> {
        ensure!(
            !proof.is_empty() && proof.len() <= FRAME_LIMIT,
            "pairing_frame_size_invalid"
        );
        if let Progress::Authorized { proof_hash } = &self.progress {
            ensure!(
                *proof_hash == digest(proof),
                "sync_pairing_completion_content_conflict"
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) fn is_authorized(&self) -> bool {
        matches!(self.progress, Progress::Authorized { .. })
    }

    pub(crate) fn mark_authorized(&mut self, proof: &[u8]) {
        self.delivery = None;
        self.progress = Progress::Authorized {
            proof_hash: digest(proof),
        };
    }

    pub(crate) fn mark_joined(&mut self) -> Result<bool> {
        match self.progress {
            Progress::Joined => Ok(false),
            Progress::AwaitingMembership { .. } => {
                self.progress = Progress::Joined;
                self.delivery = None;
                Ok(true)
            }
            _ => anyhow::bail!("sync_pairing_not_awaiting_membership"),
        }
    }
}

fn digest(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
