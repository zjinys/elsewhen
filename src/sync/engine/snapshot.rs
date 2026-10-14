//! Encrypted snapshot transfer orchestration. The receiver must explicitly select
//! a fresh staging database and request ID before accepting any snapshot content.

use anyhow::{Context, Result, ensure};

use super::*;
use crate::storage::sync::snapshot::SnapshotExporter;
use crate::sync::protocol::{ApplicationReceipt, validate_id};

/// Retires one acknowledged re-snapshot export at the receiver of that
/// snapshot: queues a synthetic `receipt-…` wire back to the sender carrying
/// `resnapshot-ack:{export_id}`. The sender's `accept_sync_receipt` turns it
/// into `acknowledge_snapshot_export`, which re-seeds the member watermark
/// and lets the request complete. This is a transport repair signal, not a
/// restore: the receiver's live domain data is untouched.
pub(crate) fn ack_resnapshot(
    store: &Store,
    recipient: &str,
    export_id: &str,
    key_version: u64,
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> Result<()> {
    // Idempotent: once any receipt for this export is queued or acked, the
    // ack is already durable and retries must not enqueue a second wire.
    let already: bool = store.connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id=?2)",
        rusqlite::params![recipient, receipt_message_id(&resnapshot_ack_op_id(export_id))],
        |r| r.get(0),
    )?;
    if already {
        return Ok(());
    }
    let identity = store.sync_identity()?.context("sync_not_initialized")?;
    let receipt = ApplicationReceipt {
        dataset_id: identity.dataset_id.clone(),
        acknowledging_device_id: identity.device_id.clone(),
        target_device_id: recipient.to_string(),
        op_id: resnapshot_ack_op_id(export_id),
        payload_hash: crate::storage::sync::hash(export_id.as_bytes()),
    };
    let plaintext = serde_json::to_vec(&receipt)?;
    let header = EnvelopeHeader::new(
        &identity.dataset_id,
        &identity.device_id,
        recipient,
        key_version,
        receipt_message_id(&receipt.op_id),
    )?;
    // Local transport retirement binds to the staged payload hash; the sender
    // side binds to the export id, not this hash (see accept_sync_receipt).
    store.acknowledge_sync_wire_preemptive(
        recipient,
        &header.message_id,
        &crate::storage::sync::hash(&plaintext),
    )?;
    store.stage_sync_wire_envelope(&header, key, signer, &plaintext)?;
    Ok(())
}

/// The synthetic op id carried by a re-snapshot application ack.
pub(crate) fn resnapshot_ack_op_id(export_id: &str) -> String {
    format!("resnapshot-ack:{export_id}")
}

/// Writes only ciphertext, one bounded chunk transaction at a time. An export
/// becomes eligible for upload only when its manifest is durable. A failed or
/// interrupted export must be requested again under a new snapshot ID.
pub(crate) fn stage_snapshot(
    store: &Store,
    recipient: &str,
    id: &str,
    key_version: u64,
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> Result<()> {
    stage_snapshot_inner(store, recipient, id, key_version, key, signer, false)
}

pub(crate) fn stage_enrollment_snapshot(
    store: &Store,
    recipient: &str,
    id: &str,
    key_version: u64,
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> Result<()> {
    stage_snapshot_inner(store, recipient, id, key_version, key, signer, true)
}

fn stage_snapshot_inner(
    store: &Store,
    recipient: &str,
    id: &str,
    key_version: u64,
    key: &ContentKey,
    signer: &DeviceIdentity,
    with_authority: bool,
) -> Result<()> {
    ensure!(
        store.connection.is_autocommit(),
        "sync_snapshot_export_requires_autocommit"
    );
    if !store.begin_snapshot_export(id, recipient, key_version)? {
        return Ok(());
    }
    let result = (|| {
        let mut exporter = SnapshotExporter::new(store, recipient, id, key_version)?;
        if with_authority {
            let signed = exporter.signed_roster()?;
            let header = EnvelopeHeader::new(
                &signed.roster.dataset_id,
                &signed.roster.owner.device_id,
                recipient,
                key_version,
                membership_message_id(&signed)?,
            )?;
            store.stage_snapshot_authority(id, &header, key, signer, &signed.encode()?)?;
        }
        while let Some(bytes) = exporter.next_encrypted_chunk(key, signer)? {
            stage_piece(store, id, &bytes, key, signer, false)?;
        }
        let manifest = exporter.finish(key, signer)?;
        stage_piece(store, id, &manifest, key, signer, true)
    })();
    if result.is_err() {
        store.abort_snapshot_export(id)?;
    }
    result
}

fn stage_piece(
    store: &Store,
    id: &str,
    bytes: &[u8],
    key: &ContentKey,
    signer: &DeviceIdentity,
    finish: bool,
) -> Result<()> {
    let envelope = EncryptedEnvelope::from_bytes(bytes)?;
    let header = envelope.unverified_header();
    let plaintext = crypto::open(&envelope, header, key, &signer.public_key())?;
    store.stage_snapshot_export_payload(id, header, key, signer, &plaintext, finish)
}

fn publish_and_receipt(
    store: &Store,
    id: &str,
    sender: &str,
    key_version: u64,
    bytes: &[u8],
    key: &ContentKey,
    public: &[u8; 32],
) -> Result<()> {
    let tx = store.begin_write_transaction()?;
    if store.publish_snapshot(id, sender, key_version, bytes, key, public)? {
        let envelope = EncryptedEnvelope::from_bytes(bytes)?;
        let header = envelope.unverified_header();
        let plaintext = crypto::open(&envelope, header, key, public)?;
        store.queue_sync_wire_receipt(
            sender,
            &header.message_id,
            &crate::storage::sync::hash(&plaintext),
        )?;
    }
    if let Some(tx) = tx {
        tx.commit()?;
    }
    Ok(())
}

pub(super) fn accept_snapshot(
    store: &Store,
    bytes: &[u8],
    header: &EnvelopeHeader,
    key: &ContentKey,
    public: &[u8; 32],
) -> Result<bool> {
    if let Some(id) = header.message_id.strip_prefix("snapshot-end:") {
        validate_id(id)?;
        publish_and_receipt(
            store,
            id,
            &header.sender_device_id,
            header.key_version,
            bytes,
            key,
            public,
        )?;
        // An incomplete manifest is durably staged, not an application receipt.
        return Ok(true);
    }
    let suffix = header
        .message_id
        .strip_prefix("snapshot:")
        .context("sync_snapshot_identity_invalid")?;
    let (id, index) = suffix
        .split_once(':')
        .context("sync_snapshot_identity_invalid")?;
    validate_id(id)?;
    let index: usize = index
        .parse()
        .map_err(|_| anyhow::anyhow!("sync_snapshot_index_invalid"))?;
    ensure!(
        header.message_id == format!("snapshot:{id}:{index}"),
        "sync_snapshot_identity_invalid"
    );
    let tx = store.begin_write_transaction()?;
    store.stage_snapshot_chunk(
        id,
        index,
        &header.sender_device_id,
        header.key_version,
        bytes,
        key,
        public,
    )?;
    if let Some(manifest) = store.staged_snapshot_manifest(id)? {
        publish_and_receipt(
            store,
            id,
            &header.sender_device_id,
            header.key_version,
            &manifest,
            key,
            public,
        )?;
    }
    if let Some(tx) = tx {
        tx.commit()?;
    }
    Ok(true)
}
