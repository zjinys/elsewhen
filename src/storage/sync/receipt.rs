//! 同步操作收发队列：pending 操作、ack、receipt 接收/标记。

use super::*;

impl Store {
    pub(crate) fn pending_sync_operations(
        &self,
        recipient: &str,
        limit: usize,
    ) -> Result<Vec<SyncOperation>> {
        ensure!(
            (1..=MAX_BATCH_OPERATIONS).contains(&limit),
            "sync_batch_limit_invalid"
        );
        let mut query=self.connection.prepare("SELECT o.op_id FROM sync_outbox q JOIN sync_operations o USING(op_id) WHERE q.recipient_device_id=?1 AND q.acknowledged=0 AND EXISTS(SELECT 1 FROM sync_members m WHERE m.device_id=q.recipient_device_id AND m.active=1) ORDER BY o.origin_stream_id,o.origin_sequence LIMIT ?2")?;
        let operation_ids = query
            .query_map(params![recipient, limit as i64], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut result = Vec::new();
        let mut bytes = 0;
        for op_id in operation_ids {
            let payload: String = self.connection.query_row(
                "SELECT payload FROM sync_operations WHERE op_id=?1",
                [&op_id],
                |r| r.get(0),
            )?;
            ensure!(
                payload.len() <= MAX_OPERATION_BYTES,
                "sync_pending_operation_too_large"
            );
            if bytes + payload.len() > MAX_OPERATION_BYTES {
                break;
            }
            bytes += payload.len();
            result.push(SyncOperation::decode(payload.as_bytes())?);
        }
        Ok(result)
    }

    /// An authenticated recipient's application acknowledgement, not a relay upload response.
    pub(crate) fn acknowledge_sync_operation(
        &self,
        recipient: &str,
        op_id: &str,
        payload_hash: &str,
    ) -> Result<()> {
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
            [recipient],
            |r| r.get(0),
        )?;
        ensure!(active, "sync_member_not_authorized");
        let expected:Option<String>=self.connection.query_row("SELECT o.payload_hash FROM sync_outbox q JOIN sync_operations o USING(op_id) WHERE q.recipient_device_id=?1 AND q.op_id=?2",params![recipient,op_id],|r|r.get(0)).optional()?;
        ensure!(
            expected.as_deref() == Some(payload_hash),
            "sync_ack_mismatch"
        );
        self.connection.execute(
            "UPDATE sync_outbox SET acknowledged=1 WHERE recipient_device_id=?1 AND op_id=?2",
            params![recipient, op_id],
        )?;
        // S03 lifecycle: an application ack advances this member's watermark for
        // the local stream. op ids are `stream:seq`; the sequence is the
        // watermark position. Local captures insert `applied` ops directly and
        // never flow through ack, so the local device does not gate itself.
        if let Some(seq) = op_id.rsplit(':').next().and_then(|s| s.parse::<i64>().ok()) {
            self.advance_sync_member_watermark(recipient, seq)?;
        }
        Ok(())
    }

    pub(crate) fn pending_sync_receipts(
        &self,
        target: &str,
        limit: usize,
    ) -> Result<Vec<crate::sync::protocol::ApplicationReceipt>> {
        ensure!(
            (1..=MAX_BATCH_OPERATIONS).contains(&limit),
            "sync_batch_limit_invalid"
        );
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let mut query = self.connection.prepare("SELECT op_id,payload_hash FROM sync_receipts WHERE target_device_id=?1 AND pending=1 AND EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1) ORDER BY op_id LIMIT ?2")?;
        Ok(query
            .query_map(params![target, limit as i64], |r| {
                Ok(crate::sync::protocol::ApplicationReceipt {
                    dataset_id: identity.dataset_id.clone(),
                    acknowledging_device_id: identity.device_id.clone(),
                    target_device_id: target.to_string(),
                    op_id: r.get(0)?,
                    payload_hash: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub(crate) fn accept_sync_receipt(
        &self,
        authenticated_device: &str,
        receipt: &crate::sync::protocol::ApplicationReceipt,
    ) -> Result<()> {
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        ensure!(
            receipt.dataset_id == identity.dataset_id
                && receipt.target_device_id == identity.device_id
                && receipt.acknowledging_device_id == authenticated_device,
            "sync_receipt_identity_mismatch"
        );
        let tx = self.begin_write_transaction()?;
        // A re-snapshot application ack is a synthetic receipt whose op_id is
        // `resnapshot-ack:{export_id}`: it retires the export (which re-seeds
        // the member watermark at the build-time frontier), never an
        // operation. The export binding (id + recipient + state) is the
        // authority, not the hash — the recipient cannot know the sender's
        // build-time watermark, so no cross-checkable payload hash exists.
        if let Some(id) = receipt.op_id.strip_prefix("resnapshot-ack:") {
            // 重撮快照的 ack 是合成回执：绑定靠导出 id + 接收方 + 状态，
            // 收端无从得知发送方的构建时 watermark，故 hash 只能对 manifest 验证。
            self.acknowledge_snapshot_export(
                authenticated_device,
                id,
                &format!("snapshot-end:{id}"),
                &receipt.payload_hash,
            )?;
        } else {
            self.acknowledge_sync_operation(
                authenticated_device,
                &receipt.op_id,
                &receipt.payload_hash,
            )?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    pub(crate) fn mark_sync_receipt_sent(&self, op_id: &str) -> Result<()> {
        self.connection
            .execute("UPDATE sync_receipts SET pending=0 WHERE op_id=?1", [op_id])?;
        Ok(())
    }

    pub(crate) fn queue_sync_receipt(&self, op_id: &str) -> Result<()> {
        self.connection.execute("INSERT INTO sync_receipts(op_id,target_device_id,payload_hash) SELECT op_id,origin_device_id,payload_hash FROM sync_operations WHERE op_id=?1 AND local=0 AND state IN ('applied','conflict') ON CONFLICT(op_id) DO UPDATE SET pending=1", [op_id])?;
        Ok(())
    }
}
