//! Durable post-approval coordination. Callers hold the workspace lease and
//! confirm each returned vault revision in native storage before publishing a
//! frame. These internal methods do not perform network I/O or change AppConfig.

use anyhow::{Context, Result, ensure};

use super::{ReplicaIdentity, Store};
use crate::sync::{
    membership::SignedRoster,
    pairing::{ApprovedPair, PendingApproval, journal::PairingRecord},
    vault::LocalVault,
};

#[derive(Debug)]
pub(crate) struct PairingCommit {
    pub invitation_id: String,
    pub vault_revision: i64,
}

/// Secret-bearing transport input; not a settings DTO or a membership grant.
pub(crate) struct PairingDispatch {
    pub delivery: crate::sync::pairing::PairingDelivery,
    pub frame: Vec<u8>,
}

impl Store {
    pub(crate) fn sync_pairing_dispatch(
        &self,
        installation: &str,
        master: &[u8; 32],
        protected_revision: i64,
        invitation: &str,
        now_ms: i64,
    ) -> Result<PairingDispatch> {
        let tx = self.begin_write_transaction()?;
        let frame =
            self.sync_pairing_frame(installation, master, protected_revision, invitation, now_ms)?;
        let vault = self.pairing_vault(installation, master, protected_revision)?;
        let delivery = vault.pairing(invitation)?.delivery(now_ms)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(PairingDispatch { delivery, frame })
    }

    fn pairing_vault(
        &self,
        installation: &str,
        master: &[u8; 32],
        revision: i64,
    ) -> Result<LocalVault> {
        let vault = self.load_sync_vault(installation, master, revision)?;
        ensure!(
            vault.revision == revision,
            "sync_pairing_protected_revision_unconfirmed"
        );
        Ok(vault)
    }

    /// Consumes precisely the displayed Noise request, not a fresh lookup by name.
    /// A crash before commit expires the invitation; after commit the exact response
    /// and reserved epoch can be recovered without executing approval again.
    pub(crate) fn approve_sync_pairing(
        &self,
        installation: &str,
        master: &[u8; 32],
        protected_revision: i64,
        approval: PendingApproval,
        now_ms: i64,
    ) -> Result<PairingCommit> {
        let tx = self.begin_write_transaction()?;
        let mut vault = self.pairing_vault(installation, master, protected_revision)?;
        ensure!(self.has_sync_authority()?, "sync_pairing_has_no_authority");
        self.require_sync_signer(vault.identity())?;
        let roster = self
            .current_signed_roster()?
            .context("sync_authority_not_ready")?;
        ensure!(
            roster.roster.owner == *vault.device(),
            "sync_membership_owner_required"
        );
        ensure!(
            !roster
                .roster
                .members
                .iter()
                .any(|m| m.device.device_id == approval.preview().device_id
                    || m.device.stream_id == approval.preview().stream_id),
            "sync_membership_identity_already_known"
        );
        let epoch = vault.next_pairing_epoch(&approval.preview().device_id)?;
        let (pending, response) = approval.approve_for_dataset(
            vault.identity(),
            epoch,
            now_ms,
            roster.roster.anchor()?,
        )?;
        let record = PairingRecord::owner(pending, response)?;
        let invitation_id = record.invitation_id.clone();
        vault.add_pairing(record, now_ms)?;
        self.save_sync_vault(&mut vault, master)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(PairingCommit {
            invitation_id,
            vault_revision: vault.revision,
        })
    }

    /// Fresh isolated dataset only. Pinning the verified authority is not a grant;
    /// the new installation remains inactive and cannot claim background work.
    pub(crate) fn install_sync_pairing_join(
        &self,
        installation: &str,
        master: &[u8; 32],
        protected_revision: i64,
        pair: ApprovedPair,
        now_ms: i64,
    ) -> Result<PairingCommit> {
        let journal: String = self
            .connection
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        if !journal.eq_ignore_ascii_case("wal") {
            ensure!(
                self.connection.is_autocommit(),
                "sync_initialize_requires_autocommit"
            );
            self.connection.pragma_update(None, "journal_mode", "WAL")?;
        }
        let tx = self.begin_write_transaction()?;
        let mut vault = self.pairing_vault(installation, master, protected_revision)?;
        ensure!(
            self.sync_identity()?.is_none() && !self.has_sync_authority()?,
            "sync_pairing_requires_new_dataset"
        );
        self.ensure_empty_snapshot_target()?;
        let anchor = pair
            .authority
            .as_ref()
            .context("sync_pairing_has_no_authority")?;
        ensure!(anchor.owner == pair.peer, "sync_pairing_authority_mismatch");
        let proof = pair.completion_proof(vault.identity())?;
        let record = PairingRecord::joiner(&pair, proof)?;
        let invitation_id = record.invitation_id.clone();
        vault.add_pairing(record, now_ms)?;
        vault.install_pair(&pair)?;
        self.initialize_sync_replica(&ReplicaIdentity {
            dataset_id: pair.dataset_id.clone(),
            device_id: vault.device().device_id.clone(),
            stream_id: vault.device().stream_id.clone(),
            processor_device_id: anchor.owner.device_id.clone(),
        })?;
        self.pin_sync_authority_from_pair(&pair)?;
        self.save_sync_vault(&mut vault, master)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(PairingCommit {
            invitation_id,
            vault_revision: vault.revision,
        })
    }

