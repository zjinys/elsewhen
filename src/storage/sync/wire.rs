//! Durable encrypted delivery attempts. Relay upload success is deliberately
//! not an application acknowledgement and never removes the source operation.

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use crate::storage::Store;
use crate::sync::crypto::{self, ContentKey, DeviceIdentity, EnvelopeHeader};

pub(crate) struct PendingWireEnvelope {
    pub message_id: String,
    pub encrypted_envelope: Vec<u8>,
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
                    // 已确认信封不可变：同 id 内容漂移＝完整性事故。
                    ensure!(hash == digest, "sync_wire_identity_content_conflict");
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
        self.connection.execute("INSERT INTO sync_wire_outbox(recipient_device_id,message_id,payload_hash,key_version,encrypted_envelope) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(recipient_device_id,message_id) DO UPDATE SET payload_hash=excluded.payload_hash,key_version=excluded.key_version,encrypted_envelope=excluded.encrypted_envelope,available_at_ms=0,relay_id=NULL",params![header.recipient_device_id,header.message_id,digest,header.key_version as i64,encrypted])?;
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
        ensure!((1..=10).contains(&limit), "sync_wire_batch_limit_invalid");
        self.require_local_sync_membership()?;
        let mut query=self.connection.prepare("SELECT message_id,encrypted_envelope FROM sync_wire_outbox w WHERE recipient_device_id=?1 AND state='pending' AND available_at_ms<=?2 AND EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1) AND NOT EXISTS(SELECT 1 FROM sync_snapshot_export_wires x JOIN sync_snapshot_exports s ON s.id=x.snapshot_id WHERE x.recipient_device_id=w.recipient_device_id AND x.message_id=w.message_id AND s.state!='ready') ORDER BY available_at_ms,rowid LIMIT ?3")?;
        let rows = query.query_map(params![peer, now_ms, limit as i64], |r| {
            Ok(PendingWireEnvelope {
                message_id: r.get(0)?,
                encrypted_envelope: r.get(1)?,
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
        let encoded = serde_json::to_string(messages)?;
        let mut query = self.connection.prepare("SELECT message_id,encrypted_envelope FROM sync_wire_outbox w
            WHERE recipient_device_id=?1 AND state='pending' AND available_at_ms<=?2
            AND EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)
            AND (message_id IN (SELECT value FROM json_each(?3)) OR (?4 IS NOT NULL AND
                (message_id LIKE 'snapshot:%' OR message_id LIKE 'snapshot-end:%') AND EXISTS(
                SELECT 1 FROM sync_snapshot_export_wires x WHERE x.snapshot_id=?4 AND x.recipient_device_id=?1 AND x.message_id=w.message_id)))
            AND (NOT EXISTS(SELECT 1 FROM sync_snapshot_export_wires x WHERE x.recipient_device_id=w.recipient_device_id AND x.message_id=w.message_id)
                OR EXISTS(SELECT 1 FROM sync_snapshot_export_wires x JOIN sync_snapshot_exports s ON s.id=x.snapshot_id
                    WHERE x.recipient_device_id=w.recipient_device_id AND x.message_id=w.message_id AND s.state='ready' AND s.key_version=w.key_version))
            ORDER BY available_at_ms,rowid LIMIT 3")?;
        let rows = query.query_map(params![peer, now_ms, encoded, snapshot], |r| {
            Ok(PendingWireEnvelope {
                message_id: r.get(0)?,
                encrypted_envelope: r.get(1)?,
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

    pub(crate) fn record_sync_wire_attempt(
        &self,
        peer: &str,
        message_id: &str,
        now_ms: i64,
        relay_id: Option<&str>,
    ) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let attempts:Option<i64>=self.connection.query_row("SELECT attempts FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id=?2 AND state='pending'",params![peer,message_id],|r|r.get(0)).optional()?;
        if let Some(attempts) = attempts {
            let delay = (1_000_i64 << attempts.clamp(0, 6)).min(60_000);
            let available = now_ms
                .checked_add(delay)
                .context("sync_retry_time_invalid")?;
            self.connection.execute("UPDATE sync_wire_outbox SET attempts=MIN(attempts+1,1000000),available_at_ms=?3,relay_id=?4 WHERE recipient_device_id=?1 AND message_id=?2",params![peer,message_id,available,relay_id])?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
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
                .record_sync_wire_attempt("b", &header.message_id, 100, Some("0000000000000001"))
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
        // 已确认信封是铁律：同 id、不同内容 → 拒绝。
        assert!(
            store
                .stage_sync_wire_envelope(&header, &key, &signer, b"tampered")
                .unwrap_err()
                .to_string()
                .contains("sync_wire_identity_content_conflict")
        );
        // 同 id 同内容：幂等短路，返回已有密文（跨 reopen 后仍在库中）。
        let stored: Vec<u8> = store
            .connection
            .query_row(
                "SELECT encrypted_envelope FROM sync_wire_outbox WHERE recipient_device_id='b' AND message_id=?1",
                [&header.message_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            store
                .stage_sync_wire_envelope(&header, &key, &signer, b"different")
                .unwrap(),
            stored
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
