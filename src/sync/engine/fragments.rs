use super::*;
use crate::sync::{
    fragments::{CHUNK_BYTES, OperationFragment},
    protocol::{MAX_OPERATION_BYTES, PROTOCOL_VERSION},
};

pub(super) fn stage_fragmented_operation(
    store: &Store,
    op: &SyncOperation,
    recipient: &str,
    key_version: u64,
    key: &ContentKey,
    signer: &DeviceIdentity,
    payload: &[u8],
) -> Result<()> {
    ensure!(
        payload.len() <= MAX_OPERATION_BYTES,
        "sync_pending_operation_too_large"
    );
    let hash = crate::storage::sync::hash(payload);
    for (index, data) in payload.chunks(CHUNK_BYTES).enumerate() {
        let fragment = OperationFragment {
            version: PROTOCOL_VERSION,
            op_id: op.op_id.clone(),
            payload_hash: hash.clone(),
            total_bytes: payload.len(),
            chunk_count: payload.len().div_ceil(CHUNK_BYTES),
            index,
            data: STANDARD.encode(data),
        };
        let header = EnvelopeHeader::new(
            &op.dataset_id,
            &op.origin_device_id,
            recipient,
            key_version,
            fragment.message_id(),
        )?;
        let tx = store.begin_write_transaction()?;
        store.stage_sync_wire_envelope(&header, key, signer, &serde_json::to_vec(&fragment)?)?;
        store.connection.execute(
            "INSERT INTO sync_operation_fragment_outbox(recipient_device_id,message_id,op_id) VALUES(?1,?2,?3) ON CONFLICT(recipient_device_id,message_id) DO NOTHING",
            rusqlite::params![recipient,header.message_id,op.op_id],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
    }
    Ok(())
}
