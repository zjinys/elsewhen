//! Bounded encrypted relay delivery primitives. Pairing supplies trusted peer
//! identities and keys; this module never discovers trust from an envelope.
//! Snapshot requests and trust come from pairing; this is not an App entry point.

pub(crate) mod enrollment;
mod fragments;
pub(crate) mod snapshot;

#[cfg(test)]
mod tests;

use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};

use crate::storage::Store;
use crate::sync::{
    crypto::{self, ContentKey, DeviceIdentity, EncryptedEnvelope, EnvelopeHeader},
    protocol::{ApplicationReceipt, EnvelopeReceipt, ProviderBundle, SyncOperation},
    transport_relay::RelayClient,
};

fn operation_message_id(op_id: &str) -> String {
    format!("op-{}", crate::storage::sync::hash(op_id.as_bytes()))
}
fn receipt_message_id(op_id: &str) -> String {
    format!("receipt-{}", crate::storage::sync::hash(op_id.as_bytes()))
}

fn envelope_receipt_id(message_id: &str) -> String {
    format!(
        "envelope-receipt-{}",
        crate::storage::sync::hash(message_id.as_bytes())
    )
}

pub(crate) fn membership_message_id(
    signed: &crate::sync::membership::SignedRoster,
) -> Result<String> {
    Ok(format!(
        "membership:{}:{}",
        signed.roster.revision,
        signed.roster.digest()?
    ))
}

pub(crate) fn stage_membership_roster(
    store: &Store,
    recipient: &str,
    key_version: u64,
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> Result<()> {
    let tx = store.begin_write_transaction()?;
    let identity = store.sync_identity()?.context("sync_not_initialized")?;
    let signed = store
        .current_signed_roster()?
        .context("sync_authority_not_ready")?;
    ensure!(
        identity.device_id == signed.roster.owner.device_id,
        "sync_membership_owner_required"
    );
    let header = EnvelopeHeader::new(
        &identity.dataset_id,
        &identity.device_id,
        recipient,
        key_version,
        membership_message_id(&signed)?,
    )?;
    store.stage_sync_wire_envelope(&header, key, signer, &signed.encode()?)?;
    if let Some(tx) = tx {
        tx.commit()?;
    }
    Ok(())
}

pub(crate) struct EncryptionPeer<'a> {
    pub device_id: &'a str,
    pub key_version: u64,
    pub key: &'a ContentKey,
}

