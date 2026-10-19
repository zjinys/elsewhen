//! 成员生命周期：签发、吊销、处理器交接与传输密钥/成员门禁。

use super::*;

impl Store {
    /// Owner-side CAS. A new identity is granted only from a confirmed pair;
    /// constructing a payload with the joiner's self-signature cannot call this.
    pub(crate) fn grant_sync_pair(
        &self,
        expected_revision: u64,
        pair: &ApprovedPair,
        owner: &DeviceIdentity,
    ) -> Result<SignedRoster> {
        let tx = self.begin_write_transaction()?;
        let current = self
            .current_signed_roster()?
            .context("sync_authority_not_ready")?;
        ensure!(
            current.roster.revision == expected_revision,
            "sync_membership_revision_changed"
        );
        ensure!(
            current.roster.owner == *pair.local_device()
                && current.roster.dataset_id == pair.dataset_id
                && current.roster.owner.signing_key == owner.public_key(),
            "sync_membership_owner_required"
        );
        let pair_anchor = pair
            .authority
            .as_ref()
            .context("sync_pairing_has_no_authority")?;
        ensure!(
            *pair_anchor == current.roster.anchor()?,
            "sync_pairing_authority_changed"
        );
        ensure!(
            !current
                .roster
                .members
                .iter()
                .any(|m| m.device.device_id == pair.peer.device_id),
            "sync_membership_identity_already_known"
        );
        let mut roster = current.roster;
        roster.revision = roster
            .revision
            .checked_add(1)
            .context("sync_membership_revision_exhausted")?;
        roster.members.push(RosterMember {
            device: pair.peer.clone(),
            active: true,
        });
        roster
            .members
            .sort_by(|a, b| a.device.device_id.cmp(&b.device.device_id));
        let signed = SignedRoster::sign(roster, owner)?;
        self.apply_signed_roster(&signed.encode()?)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(signed)
    }

