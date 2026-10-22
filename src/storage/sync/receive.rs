//! 同步 received 侧：操作接收、等待重试、mutation 应用与依赖/排序。

use super::*;

impl Store {
    pub(crate) fn receive_sync_operation(
        &self,
        authenticated_device: &str,
        bytes: &[u8],
    ) -> Result<ReceiveReport> {
        let op = match SyncOperation::decode(bytes) {
            Ok(op) => op,
            // Unknown protocol version or structurally-invalid op: quarantine
            // per source (isolated) instead of aborting the receive, so a
            // single unsupported op from a real member never blocks the stream
            // and local capture is unaffected. We never apply what we cannot
            // fully validate; the stream is healed by upgrade + snapshot.
            Err(_) => {
                let identity = self.sync_identity()?.context("sync_not_initialized")?;
                if let Ok((dataset, device, stream, seq, op_id)) =
                    SyncOperation::identity_header(bytes)
                {
                    if dataset == identity.dataset_id && device == authenticated_device {
                        let tx = self.begin_write_transaction()?;
                        self.require_local_sync_membership()?;
                        let active: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND stream_id=?2 AND active=1)",params![authenticated_device,&stream],|r|r.get(0))?;
                        if active && stream != identity.stream_id {
                            let digest = hash(bytes);
                            let payload = String::from_utf8_lossy(bytes).into_owned();
                            self.connection.execute(
                                "INSERT INTO sync_operations(op_id,origin_device_id,origin_stream_id,origin_sequence,payload,payload_hash,local,state,error_code) \
                                 VALUES(?1,?2,?3,?4,?5,?6,0,'isolated','sync_protocol_unsupported') \
                                 ON CONFLICT(op_id) DO UPDATE SET state='isolated',error_code='sync_protocol_unsupported'",
                                params![op_id,device,stream,seq,payload,digest],
                            )?;
                        }
                        let report = self.sync_drain_waiting()?;
                        if let Some(tx) = tx {
                            tx.commit()?;
                        }
                        return Ok(report);
                    }
                }
                // Not attributable to an authenticated member of this dataset:
                // keep the hard rejection (wrong dataset / spoofed sender / junk).
                SyncOperation::decode(bytes)?;
                anyhow::bail!("sync_decode_unreachable")
            }
        };
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        ensure!(op.dataset_id == identity.dataset_id, "sync_wrong_dataset");
        ensure!(
            op.origin_device_id == authenticated_device,
            "sync_sender_mismatch"
        );
        let tx = self.begin_write_transaction()?;
        self.require_local_sync_membership()?;
        let active:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND stream_id=?2 AND active=1)",params![authenticated_device,op.origin_stream_id],|r|r.get(0))?;
        ensure!(active, "sync_member_not_authorized");
        let local_clock: i64 = self.connection.query_row(
            "SELECT logical_clock FROM sync_replica WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        let payload = serde_json::to_string(&op)?;
        let digest = hash(payload.as_bytes());
        // A clock too far ahead means the peer is far ahead of us (long-offline
        // / snapshot-needed): quarantine this op per source and let the frontier
        // advance past it, rather than failing the whole receive. The stream is
        // re-synced from a later snapshot; we never apply out of a trusted base.
        if op.logical_clock > local_clock.saturating_add(1_000_000) {
            self.connection.execute(
                "INSERT INTO sync_operations(op_id,origin_device_id,origin_stream_id,origin_sequence,payload,payload_hash,local,state,error_code) \
                 VALUES(?1,?2,?3,?4,?5,?6,0,'isolated','sync_clock_requires_snapshot') \
                 ON CONFLICT(op_id) DO NOTHING",
                params![op.op_id,op.origin_device_id,op.origin_stream_id,op.origin_sequence,payload,digest],
            )?;
            let report = self.sync_drain_waiting()?;
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(report);
        }
        let existing:Option<(String,String)>=self.connection.query_row("SELECT op_id,payload_hash FROM sync_operations WHERE op_id=?1 OR (origin_stream_id=?2 AND origin_sequence=?3)",params![op.op_id,op.origin_stream_id,op.origin_sequence],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((id, existing_hash)) = existing {
            if id == op.op_id && existing_hash == digest {
                // Exact replay of an already-recorded op: fall through to the
                // shared drain + receipt so a lost acknowledgement is recreated.
            } else {
                // Same identity (op_id or stream+sequence) with different bytes:
                // quarantine this op per source instead of aborting the whole
                // receive. Persist the isolated row FIRST (flushed before the
                // drain reads), so the drain's NOT EXISTS guard sees seq as
                // undecidable and does not apply the divergent payload.
                self.connection.execute(
                    "INSERT INTO sync_operations(op_id,origin_device_id,origin_stream_id,origin_sequence,payload,payload_hash,local,state,error_code) \
                     VALUES(?1,?2,?3,?4,?5,?6,0,'isolated','sync_operation_content_conflict') \
                     ON CONFLICT(op_id) DO UPDATE SET state='isolated',error_code='sync_operation_content_conflict'",
                    params![op.op_id,op.origin_device_id,op.origin_stream_id,op.origin_sequence,payload,digest],
                )?;
                if let Some(tx) = tx {
                    tx.commit()?;
                }
                // Drain AFTER the isolated row is durable: other streams (and any
                // already-decidable ops) still make progress.
                let report = self.sync_drain_waiting()?;
                return Ok(report);
            }
        } else {
            let checkpoint:Option<(String,String,String)>=self.connection.query_row("SELECT op_id,payload_hash,origin_device_id FROM sync_checkpoint_operations WHERE op_id=?1 OR (origin_stream_id=?2 AND origin_sequence=?3)",params![op.op_id,op.origin_stream_id,op.origin_sequence],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            if let Some((id, hash, device)) = checkpoint {
                ensure!(
                    id == op.op_id && hash == digest && device == op.origin_device_id,
                    "sync_operation_content_conflict"
                );
                self.connection.execute("INSERT INTO sync_receipts(op_id,target_device_id,payload_hash) VALUES(?1,?2,?3) ON CONFLICT(op_id) DO UPDATE SET pending=1",params![op.op_id,op.origin_device_id,digest])?;
                if let Some(tx) = tx {
                    tx.commit()?;
                }
                return Ok(ReceiveReport::default());
            }
            ensure!(
                op.origin_stream_id != identity.stream_id,
                "sync_local_stream_spoof"
            );
            let (queued, queued_bytes): (i64,i64) = self.connection.query_row(
                "SELECT COUNT(*),COALESCE(SUM(length(CAST(payload AS BLOB))),0) FROM sync_operations WHERE origin_device_id=?1 AND state IN ('waiting','rejected')",
                [authenticated_device], |r| Ok((r.get(0)?,r.get(1)?)),
            )?;
            ensure!(
                queued < 1000 && queued_bytes + payload.len() as i64 <= 32 * 1024 * 1024,
                "sync_inbox_source_limit"
            );
            self.connection.execute("INSERT INTO sync_operations(op_id,origin_device_id,origin_stream_id,origin_sequence,payload,payload_hash,local,state) VALUES(?1,?2,?3,?4,?5,?6,0,'waiting')",params![op.op_id,op.origin_device_id,op.origin_stream_id,op.origin_sequence,payload,digest])?;
        }
        let report = self.sync_drain_waiting()?;
        self.queue_sync_receipt(&op.op_id)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(report)
    }

    pub(crate) fn sync_operation_durable(&self, op_id: &str) -> Result<bool> {
        Ok(self.connection.query_row("SELECT EXISTS(SELECT 1 FROM sync_operations WHERE op_id=?1 AND state IN ('applied','conflict') UNION ALL SELECT 1 FROM sync_checkpoint_operations WHERE op_id=?1)",[op_id],|r|r.get(0))?)
    }

    pub(crate) fn retry_waiting_sync_operations(&self) -> Result<ReceiveReport> {
        let tx = self.connection.unchecked_transaction()?;
        let report = self.sync_drain_waiting()?;
        tx.commit()?;
        Ok(report)
    }

    pub(crate) fn sync_drain_waiting(&self) -> Result<ReceiveReport> {
        let mut report = ReceiveReport::default();
        for _ in 0..MAX_BATCH_OPERATIONS {
            // Note: an 'isolated' op is NEVER advanced past. Its stream's
            // frontier stays put, so later same-stream ops remain 'waiting'
            // until the divergence is resolved (upgrade + snapshot / manual
            // repair). We never silently apply one of two divergent payloads.
            // Other streams drain independently, and local capture is unaffected.
            let operation_ids = {
                // Drain the next in-order waiting op. An 'isolated' op is never
                // returned here (its state is not 'waiting'), so a quarantined
                // divergence is never applied. The frontier only advances through
                // ops that were genuinely applied, so the real chain continues.
                let mut q=self.connection.prepare("SELECT o.op_id FROM sync_operations o JOIN sync_frontiers f ON f.origin_stream_id=o.origin_stream_id JOIN sync_members m ON m.stream_id=o.origin_stream_id AND m.device_id=o.origin_device_id WHERE o.state='waiting' AND o.origin_sequence=f.applied_sequence+1 AND m.active=1 ORDER BY o.origin_stream_id LIMIT 100")?;
                q.query_map([], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            };
            let mut progressed = false;
            for op_id in operation_ids {
                let payload: String = self.connection.query_row(
                    "SELECT payload FROM sync_operations WHERE op_id=?1",
                    [&op_id],
                    |r| r.get(0),
                )?;
                let op = SyncOperation::decode(payload.as_bytes())?;
                self.connection.execute_batch("SAVEPOINT sync_apply")?;
                let result = self.apply_sync_mutation(&op);
                match result {
                    Ok(Apply::Waiting) => self
                        .connection
                        .execute_batch("ROLLBACK TO sync_apply; RELEASE sync_apply")?,
                    Err(_) => {
                        self.connection
                            .execute_batch("ROLLBACK TO sync_apply; RELEASE sync_apply")?;
                        self.connection.execute("UPDATE sync_operations SET state='rejected',error_code='domain_validation_failed' WHERE op_id=?1",[&op.op_id])?;
                        report.rejected += 1;
                    }
                    Ok(result) => {
                        self.connection.execute_batch("RELEASE sync_apply")?;
                        let state = match result {
                            Apply::Applied => {
                                report.applied += 1;
                                "applied"
                            }
                            Apply::Conflict {
                                kind,
                                id,
                                revision,
                                reason,
                            } => {
                                self.connection.execute("INSERT INTO sync_conflicts(op_id,object_kind,object_id,current_revision,reason) VALUES(?1,?2,?3,?4,?5)",params![op.op_id,kind,id,revision,reason])?;
                                report.conflicts += 1;
                                "conflict"
                            }
                            Apply::Waiting => unreachable!(),
                        };
                        self.connection.execute(
                            "UPDATE sync_operations SET state=?2 WHERE op_id=?1",
                            params![op.op_id, state],
                        )?;
                        self.connection.execute("UPDATE sync_replica SET logical_clock=MAX(logical_clock,?1) WHERE singleton=1",[op.logical_clock])?;
                        self.connection.execute("UPDATE sync_frontiers SET applied_sequence=?2 WHERE origin_stream_id=?1",params![op.origin_stream_id,op.origin_sequence])?;
                        self.queue_sync_receipt(&op.op_id)?;
                        progressed = true;
                    }
                }
                if report.applied + report.conflicts + report.rejected >= MAX_BATCH_OPERATIONS {
                    break;
                }
            }
            if !progressed
                || report.applied + report.conflicts + report.rejected >= MAX_BATCH_OPERATIONS
            {
                break;
            }
        }
        report.waiting = usize::try_from(self.connection.query_row(
            "SELECT count(*) FROM sync_operations WHERE state='waiting'",
            [],
            |r| r.get::<_, i64>(0),
        )?)?;
        Ok(report)
    }

    pub(crate) fn apply_sync_mutation(&self, op: &SyncOperation) -> Result<Apply> {
        match &op.mutation {
            SyncMutation::Capture {
                event,
                input,
                message,
            } => {
                let mut conflict = None;
                if let Some(message) = message {
                    match self.sync_message_dependencies(message)? {
                        Apply::Applied => {}
                        Apply::Waiting => return Ok(Apply::Waiting),
                        other => conflict = Some(other),
                    }
                }
                if let Some(existing) = self.sync_event_record(&event.id)? {
                    ensure!(existing == *event, "sync_event_content_conflict");
                } else {
                    crate::storage::events::insert_raw_event_on(
                        &self.connection,
                        &event.id,
                        &event.occurred_at,
                        &event.recorded_at,
                        &event.raw_text,
                        &event.source,
                        &event.created_at,
                    )?;
                    self.sync_enqueue_imported_event(&event.id)?;
                }
                if conflict.is_none() {
                    if let Some(message) = message {
                        self.sync_insert_message(message, op)?;
                    }
                }
                // Conversation deletion can conflict with the message, not with
                // the immutable fact. Keep the original linkage in the conflict
                // operation and project only its usable event link into input_records.
                let projected = input.as_ref().map(|input| {
                    let mut input = input.clone();
                    if conflict.is_some() {
                        input.message_id = None;
                    }
                    input
                });
                if let Some(input) = projected.as_ref() {
                    if let Some(existing) = self.sync_input_record(&input.id)? {
                        ensure!(existing == *input, "sync_input_content_conflict");
                    } else {
                        self.connection.execute("INSERT INTO input_records(id,raw_text,source,route_status,event_id,message_id,idempotency_key,created_at,updated_at) VALUES(?1,?2,?3,'routed',?4,?5,?6,?7,?7)",params![input.id,input.raw_text,input.source,input.event_id,input.message_id,input.idempotency_key,input.created_at])?;
                    }
                }
                Ok(conflict.unwrap_or(Apply::Applied))
            }
            SyncMutation::AnalysisCompleted { analysis } => self.apply_sync_analysis(op, analysis),
            SyncMutation::MessageAppended { message } => {
                match self.sync_message_dependencies(message)? {
                    Apply::Applied => {}
                    other => return Ok(other),
                }
                self.sync_insert_message(message, op)?;
                Ok(Apply::Applied)
            }
            SyncMutation::ConversationChanged {
                conversation,
                base_revision,
                deleted,
                observed_message_ids,
            } => {
                let head = self.sync_object_head("conversation", &conversation.id)?;
                if let Some(base) = base_revision {
                    let known: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM sync_operations WHERE op_id=?1 AND state IN ('applied','conflict') UNION ALL SELECT 1 FROM sync_known_revisions WHERE object_kind='conversation' AND object_id=?2 AND revision_id=?1)", params![base,conversation.id], |r| r.get(0))?;
                    if !known && head.as_ref().map(|h| &h.0) != Some(base) {
                        return Ok(Apply::Waiting);
                    }
                }
                if head.as_ref().map(|h| h.0.as_str()) != base_revision.as_deref() {
                    return Ok(Apply::Conflict {
                        kind: "conversation",
                        id: conversation.id.clone(),
                        revision: head.map(|h| h.0),
                        reason: "base_revision_changed",
                    });
                }
                if head.as_ref().is_some_and(|h| h.1) {
                    return Ok(Apply::Conflict {
                        kind: "conversation",
                        id: conversation.id.clone(),
                        revision: head.map(|h| h.0),
                        reason: "conversation_deleted",
                    });
                }
                if *deleted {
                    let observed: std::collections::HashSet<_> =
                        observed_message_ids.iter().collect();
                    let mut query = self
                        .connection
                        .prepare("SELECT id FROM messages WHERE conversation_id=?1")?;
                    let current = query
                        .query_map([&conversation.id], |r| r.get::<_, String>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    if current.iter().any(|id| !observed.contains(id)) {
                        return Ok(Apply::Conflict {
                            kind: "conversation",
                            id: conversation.id.clone(),
                            revision: head.map(|h| h.0),
                            reason: "delete_has_unseen_messages",
                        });
                    }
                    // Existing business rules and raw-event preservation still apply.
                    let archived: Option<bool> = self
                        .connection
                        .query_row(
                            "SELECT archived FROM conversations WHERE id=?1",
                            [&conversation.id],
                            |r| r.get(0),
                        )
                        .optional()?;
                    ensure!(archived == Some(true), "sync_delete_requires_archive");
                    // input_records retain the immutable event even after chat removal.
                    self.connection.execute("UPDATE input_records SET message_id=NULL WHERE message_id IN (SELECT id FROM messages WHERE conversation_id=?1)",[&conversation.id])?;
                    self.connection
                        .execute("DELETE FROM conversations WHERE id=?1", [&conversation.id])?;
                } else if self.sync_conversation_record(&conversation.id)?.is_some() {
                    ensure!(head.is_some(), "sync_existing_conversation_has_no_base");
                    self.connection.execute("UPDATE conversations SET title=?2,tag=?3,archived=?4,updated_at=?5 WHERE id=?1",params![conversation.id,conversation.title,conversation.tag,conversation.archived,conversation.updated_at])?;
                } else {
                    ensure!(base_revision.is_none(), "sync_conversation_base_missing");
                    if let Some(slug) = &conversation.wiki_page_slug {
                        // Wiki 专属会话的引用锚：页面未在本端真实出现过 →
                        // 等待补齐，不推断；同名新页不得自动收养旧引用
                        // （S05）。足迹含已物理删除的页面（head 仍留档）。
                        let page_known: bool = self.connection.query_row(
                            "SELECT EXISTS(SELECT 1 FROM sync_wiki_slug_seen WHERE slug=?1)",
                            [slug],
                            |r| r.get(0),
                        )?;
                        if !page_known {
                            return Ok(Apply::Waiting);
                        }
                    }
                    self.connection.execute("INSERT INTO conversations(id,title,tag,archived,wiki_page_slug,assistant_mode,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![conversation.id,conversation.title,conversation.tag,conversation.archived,conversation.wiki_page_slug,conversation.assistant_mode,conversation.created_at,conversation.updated_at])?;
                }
                self.sync_set_head("conversation", &conversation.id, &op.op_id, *deleted)?;
                Ok(Apply::Applied)
            }
            SyncMutation::WikiPageChanged {
                page,
                base_revision,
                deleted,
            } => self.apply_sync_wiki_page(op, page, base_revision, *deleted),
            SyncMutation::WikiPageRenamed {
                old_slug,
                page,
                base_revision,
            } => self.apply_sync_wiki_rename(op, old_slug, page, base_revision),
            SyncMutation::WikiPageRevisionAppended { revision } => {
                self.apply_sync_wiki_revision(op, revision)
            }
            SyncMutation::TodoChanged {
                todo,
                base_revision,
                deleted,
            } => self.apply_sync_todo(op, todo, base_revision, *deleted),
            SyncMutation::HabitNudgeChanged {
                nudge,
                base_revision,
                deleted,
            } => self.apply_sync_habit_nudge(op, nudge, base_revision, *deleted),
            SyncMutation::GoalChanged {
                goal,
                base_revision,
                deleted,
            } => self.apply_sync_goal(op, goal, base_revision, *deleted),
            SyncMutation::RuleChanged {
                rule,
                base_revision,
                deleted,
            } => self.apply_sync_rule(op, rule, base_revision, *deleted),
            SyncMutation::KnowledgeReviewAppended { review } => {
                self.apply_sync_knowledge_review(review)
            }
            SyncMutation::KnowledgeReadingStateChanged { state } => {
                self.apply_sync_knowledge_reading_state(state)
            }
        }
    }

    pub(crate) fn sync_message_dependencies(&self, message: &MessageRecord) -> Result<Apply> {
        if let Some((revision, true)) =
            self.sync_object_head("conversation", &message.conversation_id)?
        {
            return Ok(Apply::Conflict {
                kind: "conversation",
                id: message.conversation_id.clone(),
                revision: Some(revision),
                reason: "message_after_delete",
            });
        }
        if !self.sync_is_syncable_conversation(&message.conversation_id)? {
            return Ok(Apply::Waiting);
        }
        if let Some(parent) = &message.parent_message_id {
            let Some(parent) = self.sync_message_record(parent)? else {
                return Ok(Apply::Waiting);
            };
            ensure!(
                parent.conversation_id == message.conversation_id,
                "sync_parent_wrong_conversation"
            );
        }
        if let Some(input) = &message.input_message_id {
            let Some(input) = self.sync_message_record(input)? else {
                return Ok(Apply::Waiting);
            };
            ensure!(
                input.role == "user" && input.conversation_id == message.conversation_id,
                "sync_response_input_invalid"
            );
        }
        Ok(Apply::Applied)
    }

    pub(crate) fn sync_insert_message(
        &self,
        message: &MessageRecord,
        op: &SyncOperation,
    ) -> Result<()> {
        if let Some(existing) = self.sync_message_record(&message.id)? {
            ensure!(existing == *message, "sync_message_content_conflict");
            return Ok(());
        }
        self.connection.execute("INSERT INTO messages(id,conversation_id,parent_message_id,role,content,created_at,kind,ref) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![message.id,message.conversation_id,message.parent_message_id,message.role,message.content,message.created_at,message.kind,message.message_ref])?;
        if let Some(input) = &message.input_message_id {
            self.connection.execute(
                "INSERT INTO message_response_links(message_id,input_message_id) VALUES(?1,?2)",
                params![message.id, input],
            )?;
        }
        self.connection.execute("INSERT INTO sync_message_order(message_id,origin_stream_id,origin_sequence,logical_clock,input_message_id) VALUES(?1,?2,?3,?4,?5)",params![message.id,op.origin_stream_id,op.origin_sequence,op.logical_clock,message.input_message_id])?;
        self.connection.execute(
            "UPDATE conversations SET updated_at=MAX(updated_at,?2) WHERE id=?1",
            params![message.conversation_id, message.created_at],
        )?;
        Ok(())
    }

    pub(crate) fn sync_order_message(&self, id: &str, op_id: &str) -> Result<()> {
        self.connection.execute("INSERT INTO sync_message_order(message_id,origin_stream_id,origin_sequence,logical_clock,input_message_id) SELECT ?1,origin_stream_id,origin_sequence,json_extract(payload,'$.logical_clock'),(SELECT input_message_id FROM message_response_links WHERE message_id=?1) FROM sync_operations WHERE op_id=?2",params![id,op_id])?;
        Ok(())
    }

    pub(crate) fn sync_object_head(&self, kind: &str, id: &str) -> Result<Option<(String, bool)>> {
        Ok(self.connection.query_row("SELECT revision_id,deleted FROM sync_object_heads WHERE object_kind=?1 AND object_id=?2",params![kind,id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?)
    }

    pub(crate) fn sync_set_head(
        &self,
        kind: &str,
        id: &str,
        revision: &str,
        deleted: bool,
    ) -> Result<()> {
        self.connection.execute("INSERT INTO sync_object_heads(object_kind,object_id,revision_id,deleted) VALUES(?1,?2,?3,?4) ON CONFLICT(object_kind,object_id) DO UPDATE SET revision_id=excluded.revision_id,deleted=excluded.deleted",params![kind,id,revision,deleted])?;
        self.connection.execute("INSERT OR IGNORE INTO sync_known_revisions(object_kind,object_id,revision_id) VALUES(?1,?2,?3)",params![kind,id,revision])?;
        Ok(())
    }

    /// 任何会话行都可同步（S05 起包含 Wiki 专属会话）；行不存在 → Waiting
    /// （会话 op 可能还在途）。
    pub(crate) fn sync_is_syncable_conversation(&self, id: &str) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1)",
            [id],
            |r| r.get(0),
        )?)
    }

    pub(crate) fn sync_event_record(&self, id: &str) -> Result<Option<EventRecord>> {
        Ok(self.connection.query_row("SELECT id,occurred_at,recorded_at,raw_text,source,created_at FROM events WHERE id=?1",[id],|r|Ok(EventRecord{id:r.get(0)?,occurred_at:r.get(1)?,recorded_at:r.get(2)?,raw_text:r.get(3)?,source:r.get(4)?,created_at:r.get(5)?})).optional()?)
    }

    pub(crate) fn sync_input_record(&self, id: &str) -> Result<Option<InputRecord>> {
        Ok(self.connection.query_row("SELECT id,raw_text,source,event_id,message_id,idempotency_key,created_at FROM input_records WHERE id=?1 AND event_id IS NOT NULL",[id],|r|Ok(InputRecord{id:r.get(0)?,raw_text:r.get(1)?,source:r.get(2)?,event_id:r.get(3)?,message_id:r.get(4)?,idempotency_key:r.get(5)?,created_at:r.get(6)?})).optional()?)
    }

    pub(crate) fn sync_message_record(&self, id: &str) -> Result<Option<MessageRecord>> {
        Ok(self.connection.query_row("SELECT m.id,m.conversation_id,m.parent_message_id,m.role,m.content,m.created_at,l.input_message_id,m.kind,m.ref FROM messages m LEFT JOIN message_response_links l ON l.message_id=m.id WHERE m.id=?1",[id],|r|Ok(MessageRecord{id:r.get(0)?,conversation_id:r.get(1)?,parent_message_id:r.get(2)?,role:r.get(3)?,content:r.get(4)?,created_at:r.get(5)?,input_message_id:r.get(6)?,kind:r.get(7)?,message_ref:r.get(8)?})).optional()?)
    }

    pub(crate) fn sync_conversation_record(&self, id: &str) -> Result<Option<ConversationRecord>> {
        Ok(self.connection.query_row("SELECT id,title,tag,archived,wiki_page_slug,assistant_mode,created_at,updated_at FROM conversations WHERE id=?1",[id],|r|Ok(ConversationRecord{id:r.get(0)?,title:r.get(1)?,tag:r.get(2)?,archived:r.get(3)?,wiki_page_slug:r.get(4)?,assistant_mode:r.get(5)?,created_at:r.get(6)?,updated_at:r.get(7)?})).optional()?)
    }
}
