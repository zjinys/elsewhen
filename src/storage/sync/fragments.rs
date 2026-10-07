//! Encrypted, bounded fragment staging. Completed pieces retain hashes for replay
//! checks but release their ciphertext after the operation is durably inventoried.

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use crate::storage::Store;
use crate::sync::{
    crypto::{self, ContentKey, EncryptedEnvelope, EnvelopeHeader},
    fragments::OperationFragment,
    protocol::SyncOperation,
};

const MAX_STAGED_BYTES: i64 = 64 * 1024 * 1024;

impl Store {
    pub(crate) fn receive_operation_fragment(
        &self,
        bytes: &[u8],
        expected: &EnvelopeHeader,
        key: &ContentKey,
        trusted_sender: &[u8; 32],
    ) -> Result<()> {
        self.require_sync_transport_key(&expected.sender_device_id, trusted_sender, false)?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        ensure!(
            expected.dataset_id == identity.dataset_id
                && expected.recipient_device_id == identity.device_id,
            "sync_fragment_recipient_mismatch"
        );
        let payload = crypto::open(
            &EncryptedEnvelope::from_bytes(bytes)?,
            expected,
            key,
            trusted_sender,
        )?;
        let (fragment, data) = OperationFragment::decode(&payload)?;
        ensure!(
            fragment.message_id() == expected.message_id,
            "sync_fragment_identity_mismatch"
        );
        let sender = &expected.sender_device_id;
        let version =
            i64::try_from(expected.key_version).context("sync_fragment_key_version_invalid")?;
        let stream = fragment.op_id.split_once(':').unwrap().0;
        let tx = self.begin_write_transaction()?;
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND stream_id=?2 AND active=1)",
            params![sender,stream], |r|r.get(0),
        )?;
        ensure!(
            active && stream != identity.stream_id,
            "sync_fragment_source_not_authorized"
        );
        let group: Option<(String,i64,i64,bool)> = self.connection.query_row(
            "SELECT payload_hash,total_bytes,chunk_count,assembled FROM sync_operation_fragments WHERE sender_device_id=?1 AND op_id=?2 AND key_version=?3",
            params![sender,fragment.op_id,version], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
        ).optional()?;
        let assembled = if let Some((hash, size, count, assembled)) = group {
            ensure!(
                hash == fragment.payload_hash
                    && size == fragment.total_bytes as i64
                    && count == fragment.chunk_count as i64,
                "sync_fragment_content_conflict"
            );
            assembled
        } else {
            let (peer_count,total_count): (i64,i64) = self.connection.query_row(
                "SELECT COALESCE(SUM(sender_device_id=?1),0),COUNT(*) FROM sync_operation_fragments WHERE assembled=0",
                [sender], |r|Ok((r.get(0)?,r.get(1)?)),
            )?;
            ensure!(
                peer_count < 4 && total_count < 16,
                "sync_fragment_staging_full"
            );
            self.connection.execute(
                "INSERT INTO sync_operation_fragments(sender_device_id,op_id,key_version,payload_hash,total_bytes,chunk_count) VALUES(?1,?2,?3,?4,?5,?6)",
                params![sender,fragment.op_id,version,fragment.payload_hash,fragment.total_bytes as i64,fragment.chunk_count as i64],
            )?;
            false
        };
        let digest = super::hash(&data);
        let existing: Option<String> = self.connection.query_row(
            "SELECT content_hash FROM sync_operation_fragment_parts WHERE sender_device_id=?1 AND op_id=?2 AND key_version=?3 AND chunk_index=?4",
            params![sender,fragment.op_id,version,fragment.index as i64], |r|r.get(0),
        ).optional()?;
        if let Some(existing) = existing {
            ensure!(existing == digest, "sync_fragment_content_conflict");
        } else {
            ensure!(!assembled, "sync_fragment_completed_part_missing");
            let used: i64 = self.connection.query_row(
                "SELECT COALESCE(SUM(length(encrypted_envelope)),0) FROM sync_operation_fragment_parts", [], |r|r.get(0),
            )?;
            ensure!(
                used + bytes.len() as i64 <= MAX_STAGED_BYTES,
                "sync_fragment_staging_full"
            );
            self.connection.execute(
                "INSERT INTO sync_operation_fragment_parts(sender_device_id,op_id,key_version,chunk_index,content_hash,encrypted_envelope) VALUES(?1,?2,?3,?4,?5,?6)",
                params![sender,fragment.op_id,version,fragment.index as i64,digest,bytes],
            )?;
        }
        if assembled {
            self.sync_drain_waiting()?;
            self.queue_sync_receipt(&fragment.op_id)?;
        } else {
            let count: i64 = self.connection.query_row(
                "SELECT COUNT(*) FROM sync_operation_fragment_parts WHERE sender_device_id=?1 AND op_id=?2 AND key_version=?3",
                params![sender,fragment.op_id,version], |r|r.get(0),
            )?;
            if count == fragment.chunk_count as i64 {
                let mut operation = Vec::with_capacity(fragment.total_bytes);
                for index in 0..fragment.chunk_count {
                    let bytes: Vec<u8> = self.connection.query_row(
                        "SELECT encrypted_envelope FROM sync_operation_fragment_parts WHERE sender_device_id=?1 AND op_id=?2 AND key_version=?3 AND chunk_index=?4",
                        params![sender,fragment.op_id,version,index as i64], |r|r.get(0),
                    )?;
                    let header = EnvelopeHeader::new(
                        &expected.dataset_id,
                        sender,
                        &expected.recipient_device_id,
                        expected.key_version,
                        format!("op-part-{}-{index}", super::hash(fragment.op_id.as_bytes())),
                    )?;
                    let (part, data) = OperationFragment::decode(&crypto::open(
                        &EncryptedEnvelope::from_bytes(&bytes)?,
                        &header,
                        key,
                        trusted_sender,
                    )?)?;
                    ensure!(
                        part.op_id == fragment.op_id
                            && part.index == index
                            && part.total_bytes == fragment.total_bytes
                            && part.chunk_count == fragment.chunk_count
                            && part.payload_hash == fragment.payload_hash,
                        "sync_fragment_content_conflict"
                    );
                    operation.extend(data);
                }
                ensure!(
                    operation.len() == fragment.total_bytes
                        && super::hash(&operation) == fragment.payload_hash,
                    "sync_fragment_hash_mismatch"
                );
                let op = SyncOperation::decode(&operation)?;
                ensure!(
                    op.op_id == fragment.op_id
                        && super::hash(&serde_json::to_vec(&op)?) == fragment.payload_hash,
                    "sync_fragment_operation_mismatch"
                );
                self.receive_sync_operation(sender, &operation)?;
                self.connection.execute(
                    "UPDATE sync_operation_fragments SET assembled=1 WHERE sender_device_id=?1 AND op_id=?2 AND key_version=?3",
                    params![sender,fragment.op_id,version],
                )?;
                self.connection.execute(
                    "UPDATE sync_operation_fragment_parts SET encrypted_envelope=NULL WHERE sender_device_id=?1 AND op_id=?2 AND key_version=?3",
                    params![sender,fragment.op_id,version],
                )?;
            }
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
}
