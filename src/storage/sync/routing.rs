//! The signed owner is the durable forwarding hub. Membership alone never
//! establishes encryption keys between two joined installations.
use super::*;

impl Store {
    pub(crate) fn sync_upstream_device(&self) -> Result<Option<String>> {
        let local = self.sync_identity()?.context("sync_not_initialized")?;
        let owner: Option<String> = self
            .connection
            .query_row(
                "SELECT json_extract(CAST(signed_roster AS TEXT),'$.roster.owner.device_id')
             FROM sync_authority WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(owner.filter(|owner| owner != &local.device_id))
    }

    pub(crate) fn require_sync_operation_source(
        &self,
        sender: &str,
        origin: &str,
        stream: &str,
    ) -> Result<()> {
        let allowed = sender == origin || self.sync_upstream_device()?.as_deref() == Some(sender);
        ensure!(allowed, "sync_sender_mismatch");
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND stream_id=?2 AND active=1)
             AND EXISTS(SELECT 1 FROM sync_members WHERE device_id=?3 AND active=1)",
            params![origin,stream,sender], |r|r.get(0),
        )?;
        ensure!(active, "sync_member_not_authorized");
        Ok(())
    }

    pub(crate) fn queue_sync_forwarding(&self, op_id: &str) -> Result<()> {
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let Some(roster) = self.current_signed_roster()? else {
            return Ok(());
        };
        if roster.roster.owner.device_id != identity.device_id {
            return Ok(());
        }
        self.connection.execute(
            "INSERT OR IGNORE INTO sync_outbox(op_id,recipient_device_id)
             SELECT o.op_id,m.device_id FROM sync_operations o CROSS JOIN sync_members m
             WHERE o.op_id=?1 AND o.local=0 AND o.state IN ('applied','conflict')
               AND m.active=1 AND m.device_id!=?2 AND m.device_id!=o.origin_device_id",
            params![op_id, identity.device_id],
        )?;
        Ok(())
    }

    /// Upgrade old leaf-to-leaf intents to the channel that actually exists.
    /// The hub durably inventories local operations before acknowledging them.
    pub(crate) fn reconcile_sync_upstream_outbox(&self) -> Result<()> {
        if let Some(owner) = self.sync_upstream_device()? {
            let tx = self.begin_write_transaction()?;
            self.connection.execute(
                "INSERT OR IGNORE INTO sync_outbox(op_id,recipient_device_id)
                 SELECT op_id,?1 FROM sync_operations WHERE local=1",
                [&owner],
            )?;
            self.connection.execute(
                "DELETE FROM sync_outbox WHERE recipient_device_id!=?1
                 AND op_id IN (SELECT op_id FROM sync_operations WHERE local=1)",
                [&owner],
            )?;
            // Earlier versions sealed leaf-to-leaf business envelopes even
            // though no pair channel existed. Retire only that transport work;
            // local operations now have a durable owner delivery above.
            self.connection.execute(
                "DELETE FROM sync_operation_fragment_outbox
                 WHERE recipient_device_id!=?1 AND EXISTS(
                   SELECT 1 FROM sync_wire_outbox w
                   WHERE w.recipient_device_id=sync_operation_fragment_outbox.recipient_device_id
                     AND w.message_id=sync_operation_fragment_outbox.message_id AND w.state='pending'
                     AND NOT EXISTS(SELECT 1 FROM sync_snapshot_export_wires x
                       WHERE x.recipient_device_id=w.recipient_device_id AND x.message_id=w.message_id))",
                [&owner],
            )?;
            self.connection.execute(
                "DELETE FROM sync_wire_outbox AS w WHERE recipient_device_id!=?1 AND state='pending'
                 AND (message_id LIKE 'op-%' OR message_id LIKE 'provider:%'
                   OR message_id LIKE 'receipt-%' OR message_id LIKE 'envelope-receipt-%')
                 AND NOT EXISTS(SELECT 1 FROM sync_snapshot_export_wires x
                   WHERE x.recipient_device_id=w.recipient_device_id AND x.message_id=w.message_id)",
                [&owner],
            )?;
            if let Some(tx) = tx {
                tx.commit()?;
            }
        }
        Ok(())
    }
}
