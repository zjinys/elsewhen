//! 同步权威与名册：初始化、锚定、处理器交接状态与已签名名册应用。

use super::*;

impl Store {
    pub(crate) fn has_sync_authority(&self) -> Result<bool> {
        Ok(self
            .connection
            .query_row("SELECT EXISTS(SELECT 1 FROM sync_authority)", [], |r| {
                r.get(0)
            })?)
    }

    /// 本机是否为同步名册的 owner（设置页里的「主设备」）。
    ///
    /// 尚未建立同步权威的单机安装没有 owner 概念，本机就是唯一设备；一旦
    /// 有了权威，只允许 owner 执行那些必须由单一设备负责的本地副作用。
    /// 已建同步副本但权威尚未落地的中间态（加入/恢复中）失败关闭。
    pub(crate) fn is_sync_owner_device(&self) -> Result<bool> {
        let identity = self.sync_identity()?;
        if !self.has_sync_authority()? {
            return Ok(identity.is_none());
        }
        let Some(identity) = identity else {
            return Ok(false);
        };
        Ok(identity.device_id == self.sync_pinned_authority()?.owner.device_id)
    }

    pub(crate) fn sync_pinned_authority(&self) -> Result<RosterAnchor> {
        let json: String = self.connection.query_row(
            "SELECT anchor FROM sync_authority WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        let anchor: RosterAnchor = serde_json::from_str(&json)
            .map_err(|_| anyhow::anyhow!("sync_authority_record_invalid"))?;
        anchor.validate()?;
        Ok(anchor)
    }
    pub(crate) fn current_signed_roster(&self) -> Result<Option<SignedRoster>> {
        let bytes: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT signed_roster FROM sync_authority WHERE singleton=1",
                [],
                |r| r.get::<_, Option<Vec<u8>>>(0),
            )
            .optional()?
            .flatten();
        bytes
            .map(|bytes| SignedRoster::decode(&bytes, &self.sync_pinned_authority()?))
            .transpose()
    }
    pub(crate) fn sync_roster_anchor(&self) -> Result<RosterAnchor> {
        match self.current_signed_roster()? {
            Some(signed) => signed.roster.anchor(),
            None => self.sync_pinned_authority(),
        }
    }

