//! Atomic, native-revision-fenced enrollment transitions. Secrets remain in the
//! vault; existing snapshot/wire tables are the source of transfer progress.
use anyhow::{Context, Result, ensure};
use rusqlite::OptionalExtension;

use super::{PairingCommit, Store};
use crate::sync::{
    enrollment::{
        EnrollmentRecord, EnrollmentRole, SnapshotChoice, SnapshotOffer, completion_id, offer_id,
    },
    vault::LocalVault,
};

impl Store {
    pub(crate) fn checked_enrollment(
        &self,
        installation: &str,
        master: &[u8; 32],
        revision: i64,
        invitation: &str,
    ) -> Result<(LocalVault, EnrollmentRecord)> {
        let vault = self.load_sync_vault(installation, master, revision)?;
        ensure!(
            vault.revision == revision,
            "sync_enrollment_revision_changed"
        );
        let record = vault.enrollment(invitation)?.clone();
        record.validate()?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        ensure!(
            record.dataset_id == identity.dataset_id
                && record.local_device_id == identity.device_id
                && vault.device().device_id == identity.device_id,
            "sync_enrollment_identity_mismatch"
        );
        let (_, receive_key, epoch) =
            vault.peer_keys(&record.dataset_id, &record.peer.device_id)?;
        ensure!(
            epoch == record.key_version,
            "sync_enrollment_key_changed_requires_repair"
        );
        let route = vault.peer_route(&record.dataset_id, &record.peer.device_id)?;
        ensure!(
            route.relay_url == record.route.relay_url
                && route.local_mailbox == record.route.local_mailbox
                && route.peer_mailbox == record.route.peer_mailbox,
            "sync_enrollment_route_changed"
        );
        let expected = crate::sync::transport_relay::RelayClient::derive_sender_credential(
            &receive_key.to_bytes(),
            &route.peer_mailbox,
            &route.local_mailbox,
        )?;
        ensure!(
            expected == record.receive.credential,
            "sync_enrollment_grant_invalid"
        );
        let anchor = self.sync_roster_anchor()?;
        ensure!(
            anchor.dataset_id == record.dataset_id
                && anchor.owner
                    == if record.role == EnrollmentRole::Owner {
                        vault.device().clone()
                    } else {
                        record.peer.clone()
                    },
            "sync_enrollment_authority_mismatch"
        );
        if let Some(roster) = self.current_signed_roster()? {
            for device in [vault.device(), &record.peer] {
                ensure!(
                    roster
                        .roster
                        .members
                        .iter()
                        .any(|m| m.active && m.device == *device),
                    "sync_enrollment_member_revoked"
                );
            }
        } else {
            ensure!(
                record.role == EnrollmentRole::Joiner
                    && vault.pairing(invitation)?.status(0).state == "awaiting_membership",
                "sync_enrollment_membership_missing"
            );
        }
        if record.owner_confirmed {
            let snapshot = record
                .snapshot
                .as_ref()
                .context("sync_enrollment_snapshot_missing")?;
            let applied = if record.role == EnrollmentRole::Owner {
                self.enrollment_export_state(&snapshot.id)?.as_deref() == Some("acknowledged")
            } else {
                self.enrollment_import_state(&snapshot.id)?.as_deref() == Some("applied")
            };
            ensure!(
                applied,
                "sync_enrollment_completion_missing_requires_repair"
            );
        }
        Ok((vault, record))
    }