    pub(crate) fn sync_pairing_statuses(
        &self,
        installation: &str,
        master: &[u8; 32],
        protected_revision: i64,
        now_ms: i64,
    ) -> Result<Vec<crate::sync::pairing::journal::PairingStatus>> {
        ensure!(now_ms >= 0, "pairing_time_invalid");
        let vault = self.pairing_vault(installation, master, protected_revision)?;
        let mut statuses = vault.pairing_statuses(now_ms);
        if self.has_sync_authority()? {
            if let Some(roster) = self.current_signed_roster()? {
                for status in &mut statuses {
                    if roster.roster.members.iter().any(|m| {
                        !m.active
                            && (m.device.device_id == status.peer_device_id
                                || m.device.device_id == vault.device().device_id)
                    }) {
                        status.state = "revoked";
                    }
                }
            }
        }
        Ok(statuses)
    }

    /// No frame is returned by a mutation. Native storage must first persist and
    /// read back the commit revision; retrying returns identical bytes/nonce.
    pub(crate) fn sync_pairing_frame(
        &self,
        installation: &str,
        master: &[u8; 32],
        protected_revision: i64,
        invitation: &str,
        now_ms: i64,
    ) -> Result<Vec<u8>> {
        let vault = self.pairing_vault(installation, master, protected_revision)?;
        ensure!(self.has_sync_authority()?, "sync_pairing_has_no_authority");
        let record = vault.pairing(invitation)?;
        if let Some(roster) = self.current_signed_roster()? {
            ensure!(
                !roster.roster.members.iter().any(|m| !m.active
                    && (m.device.device_id == record.peer.device_id
                        || m.device.device_id == vault.device().device_id)),
                "sync_pairing_member_revoked"
            );
        }
        Ok(record.frame(now_ms)?.to_vec())
    }

    /// Installation proof, key installation, signed membership, source/outbox
    /// projection and the consumed-invitation receipt share a single transaction.
    pub(crate) fn confirm_sync_pairing_installation(
        &self,
        installation: &str,
        master: &[u8; 32],
        protected_revision: i64,
        invitation: &str,
        proof: &[u8],
        now_ms: i64,
    ) -> Result<PairingCommit> {
        let tx = self.begin_write_transaction()?;
        let mut vault = self.pairing_vault(installation, master, protected_revision)?;
        ensure!(self.has_sync_authority()?, "sync_pairing_has_no_authority");
        self.require_sync_signer(vault.identity())?;
        let record = vault.pairing(invitation)?;
        if record.authorized(proof)? {
            self.require_pairing_peer_active(&vault, invitation)?;
        } else {
            let pair = record.confirmed_pair(proof, now_ms)?;
            let revision = pair
                .authority
                .as_ref()
                .context("sync_pairing_has_no_authority")?
                .revision;
            vault.install_pair(&pair)?;
            self.grant_sync_pair(revision, &pair, vault.identity())?;
            vault.pairing_mut(invitation)?.mark_authorized(proof);
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

    fn require_pairing_peer_active(
        &self,
        vault: &LocalVault,
        invitation: &str,
    ) -> Result<SignedRoster> {
        let record = vault.pairing(invitation)?;
        let roster = self
            .current_signed_roster()?
            .context("sync_authority_not_ready")?;
        self.require_sync_signer(vault.identity())?;
        ensure!(
            roster
                .roster
                .members
                .iter()
                .any(|m| m.active && m.device == record.peer),
            "sync_pairing_peer_revoked_or_changed"
        );
        Ok(roster)
    }

    pub(crate) fn sync_pairing_roster(
        &self,
        installation: &str,
        master: &[u8; 32],
        protected_revision: i64,
        invitation: &str,
    ) -> Result<SignedRoster> {
        let vault = self.pairing_vault(installation, master, protected_revision)?;
        ensure!(
            vault.pairing(invitation)?.is_authorized(),
            "sync_pairing_not_authorized"
        );
        self.require_pairing_peer_active(&vault, invitation)
    }

    /// Membership can arrive after the invitation's deadline: the owner already
    /// verified an on-time installation proof. It is still checked against the
    /// pinned root and irreversible revocations before any Provider is imported.
    pub(crate) fn finish_sync_pairing_join(
        &self,
        installation: &str,
        master: &[u8; 32],
        protected_revision: i64,
        invitation: &str,
        roster: &[u8],
    ) -> Result<PairingCommit> {
        let tx = self.begin_write_transaction()?;
        let mut vault = self.pairing_vault(installation, master, protected_revision)?;
        let changed = vault.pairing_mut(invitation)?.mark_joined()?;
        self.apply_signed_roster(roster)?;
        self.require_pairing_peer_active(&vault, invitation)?;
        // Uses the same connection/transaction and the explicitly supplied master.
        ensure!(
            self.provider_master.as_ref() == Some(master),
            "provider_keys_locked"
        );
        self.enable_provider_key_sealing()?;
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
}

#[cfg(test)]
mod tests;