    pub(crate) fn revoke_signed_sync_member(
        &self,
        expected_revision: u64,
        device: &str,
        owner: &DeviceIdentity,
    ) -> Result<SignedRoster> {
        let tx = self.begin_write_transaction()?;
        let current = self
            .current_signed_roster()?
            .context("sync_authority_not_ready")?;
        ensure!(
            current.roster.revision == expected_revision,
            "sync_membership_revision_changed"
        );
        ensure!(
            current.roster.owner.signing_key == owner.public_key(),
            "sync_membership_owner_required"
        );
        ensure!(
            device != current.roster.owner.device_id
                && device != current.roster.processor_device_id,
            "sync_membership_handoff_required"
        );
        let mut roster = current.roster;
        let member = roster
            .members
            .iter_mut()
            .find(|m| m.device.device_id == device)
            .context("sync_membership_member_missing")?;
        ensure!(member.active, "sync_membership_already_revoked");
        member.active = false;
        roster.revision = roster
            .revision
            .checked_add(1)
            .context("sync_membership_revision_exhausted")?;
        let signed = SignedRoster::sign(roster, owner)?;
        self.apply_signed_roster(&signed.encode()?)?;
        // S02 清理缺口：撤销成员时清除发往该设备的在途信封（pending），
        // 否则 outbox 会对一个永远无法回执的收件人反复重试（悬空流量）。
        // 已回执（acknowledged）的历史记录保留。
        self.connection.execute(
            "DELETE FROM sync_wire_outbox WHERE recipient_device_id=?1 AND state='pending'",
            [device],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(signed)
    }

    /// Owner-side handoff approval: verify the old processor's signed yield
    /// proof against the live roster and issue the successor roster. The proof
    /// never grants anything by itself; only the owner signature broadcasts it.
    pub(crate) fn approve_processor_handoff(
        &self,
        proof: &crate::sync::membership::ProcessingYieldProof,
        owner: &DeviceIdentity,
    ) -> Result<SignedRoster> {
        let tx = self.begin_write_transaction()?;
        proof.validate()?;
        let current = self
            .current_signed_roster()?
            .context("sync_authority_not_ready")?;
        ensure!(
            current.roster.owner.signing_key == owner.public_key(),
            "sync_membership_owner_required"
        );
        ensure!(
            proof.dataset_id == current.roster.dataset_id,
            "sync_handoff_dataset_mismatch"
        );
        ensure!(
            proof.roster_revision
                == current
                    .roster
                    .revision
                    .checked_add(1)
                    .context("sync_membership_revision_exhausted")?,
            "sync_handoff_revision_mismatch"
        );
        ensure!(
            proof.from_device_id == current.roster.processor_device_id,
            "sync_handoff_source_mismatch"
        );
        // The proof must verify under the old processor's registered member key.
        let old_processor = current
            .roster
            .members
            .iter()
            .find(|m| m.active && m.device.device_id == current.roster.processor_device_id)
            .context("sync_handoff_old_processor_inactive")?;
        proof.verify(&old_processor.device.signing_key)?;
        ensure!(
            current
                .roster
                .members
                .iter()
                .any(|m| m.active && m.device.device_id == proof.to_device_id),
            "sync_handoff_target_missing"
        );
        let mut roster = current.roster;
        roster.revision = proof.roster_revision;
        roster.processor_device_id = proof.to_device_id.clone();
        roster.handoff = Some(proof.clone());
        let signed = SignedRoster::sign(roster, owner)?;
        self.apply_signed_roster(&signed.encode()?)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(signed)
    }

    pub(crate) fn require_sync_transport_key(
        &self,
        sender: &str,
        key: &[u8; 32],
        owner_control: bool,
    ) -> Result<()> {
        if !self.has_sync_authority()? {
            return Ok(());
        }
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let anchor = self.sync_pinned_authority()?;
        if owner_control {
            ensure!(
                sender == anchor.owner.device_id && *key == anchor.owner.signing_key,
                "sync_membership_owner_required"
            );
            return Ok(());
        }
        self.require_local_sync_membership()?;
        let expected:Option<Vec<u8>>=self.connection.query_row("SELECT k.signing_key FROM sync_member_keys k JOIN sync_members m ON m.device_id=k.device_id AND m.stream_id=k.stream_id WHERE k.device_id=?1 AND k.active=1 AND m.active=1",[sender],|r|r.get(0)).optional()?;
        ensure!(
            expected.as_deref() == Some(key.as_slice()),
            "sync_membership_signing_key_mismatch"
        );
        ensure!(sender != identity.device_id, "sync_local_stream_spoof");
        Ok(())
    }

    pub(crate) fn require_local_sync_membership(&self) -> Result<()> {
        if !self.has_sync_authority()? {
            return Ok(());
        }
        ensure!(
            self.local_sync_membership_active()?,
            "sync_local_membership_inactive"
        );
        Ok(())
    }

    pub(crate) fn local_sync_membership_active(&self) -> Result<bool> {
        Ok(self.connection.query_row("SELECT EXISTS(SELECT 1 FROM sync_replica r JOIN sync_member_keys k ON k.device_id=r.device_id AND k.stream_id=r.stream_id JOIN sync_members m ON m.stream_id=r.stream_id AND m.device_id=r.device_id WHERE k.active=1 AND m.active=1)",[],|r|r.get(0))?)
    }

    pub(crate) fn require_sync_signer(&self, signer: &DeviceIdentity) -> Result<()> {
        if !self.has_sync_authority()? {
            return Ok(());
        }
        self.require_local_sync_membership()?;
        let key:Vec<u8>=self.connection.query_row("SELECT k.signing_key FROM sync_replica r JOIN sync_member_keys k ON k.device_id=r.device_id",[],|r|r.get(0))?;
        ensure!(
            key == signer.public_key(),
            "sync_local_signing_key_mismatch"
        );
        Ok(())
    }

    pub(crate) fn signed_snapshot_member(&self, device: &str, stream: &str) -> Result<bool> {
        if !self.has_sync_authority()? {
            return Ok(false);
        }
        let known: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_member_keys WHERE device_id=?1 AND stream_id=?2)",
            params![device, stream],
            |r| r.get(0),
        )?;
        ensure!(known, "sync_snapshot_member_needs_authority");
        Ok(true)
    }
}
