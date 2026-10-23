//! Snapshot publication and relay scan progress are durable but distinct from
//! per-operation application acknowledgements. Incomplete exports never upload.

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use crate::storage::Store;
use crate::sync::crypto::{ContentKey, DeviceIdentity, EnvelopeHeader};
use crate::sync::protocol::validate_id;

impl Store {
    /// 导出前用域表事实自愈头表：live head 指向已不存在的域行会让接收端
    /// 的 (deleted ⇔ 行存在) 一致性闸门必炸（快照整块作废，见
    /// snapshot/apply.rs 的 Record::Head 校验）。只修状态位，不删记录，
    /// 幂等；历史 FK 中断等原因造成的陈旧头由此复原。
    pub(crate) fn heal_snapshot_heads(&self) -> Result<usize> {
        let mut touched = 0;
        for kind in [
            "conversation",
            "wiki_page",
            "todo",
            "habit_nudge",
            "goal",
            "rule",
        ] {
            let table = crate::storage::sync::snapshot::decode::domain_table(kind)?;
            touched += self.connection.execute(
                &format!(
                    "UPDATE sync_object_heads SET deleted=1
                     WHERE object_kind=?1 AND deleted=0
                       AND NOT EXISTS(SELECT 1 FROM {table} WHERE id=object_id)"
                ),
                [kind],
            )?;
        }
        Ok(touched)
    }