/// Configuration publication is one local transaction across every current peer.
/// A settings edit cannot race between staging a version and freezing its base.
pub(crate) fn stage_provider_updates(
    store: &Store,
    peers: &[EncryptionPeer<'_>],
    signer: &DeviceIdentity,
) -> Result<()> {
    let tx = store.begin_write_transaction()?;
    store.require_sync_signer(signer)?;
    store.ensure_sync_provider_baseline()?;
    let bundle = store.current_provider_bundle()?;
    let mut seen = std::collections::HashSet::new();
    for peer in peers {
        ensure!(seen.insert(peer.device_id), "sync_duplicate_recipient");
        stage_provider_bundle(
            store,
            &bundle,
            peer.device_id,
            peer.key_version,
            peer.key,
            signer,
        )?;
    }
    store.mark_provider_revision_prepared(&bundle.revision_id)?;
    if let Some(tx) = tx {
        tx.commit()?;
    }
    Ok(())
}

pub(crate) fn stage_provider_bundle(
    store: &Store,
    bundle: &ProviderBundle,
    recipient: &str,
    key_version: u64,
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> Result<()> {
    bundle.validate()?;
    let identity = store.sync_identity()?.context("sync_not_initialized")?;
    let header = EnvelopeHeader::new(
        &identity.dataset_id,
        &identity.device_id,
        recipient,
        key_version,
        format!("provider:{}", bundle.revision_id),
    )?;
    store.stage_sync_wire_envelope(&header, key, signer, &serde_json::to_vec(bundle)?)?;
    Ok(())
}

pub(crate) fn stage_operations(
    store: &Store,
    recipient: &str,
    key_version: u64,
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> Result<usize> {
    let identity = store.sync_identity()?.context("sync_not_initialized")?;
    let operations = store.pending_sync_operations(recipient, 10)?;
    for op in &operations {
        let payload = serde_json::to_vec(op)?;
        if payload.len() > crypto::MAX_PLAINTEXT_BYTES {
            fragments::stage_fragmented_operation(
                store,
                op,
                recipient,
                key_version,
                key,
                signer,
                &payload,
            )?;
            continue;
        }
        let header = EnvelopeHeader::new(
            &identity.dataset_id,
            &identity.device_id,
            recipient,
            key_version,
            operation_message_id(&op.op_id),
        )?;
        store.stage_sync_wire_envelope(&header, key, signer, &payload)?;
    }
    Ok(operations.len())
}

pub(crate) fn stage_receipts(
    store: &Store,
    recipient: &str,
    key_version: u64,
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> Result<usize> {
    let identity = store.sync_identity()?.context("sync_not_initialized")?;
    let receipts = store.pending_sync_receipts(recipient, 10)?;
    for receipt in &receipts {
        let header = EnvelopeHeader::new(
            &identity.dataset_id,
            &identity.device_id,
            recipient,
            key_version,
            receipt_message_id(&receipt.op_id),
        )?;
        store.stage_sync_wire_envelope(&header, key, signer, &serde_json::to_vec(receipt)?)?;
        // A previous receipt upload may have been lost. Re-arm only a completed
        // wire attempt; pending retries retain their existing backoff.
        store.rearm_sync_wire_receipt(recipient, &header.message_id)?;
    }
    let envelopes = store.pending_sync_wire_receipts(recipient, 10)?;
    for receipt in &envelopes {
        let header = EnvelopeHeader::new(
            &identity.dataset_id,
            &identity.device_id,
            recipient,
            key_version,
            envelope_receipt_id(&receipt.message_id),
        )?;
        store.stage_sync_wire_envelope(&header, key, signer, &serde_json::to_vec(receipt)?)?;
        store.rearm_sync_wire_receipt(recipient, &header.message_id)?;
    }
    Ok(receipts.len() + envelopes.len())
}

/// P4e 名册收敛：owner 周期性确保每个活跃成员都持有当前名册修订。
/// 长离线成员回连后，下一个 owner tick 自动补发当前修订；对端已确认或
/// 该修订在途则跳过。成员侧无推送义务（各自等待 owner），返回 false。
pub(crate) fn stage_membership_roster_if_stale(
    store: &Store,
    recipient: &str,
    key_version: u64,
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> Result<bool> {
    let identity = store.sync_identity()?.context("sync_not_initialized")?;
    let Some(signed) = store.current_signed_roster()? else {
        return Ok(false);
    };
    if identity.device_id != signed.roster.owner.device_id {
        return Ok(false);
    }
    if !store.sync_member_active(recipient)? {
        return Ok(false);
    }
    let note = format!(
        "membership:{}:{}",
        signed.roster.revision,
        signed.roster.digest()?
    );
    let tx = store.begin_write_transaction()?;
    let needed = store.membership_envelope_needed(recipient, &note)?;
    if needed {
        stage_membership_roster(store, recipient, key_version, key, signer)?;
    }
    if let Some(tx) = tx {
        tx.commit()?;
    }
    Ok(needed)
}

#[derive(Debug, Default)]
pub(crate) struct PushReport {
    pub uploaded: usize,
    pub failed: usize,
}

pub(crate) fn push_due(
    store: &Store,
    client: &RelayClient,
    recipient: &str,
    recipient_mailbox: &str,
    sender_mailbox: &str,
    credential: &[u8; 32],
    now_ms: i64,
) -> Result<PushReport> {
    // 日常增量每拍 ≤3 封（30s 一拍足够）。初始交接/重快照的数百封快照线
    // 若沿用 3 封/拍要拖半小时：检测到快照导出线在途时放宽到 16 封/拍，
    // 配合 Dart 侧传输期的即时唤醒，把初始同步压到分钟级。
    let limit = if store.has_pending_snapshot_export_wires(recipient)? {
        16
    } else {
        3
    };
    let pending = store.due_sync_wire_envelopes(recipient, now_ms, limit)?;
    let mut report = PushReport::default();
    for entry in pending {
        let expiry = now_ms
            .checked_add(7 * 24 * 60 * 60 * 1000)
            .context("sync_expiry_invalid")?;
        match client.send(
            recipient_mailbox,
            sender_mailbox,
            credential,
            0,
            &STANDARD.encode(&entry.encrypted_envelope),
            expiry,
        ) {
            Ok(id) => {
                store.record_sync_wire_attempt(recipient, &entry.message_id, now_ms, Some(&id))?;
                report.uploaded += 1;
                if entry.message_id.starts_with("receipt-") {
                    let envelope = EncryptedEnvelope::from_bytes(&entry.encrypted_envelope)?;
                    ensure!(
                        envelope.unverified_header().recipient_device_id == recipient,
                        "sync_stored_recipient_mismatch"
                    );
                    // The source operation remains pending until this receipt is
                    // applied there. Its retry regenerates a lost receipt here.
                    let tx = store.begin_write_transaction()?;
                    let receipts = store.pending_sync_receipts(recipient, 100)?;
                    if let Some(receipt) = receipts
                        .iter()
                        .find(|r| receipt_message_id(&r.op_id) == entry.message_id)
                    {
                        store.mark_sync_receipt_sent(&receipt.op_id)?;
                        store.acknowledge_sync_wire(
                            recipient,
                            &entry.message_id,
                            &crate::storage::sync::hash(&serde_json::to_vec(receipt)?),
                        )?;
                    } else {
                        // Synthetic re-snapshot acks are staged preemptively
                        // acknowledged (see `ack_resnapshot`): no per-envelope
                        // retirement needed here.
                    }
                    if let Some(tx) = tx {
                        tx.commit()?;
                    }
                } else if entry.message_id.starts_with("envelope-receipt-") {
                    let tx = store.begin_write_transaction()?;
                    let receipts = store.pending_sync_wire_receipts(recipient, 100)?;
                    if let Some(receipt) = receipts
                        .iter()
                        .find(|r| envelope_receipt_id(&r.message_id) == entry.message_id)
                    {
                        store.mark_sync_wire_receipt_sent(recipient, &receipt.message_id)?;
                        store.acknowledge_sync_wire(
                            recipient,
                            &entry.message_id,
                            &crate::storage::sync::hash(&serde_json::to_vec(receipt)?),
                        )?;
                    }
                    if let Some(tx) = tx {
                        tx.commit()?;
                    }
                }
            }
            Err(_) => {
                // Do not persist remote error bodies or secret-bearing requests.
                store.record_sync_wire_attempt(recipient, &entry.message_id, now_ms, None)?;
                report.failed += 1;
            }
        }
    }
    if report.failed > 0 {
        // 推送失败在桌面端只进本地日志（tee 到 agent-debug.log），不中断 tick。
        eprintln!(
            "[sync-wire] push failed: recipient={} uploaded={} failed={}",
            recipient, report.uploaded, report.failed
        );
    }
    Ok(report)
}

pub(crate) struct ReceivingPeer<'a> {
    pub device_id: &'a str,
    pub mailbox_id: &'a str,
    pub key_version: u64,
    pub key: &'a ContentKey,
    pub public_key: &'a [u8; 32],
}

#[derive(Debug, Default)]
pub(crate) struct PullReport {
    pub accepted: usize,
    pub waiting: usize,
    pub rejected: usize,
    /// 拒绝中因协议版本不匹配者（升级容差/诊断：不是内容错误，是部署错位）。
    pub rejected_version_mismatch: usize,
    pub acknowledgement_failed: bool,
}

/// Scan one small page. Unknown, invalid or dependency-blocked mail is not acked;
/// scan progress still advances so one source cannot starve other sources. At the
/// end we wrap, including after process restart, rather than forgetting old gaps.
pub(crate) fn pull_page(
    store: &Store,
    client: &RelayClient,
    mailbox: &str,
    owner_credential: &[u8; 32],
    peers: &[ReceivingPeer<'_>],
) -> Result<PullReport> {
    let mut seen = std::collections::HashSet::new();
    for peer in peers {
        ensure!(
            seen.insert(peer.mailbox_id),
            "sync_duplicate_sender_mailbox"
        );
    }
    let after = store.relay_scan_cursor(mailbox)?;
    let page = client.fetch_page(mailbox, owner_credential, &after, 3)?;
    let mut report = PullReport::default();
    let mut acknowledged = Vec::new();
    for mail in &page.envelopes {
        let Some(peer) = peers.iter().find(|p| p.mailbox_id == mail.sender_mb) else {
            report.rejected += 1;
            continue;
        };
        match accept_relay_payload(
            store,
            &mail.ciphertext,
            peer.device_id,
            peer.key_version,
            peer.key,
            peer.public_key,
        ) {
            Ok(true) => {
                acknowledged.push(mail.id.clone());
                report.accepted += 1;
            }
            Ok(false) => report.waiting += 1,
            Err(err) => {
                report.rejected += 1;
                // 错误链文本匹配而非 downcast：错误可能被 context 层包装。
                if format!("{err:#}").contains("sync_protocol_unsupported") {
                    report.rejected_version_mismatch += 1;
                }
            }
        }
    }
    if !acknowledged.is_empty() {
        report.acknowledgement_failed = client
            .ack(mailbox, owner_credential, &acknowledged)
            .is_err();
    }
    store.save_relay_scan_cursor(
        mailbox,
        if page.has_more {
            &page.next_cursor
        } else {
            "0"
        },
    )?;
    Ok(report)
}

pub(super) struct AuthenticatedWire {
    header: EnvelopeHeader,
    plaintext: Vec<u8>,
    bytes: Vec<u8>,
}

pub(super) fn authenticate_relay_payload(
    store: &Store,
    ciphertext_b64: &str,
    sender: &str,
    key_version: u64,
    key: &ContentKey,
    trusted_sender: &[u8; 32],
) -> Result<AuthenticatedWire> {
    ensure!(
        ciphertext_b64.len() <= crypto::MAX_ENVELOPE_BYTES.div_ceil(3) * 4,
        "sync_relay_payload_too_large"
    );
    let bytes = STANDARD
        .decode(ciphertext_b64)
        .map_err(|_| anyhow::anyhow!("sync_relay_payload_invalid"))?;
    let envelope = EncryptedEnvelope::from_bytes(&bytes)?;
    let identity = store.sync_identity()?.context("sync_not_initialized")?;
    let message_id = envelope.unverified_header().message_id.clone();
    let header = EnvelopeHeader::new(
        &identity.dataset_id,
        sender,
        &identity.device_id,
        key_version,
        &message_id,
    )?;
    store.require_sync_transport_key(
        sender,
        trusted_sender,
        message_id.starts_with("membership:"),
    )?;
    let plaintext = crypto::open(&envelope, &header, key, trusted_sender)?;
    Ok(AuthenticatedWire {
        header,
        plaintext,
        bytes,
    })
}

/// Returns true only when the authenticated operation, receipt or encrypted
/// snapshot piece is durable and the caller may acknowledge this relay mail.
/// Snapshot staging is not publication; missing operation dependencies return
/// false and never silently advance an application frontier.
pub(crate) fn accept_relay_payload(
    store: &Store,
    ciphertext_b64: &str,
    sender: &str,
    key_version: u64,
    key: &ContentKey,
    trusted_sender: &[u8; 32],
) -> Result<bool> {
    let wire = authenticate_relay_payload(
        store,
        ciphertext_b64,
        sender,
        key_version,
        key,
        trusted_sender,
    )?;
    let identity = store.sync_identity()?.context("sync_not_initialized")?;
    let expected = wire.header;
    let message_id = expected.message_id.clone();
    let plaintext = wire.plaintext;
    let bytes = wire.bytes;
    if message_id.starts_with("membership:") {
        let tx = store.begin_write_transaction()?;
        store.apply_signed_roster(&plaintext)?;
        let signed: crate::sync::membership::SignedRoster = serde_json::from_slice(&plaintext)
            .map_err(|_| anyhow::anyhow!("sync_membership_record_invalid"))?;
        ensure!(
            message_id == membership_message_id(&signed)?,
            "sync_membership_message_mismatch"
        );
        store.queue_sync_wire_receipt(
            sender,
            &message_id,
            &crate::storage::sync::hash(&plaintext),
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        return Ok(true);
    }
    if message_id.starts_with("snapshot:") || message_id.starts_with("snapshot-end:") {
        return snapshot::accept_snapshot(store, &bytes, &expected, key, trusted_sender);
    }
    // Keep deltas at the relay until the initial snapshot is atomically published.
    // Applying them into an unfinished staging database would either lose their
    // writes or prevent its fresh-database publication contract from succeeding.
    if (message_id.starts_with("provider:") || message_id.starts_with("op-"))
        && store.snapshot_import_pending()?
    {
        return Ok(false);
    }
    if message_id.starts_with("provider:") {
        let tx = store.begin_write_transaction()?;
        store.apply_encrypted_provider_bundle(&bytes, &expected, key, trusted_sender)?;
        store.queue_sync_wire_receipt(
            sender,
            &message_id,
            &crate::storage::sync::hash(&plaintext),
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        return Ok(true);
    }
    if message_id.starts_with("envelope-receipt-") {
        let receipt: EnvelopeReceipt = serde_json::from_slice(&plaintext)
            .map_err(|_| anyhow::anyhow!("sync_receipt_invalid"))?;
        ensure!(
            receipt.dataset_id == identity.dataset_id
                && receipt.acknowledging_device_id == sender
                && receipt.target_device_id == identity.device_id
                && (receipt.message_id.starts_with("provider:")
                    || receipt.message_id.starts_with("snapshot-end:")
                    || receipt.message_id.starts_with("membership:"))
                && envelope_receipt_id(&receipt.message_id) == message_id,
            "sync_receipt_identity_mismatch"
        );
        if let Some(id) = receipt.message_id.strip_prefix("snapshot-end:") {
            store.acknowledge_snapshot_export(sender, id, &receipt.payload_hash)?;
        } else {
            store.acknowledge_sync_wire(sender, &receipt.message_id, &receipt.payload_hash)?;
        }
        return Ok(true);
    }
    if message_id.starts_with("op-part-") {
        store.receive_operation_fragment(&bytes, &expected, key, trusted_sender)?;
        return Ok(true);
    }
    if message_id.starts_with("op-") {
        let op = SyncOperation::decode(&plaintext)?;
        ensure!(
            operation_message_id(&op.op_id) == message_id,
            "sync_wire_operation_mismatch"
        );
        store.receive_sync_operation(sender, &plaintext)?;
        return store.sync_operation_durable(&op.op_id);
    }
    ensure!(
        message_id.starts_with("receipt-"),
        "sync_wire_kind_unsupported"
    );
    let receipt: ApplicationReceipt =
        serde_json::from_slice(&plaintext).map_err(|_| anyhow::anyhow!("sync_receipt_invalid"))?;
    ensure!(
        receipt_message_id(&receipt.op_id) == message_id,
        "sync_wire_receipt_mismatch"
    );
    let tx = store.begin_write_transaction()?;
    store.accept_sync_receipt(sender, &receipt)?;
    let op_message_id = operation_message_id(&receipt.op_id);
    let exists:bool=store.connection.query_row("SELECT EXISTS(SELECT 1 FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id=?2)",rusqlite::params![sender,op_message_id],|r|r.get(0))?;
    if exists {
        store.acknowledge_sync_wire(sender, &op_message_id, &receipt.payload_hash)?;
    }
    // The domain receipt authenticates the complete operation, not individual
    // pieces. Retire all its fragments only after that receipt is validated.
    store.connection.execute(
        "UPDATE sync_wire_outbox SET state='acknowledged' WHERE recipient_device_id=?1 AND message_id IN (SELECT message_id FROM sync_operation_fragment_outbox WHERE recipient_device_id=?1 AND op_id=?2)",
        rusqlite::params![sender,receipt.op_id],
    )?;
    if let Some(tx) = tx {
        tx.commit()?;
    }
    Ok(true)
}
