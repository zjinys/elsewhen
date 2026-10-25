//! Durable encrypted delivery attempts. Relay upload success is deliberately
//! not an application acknowledgement and never removes the source operation.

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use crate::storage::Store;
use crate::sync::crypto::{self, ContentKey, DeviceIdentity, EnvelopeHeader};

pub(crate) struct PendingWireEnvelope {
    pub message_id: String,
    pub encrypted_envelope: Vec<u8>,
    /// relay 侧投递幂等键（明文、随 sender 签名上行）。同一逻辑投递的重发必须
    /// 保持同值，不同投递必须互不相同——详见 `migrations/59-sync-wire-seq-hint`。
    pub seq_hint: i64,
}

impl Store {
    pub(crate) fn queue_sync_wire_receipt(
        &self,
        target: &str,
        message_id: &str,
        payload_hash: &str,
    ) -> Result<()> {
        ensure!(
            !self.connection.is_autocommit(),
            "sync_requires_domain_transaction"
        );
        let existing:Option<String>=self.connection.query_row("SELECT payload_hash FROM sync_wire_receipts WHERE target_device_id=?1 AND message_id=?2",params![target,message_id],|r|r.get(0)).optional()?;
        ensure!(
            existing.as_deref().is_none_or(|hash| hash == payload_hash),
            "sync_wire_receipt_content_conflict"
        );
        self.connection.execute("INSERT INTO sync_wire_receipts(target_device_id,message_id,payload_hash) VALUES(?1,?2,?3) ON CONFLICT(target_device_id,message_id) DO UPDATE SET pending=1",params![target,message_id,payload_hash])?;
        Ok(())
    }

