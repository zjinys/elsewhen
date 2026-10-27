//! Typed enrollment controls and scoped transfer. No ordinary business delta is
//! applied or uploaded by this path while the initial target is being prepared.
use super::*;
use crate::storage::sync::PairingCommit;
use crate::sync::enrollment::{
    EnrollmentRecord, EnrollmentRole, SnapshotOffer, completion_id, offer_id,
};
use rusqlite::OptionalExtension;

pub(crate) enum ReceiveOutcome {
    Waiting,
    Durable(Option<PairingCommit>),
}

pub(crate) fn snapshot_roster_id(
    store: &Store,
    record: &EnrollmentRecord,
) -> Result<Option<String>> {
    let Some(snapshot) = &record.snapshot else {
        return Ok(None);
    };
    Ok(store.connection.query_row("SELECT message_id FROM sync_snapshot_export_wires WHERE snapshot_id=?1 AND recipient_device_id=?2 AND message_id LIKE 'membership:%'",
        rusqlite::params![snapshot.id, record.peer.device_id], |r| r.get(0)).optional()?)
}

pub(crate) fn owner_offer(
    store: &Store,
    record: &EnrollmentRecord,
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> Result<SnapshotOffer> {
    let snapshot = record
        .snapshot
        .as_ref()
        .context("sync_enrollment_snapshot_missing")?;
    ensure!(
        matches!(
            store.enrollment_export_state(&snapshot.id)?.as_deref(),
            Some("ready" | "acknowledged")
        ),
        "sync_enrollment_snapshot_not_ready"
    );
    let message = format!("snapshot-end:{}", snapshot.id);
    let bytes: Vec<u8> = store.connection.query_row("SELECT encrypted_envelope FROM sync_wire_outbox WHERE recipient_device_id=?1 AND message_id=?2",
        rusqlite::params![record.peer.device_id, message], |r| r.get(0))?;
    let header = EnvelopeHeader::new(
        &record.dataset_id,
        &record.local_device_id,
        &record.peer.device_id,
        record.key_version,
        message,
    )?;
    let plain = crypto::open(
        &EncryptedEnvelope::from_bytes(&bytes)?,
        &header,
        key,
        &signer.public_key(),
    )?;
    let manifest: crate::storage::sync::snapshot::SnapshotManifest =
        serde_json::from_slice(&plain)?;
    ensure!(
        manifest.id == snapshot.id
            && manifest.dataset_id == record.dataset_id
            && manifest.source_device_id == record.local_device_id
            && manifest.recipient_device_id == record.peer.device_id
            && manifest.key_version == record.key_version,
        "sync_enrollment_manifest_mismatch"
    );
    Ok(SnapshotOffer {
        version: 1,
        invitation_id: record.invitation_id.clone(),
        dataset_id: record.dataset_id.clone(),
        sender_device_id: record.local_device_id.clone(),
        recipient_device_id: record.peer.device_id.clone(),
        key_version: record.key_version,
        snapshot_id: snapshot.id.clone(),
        manifest_hash: crate::storage::sync::hash(&plain),
        chunks: manifest.chunks as i64,
        bytes: manifest.bytes as i64,
    })
}

pub(crate) fn stage_offer(
    store: &Store,
    record: &EnrollmentRecord,
    key: &ContentKey,
    signer: &DeviceIdentity,
    complete: bool,
) -> Result<()> {
    ensure!(
        record.role == EnrollmentRole::Owner && (!complete || record.owner_confirmed),
        "sync_enrollment_owner_required"
    );
    let offer = owner_offer(store, record, key, signer)?;
    let id = if complete {
        completion_id(&offer.invitation_id, &offer.snapshot_id)
    } else {
        offer_id(&offer.invitation_id, &offer.snapshot_id)
    };
    let header = EnvelopeHeader::new(
        &record.dataset_id,
        &record.local_device_id,
        &record.peer.device_id,
        record.key_version,
        id,
    )?;
    store.stage_sync_wire_envelope(&header, key, signer, &serde_json::to_vec(&offer)?)?;
    Ok(())
}

/// 本次交接需要回执的 subject。固定四项（名册/offer/manifest/completion）不够：
/// 接收方是在「触发 publish 的那一封」上发回执的，乱序到达时它可能是一片
/// `snapshot:{id}:{idx}`（manifest 先到并被消费、最后一片到齐才发布）。写死列表
/// 会让这封回执既不进 outbox、也不在发送 scope 里，于是导出永远拿不到确认，
/// 两端都停在「等待成员授权与初始同步」。所以固定项之外，把已排队待发的持久
/// 回执（snapshot 家族的）一并纳入。
fn receipt_subjects(store: &Store, record: &EnrollmentRecord) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    if let Some(id) = &record.roster_message_id {
        ids.push(id.clone());
    }
    if let Some(snapshot) = &record.snapshot {
        ids.push(offer_id(&record.invitation_id, &snapshot.id));
        ids.push(format!("snapshot-start:{}", snapshot.id));
        ids.push(format!("snapshot-end:{}", snapshot.id));
        ids.push(completion_id(&record.invitation_id, &snapshot.id));
    }
    for receipt in store.pending_sync_wire_receipts(&record.peer.device_id, 100)? {
        let id = &receipt.message_id;
        let in_scope = id.starts_with("membership:")
            || id.starts_with("enrollment-")
            || id.starts_with("snapshot-start:")
            || id.starts_with("snapshot-end:");
        if in_scope && !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    // due_enrollment_wires 的白名单上限是 8 条（两处必须一致，否则
    // `sync_enrollment_wire_scope_invalid`）。这个数原本被抬到 32 是为了让 174 片
    // 逐片回执每拍多走几条 —— 逐片回执随 mail 分片路径删除后，固定项（roster /
    // offer / snapshot-end / completion）+ 几条迟到的 membership 回执就是全部，
    // 8 条足够，也不再需要为「几百封信」留预算。
    ids.truncate(8);
    Ok(ids)
}

pub(crate) fn stage_receipts(
    store: &Store,
    record: &EnrollmentRecord,
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> Result<()> {
    let subjects = receipt_subjects(store, record)?;
    for message in subjects {
        let hash: Option<String> = store.connection.query_row("SELECT payload_hash FROM sync_wire_receipts WHERE target_device_id=?1 AND message_id=?2 AND pending=1",
            rusqlite::params![record.peer.device_id, message], |r| r.get(0)).optional()?;
        let Some(hash) = hash else {
            continue;
        };
        let wire_id = envelope_receipt_id(&message);
        // joiner 的回执是链路终点：没有任何一方会再回执它，outbox 行投递成功后
        // 永远停在 pending。所以「已送达」只能看 relay 有没有给 relay_id
        // （enrollment_wire_state 的第二列）。已送达就把该 subject 收尾，
        // 否则每拍的重推会无限 prolong 交接（且 owner 的 completion 信令因为
        // 拿不到回执永远不 acknowledged，两端卡在「等待成员授权与初始同步」）。
        //
        // 曾经这里有「owner_confirmed 就直接全部 mark_sent」的早退，它把
        // completion 回执一起吐了——而 completion 回执恰恰只能在 owner_confirmed
        // 之后才存在，于是回执永不发出。现在改成逐条看送达状态：已送达才收尾，
        // 未送达的继续 stage（stage_sync_wire_envelope 对同 hash 幂等，不会重封）。
        if store
            .enrollment_wire_state(&record.peer.device_id, &wire_id)?
            .is_some_and(|(_, delivered)| delivered)
        {
            store.mark_sync_wire_receipt_sent(&record.peer.device_id, &message)?;
            continue;
        }
        let receipt = EnvelopeReceipt {
            dataset_id: record.dataset_id.clone(),
            acknowledging_device_id: record.local_device_id.clone(),
            target_device_id: record.peer.device_id.clone(),
            message_id: message.clone(),
            payload_hash: hash,
        };
        let header = EnvelopeHeader::new(
            &record.dataset_id,
            &record.local_device_id,
            &record.peer.device_id,
            record.key_version,
            &wire_id,
        )?;
        store.stage_sync_wire_envelope(&header, key, signer, &serde_json::to_vec(&receipt)?)?;
        store.rearm_sync_wire_receipt(&record.peer.device_id, &wire_id)?;
    }
    Ok(())
}

pub(crate) fn wire_acknowledged(store: &Store, peer: &str, message: &str) -> Result<bool> {
    Ok(store
        .enrollment_wire_state(peer, message)?
        .is_some_and(|(state, _)| state == "acknowledged"))
}

/// 这一拍允许发哪些信。
///
/// 快照线只有一封：`snapshot-end:{id}`（manifest）。它要在 offer 被确认后才进
/// 白名单，而 SQL 里还有一道 `sync_snapshot_exports.state='ready'` 的闸门 ——
/// 后者保证「分片还在传就宣告完成」不可能发生。以前这里返回第二个值
/// (`snapshot_allowed`) 去放开 `snapshot:{id}:{n}` 的逐片信封，随 mail 分片路径
/// 删除（2026-10-26 C5-A）。
pub(crate) fn outgoing_scope(store: &Store, record: &EnrollmentRecord) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    if record.role == EnrollmentRole::Joiner {
        ids = receipt_subjects(store, record)?
            .iter()
            .map(|id| envelope_receipt_id(id))
            .collect();
    } else if let Some(roster) = snapshot_roster_id(store, record)? {
        ids.push(roster.clone());
        if wire_acknowledged(store, &record.peer.device_id, &roster)? {
            let snapshot = record.snapshot.as_ref().unwrap();
            let offer = offer_id(&record.invitation_id, &snapshot.id);
            ids.push(offer.clone());
            // `snapshot-start:{id}`：blob 数据面的开场信令。它不带分片，只告诉
            // 接收端「分片在哪个 blob 上」，所以可以和 offer 一起推；数据面本身
            // 有 `blob_open` 的 404 兜底，信令迟到不会让双方各说各话。
            ids.push(format!("snapshot-start:{}", snapshot.id));
            if wire_acknowledged(store, &record.peer.device_id, &offer)? {
                ids.push(format!("snapshot-end:{}", snapshot.id));
            }
            if record.owner_confirmed {
                ids.push(completion_id(&record.invitation_id, &snapshot.id));
            }
        }
    }
    Ok(ids)
}

pub(crate) fn push_due(
    store: &Store,
    client: &RelayClient,
    record: &EnrollmentRecord,
    key: &ContentKey,
    signer: &DeviceIdentity,
    now_ms: i64,
    check: impl Fn() -> Result<()>,
) -> Result<PushReport> {
    if record.role == EnrollmentRole::Joiner {
        stage_receipts(store, record, key, signer)?;
    }
    let ids = outgoing_scope(store, record)?;
    if ids.is_empty() {
        return Ok(PushReport::default());
    }
    let entries = store.due_enrollment_wires(&record.peer.device_id, now_ms, &ids)?;
    let credential = RelayClient::derive_sender_credential(
        &key.to_bytes(),
        &record.route.local_mailbox,
        &record.route.peer_mailbox,
    )?;
    let mut report = PushReport::default();
    for entry in entries {
        check()?;
        // 同 engine::push_due：seq_hint 取自 outbox 持久列，重发保持同值。
        let result = client.send(
            &record.route.peer_mailbox,
            &record.route.local_mailbox,
            &credential,
            entry.seq_hint,
            &STANDARD.encode(&entry.encrypted_envelope),
            now_ms + crate::sync::enrollment::GRANT_LIFETIME_MS,
        );
        check()?;
        match result {
            Ok(relay_id) => {
                store.record_sync_wire_attempt(
                    &record.peer.device_id,
                    &entry.message_id,
                    now_ms,
                    Some(&relay_id),
                    None,
                )?;
                report.uploaded += 1;
                report.uploaded_bytes += entry.encrypted_envelope.len();
                if entry.message_id.starts_with("envelope-receipt-") {
                    let envelope = EncryptedEnvelope::from_bytes(&entry.encrypted_envelope)?;
                    let header = EnvelopeHeader::new(
                        &record.dataset_id,
                        &record.local_device_id,
                        &record.peer.device_id,
                        record.key_version,
                        &entry.message_id,
                    )?;
                    let payload = crypto::open(&envelope, &header, key, &signer.public_key())?;
                    let receipt: EnvelopeReceipt = serde_json::from_slice(&payload)?;
                    ensure!(
                        receipt_subjects(store, record)?.contains(&receipt.message_id),
                        "sync_enrollment_receipt_scope_invalid"
                    );
                    let tx = store.begin_write_transaction()?;
                    store
                        .mark_sync_wire_receipt_sent(&record.peer.device_id, &receipt.message_id)?;
                    store.acknowledge_sync_wire(
                        &record.peer.device_id,
                        &entry.message_id,
                        &crate::storage::sync::hash(&payload),
                    )?;
                    if let Some(tx) = tx {
                        tx.commit()?;
                    }
                }
            }
            Err(error) => {
                // 同 engine::push_due：507 不是"没送到"，是"现在发什么都没用"。
                // 停止本拍发送并退避，否则 16 封一起撞上限，每封都白洗一次计数。
                let failure = crate::sync::engine::deliver::classify_delivery_failure(&error);
                // 同 engine::push_due：一封一行只记分类码，不落远端响应体。
                crate::sync::engine::deliver::wire_log(
                    "push",
                    &entry.message_id,
                    entry.encrypted_envelope.len(),
                    failure.code(),
                );
                if failure == crate::sync::engine::deliver::DeliveryFailure::Quota {
                    // 阻塞要落表（`blocked_reason` + `last_error`），否则
                    // `deliver_statuses` 的 blocked 分支永远没人写，面板只能靠猜。
                    store.block_sync_wire(
                        &record.peer.device_id,
                        &entry.message_id,
                        failure.code(),
                        now_ms + crate::sync::engine::deliver::backoff_ms(failure, 0),
                    )?;
                    report.quota_exhausted = true;
                    break;
                }
                store.record_sync_wire_attempt(
                    &record.peer.device_id,
                    &entry.message_id,
                    now_ms,
                    None,
                    Some(failure),
                )?;
                report.failed += 1;
            }
        }
    }
    // 与 `engine::push_due` 同一口径（C3-1）：交接路径也必须「成败都打」，
    // 否则初始同步的 174 封信只会在 relay 侧看得到，本地日志一片空白。
    if let Some(line) = crate::sync::engine::push_summary_line_opt(&record.peer.device_id, &report)
    {
        crate::sync::engine::deliver::wire_emit(line);
    }
    Ok(report)
}

pub(crate) fn accept(
    store: &Store,
    installation: &str,
    master: &[u8; 32],
    revision: i64,
    record: &EnrollmentRecord,
    ciphertext: &str,
    key: &ContentKey,
) -> Result<ReceiveOutcome> {
    if super::defer_snapshot_start(store, ciphertext)? {
        return Ok(ReceiveOutcome::Waiting);
    }
    let wire = authenticate_relay_payload(
        store,
        ciphertext,
        &record.peer.device_id,
        record.key_version,
        key,
        &record.peer.signing_key,
    )?;
    let id = &wire.header.message_id;
    if id.starts_with("membership:") {
        if record.role != EnrollmentRole::Joiner {
            return Ok(ReceiveOutcome::Waiting);
        }
        let commit = store.apply_enrollment_roster(
            installation,
            master,
            revision,
            &record.invitation_id,
            id,
            &wire.plaintext,
        )?;
        return Ok(ReceiveOutcome::Durable(Some(commit)));
    }
    if id.starts_with("enrollment-offer:") || id.starts_with("enrollment-complete:") {
        if record.role != EnrollmentRole::Joiner {
            return Ok(ReceiveOutcome::Waiting);
        }
        ensure!(
            wire.plaintext.len() <= 16 * 1024,
            "sync_enrollment_control_too_large"
        );
        let offer: SnapshotOffer = serde_json::from_slice(&wire.plaintext)
            .map_err(|_| anyhow::anyhow!("sync_enrollment_control_invalid"))?;
        offer.validate_for(record)?;
        let complete = id.starts_with("enrollment-complete:");
        let expected = if complete {
            completion_id(&offer.invitation_id, &offer.snapshot_id)
        } else {
            offer_id(&offer.invitation_id, &offer.snapshot_id)
        };
        ensure!(*id == expected, "sync_enrollment_control_identity_mismatch");
        let commit = if complete {
            store.complete_enrollment(
                installation,
                master,
                revision,
                &record.invitation_id,
                Some((&offer, &wire.plaintext)),
            )?
        } else {
            store.select_enrollment_snapshot(
                installation,
                master,
                revision,
                &record.invitation_id,
                &offer,
                &wire.plaintext,
            )?
        };
        return Ok(ReceiveOutcome::Durable(Some(commit)));
    }
    if id.starts_with("snapshot-start:")
        || id.starts_with("snapshot:")
        || id.starts_with("snapshot-end:")
    {
        if record.role != EnrollmentRole::Joiner {
            return Ok(ReceiveOutcome::Waiting);
        }
        let is_start = id.starts_with("snapshot-start:");
        match &record.snapshot {
            Some(selected) => ensure!(
                // `snapshot-start` 是 blob 传输的开场信令：它先于任何分片到达，
                // 只带 blob 身份，安全等级与 snapshot 分片本身相同。
                id == &format!("snapshot-start:{}", selected.id)
                    || id == &format!("snapshot-end:{}", selected.id)
                    || id.starts_with(&format!("snapshot:{}:", selected.id)),
                "sync_enrollment_snapshot_identity_mismatch"
            ),
            // 还没选过快照（offer 还没到）：**只有** `snapshot-start` 可以先收。
            //
            // 它是「分片在哪个 blob 上」的线索，而且它是接收端唯一能开始拉数据的
            // 入口 —— 压着它等 offer，就等成了互等：offer 要导出 `ready`，`ready`
            // 要全部分片传完，分片传完要接收端 ack，接收端 ack 要先去拉。
            // 收下它不改本机任何既有数据：`begin_blob_snapshot_import` 自带
            // 「发送方是活跃成员」与「目标库为空」两道闸，真正的**发布**仍然要等
            // offer 里的 manifest 哈希（下面 `snapshot-end` 分支），所以未选中的
            // 快照永远进不了本机库。
            None if is_start => {}
            None => return Ok(ReceiveOutcome::Waiting),
        }
        if id.starts_with("snapshot-end:") {
            // `snapshot-end:` 只能来自已选中的快照（上面 `match` 已把未选中的情况
            // 退成 `Waiting`），所以这里 `record.snapshot` 一定在。
            let selected = record
                .snapshot
                .as_ref()
                .context("sync_enrollment_snapshot_missing")?;
            ensure!(
                selected.manifest_hash.as_deref()
                    == Some(crate::storage::sync::hash(&wire.plaintext).as_str()),
                "sync_enrollment_manifest_changed"
            );
        }
        snapshot::accept_snapshot(
            store,
            &wire.bytes,
            &wire.header,
            key,
            &record.peer.signing_key,
            &wire.plaintext,
        )?;
        return Ok(ReceiveOutcome::Durable(None));
    }
    if id.starts_with("envelope-receipt-") && record.role == EnrollmentRole::Owner {
        ensure!(
            wire.plaintext.len() <= 16 * 1024,
            "sync_enrollment_control_too_large"
        );
        let receipt: EnvelopeReceipt = serde_json::from_slice(&wire.plaintext)?;
        let mut subjects = Vec::new();
        if let Some(roster) = snapshot_roster_id(store, record)? {
            subjects.push(roster);
        }
        if let Some(selected) = &record.snapshot {
            subjects.extend([
                offer_id(&record.invitation_id, &selected.id),
                // `snapshot-start:{id}` 也要收：它是 blob 数据面的开场通知，只
                // 带 blob 身份；不收它的回执，发送端就会每拍重投这条信令。
                format!("snapshot-start:{}", selected.id),
                format!("snapshot-end:{}", selected.id),
                completion_id(&record.invitation_id, &selected.id),
            ]);
        }
        // 只有一种快照回执：`snapshot-end:{id}` = 整份导出已在接收方发布。
        // 它是唯一能推进导出状态的回执（`acknowledge_snapshot_export` 顺带播种
        // watermark、给 GC 松绑）。逐片回执（`snapshot:{id}:{n}`）曾是 mail 传输
        // 的不变式①在信箱里的对偶，随 mail 分片路径一起删除（2026-10-26 C5-A）；
        // 分片级的「已落盘」现在由数据面的 `blob_ack` 表达，不必再绕一圈信件。
        let published = receipt
            .message_id
            .strip_prefix("snapshot-end:")
            .map(str::to_string);
        ensure!(
            receipt.dataset_id == record.dataset_id
                && receipt.acknowledging_device_id == record.peer.device_id
                && receipt.target_device_id == record.local_device_id
                && (subjects.contains(&receipt.message_id) || published.is_some())
                && *id == envelope_receipt_id(&receipt.message_id),
            "sync_enrollment_receipt_identity_mismatch"
        );
        if let Some(snapshot) = published {
            store.acknowledge_snapshot_export(
                &record.peer.device_id,
                &snapshot,
                &receipt.message_id,
                &receipt.payload_hash,
            )?;
        } else {
            store.acknowledge_sync_wire(
                &record.peer.device_id,
                &receipt.message_id,
                &receipt.payload_hash,
            )?;
        }
        return Ok(ReceiveOutcome::Durable(None));
    }
    // In particular, provider/op/op-part content cannot precede initial selection.
    Ok(ReceiveOutcome::Waiting)
}
