//! Enrollment intent is installation-local and may only be persisted in the
//! encrypted vault. Relay outcomes never silently change the original request.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::{
    pairing::{ApprovedPair, InvitationRoute, PairedRoute, PairingDevice},
    protocol::validate_id,
    transport_relay::RelayClient,
};

pub(crate) const GRANT_LIFETIME_MS: i64 = 7 * 24 * 60 * 60 * 1000;
pub(crate) const MAX_ENROLLMENTS: usize = 256;
pub(crate) const MAX_PENDING_ENROLLMENTS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EnrollmentRole {
    Owner,
    Joiner,
}
impl EnrollmentRole {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Joiner => "joiner",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GrantIntent {
    pub intent_id: String,
    pub mailbox: String,
    pub sender: String,
    pub credential: [u8; 32],
    pub expires_at_ms: i64,
    pub expected_revision: i64,
}
impl std::fmt::Debug for GrantIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GrantIntent([REDACTED])")
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RevokeIntent {
    pub intent_id: String,
    pub mailbox: String,
    pub sender: String,
    pub expected_revision: i64,
}
impl std::fmt::Debug for RevokeIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RevokeIntent([REDACTED])")
    }
}

fn intent_id(invitation: &str, role: EnrollmentRole, kind: &str) -> String {
    format!(
        "enrollment-{kind}:{}",
        crate::storage::sync::hash(format!("{invitation}\0{}", role.name()).as_bytes())
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SnapshotChoice {
    pub id: String,
    pub manifest_hash: Option<String>,
    pub chunks: i64,
    pub bytes: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EnrollmentRecord {
    pub version: u32,
    pub invitation_id: String,
    pub dataset_id: String,
    pub local_device_id: String,
    pub peer: PairingDevice,
    pub role: EnrollmentRole,
    pub key_version: u64,
    pub route: PairedRoute,
    pub bootstrap: InvitationRoute,
    pub receive: GrantIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoke: Option<RevokeIntent>,
    pub receive_ready: bool,
    pub temporary_revoked: bool,
    pub roster_message_id: Option<String>,
    pub snapshot: Option<SnapshotChoice>,
    pub snapshot_attempts: u32,
    pub owner_confirmed: bool,
}
impl std::fmt::Debug for EnrollmentRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnrollmentRecord")
            .field("role", &self.role)
            .field("invitation_id", &self.invitation_id)
            .finish_non_exhaustive()
    }
}
impl EnrollmentRecord {
    pub(crate) fn new(pair: &ApprovedPair, role: EnrollmentRole, now_ms: i64) -> Result<Self> {
        let route = pair
            .route
            .clone()
            .context("sync_enrollment_route_missing")?;
        let bootstrap = pair
            .bootstrap
            .clone()
            .context("sync_enrollment_bootstrap_missing_requires_repair")?;
        let expires_at_ms = now_ms
            .checked_add(GRANT_LIFETIME_MS)
            .context("sync_enrollment_time_invalid")?;
        let credential = RelayClient::derive_sender_credential(
            &pair.receive_key.to_bytes(),
            &route.peer_mailbox,
            &route.local_mailbox,
        )?;
        let record = Self {
            version: 1,
            invitation_id: pair.invitation_id().into(),
            dataset_id: pair.dataset_id.clone(),
            local_device_id: pair.local_device().device_id.clone(),
            peer: pair.peer.clone(),
            role,
            key_version: pair.key_version,
            receive: GrantIntent {
                intent_id: intent_id(pair.invitation_id(), role, "grant"),
                mailbox: route.local_mailbox.clone(),
                sender: route.peer_mailbox.clone(),
                credential,
                expires_at_ms,
                expected_revision: if role == EnrollmentRole::Owner { 0 } else { 1 },
            },
            revoke: (role == EnrollmentRole::Owner).then(|| RevokeIntent {
                intent_id: intent_id(pair.invitation_id(), role, "revoke"),
                mailbox: route.local_mailbox.clone(),
                sender: bootstrap.request_sender.clone(),
                expected_revision: 1,
            }),
            route,
            bootstrap,
            receive_ready: false,
            temporary_revoked: role == EnrollmentRole::Joiner,
            roster_message_id: None,
            snapshot: None,
            snapshot_attempts: 0,
            owner_confirmed: false,
        };
        record.validate()?;
        Ok(record)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "sync_enrollment_version_unsupported");
        for id in [&self.invitation_id, &self.dataset_id, &self.local_device_id] {
            validate_id(id)?;
        }
        self.peer.validate()?;
        self.route.validate()?;
        self.bootstrap.validate()?;
        ensure!(
            self.peer.device_id != self.local_device_id
                && (1..=i64::MAX as u64).contains(&self.key_version),
            "sync_enrollment_identity_invalid"
        );
        let issuer = if self.role == EnrollmentRole::Owner {
            &self.route.local_mailbox
        } else {
            &self.route.peer_mailbox
        };
        ensure!(
            self.bootstrap.issuer_mailbox == *issuer
                && self.bootstrap.relay_url == self.route.relay_url
                && self.bootstrap.request_sender != self.route.local_mailbox
                && self.bootstrap.request_sender != self.route.peer_mailbox,
            "sync_enrollment_bootstrap_mismatch"
        );
        ensure!(
            self.receive.intent_id == intent_id(&self.invitation_id, self.role, "grant")
                && self.receive.mailbox == self.route.local_mailbox
                && self.receive.sender == self.route.peer_mailbox
                && self.receive.expires_at_ms > 0
                && self.receive.expected_revision
                    == if self.role == EnrollmentRole::Owner {
                        0
                    } else {
                        1
                    },
            "sync_enrollment_grant_invalid"
        );
        if self.role == EnrollmentRole::Owner {
            let revoke = self
                .revoke
                .as_ref()
                .context("sync_enrollment_revoke_missing_requires_repair")?;
            ensure!(
                revoke.intent_id == intent_id(&self.invitation_id, self.role, "revoke")
                    && revoke.mailbox == self.route.local_mailbox
                    && revoke.sender == self.bootstrap.request_sender
                    && revoke.expected_revision == 1,
                "sync_enrollment_revoke_invalid"
            );
        } else {
            ensure!(
                self.revoke.is_none() && self.temporary_revoked,
                "sync_enrollment_revoke_invalid"
            );
        }
        ensure!(
            self.snapshot_attempts <= 3,
            "sync_enrollment_snapshot_attempt_limit"
        );
        if let Some(snapshot) = &self.snapshot {
            validate_id(&snapshot.id)?;
            if let Some(hash) = &snapshot.manifest_hash {
                ensure!(
                    valid_hash(hash)
                        && (1..=1024).contains(&snapshot.chunks)
                        && (1..=256 * 1024 * 1024).contains(&snapshot.bytes),
                    "sync_enrollment_snapshot_invalid"
                );
            } else {
                ensure!(
                    self.role == EnrollmentRole::Owner
                        && snapshot.chunks == 0
                        && snapshot.bytes == 0,
                    "sync_enrollment_snapshot_invalid"
                );
            }
        }
        if let Some(id) = &self.roster_message_id {
            ensure!(
                id.starts_with("membership:") && id.len() <= 200,
                "sync_enrollment_roster_invalid"
            );
        }
        ensure!(
            !self.owner_confirmed || self.snapshot.is_some(),
            "sync_enrollment_completion_invalid"
        );
        Ok(())
    }
    pub(crate) fn require_live(&self, now_ms: i64) -> Result<()> {
        self.validate()?;
        ensure!(
            now_ms >= 0 && now_ms < self.receive.expires_at_ms,
            "sync_enrollment_grant_expired_requires_repair"
        );
        Ok(())
    }
}