    fn mutate_enrollment(
        &self,
        installation: &str,
        master: &[u8; 32],
        revision: i64,
        invitation: &str,
        mutate: impl FnOnce(&Self, &mut EnrollmentRecord) -> Result<bool>,
    ) -> Result<PairingCommit> {
        let tx = self.begin_write_transaction()?;
        let (mut vault, _) = self.checked_enrollment(installation, master, revision, invitation)?;
        let changed = mutate(self, vault.enrollment_mut(invitation)?)?;
        vault.enrollment(invitation)?.validate()?;
        if changed {
            self.save_sync_vault(&mut vault, master)?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(PairingCommit {
            invitation_id: invitation.into(),
            vault_revision: vault.revision,
        })
    }

    pub(crate) fn confirm_enrollment_grant(
        &self,
        installation: &str,
        master: &[u8; 32],
        revision: i64,
        invitation: &str,
        remote_revision: i64,
    ) -> Result<PairingCommit> {
        self.mutate_enrollment(installation, master, revision, invitation, |_, record| {
            ensure!(
                remote_revision == record.receive.expected_revision + 1,
                "sync_enrollment_grant_result_mismatch"
            );
            let changed = !record.receive_ready;
            record.receive_ready = true;
            Ok(changed)
        })
    }

    pub(crate) fn confirm_enrollment_revoke(
        &self,
        installation: &str,
        master: &[u8; 32],
        revision: i64,
        invitation: &str,
        remote_revision: i64,
    ) -> Result<PairingCommit> {
        self.mutate_enrollment(installation, master, revision, invitation, |_, record| {
            let revoke = record
                .revoke
                .as_ref()
                .context("sync_enrollment_revoke_missing_requires_repair")?;
            ensure!(
                record.role == EnrollmentRole::Owner
                    && record.receive_ready
                    && remote_revision == revoke.expected_revision + 1,
                "sync_enrollment_revoke_result_mismatch"
            );
            let changed = !record.temporary_revoked;
            record.temporary_revoked = true;
            Ok(changed)
        })
    }

    pub(crate) fn enrollment_export_state(&self, snapshot: &str) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT state FROM sync_snapshot_exports WHERE id=?1",
                [snapshot],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub(crate) fn enrollment_import_state(&self, snapshot: &str) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT state FROM sync_snapshots WHERE id=?1",
                [snapshot],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub(crate) fn enrollment_wire_state(
        &self,
        peer: &str,
        message: &str,
    ) -> Result<Option<(String, bool)>> {
        Ok(self.connection.query_row("SELECT state,relay_id IS NOT NULL FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id=?2",
            rusqlite::params![peer, message], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
    }

    pub(crate) fn reserve_enrollment_snapshot(
        &self,
        installation: &str,
        master: &[u8; 32],
        revision: i64,
        invitation: &str,
    ) -> Result<PairingCommit> {
        self.mutate_enrollment(
            installation,
            master,
            revision,
            invitation,
            |store, record| {
                ensure!(
                    record.role == EnrollmentRole::Owner
                        && record.receive_ready
                        && record.temporary_revoked,
                    "sync_enrollment_channel_not_ready"
                );
                if let Some(old) = &record.snapshot {
                    match store.enrollment_export_state(&old.id)?.as_deref() {
                        None | Some("ready" | "acknowledged") => return Ok(false),
                        Some("building" | "aborted") => {
                            ensure!(
                                store
                                    .enrollment_wire_state(
                                        &record.peer.device_id,
                                        &offer_id(invitation, &old.id)
                                    )?
                                    .is_none(),
                                "sync_enrollment_offered_snapshot_cannot_change"
                            );
                            store.abort_snapshot_export(&old.id)?;
                        }
                        _ => anyhow::bail!("sync_enrollment_export_invalid"),
                    }
                }
                ensure!(
                    record.snapshot_attempts < 3,
                    "sync_enrollment_snapshot_attempt_limit"
                );
                record.snapshot_attempts += 1;
                record.snapshot = Some(SnapshotChoice {
                    id: uuid::Uuid::new_v4().to_string(),
                    manifest_hash: None,
                    chunks: 0,
                    bytes: 0,
                });
                Ok(true)
            },
        )
    }

    pub(crate) fn apply_enrollment_roster(
        &self,
        installation: &str,
        master: &[u8; 32],
        revision: i64,
        invitation: &str,
        message_id: &str,
        bytes: &[u8],
    ) -> Result<PairingCommit> {
        let tx = self.begin_write_transaction()?;
        let (_, record) = self.checked_enrollment(installation, master, revision, invitation)?;
        ensure!(
            record.role == EnrollmentRole::Joiner && record.receive_ready,
            "sync_enrollment_channel_not_ready"
        );
        let signed =
            crate::sync::membership::SignedRoster::decode(bytes, &self.sync_roster_anchor()?)?;
        ensure!(
            crate::sync::engine::membership_message_id(&signed)? == message_id
                && record
                    .roster_message_id
                    .as_deref()
                    .is_none_or(|id| id == message_id),
            "sync_enrollment_roster_changed"
        );
        let commit =
            self.finish_sync_pairing_join(installation, master, revision, invitation, bytes)?;
        self.queue_sync_wire_receipt(&record.peer.device_id, message_id, &super::hash(bytes))?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(commit)
    }

    pub(crate) fn select_enrollment_snapshot(
        &self,
        installation: &str,
        master: &[u8; 32],
        revision: i64,
        invitation: &str,
        offer: &SnapshotOffer,
        payload: &[u8],
    ) -> Result<PairingCommit> {
        let tx = self.begin_write_transaction()?;
        let commit = self.mutate_enrollment(
            installation,
            master,
            revision,
            invitation,
            |store, record| {
                ensure!(
                    record.role == EnrollmentRole::Joiner
                        && record.receive_ready
                        && record.roster_message_id.is_some(),
                    "sync_enrollment_roster_not_ready"
                );
                offer.validate_for(record)?;
                let choice = offer.choice();
                if let Some(previous) = &record.snapshot {
                    ensure!(
                        *previous == choice,
                        "sync_enrollment_snapshot_already_selected"
                    );
                    return Ok(false);
                }
                store.begin_snapshot_import(
                    &choice.id,
                    &record.peer.device_id,
                    record.key_version,
                )?;
                record.snapshot = Some(choice);
                Ok(true)
            },
        )?;
        self.queue_sync_wire_receipt(
            &offer.sender_device_id,
            &offer_id(invitation, &offer.snapshot_id),
            &super::hash(payload),
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(commit)
    }

    pub(crate) fn complete_enrollment(
        &self,
        installation: &str,
        master: &[u8; 32],
        revision: i64,
        invitation: &str,
        confirmation: Option<(&SnapshotOffer, &[u8])>,
    ) -> Result<PairingCommit> {
        let tx = self.begin_write_transaction()?;
        let commit = self.mutate_enrollment(
            installation,
            master,
            revision,
            invitation,
            |store, record| {
                let snapshot = record
                    .snapshot
                    .as_ref()
                    .context("sync_enrollment_snapshot_missing")?;
                if record.role == EnrollmentRole::Owner {
                    ensure!(
                        confirmation.is_none()
                            && store.enrollment_export_state(&snapshot.id)?.as_deref()
                                == Some("acknowledged"),
                        "sync_enrollment_receipt_missing"
                    );
                } else {
                    let (offer, _) =
                        confirmation.context("sync_enrollment_confirmation_missing")?;
                    offer.validate_for(record)?;
                    ensure!(
                        offer.choice() == *snapshot
                            && store.enrollment_import_state(&snapshot.id)?.as_deref()
                                == Some("applied"),
                        "sync_enrollment_snapshot_not_applied"
                    );
                }
                let changed = !record.owner_confirmed;
                record.owner_confirmed = true;
                Ok(changed)
            },
        )?;
        if let Some((offer, payload)) = confirmation {
            self.queue_sync_wire_receipt(
                &offer.sender_device_id,
                &completion_id(invitation, &offer.snapshot_id),
                &super::hash(payload),
            )?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(commit)
    }
}