    pub(crate) fn pending_sync_wire_receipts(
        &self,
        target: &str,
        limit: usize,
    ) -> Result<Vec<crate::sync::protocol::EnvelopeReceipt>> {
        ensure!((1..=100).contains(&limit), "sync_batch_limit_invalid");
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let mut query=self.connection.prepare("SELECT message_id,payload_hash FROM sync_wire_receipts WHERE target_device_id=?1 AND pending=1 AND EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1) ORDER BY message_id LIMIT ?2")?;
        Ok(query
            .query_map(params![target, limit as i64], |r| {
                Ok(crate::sync::protocol::EnvelopeReceipt {
                    dataset_id: identity.dataset_id.clone(),
                    acknowledging_device_id: identity.device_id.clone(),
                    target_device_id: target.to_string(),
                    message_id: r.get(0)?,
                    payload_hash: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub(crate) fn mark_sync_wire_receipt_sent(&self, target: &str, message_id: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE sync_wire_receipts SET pending=0 WHERE target_device_id=?1 AND message_id=?2",
            params![target, message_id],
        )?;
        Ok(())
    }

    /// P4e 名册收敛：该成员是否仍需要这条名册信封。outbox 记录存在且
    /// 已确认（对端回执）⇒ 无需再投；在途或不存在 ⇒ 允许补发/重试。
    pub(crate) fn membership_envelope_needed(
        &self,
        recipient: &str,
        message_id: &str,
    ) -> Result<bool> {
        ensure!(
            !self.connection.is_autocommit(),
            "sync_requires_domain_transaction"
        );
        let state: Option<String> = self
            .connection
            .query_row(
                "SELECT state FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id=?2",
                params![recipient, message_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(state.is_none())
    }

    pub(crate) fn rearm_sync_wire_receipt(&self, target: &str, message_id: &str) -> Result<()> {
        self.connection.execute("UPDATE sync_wire_outbox SET state='pending',available_at_ms=0 WHERE recipient_device_id=?1 AND message_id=?2 AND state='acknowledged'",params![target,message_id])?;
        Ok(())
    }

    pub(crate) fn stage_sync_wire_envelope(
        &self,
        header: &EnvelopeHeader,
        key: &ContentKey,
        signer: &DeviceIdentity,
        payload: &[u8],
    ) -> Result<Vec<u8>> {
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        ensure!(
            header.dataset_id == identity.dataset_id
                && header.sender_device_id == identity.device_id,
            "sync_wire_sender_mismatch"
        );
        ensure!(
            header.key_version <= i64::MAX as u64,
            "sync_wire_key_version_invalid"
        );
        let digest = super::hash(payload);
        let tx = self.begin_write_transaction()?;
        self.require_sync_signer(signer)?;
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
            [&header.recipient_device_id],
            |r| r.get(0),
        )?;
        ensure!(active, "sync_member_not_authorized");
        let existing: Option<(String, i64, Vec<u8>, String)> = self.connection.query_row("SELECT payload_hash,key_version,encrypted_envelope,state FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id=?2",params![header.recipient_device_id,header.message_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        if let Some((hash, version, bytes, state)) = existing {
            ensure!(
                header.key_version >= version as u64,
                "sync_wire_key_version_stale"
            );
            if header.key_version == version as u64 {
                if state == "acknowledged" {
                    // 已确认信封不可变。但密文含随机 nonce：同一明文重封密文必变，
                    // 而确认过的语义是「对方已拿到这份内容」，重发不携带新信息。
                    // 信封 id 自带内容版本（provider:{revision} / op hash），明文
                    // 一旦漂移不可能撞同 id —— 因此同 id + 不同密文＝同一内容的
                    // 确定性重发，按幂等处理并留审计日志，不再堵死发件队列。
                    if hash != digest {
                        // 走同步投递的统一日志格式（单一门控 + 四字段）：
                        // 第三个槽位是已存密文长度，两个摘要在 detail 里。
                        crate::sync::engine::deliver::wire_log(
                            "wire-reseal-skipped",
                            &header.message_id,
                            bytes.len(),
                            &format!(
                                "recipient {} stored {} incoming {}",
                                header.recipient_device_id, hash, digest
                            ),
                        );
                    }
                    if let Some(tx) = tx {
                        tx.commit()?;
                    }
                    return Ok(bytes);
                }
                if hash == digest {
                    // 同内容幂等短路：返回已封密文，不再重新 seal。
                    if let Some(tx) = tx {
                        tx.commit()?;
                    }
                    return Ok(bytes);
                }
                // pending（收端从未确认过旧内容）允许覆盖重封：配对重置/
                // 快照重建会让同 stream:seq 的 op 换 payload，若拒绝重封，
                // 死行会永久堵住 outbox 并阻断其后全部 op。ack 行在上方
                // 已被 ensure 挡下，铁律不变。
            }
        }
        let encrypted = crypto::seal(header.clone(), key, signer, payload)?.to_bytes()?;
        // seq_hint 只在**首次插入**时发号，`ON CONFLICT` 不覆写：重发必须带同一个
        // 号，relay 才能认出「这是同一封信」并替换旧副本而不是再插一份。
        // 重封 = 新内容、新事实：旧的 `blocked_reason` / `last_error` 说的是**上一份**
        // payload 的病（可能是配额、可能是 413），原样留着会让面板一直报一个
        // 已经不成立的阻塞原因（m3）。整行状态本来就回到「没上过 relay」，
        // 与它一致。
        let seq: i64 = self.connection.query_row(
            "UPDATE sync_wire_seq SET next=next+1 WHERE singleton=1 RETURNING next-1",
            [],
            |r| r.get(0),
        )?;
        self.connection.execute("INSERT INTO sync_wire_outbox(recipient_device_id,message_id,payload_hash,key_version,encrypted_envelope,seq_hint) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(recipient_device_id,message_id) DO UPDATE SET payload_hash=excluded.payload_hash,key_version=excluded.key_version,encrypted_envelope=excluded.encrypted_envelope,available_at_ms=0,relay_id=NULL,blocked_reason=NULL,last_error=''",params![header.recipient_device_id,header.message_id,digest,header.key_version as i64,encrypted,seq])?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(encrypted)
    }

    pub(crate) fn due_sync_wire_envelopes(
        &self,
        peer: &str,
        now_ms: i64,
        limit: usize,
    ) -> Result<Vec<PendingWireEnvelope>> {
        ensure!((1..=16).contains(&limit), "sync_wire_batch_limit_invalid");
        self.require_local_sync_membership()?;
        let mut query=self.connection.prepare("SELECT message_id,encrypted_envelope,seq_hint FROM sync_wire_outbox w WHERE recipient_device_id=?1 AND state='pending' AND available_at_ms<=?2 AND EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1) AND NOT EXISTS(SELECT 1 FROM sync_snapshot_export_wires x JOIN sync_snapshot_exports s ON s.id=x.snapshot_id WHERE x.recipient_device_id=w.recipient_device_id AND x.message_id=w.message_id AND s.state!='ready') ORDER BY available_at_ms,rowid LIMIT ?3")?;
        let rows = query.query_map(params![peer, now_ms, limit as i64], |r| {
            Ok(PendingWireEnvelope {
                message_id: r.get(0)?,
                encrypted_envelope: r.get(1)?,
                seq_hint: r.get(2)?,
            })
        })?;
        let mut result = Vec::new();
        let mut bytes = 0;
        for row in rows {
            let row = row?;
            ensure!(
                row.encrypted_envelope.len() <= crypto::MAX_ENVELOPE_BYTES,
                "sync_stored_wire_too_large"
            );
            if bytes + row.encrypted_envelope.len() > 8 * 1024 * 1024 {
                break;
            }
            bytes += row.encrypted_envelope.len();
            result.push(row);
        }
        Ok(result)
    }

    /// 取一封指定的 pending 信封：不参与任何「等导出就绪」的过滤。
    ///
    /// 只给 `snapshot-start` 这种**导出开始前就要到**的信令用：它在导出还没
    /// ready 时就必须发出去，否则接收端无从知道该去哪个 blob 拉片，窗口流控
    /// 会双边互等（发送方等 ack，接收方等 manifest）。
    pub(crate) fn pending_wire_envelope(
        &self,
        peer: &str,
        message_id: &str,
    ) -> Result<Option<PendingWireEnvelope>> {
        self.require_local_sync_membership()?;
        Ok(self
            .connection
            .query_row(
                "SELECT message_id,encrypted_envelope,seq_hint FROM sync_wire_outbox
                 WHERE recipient_device_id=?1 AND message_id=?2 AND state='pending'",
                params![peer, message_id],
                |r| {
                    Ok(PendingWireEnvelope {
                        message_id: r.get(0)?,
                        encrypted_envelope: r.get(1)?,
                        seq_hint: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    /// 该对端是否还有待发的快照导出线（初次交接/重快照在途）。此时放宽每拍
    /// 发送预算，让初始同步在一分钟内完成，而不是拖上半小时（见 engine.rs）。
    pub(crate) fn has_pending_snapshot_export_wires(&self, peer: &str) -> Result<bool> {
        self.require_local_sync_membership()?;
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_snapshot_export_wires x JOIN sync_wire_outbox w ON w.recipient_device_id=x.recipient_device_id AND w.message_id=x.message_id WHERE x.recipient_device_id=?1 AND w.state='pending')",
            [peer],
            |r| r.get(0),
        )?)
    }

    /// Enrollment explicitly selects only its control messages and, after the
    /// selection receipt, its snapshot. Pending business deltas cannot slip in.
    pub(crate) fn due_enrollment_wires(
        &self,
        peer: &str,
        now_ms: i64,
        messages: &[String],
        snapshot: Option<&str>,
    ) -> Result<Vec<PendingWireEnvelope>> {
        ensure!(
            messages.len() <= 8
                && messages.iter().all(|id| id.len() <= 240
                    && (id.starts_with("membership:")
                        || id.starts_with("enrollment-")
                        || id.starts_with("envelope-receipt-"))),
            "sync_enrollment_wire_scope_invalid"
        );
        self.require_local_sync_membership()?;
        // 与 engine::push_due 同一套预算：日常控制消息每拍 ≤3 封足够，但初次
        // 交接的快照线有几百片，3 封/拍会让 174 片拖上十几分钟（用户看到的就是
        // 「一次只同步 3 条」）。快照导出线在途时放宽到 16 封，真正的上限是下面
        // 的 8MB 字节预算。
        let limit = if self.has_pending_snapshot_export_wires(peer)? {
            16
        } else {
            3
        };
        let encoded = serde_json::to_string(messages)?;
        let mut query = self.connection.prepare(&format!("SELECT message_id,encrypted_envelope,seq_hint FROM sync_wire_outbox w
            WHERE recipient_device_id=?1 AND state='pending' AND available_at_ms<=?2
            AND EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)
            AND (message_id IN (SELECT value FROM json_each(?3)) OR (?4 IS NOT NULL AND
                (message_id LIKE 'snapshot:%' OR message_id LIKE 'snapshot-end:%') AND EXISTS(
                SELECT 1 FROM sync_snapshot_export_wires x WHERE x.snapshot_id=?4 AND x.recipient_device_id=?1 AND x.message_id=w.message_id)))
            AND (NOT EXISTS(SELECT 1 FROM sync_snapshot_export_wires x WHERE x.recipient_device_id=w.recipient_device_id AND x.message_id=w.message_id)
                OR EXISTS(SELECT 1 FROM sync_snapshot_export_wires x JOIN sync_snapshot_exports s ON s.id=x.snapshot_id
                    WHERE x.recipient_device_id=w.recipient_device_id AND x.message_id=w.message_id AND s.state='ready' AND s.key_version=w.key_version))
            ORDER BY available_at_ms,rowid LIMIT {limit}"))?;
        let rows = query.query_map(params![peer, now_ms, encoded, snapshot], |r| {
            Ok(PendingWireEnvelope {
                message_id: r.get(0)?,
                encrypted_envelope: r.get(1)?,
                seq_hint: r.get(2)?,
            })
        })?;
        let mut result = Vec::new();
        let mut bytes = 0;
        for row in rows {
            let row = row?;
            ensure!(
                row.encrypted_envelope.len() <= crypto::MAX_ENVELOPE_BYTES,
                "sync_stored_wire_too_large"
            );
            bytes += row.encrypted_envelope.len();
            if bytes > 8 * 1024 * 1024 {
                break;
            }
            result.push(row);
        }
        Ok(result)
    }

    /// 记一次投递尝试。`failure=None` 表示成功。
    ///
    /// 失败时 `failure` 决定退避节奏（见 `deliver::backoff_ms`）并写下**分类码**
    /// （`last_error`），让 `deliver` 状态机能回答「为什么在退避」——在这之前
    /// 表里只有 attempts 与时间，UI 只能说「同步中」。
    pub(crate) fn record_sync_wire_attempt(
        &self,
        peer: &str,
        message_id: &str,
        now_ms: i64,
        relay_id: Option<&str>,
        failure: Option<crate::sync::engine::deliver::DeliveryFailure>,
    ) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let attempts:Option<i64>=self.connection.query_row("SELECT attempts FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id=?2 AND state='pending'",params![peer,message_id],|r|r.get(0)).optional()?;
        if let Some(attempts) = attempts {
            let delay = match failure {
                // 失败：按分类退避（见 `deliver::backoff_ms`）。
                Some(failure) => crate::sync::engine::deliver::backoff_ms(failure, attempts),
                // 成功：只是压住「每拍重发同一封」，与退避是两回事。
                None => crate::sync::engine::deliver::resend_spacing_ms(attempts),
            };
            let available = now_ms
                .checked_add(delay)
                .context("sync_retry_time_invalid")?;
            // relay_id 用 COALESCE 保留旧值：它是「这封信真的上过 relay」的唯一
            // 证据，失败重试把它洗成 NULL 会让已在途的信封看起来从未发出，
            // holes 与配额账目跟着一起失真（事故档案 §四 的直接原因）。
            // blocked_reason 一律清空：能走到这里的只有两类事实 ——「成功了」与
            // 「这次失败不阻塞」。两者都说明**旧的那条阻塞原因已经不成立**
            // （配额可能已回落、也可能是另一类错），继续显示它就是在报旧账。
            // 真正该保留的情况由 `block_sync_wire` 自己写（它不经过这里）。
            // last_error 在成功时必须清空：它参与 `deliver_statuses` 的状态推导
            // （`available_at_ms > now AND last_error <> ''` 才算 backoff），
            // 留着旧分类码会把「已送出、等回执」误判成「正在退避」。
            self.connection.execute("UPDATE sync_wire_outbox SET attempts=MIN(attempts+1,1000000),available_at_ms=?3,relay_id=COALESCE(?4,relay_id),blocked_reason=NULL,last_error=CASE WHEN ?4 IS NULL THEN ?5 ELSE '' END WHERE recipient_device_id=?1 AND message_id=?2",params![peer,message_id,available,relay_id,failure.map(|f|f.code()).unwrap_or("")])?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 把一封信标成「阻塞」（要人看），并把下次尝试推后。
    ///
    /// 为什么真的要有这一步（review 2026-10-25 M3）：改之前 `blocked` 只存在于
    /// 读侧枚举里，全仓没有任何一处写 `blocked_reason` —— `is_blocking()` 是个
    /// 永远为假的死分支，413 这类**结构性不可恢复**的错误照样每拍重试，正是
    /// R5「3 次必表态」要消灭的那种毒信循环。
    ///
    /// 阻塞**不等于**永久停：行仍是 `pending`，`until_ms` 一到还会再试一次
    /// （用户可能升级了 relay 或两端上限已同步）。节奏由 `backoff_ms` 定：
    /// 507 是 15 s（空间靠对端 ack 腾出，几秒内就发生），协议错位是 10 分钟
    /// （重试再快也不会成功，快重试只是把毒信伪装成「正在重试」）。
    ///
    /// `reason` 同时写 `blocked_reason` 与 `last_error`：前者是「阻塞原因」，
    /// 后者是分类码 —— 只写一个会让面板每次选择都少一半信息（m2）。
    pub(crate) fn block_sync_wire(
        &self,
        peer: &str,
        message_id: &str,
        reason: &str,
        until_ms: i64,
    ) -> Result<()> {
        self.connection.execute(
            "UPDATE sync_wire_outbox SET available_at_ms=?3,blocked_reason=?4,last_error=?4 WHERE recipient_device_id=?1 AND message_id=?2 AND state='pending'",
            params![peer, message_id, until_ms, reason],
        )?;
        Ok(())
    }

    pub(crate) fn acknowledge_sync_wire(
        &self,
        peer: &str,
        message_id: &str,
        payload_hash: &str,
    ) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
            [peer],
            |r| r.get(0),
        )?;
        ensure!(active, "sync_member_not_authorized");
        let expected:Option<String>=self.connection.query_row("SELECT payload_hash FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id=?2",params![peer,message_id],|r|r.get(0)).optional()?;
        ensure!(
            expected.as_deref() == Some(payload_hash),
            "sync_wire_ack_mismatch"
        );
        self.connection.execute("UPDATE sync_wire_outbox SET state='acknowledged' WHERE recipient_device_id=?1 AND message_id=?2",params![peer,message_id])?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// Stages the ack for a message that is about to be staged itself. The row
    /// must not exist yet; the payload hash is bound eagerly so a later
    /// per-envelope `acknowledge_sync_wire` on the same id stays hash-checked.
    pub(crate) fn acknowledge_sync_wire_preemptive(
        &self,
        peer: &str,
        message_id: &str,
        payload_hash: &str,
    ) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
            [peer],
            |r| r.get(0),
        )?;
        ensure!(active, "sync_member_not_authorized");
        let changed = self.connection.execute("INSERT INTO sync_wire_outbox(recipient_device_id,message_id,payload_hash,key_version,encrypted_envelope,state) VALUES(?1,?2,?3,1,x'','acknowledged') ON CONFLICT(recipient_device_id,message_id) DO NOTHING",params![peer,message_id,payload_hash])?;
        ensure!(changed == 1, "sync_wire_ack_conflict");
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sync::ReplicaIdentity;
    use crate::sync::crypto::EncryptedEnvelope;
    use uuid::Uuid;

    #[test]
    fn encrypted_retry_bytes_and_backoff_survive_reopen_without_claiming_application_success() {
        let dir = std::env::temp_dir().join(format!("elsewhen-wire-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let signer = DeviceIdentity::generate().unwrap();
        let key = ContentKey::generate().unwrap();
        let header = EnvelopeHeader::new("dataset", "a", "b", 1, "provider:revision").unwrap();
        let payload = b"SYNTHETIC_PROVIDER_SECRET";
        let bytes = {
            let store = Store::open(&path).unwrap();
            store
                .initialize_sync_replica(&ReplicaIdentity {
                    dataset_id: "dataset".into(),
                    device_id: "a".into(),
                    stream_id: "stream-a".into(),
                    processor_device_id: "a".into(),
                })
                .unwrap();
            store.authorize_sync_member("b", "stream-b").unwrap();
            let bytes = store
                .stage_sync_wire_envelope(&header, &key, &signer, payload)
                .unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains("SYNTHETIC_PROVIDER_SECRET"));
            assert_eq!(
                store
                    .stage_sync_wire_envelope(&header, &key, &signer, payload)
                    .unwrap(),
                bytes
            );
            // pending 行（未 ack）：同 id 内容变化允许覆盖重封
            //（配对重置/快照重建场景，否则死行堵住 outbox）。
            let restaged = store
                .stage_sync_wire_envelope(&header, &key, &signer, b"different")
                .unwrap();
            assert_ne!(restaged, bytes);
            // 覆盖后收端收到的应是新 payload。
            let parsed_re = EncryptedEnvelope::from_bytes(&restaged).unwrap();
            assert_eq!(
                crypto::open(&parsed_re, &header, &key, &signer.public_key()).unwrap(),
                b"different"
            );
            // 但 ack 之后同 id 不同内容依旧是完整性事故，不可覆盖。
            store
                .record_sync_wire_attempt(
                    "b",
                    &header.message_id,
                    100,
                    Some("0000000000000001"),
                    None,
                )
                .unwrap();
            assert!(
                store
                    .due_sync_wire_envelopes("b", 100, 10)
                    .unwrap()
                    .is_empty()
            );
            restaged
        };
        let store = Store::open(&path).unwrap();
        let due = store.due_sync_wire_envelopes("b", 1100, 10).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].message_id, header.message_id);
        assert_eq!(due[0].encrypted_envelope, bytes);
        // 覆盖后库中现存的是最后一次封包，解开必须是新内容。
        let parsed = EncryptedEnvelope::from_bytes(&bytes).unwrap();
        assert_eq!(
            crypto::open(&parsed, &header, &key, &signer.public_key()).unwrap(),
            b"different"
        );
        // 密文不泄漏任一版本明文。
        assert!(!String::from_utf8_lossy(&bytes).contains("SYNTHETIC_PROVIDER_SECRET"));
        assert!(
            store
                .acknowledge_sync_wire("b", &header.message_id, "wrong")
                .is_err()
        );
        store
            .acknowledge_sync_wire(
                "b",
                &header.message_id,
                &crate::storage::sync::hash(b"different"),
            )
            .unwrap();
        assert!(
            store
                .due_sync_wire_envelopes("b", 999999, 10)
                .unwrap()
                .is_empty()
        );
        // 已确认信封返回已存密文：同 id、不同密文＝同明文重封（nonce 随机），
        // 幂等返回而非报错 —— 报错会堵死整个发件队列（provider 重发事故验证）。
        // 真正的明文漂移不可撞同 id：id 自带内容版本/哈希。
        let stored_from_db: Vec<u8> = store
            .connection
            .query_row(
                "SELECT encrypted_envelope FROM sync_wire_outbox WHERE recipient_device_id='b' AND message_id=?1",
                [&header.message_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            store
                .stage_sync_wire_envelope(&header, &key, &signer, b"tampered")
                .unwrap(),
            stored_from_db
        );
        // 同 id 同内容：幂等短路，返回已有密文（跨 reopen 后仍在库中）。
        assert_eq!(
            store
                .stage_sync_wire_envelope(&header, &key, &signer, b"different")
                .unwrap(),
            stored_from_db
        );
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn revoked_peer_cannot_stage_or_send_and_rekey_rejects_rollback() {
        let dir = std::env::temp_dir().join(format!("elsewhen-wire-rekey-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "dataset".into(),
                device_id: "a".into(),
                stream_id: "stream-a".into(),
                processor_device_id: "a".into(),
            })
            .unwrap();
        store.authorize_sync_member("b", "stream-b").unwrap();
        let signer = DeviceIdentity::generate().unwrap();
        let key = ContentKey::generate().unwrap();
        let mut header = EnvelopeHeader::new("dataset", "a", "b", 1, "message").unwrap();
        let old = store
            .stage_sync_wire_envelope(&header, &key, &signer, b"data")
            .unwrap();
        header.key_version = 2;
        let key2 = ContentKey::generate().unwrap();
        let new = store
            .stage_sync_wire_envelope(&header, &key2, &signer, b"data")
            .unwrap();
        assert_ne!(old, new);
        header.key_version = 1;
        assert!(
            store
                .stage_sync_wire_envelope(&header, &key, &signer, b"data")
                .is_err()
        );
        store.revoke_sync_member("b").unwrap();
        assert!(
            store
                .due_sync_wire_envelopes("b", 0, 10)
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .stage_sync_wire_envelope(&header, &key, &signer, b"data")
                .is_err()
        );
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