pub(crate) fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SnapshotOffer {
    pub version: u32,
    pub invitation_id: String,
    pub dataset_id: String,
    pub sender_device_id: String,
    pub recipient_device_id: String,
    pub key_version: u64,
    pub snapshot_id: String,
    pub manifest_hash: String,
    pub chunks: i64,
    pub bytes: i64,
}
impl SnapshotOffer {
    pub(crate) fn validate_for(&self, record: &EnrollmentRecord) -> Result<()> {
        ensure!(
            self.version == 1
                && self.invitation_id == record.invitation_id
                && self.dataset_id == record.dataset_id
                && self.sender_device_id == record.peer.device_id
                && self.recipient_device_id == record.local_device_id
                && self.key_version == record.key_version,
            "sync_enrollment_offer_identity_mismatch"
        );
        validate_id(&self.snapshot_id)?;
        ensure!(
            valid_hash(&self.manifest_hash)
                && (1..=1024).contains(&self.chunks)
                && (1..=256 * 1024 * 1024).contains(&self.bytes),
            "sync_enrollment_offer_invalid"
        );
        Ok(())
    }
    pub(crate) fn choice(&self) -> SnapshotChoice {
        SnapshotChoice {
            id: self.snapshot_id.clone(),
            manifest_hash: Some(self.manifest_hash.clone()),
            chunks: self.chunks,
            bytes: self.bytes,
        }
    }
}

pub(crate) fn offer_id(invitation: &str, snapshot: &str) -> String {
    format!(
        "enrollment-offer:{}",
        crate::storage::sync::hash(format!("{invitation}\0{snapshot}").as_bytes())
    )
}
pub(crate) fn completion_id(invitation: &str, snapshot: &str) -> String {
    format!(
        "enrollment-complete:{}",
        crate::storage::sync::hash(format!("{invitation}\0{snapshot}").as_bytes())
    )
}