    pub(crate) fn initialize_sync_authority(
        &self,
        owner: &PairingDevice,
        signer: &DeviceIdentity,
    ) -> Result<SignedRoster> {
        let tx = self.begin_write_transaction()?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        ensure!(
            identity.device_id == owner.device_id
                && identity.stream_id == owner.stream_id
                && identity.processor_device_id == owner.device_id
                && owner.signing_key == signer.public_key(),
            "sync_authority_owner_mismatch"
        );
        if self.has_sync_authority()? {
            let current = self
                .current_signed_roster()?
                .context("sync_authority_not_ready")?;
            ensure!(
                current.roster.owner == *owner,
                "sync_authority_already_pinned"
            );
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(current);
        }
        let other: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id!=?1)",
            [&owner.device_id],
            |r| r.get(0),
        )?;
        ensure!(!other, "sync_authority_requires_unshared_owner");
        let signed = SignedRoster::sign(
            MembershipRoster {
                version: 1,
                dataset_id: identity.dataset_id,
                owner: owner.clone(),
                revision: 1,
                processor_device_id: owner.device_id.clone(),
                members: vec![RosterMember {
                    device: owner.clone(),
                    active: true,
                }],
                handoff: None,
            },
            signer,
        )?;
        self.connection.execute(
            "INSERT INTO sync_authority(singleton,anchor,local_device) VALUES(1,?1,?2)",
            params![
                serde_json::to_string(&signed.roster.anchor()?)?,
                serde_json::to_string(owner)?
            ],
        )?;
        self.apply_signed_roster(&signed.encode()?)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(signed)
    }

    /// Only the joiner's explicitly approved Noise result carries its owner root.
    /// Pinning deliberately disables membership until a signed grant arrives.
    pub(crate) fn pin_sync_authority_from_pair(&self, pair: &ApprovedPair) -> Result<()> {
        let anchor = pair
            .authority
            .as_ref()
            .context("sync_pairing_has_no_authority")?;
        anchor.validate()?;
        ensure!(
            anchor.owner == pair.peer && anchor.dataset_id == pair.dataset_id,
            "sync_pairing_authority_mismatch"
        );
        let tx = self.begin_write_transaction()?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        ensure!(
            identity.dataset_id == pair.dataset_id
                && identity.device_id == pair.local_device().device_id
                && identity.stream_id == pair.local_device().stream_id
                && identity.processor_device_id == anchor.owner.device_id,
            "sync_pairing_replica_mismatch"
        );
        if self.has_sync_authority()? {
            ensure!(
                self.sync_pinned_authority()? == *anchor,
                "sync_authority_already_pinned"
            );
        } else {
            self.ensure_empty_snapshot_target()?;
            self.connection.execute(
                "INSERT INTO sync_authority(singleton,anchor,local_device) VALUES(1,?1,?2)",
                params![
                    serde_json::to_string(anchor)?,
                    serde_json::to_string(pair.local_device())?
                ],
            )?;
            self.connection
                .execute("UPDATE sync_members SET active=0", [])?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// Old active processor yields its role (quiescent + proof generation).
    /// The owner uses the returned proof to build a successor roster.
    /// `signer` must be the current device's identity (loaded from vault by caller).
    pub(crate) fn yield_processor_role(
        &self,
        target_revision: u64,
        to_device_id: &str,
        signer: &crate::sync::crypto::DeviceIdentity,
    ) -> Result<crate::sync::membership::ProcessingYieldProof> {
        let tx = self.begin_write_transaction()?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        ensure!(
            identity.processor_device_id == identity.device_id,
            "sync_membership_not_processor"
        );

        self.require_background_quiescent_for_sync()?;

        let next_sequence: i64 = self.connection.query_row(
            "SELECT next_sequence FROM sync_replica WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        let final_sequence = next_sequence - 1;

        let proof = crate::sync::membership::ProcessingYieldProof::sign(
            &identity.dataset_id,
            &identity.device_id,
            to_device_id,
            target_revision,
            final_sequence as u64,
            signer,
        )?;

        self.connection.execute(
            "INSERT INTO sync_processor_state(singleton,state,from_device_id,to_device_id,final_sequence) VALUES(1,'relinquished',?1,?2,?3)
             ON CONFLICT(singleton) DO UPDATE SET state=excluded.state,from_device_id=excluded.from_device_id,to_device_id=excluded.to_device_id,final_sequence=excluded.final_sequence,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![identity.device_id, to_device_id, final_sequence],
        )?;

        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(proof)
    }

    /// New processor confirms takeover after frontier catches up.
    pub(crate) fn confirm_processor_takeover(&self) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;

        let row: Option<(
            String,
            Option<String>,
            Option<String>,
            i64,
        )> = self
            .connection
            .query_row(
                "SELECT state,from_device_id,to_device_id,final_sequence FROM sync_processor_state WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;

        let (st, from_device, target, final_seq) =
            row.context("sync_processor_not_in_pending_takeover")?;
        ensure!(
            st == "pending_takeover",
            "sync_processor_not_in_pending_takeover"
        );
        ensure!(
            identity.device_id == target.as_deref().unwrap_or(""),
            "sync_processor_takeover_target_mismatch"
        );
        let from_device = from_device.context("sync_processor_takeover_source_missing")?;

        // Frontier gate: the takeover source's stream must be fully applied up
        // to the final sequence the old processor certified in the handoff.
        let stream: String = self
            .connection
            .query_row(
                "SELECT stream_id FROM sync_members WHERE device_id=?1",
                [&from_device],
                |r| r.get(0),
            )
            .context("sync_processor_takeover_source_missing")?;
        let applied: i64 = self
            .connection
            .query_row(
                "SELECT COALESCE((SELECT applied_sequence FROM sync_frontiers WHERE origin_stream_id=?1),0)",
                [&stream],
                |r| r.get(0),
            )?;
        ensure!(applied >= final_seq, "sync_frontier_lagging");

        self.connection.execute(
            "UPDATE sync_replica SET processor_device_id=?1 WHERE singleton=1",
            params![identity.device_id],
        )?;

        self.connection.execute(
            "UPDATE sync_processor_state SET state='active' WHERE singleton=1",
            [],
        )?;

        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 查询本机加工移交状态（独立 DTO，不影响 LocalSyncStateDto）。
    pub(crate) fn get_processor_handoff_state(&self) -> Result<ProcessorHandoffState> {
        let identity = self.sync_identity()?;
        let row: Option<(String, Option<String>, Option<String>, i64)> = self
            .connection
            .query_row(
                "SELECT state,from_device_id,to_device_id,final_sequence FROM sync_processor_state WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;

        let mut dto = ProcessorHandoffState {
            state: row
                .as_ref()
                .map(|(s, _, _, _)| s.clone())
                .unwrap_or_else(|| "idle".to_string()),
            from_device_id: row.as_ref().and_then(|(_, f, _, _)| f.clone()),
            to_device_id: row.as_ref().and_then(|(_, _, t, _)| t.clone()),
            final_sequence: row.as_ref().map(|(_, _, _, f)| *f),
            can_confirm: false,
        };

        if let Some((st, _, _, _)) = row {
            if st == "pending_takeover" {
                if let Some(id) = identity {
                    dto.can_confirm = id.device_id == dto.to_device_id.as_deref().unwrap_or("");
                }
            }
        }
        Ok(dto)
    }

    /// A roster is complete state. Skipped revisions are safe only because every
    /// previously known identity and irreversible revocation must be retained.
    pub(crate) fn sync_member_active(&self, device: &str) -> Result<bool> {
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
            params![device],
            |r| r.get(0),
        )?;
        Ok(active)
    }

    pub(crate) fn apply_signed_roster(&self, bytes: &[u8]) -> Result<bool> {
        let tx = self.begin_write_transaction()?;
        let signed = SignedRoster::decode(bytes, &self.sync_pinned_authority()?)?;
        let roster = &signed.roster;
        let digest = roster.digest()?;
        let known: Option<String> = self
            .connection
            .query_row(
                "SELECT digest FROM sync_authority_versions WHERE revision=?1",
                [roster.revision as i64],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(known) = known {
            ensure!(known == digest, "sync_membership_version_content_conflict");
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(false);
        }
        if let Some(previous) = self.current_signed_roster()? {
            if roster.revision < previous.roster.revision {
                self.connection.execute(
                    "INSERT INTO sync_authority_versions(revision,digest) VALUES(?1,?2)",
                    params![roster.revision as i64, digest],
                )?;
                if let Some(tx) = tx {
                    tx.commit()?;
                }
                return Ok(false);
            }
            roster.validate_successor(&previous.roster)?;
        }
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        ensure!(
            roster.dataset_id == identity.dataset_id,
            "sync_processing_handoff_required"
        );

        if roster.processor_device_id != identity.processor_device_id {
            // Handoff roster: validate_successor already enforced that the proof
            // is signed by the previous processor and revision-bound; here we
            // only re-check it against the locally recorded last processor.
            let handoff = roster
                .handoff
                .as_ref()
                .context("sync_handoff_proof_required")?;
            ensure!(
                handoff.from_device_id == identity.processor_device_id,
                "sync_handoff_proof_source_mismatch"
            );
            ensure!(
                handoff.to_device_id == roster.processor_device_id,
                "sync_handoff_proof_target_mismatch"
            );

            // Record the cryptographic handoff credential for delayed-analysis
            // boundary enforcement and future audits.
            self.connection.execute(
                "INSERT INTO sync_processor_handoffs(roster_revision,dataset_id,from_device_id,to_device_id,from_sequence,proof_json) VALUES(?1,?2,?3,?4,?5,?6)
                 ON CONFLICT(roster_revision) DO UPDATE SET proof_json=excluded.proof_json",
                params![
                    roster.revision as i64,
                    handoff.dataset_id,
                    handoff.from_device_id,
                    handoff.to_device_id,
                    handoff.final_sequence as i64,
                    serde_json::to_string(handoff)?,
                ],
            )?;

            if identity.device_id == handoff.to_device_id {
                // This device is the takeover target: keep the previous
                // processor as the effective processor and wait for the local
                // frontier to catch up before an explicit confirm.
                self.connection.execute(
                    "INSERT INTO sync_processor_state(singleton,state,from_device_id,to_device_id,final_sequence) VALUES(1,'pending_takeover',?1,?2,?3)
                     ON CONFLICT(singleton) DO UPDATE SET state=excluded.state,from_device_id=excluded.from_device_id,to_device_id=excluded.to_device_id,final_sequence=excluded.final_sequence,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
                    params![
                        handoff.from_device_id,
                        handoff.to_device_id,
                        handoff.final_sequence as i64,
                    ],
                )?;
            } else {
                // Old processor or observer: adopt the successor immediately.
                self.connection.execute(
                    "UPDATE sync_replica SET processor_device_id=?1 WHERE singleton=1",
                    params![roster.processor_device_id],
                )?;
                // The old processor already marked itself 'relinquished' at
                // yield; observers without a state row stay out of this table.
                self.connection.execute(
                    "UPDATE sync_processor_state SET to_device_id=?1,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE singleton=1 AND state='relinquished'",
                    [handoff.to_device_id.as_str()],
                )?;
            }
        }
        let local_json: String = self.connection.query_row(
            "SELECT local_device FROM sync_authority WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        let local: PairingDevice = serde_json::from_str(&local_json)?;
        if let Some(member) = roster
            .members
            .iter()
            .find(|m| m.device.device_id == identity.device_id)
        {
            ensure!(
                same_identity(&member.device, &local),
                "sync_membership_local_key_mismatch"
            );
        }
        self.connection
            .execute("UPDATE sync_members SET active=0", [])?;
        for member in &roster.members {
            let device = &member.device;
            let previous: Option<String> = self
                .connection
                .query_row(
                    "SELECT device_id FROM sync_members WHERE stream_id=?1",
                    [&device.stream_id],
                    |r| r.get(0),
                )
                .optional()?;
            ensure!(
                previous.as_deref().is_none_or(|id| id == device.device_id),
                "sync_stream_identity_conflict"
            );
            self.connection.execute("INSERT INTO sync_member_keys(device_id,stream_id,signing_key,active) VALUES(?1,?2,?3,?4) ON CONFLICT(device_id) DO UPDATE SET active=excluded.active",params![device.device_id,device.stream_id,device.signing_key.as_slice(),member.active])?;
            self.connection.execute("INSERT INTO sync_members(device_id,stream_id,active) VALUES(?1,?2,?3) ON CONFLICT(stream_id) DO UPDATE SET active=excluded.active",params![device.device_id,device.stream_id,member.active])?;
            self.connection.execute(
                "INSERT OR IGNORE INTO sync_frontiers(origin_stream_id) VALUES(?1)",
                [&device.stream_id],
            )?;
            if member.active && device.device_id != identity.device_id {
                self.connection.execute("INSERT OR IGNORE INTO sync_outbox(op_id,recipient_device_id) SELECT op_id,?1 FROM sync_operations WHERE local=1",[&device.device_id])?;
            }
        }
        self.connection.execute(
            "UPDATE sync_authority SET signed_roster=?1 WHERE singleton=1",
            [signed.encode()?],
        )?;
        self.connection.execute(
            "INSERT INTO sync_authority_versions(revision,digest) VALUES(?1,?2)",
            params![roster.revision as i64, digest],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(true)
    }
}
