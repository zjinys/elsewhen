//! 同步身份与授权：replica 初始化、成员授权/撤销、变更捕获入队。

use super::*;

impl Store {
    pub(crate) fn sync_identity(&self) -> Result<Option<ReplicaIdentity>> {
        Ok(self.connection.query_row(
            "SELECT dataset_id,device_id,stream_id,processor_device_id FROM sync_replica WHERE singleton=1",
            [], |r| Ok(ReplicaIdentity { dataset_id:r.get(0)?, device_id:r.get(1)?,
                stream_id:r.get(2)?, processor_device_id:r.get(3)? }),
        ).optional()?)
    }

    /// Called only after the pairing/restore layer establishes an installation identity.
    pub(crate) fn initialize_sync_replica(&self, identity: &ReplicaIdentity) -> Result<()> {
        for id in [
            &identity.dataset_id,
            &identity.device_id,
            &identity.stream_id,
            &identity.processor_device_id,
        ] {
            validate_id(id)?;
        }
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
        if let Some(existing) = self.sync_identity()? {
            ensure!(existing == *identity, "sync_replica_already_initialized");
        } else {
            self.require_background_quiescent_for_sync()?;
            self.connection.execute("INSERT INTO sync_replica(singleton,dataset_id,device_id,stream_id,processor_device_id) VALUES(1,?1,?2,?3,?4)",
                params![identity.dataset_id,identity.device_id,identity.stream_id,identity.processor_device_id])?;
            self.connection.execute(
                "INSERT INTO sync_members(device_id,stream_id) VALUES(?1,?2)",
                params![identity.device_id, identity.stream_id],
            )?;
            self.connection.execute(
                "INSERT INTO sync_frontiers(origin_stream_id) VALUES(?1)",
                [&identity.stream_id],
            )?;
            self.connection.execute("INSERT INTO sync_object_heads(object_kind,object_id,revision_id,deleted) SELECT 'conversation',id,'baseline:'||?1||':'||id,0 FROM conversations",[&identity.dataset_id])?;
            self.connection.execute("INSERT INTO sync_known_revisions SELECT object_kind,object_id,revision_id FROM sync_object_heads",[])?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// Membership control is separate from content operations. Never call this
    /// merely because an untrusted envelope declares a new origin.
    pub(crate) fn authorize_sync_member(&self, device: &str, stream: &str) -> Result<()> {
        validate_id(device)?;
        validate_id(stream)?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let tx = self.begin_write_transaction()?;
        if self.has_sync_authority()? {
            let granted:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM sync_member_keys WHERE device_id=?1 AND stream_id=?2 AND active=1)",params![device,stream],|r|r.get(0))?;
            ensure!(granted, "sync_member_signed_grant_required");
        }
        let existing: Option<String> = self
            .connection
            .query_row(
                "SELECT device_id FROM sync_members WHERE stream_id=?1",
                [stream],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            ensure!(existing == device, "sync_stream_identity_conflict");
            // Reauthorization is an explicit control operation, not content replay.
            self.connection.execute(
                "UPDATE sync_members SET active=1 WHERE stream_id=?1",
                [stream],
            )?;
        } else {
            self.connection.execute(
                "INSERT INTO sync_members(device_id,stream_id) VALUES(?1,?2)",
                params![device, stream],
            )?;
            self.connection.execute(
                "INSERT OR IGNORE INTO sync_frontiers(origin_stream_id) VALUES(?1)",
                [stream],
            )?;
        }
        if device != identity.device_id {
            self.connection.execute("INSERT OR IGNORE INTO sync_outbox(op_id,recipient_device_id) SELECT op_id,?1 FROM sync_operations WHERE local=1",[device])?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    pub(crate) fn revoke_sync_member(&self, device: &str) -> Result<()> {
        ensure!(
            !self.has_sync_authority()?,
            "sync_member_signed_revocation_required"
        );
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        ensure!(device != identity.device_id, "sync_cannot_revoke_self");
        self.connection.execute(
            "UPDATE sync_members SET active=0 WHERE device_id=?1",
            [device],
        )?;
        Ok(())
    }

    pub(crate) fn append_sync_mutation(&self, mutation: SyncMutation) -> Result<Option<String>> {
        let Some(identity) = self.sync_identity()? else {
            return Ok(None);
        };
        ensure!(
            !self.connection.is_autocommit(),
            "sync_requires_domain_transaction"
        );
        let seq: i64 = self.connection.query_row(
            "SELECT next_sequence FROM sync_replica WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        let next = seq.checked_add(1).context("sync_sequence_exhausted")?;
        let clock: i64 = self.connection.query_row(
            "SELECT logical_clock FROM sync_replica WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        let clock = clock.checked_add(1).context("sync_clock_exhausted")?;
        let operation = SyncOperation {
            protocol_version: PROTOCOL_VERSION,
            dataset_id: identity.dataset_id,
            origin_device_id: identity.device_id,
            origin_stream_id: identity.stream_id.clone(),
            origin_sequence: seq,
            logical_clock: clock,
            op_id: format!("{}:{seq}", identity.stream_id),
            mutation,
        };
        // Local capture must still succeed for a large record. Bounded transport
        // reports an oversized pending operation instead of discarding the input.
        operation.validate()?;
        let payload = serde_json::to_string(&operation)?;
        self.connection.execute("INSERT INTO sync_operations(op_id,origin_device_id,origin_stream_id,origin_sequence,payload,payload_hash,local,state) VALUES(?1,?2,?3,?4,?5,?6,1,'applied')",
            params![operation.op_id,operation.origin_device_id,operation.origin_stream_id,seq,payload,hash(payload.as_bytes())])?;
        self.connection.execute(
            "UPDATE sync_replica SET next_sequence=?1,logical_clock=?2 WHERE singleton=1",
            params![next, clock],
        )?;
        self.connection.execute(
            "UPDATE sync_frontiers SET applied_sequence=?2 WHERE origin_stream_id=?1",
            params![operation.origin_stream_id, seq],
        )?;
        self.connection.execute("INSERT INTO sync_outbox(op_id,recipient_device_id) SELECT ?1,device_id FROM sync_members WHERE active=1 AND device_id!=?2 GROUP BY device_id",params![operation.op_id,operation.origin_device_id])?;
        Ok(Some(operation.op_id))
    }

    pub(crate) fn sync_record_capture(&self, event_id: &str, input_id: Option<&str>) -> Result<()> {
        if self.sync_identity()?.is_none() {
            return Ok(());
        }
        let event = self
            .sync_event_record(event_id)?
            .context("sync_capture_event_missing")?;
        let input = input_id
            .map(|id| self.sync_input_record(id))
            .transpose()?
            .flatten();
        ensure!(
            input_id.is_none() || input.is_some(),
            "sync_capture_input_missing"
        );
        let message = input
            .as_ref()
            .and_then(|i| i.message_id.as_deref())
            .map(|id| self.sync_message_record(id))
            .transpose()?
            .flatten();
        if let Some(message) = &message {
            if !self.sync_is_syncable_conversation(&message.conversation_id)? {
                return Ok(());
            }
        }
        let op_id = self.append_sync_mutation(SyncMutation::Capture {
            event,
            input,
            message: message.clone(),
        })?;
        if let (Some(message), Some(op_id)) = (message, op_id) {
            self.sync_order_message(&message.id, &op_id)?;
        }
        Ok(())
    }

    pub(crate) fn sync_record_message(&self, message_id: &str) -> Result<()> {
        if self.sync_identity()?.is_none() {
            return Ok(());
        }
        let message = self
            .sync_message_record(message_id)?
            .context("sync_message_missing")?;
        if !self.sync_is_syncable_conversation(&message.conversation_id)? {
            return Ok(());
        }
        if let Some(op_id) = self.append_sync_mutation(SyncMutation::MessageAppended { message })? {
            self.sync_order_message(message_id, &op_id)?;
        }
        Ok(())
    }

    pub(crate) fn sync_record_conversation(&self, id: &str, deleted: bool) -> Result<()> {
        if self.sync_identity()?.is_none() || !self.sync_is_syncable_conversation(id)? {
            return Ok(());
        }
        let conversation = self
            .sync_conversation_record(id)?
            .context("sync_conversation_missing")?;
        if let Some(slug) = &conversation.wiki_page_slug {
            // Wiki 专属会话开放（S05）：引用锚指向的页面必须保证比会话 op
            // 先到达对端，否则对端按「等待补齐」会把同流后续 op 永久阻塞。
            // 页面 head 缺失（P5a 之前创建且从未再编辑过的页）时立即补发
            // 页面捕获，使页面 op 与会话 op 严格同流有序。
            let page_head_known: Option<bool> = self
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sync_object_heads WHERE object_kind='wiki_page' \
                     AND object_id=(SELECT id FROM wiki_pages WHERE slug=?1))",
                    [slug],
                    |r| r.get(0),
                )
                .optional()?;
            if page_head_known == Some(false) {
                let page_id: String = self
                    .connection
                    .query_row("SELECT id FROM wiki_pages WHERE slug=?1", [slug], |r| {
                        r.get(0)
                    })
                    .context("sync_conversation_slug_orphaned")?;
                self.sync_record_wiki_page(&page_id, false)?;
            }
        }
        let base_revision = self.sync_object_head("conversation", id)?.map(|h| h.0);
        let observed_message_ids = if deleted {
            let mut query = self
                .connection
                .prepare("SELECT id FROM messages WHERE conversation_id=?1 ORDER BY id")?;
            query
                .query_map([id], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        if let Some(op_id) = self.append_sync_mutation(SyncMutation::ConversationChanged {
            conversation,
            base_revision,
            deleted,
            observed_message_ids,
        })? {
            self.sync_set_head("conversation", id, &op_id, deleted)?;
        }
        Ok(())
    }
}