    /// False means this exact transfer is already ready (or acknowledged). An
    /// interrupted export needs a new request, not a different view under old IDs.
    pub(crate) fn begin_snapshot_export(
        &self,
        id: &str,
        recipient: &str,
        key_version: u64,
    ) -> Result<bool> {
        validate_id(id)?;
        validate_id(recipient)?;
        ensure!(
            (1..=i64::MAX as u64).contains(&key_version),
            "sync_snapshot_key_version_invalid"
        );
        let tx = self.begin_write_transaction()?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
            [recipient],
            |r| r.get(0),
        )?;
        ensure!(
            active && recipient != identity.device_id,
            "sync_snapshot_recipient_not_authorized"
        );
        let existing: Option<(String, i64, String)> = self.connection.query_row(
            "SELECT recipient_device_id,key_version,state FROM sync_snapshot_exports WHERE id=?1",
            [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).optional()?;
        if let Some((peer, version, state)) = existing {
            ensure!(
                peer == recipient && version == key_version as i64,
                "sync_snapshot_identity_conflict"
            );
            ensure!(
                matches!(state.as_str(), "ready" | "acknowledged"),
                "sync_snapshot_export_requires_new_request"
            );
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(false);
        }
        // A claimed re-snapshot (v30) may only be built under the request's
        // current id: an id rotated after re-flagging can never be satisfied
        // by an export built for the older request.
        let expected_request: Option<String> = self
            .connection
            .query_row(
                "SELECT snapshot_id FROM sync_resnapshot_requests WHERE device_id=?1",
                [recipient],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(expected) = expected_request {
            ensure!(expected == id, "sync_snapshot_resnapshot_request_mismatch");
        }
        // Pin the recipient's starting application watermark to the LOCAL
        // stream frontier at snapshot-build time. The exported snapshot contains
        // exactly the ops up to this frontier, so this is the correct starting
        // watermark — never 0 (holds back GC) and never the live frontier
        // (over-claims ops captured after the snapshot).
        let local_watermark: i64 = self
            .connection
            .query_row(
                "SELECT applied_sequence FROM sync_frontiers WHERE origin_stream_id=?1",
                [&identity.stream_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        // Which signed roster the export will embed as its authority frame.
        // 0 stays valid for pre-authority flows (none in-tree today); the
        // receiver cross-checks the embedded roster revision on completion.
        let roster_revision: i64 = self
            .current_signed_roster()?
            .map(|s| s.roster.revision as i64)
            .unwrap_or(0);
        self.connection.execute(
            "INSERT INTO sync_snapshot_exports(id,recipient_device_id,key_version,state,local_watermark,roster_revision) VALUES(?1,?2,?3,'building',?4,?5)",
            params![id,recipient,key_version as i64,local_watermark,roster_revision],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(true)
    }

    pub(crate) fn stage_snapshot_authority(
        &self,
        id: &str,
        header: &EnvelopeHeader,
        key: &ContentKey,
        signer: &DeviceIdentity,
        payload: &[u8],
    ) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let valid: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_snapshot_exports WHERE id=?1 AND recipient_device_id=?2 AND key_version=?3 AND state='building')",
            params![id, header.recipient_device_id, header.key_version as i64], |r| r.get(0))?;
        ensure!(
            valid && header.message_id.starts_with("membership:"),
            "sync_snapshot_authority_mismatch"
        );
        self.stage_sync_wire_envelope(header, key, signer, payload)?;
        // A wire has one export owner. Only an abandoned attempt may hand its
        // identical roster to a new attempt; ready exports keep their bindings.
        let changed = self.connection.execute(
            "INSERT INTO sync_snapshot_export_wires(snapshot_id,recipient_device_id,message_id) VALUES(?1,?2,?3)
             ON CONFLICT(recipient_device_id,message_id) DO UPDATE SET snapshot_id=excluded.snapshot_id
             WHERE sync_snapshot_export_wires.snapshot_id=excluded.snapshot_id
                OR EXISTS(SELECT 1 FROM sync_snapshot_exports s WHERE s.id=sync_snapshot_export_wires.snapshot_id AND s.state='aborted')",
            params![id, header.recipient_device_id, header.message_id],
        )?;
        ensure!(changed == 1, "sync_snapshot_authority_already_in_use");
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    pub(crate) fn stage_snapshot_export_payload(
        &self,
        id: &str,
        header: &EnvelopeHeader,
        key: &ContentKey,
        signer: &DeviceIdentity,
        payload: &[u8],
        finish: bool,
    ) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let (peer, version, state): (String, i64, String) = self.connection.query_row(
            "SELECT recipient_device_id,key_version,state FROM sync_snapshot_exports WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        ensure!(
            state == "building"
                && peer == header.recipient_device_id
                && version as u64 == header.key_version,
            "sync_snapshot_export_not_building"
        );
        ensure!(
            if finish {
                header.message_id == format!("snapshot-end:{id}")
            } else {
                header.message_id.starts_with(&format!("snapshot:{id}:"))
            },
            "sync_snapshot_identity_conflict"
        );
        self.stage_sync_wire_envelope(header, key, signer, payload)?;
        self.connection.execute(
            "INSERT INTO sync_snapshot_export_wires(snapshot_id,recipient_device_id,message_id) VALUES(?1,?2,?3)",
            params![id,peer,header.message_id],
        )?;
        if finish {
            self.connection.execute(
                "UPDATE sync_snapshot_exports SET state='ready' WHERE id=?1",
                [id],
            )?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    pub(crate) fn abort_snapshot_export(&self, id: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE sync_snapshot_exports SET state='aborted' WHERE id=?1 AND state='building'",
            [id],
        )?;
        Ok(())
    }

    /// Only the authenticated final application receipt retires the chunk sends.
    /// Covered operations keep their own acknowledgements; this is not a GC mark.
    pub(crate) fn acknowledge_snapshot_export(
        &self,
        peer: &str,
        id: &str,
        payload_hash: &str,
    ) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let ready: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_snapshot_exports WHERE id=?1 AND recipient_device_id=?2 AND state IN ('ready','acknowledged'))",
            params![id,peer], |r| r.get(0),
        )?;
        ensure!(ready, "sync_snapshot_export_not_ready");
        self.acknowledge_sync_wire(peer, &format!("snapshot-end:{id}"), payload_hash)?;
        self.connection.execute(
            "UPDATE sync_wire_outbox SET state='acknowledged' WHERE recipient_device_id=?1 AND message_id IN (SELECT message_id FROM sync_snapshot_export_wires WHERE snapshot_id=?2 AND recipient_device_id=?1)",
            params![peer,id],
        )?;
        self.connection.execute(
            "UPDATE sync_snapshot_exports SET state='acknowledged' WHERE id=?1",
            [id],
        )?;
        // The recipient has confirmed the snapshot: seed its application
        // watermark at the pinned build-time frontier so it neither holds back
        // GC (as 0 would) nor over-claims (as the live frontier would).
        let watermark: i64 = self.connection.query_row(
            "SELECT local_watermark FROM sync_snapshot_exports WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let mut frontiers = std::collections::BTreeMap::new();
        frontiers.insert(identity.stream_id.clone(), watermark);
        self.mark_member_snapshotted(peer, &frontiers)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    pub(crate) fn snapshot_import_pending(&self) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_snapshot_target t JOIN sync_snapshots s ON s.id=t.snapshot_id WHERE s.state='staging')",
            [], |r| r.get(0),
        )?)
    }

    pub(crate) fn staged_snapshot_manifest(&self, id: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.connection.query_row(
            "SELECT manifest FROM sync_snapshots WHERE id=?1",
            [id],
            |r| r.get(0),
        )?)
    }

    pub(crate) fn relay_scan_cursor(&self, mailbox: &str) -> Result<String> {
        Ok(self
            .connection
            .query_row(
                "SELECT after_id FROM sync_relay_cursors WHERE mailbox_id=?1",
                [mailbox],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| "0".into()))
    }

    /// This is a scan cursor, not an ack: unresolved mail remains at the relay and
    /// is revisited after wrapping. Poison mail cannot monopolize the first page.
    pub(crate) fn save_relay_scan_cursor(&self, mailbox: &str, cursor: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO sync_relay_cursors(mailbox_id,after_id) VALUES(?1,?2) ON CONFLICT(mailbox_id) DO UPDATE SET after_id=excluded.after_id",
            params![mailbox,cursor],
        )?;
        Ok(())
    }
}
