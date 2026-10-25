//! Encrypted snapshot transfer orchestration. The receiver must explicitly select
//! a fresh staging database and request ID before accepting any snapshot content.

use anyhow::{Context, Result, ensure};

use super::*;
use crate::storage::sync::{BeginExport, SnapshotTransport};
use crate::storage::sync::snapshot::{SnapshotExporter, SnapshotStart};
use crate::sync::protocol::{ApplicationReceipt, validate_id};
use crate::sync::transport_relay::{BlobState, RelayClient, WINDOW_CHUNKS, random_hex};

/// 上传一份快照所需的 relay 上下文。发送侧用的是**接收方信箱里的 sender 凭据**：
/// 上传方在 v2 里本来就是那个信箱里被授权的 sender 身份，让它持 owner 凭据等于
/// 把能读/删整个信箱的主密钥交给上传方，与盲中继的信任模型冲突。
///
/// 没有这个上下文（离线重建、纯本地测试）时自动退回 mail 传输。
pub(crate) struct BlobUpload {
    pub(crate) client: RelayClient,
    /// 对端信箱（收信方）。
    pub(crate) mailbox: String,
    /// 本机在该信箱里的 sender 身份。
    pub(crate) sender_mb: String,
    pub(crate) credential: [u8; 32],
    /// 对端设备 id 与这轮使用的密钥版本（`snapshot-start` 信令要用）。
    pub(crate) recipient: String,
    pub(crate) key_version: u64,
    /// 这一拍最多等接收方 ack 多久。这是**调用方**的性质（一个 tick 的预算），
    /// 不是进程的配置：生产给 `WINDOW_WAIT_MS`，测试给几百毫秒，否则集成测试
    /// 每窗都等满 20 秒。
    pub(crate) window_wait_ms: u64,
}

/// 生产环境一拍的等 ack 上限。等不到就本拍收手留在 `building`，下一拍从断点
/// 续传，绝不在调用线程里无限等（那是把配对卡死的经典写法）。真实规模
/// （122 MB / 174 片）下单拍传不完，本拍收手是常态。
pub(crate) const WINDOW_WAIT_MS: u64 = 20_000;
/// 轮询间隔：一轮 = 接收方一个 tick 的量级。
const WINDOW_POLL_MS: u64 = 250;

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
    upload: Option<&BlobUpload>,
) -> Result<()> {
    stage_snapshot_inner(store, recipient, id, key_version, key, signer, false, upload)
}

pub(crate) fn stage_enrollment_snapshot(
    store: &Store,
    recipient: &str,
    id: &str,
    key_version: u64,
    key: &ContentKey,
    signer: &DeviceIdentity,
    upload: Option<&BlobUpload>,
) -> Result<()> {
    stage_snapshot_inner(
        store,
        recipient,
        id,
        key_version,
        key,
        signer,
        true,
        upload,
    )
}

fn stage_snapshot_inner(
    store: &Store,
    recipient: &str,
    id: &str,
    key_version: u64,
    key: &ContentKey,
    signer: &DeviceIdentity,
    with_authority: bool,
    upload: Option<&BlobUpload>,
) -> Result<()> {
    ensure!(
        store.connection.is_autocommit(),
        "sync_snapshot_export_requires_autocommit"
    );
    // 导出前补收：正文中占位符/遗留绝对路径引用的图片字节若未入库
    // （存量页导入早于资产表），先收齐再钉住导出视图，否则接收端拿空。
    // 收不齐不阻断导出：接收端缺图时显示占位符原文，重拍快照可再补。
    let _ = store.ingest_wiki_page_assets();
    // 导出前用域表事实自愈头表：live head 指向已不存在的域行会让接收端
    // 的 (deleted ⇔ 行存在) 一致性闸门必炸（快照整块作废）。只修不改
    // 其它状态，幂等。
    store.heal_snapshot_heads()?;
    // 传输方式只看「有没有 blob 上传上下文」：调用方给了 `upload` 就是要走
    // 数据面（那是 `snapshot_upload()` 在 env 里读到 blob 才构造出来的），否则
    // 走信件；要求 blob 但拿不到 relay 上下文（离线/纯本地）时同样退回信件 ——
    // 宁可慢，也不能让"快照只存在于一个我们连不上的地方"。
    let transport = if upload.is_some() {
        SnapshotTransport::Blob
    } else {
        SnapshotTransport::Mail
    };
    let (transport, resume) = match store.begin_snapshot_export(id, recipient, key_version, transport)?
    {
        BeginExport::Done => return Ok(()),
        BeginExport::Begun(transport) => (transport, None),
        BeginExport::Resume {
            transport,
            blob_index,
            blob_id,
        } => (transport, Some((blob_index, blob_id))),
    };
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
        if transport == SnapshotTransport::Blob {
            let upload = upload.context("sync_snapshot_blob_uploader_missing")?;
            // 只有 manifest 进 outbox（R4-3）：分片全在 relay 的 blob 上。
            return upload_blob_chunks(store, upload, id, &mut exporter, key, signer, resume);
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

/// blob 数据面上传：一次一片，窗口满就等接收方 ack，等不到就本拍收手
/// （导出保持 `building`，下一拍从 `blob_index` 断点续传）。
///
/// 片密文与信件路径**同构**（同一套 `crypto::seal`），只是不进 outbox 而是直接
/// PUT；因此接收端的 `stage_snapshot_chunk` 一行都不用改。
#[allow(clippy::too_many_arguments)]
fn upload_blob_chunks(
    store: &Store,
    upload: &BlobUpload,
    id: &str,
    exporter: &mut SnapshotExporter,
    key: &ContentKey,
    signer: &DeviceIdentity,
    resume: Option<(u64, Option<String>)>,
) -> Result<()> {
    let client = &upload.client;
    let mailbox = upload.mailbox.as_str();
    let sender_mb = upload.sender_mb.as_str();
    let credential = &upload.credential;
    // blob 身份：同一份导出只开一次，重试用同一个 id（旧的靠 TTL 收）。
    let blob_id = match resume.as_ref().and_then(|(_, blob_id)| blob_id.clone()) {
        Some(blob_id) => blob_id,
        None => {
            let blob_id = random_hex(24);
            store.set_snapshot_blob_id(id, &blob_id)?;
            blob_id
        }
    };
    exporter.declare_blob(&blob_id);
    let opened = client.blob_open(mailbox, sender_mb, credential, &blob_id)?;
    announce_blob_snapshot(store, upload, id, signer, key, &blob_id)?;
    // 续传点必须跟 relay 的事实对齐：blob 刚建（原来的被 TTL 收掉/换了 relay）
    // 就只能从 0 重来，否则跳过的那段片永远到不了对端。
    let start = if opened.created {
        0
    } else {
        let stored = resume.map(|(index, _)| index).unwrap_or(0);
        let state = client.blob_state(mailbox, &blob_id, credential, Some(sender_mb))?;
        let relayed = state
            .as_ref()
            .and_then(|state| state.highest)
            .map_or(0, |highest| highest as u64 + 1);
        stored.max(relayed)
    };
    if start > 0 {
        store.mark_snapshot_blob_progress(id, start)?;
    }
    exporter.skip_chunks(usize::try_from(start)?)?;
    let mut sent_bytes = 0usize;
    let mut index = usize::try_from(start)?;
    let mut acked = current_ack(&ack_state(client, mailbox, sender_mb, credential, &blob_id)?, index);
    let wait_deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(upload.window_wait_ms);
    while let Some(bytes) = exporter.next_encrypted_chunk(key, signer)? {
        // 在途窗口：接收方还没 ack 的片数不能超过一窗，否则 relay 上就会堆下整份。
        while index.saturating_sub(acked) >= WINDOW_CHUNKS {
            if std::time::Instant::now() >= wait_deadline {
                // 接收方这一拍没跟上：留在 building，下一拍从 index 继续。
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(WINDOW_POLL_MS));
            acked = current_ack(&ack_state(client, mailbox, sender_mb, credential, &blob_id)?, index);
        }
        client.blob_put(mailbox, sender_mb, credential, &blob_id, index, &bytes)?;
        index += 1;
        sent_bytes += bytes.len();
        store.mark_snapshot_blob_progress(id, index as u64)?;
    }
    let manifest = exporter.finish(key, signer)?;
    // 诊断：blob 上传收尾。`subject` 是快照 id 前缀（blob 事件不属于某封信，
    // 但这一行要能跟 relay 日志里的同一个 blob 对上），字节数是本拍真传上去的量。
    super::deliver::wire_log(
        "blob-upload",
        &id[..8.min(id.len())],
        sent_bytes,
        &format!("chunks {} blob {}", index, &blob_id[..8.min(blob_id.len())]),
    );
    // manifest 是唯一进 outbox 的东西 —— 它才是「去 blob 拉这份快照」的信令。
    stage_piece(store, id, &manifest, key, signer, true)
}

/// 发出 `snapshot-start` 信令：告诉接收端「这份快照的分片在这个 blob 上，可以
/// 开始拉了」。**必须立刻推一次**——放进 outbox 等下一拍，接收端整整一拍都不会
/// 动，而发送端这一拍正卡在窗口上等它的 ack。
fn announce_blob_snapshot(
    store: &Store,
    upload: &BlobUpload,
    id: &str,
    signer: &DeviceIdentity,
    key: &ContentKey,
    blob_id: &str,
) -> Result<()> {
    let identity = store.sync_identity()?.context("sync_not_initialized")?;
    let message_id = format!("snapshot-start:{id}");
    let header = EnvelopeHeader::new(
        &identity.dataset_id,
        &identity.device_id,
        &upload.recipient,
        upload.key_version,
        message_id.clone(),
    )?;
    let payload = serde_json::to_vec(&serde_json::json!({
        "version": 1,
        "blob_id": blob_id,
    }))?;
    store.stage_sync_wire_envelope(&header, key, signer, &payload)?;
    let Some(entry) = store.pending_wire_envelope(&upload.recipient, &message_id)? else {
        // 已经发出去过了（重试/续传）：不需要再推，接收端迟早会拉到。
        return Ok(());
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    let expiry = now_ms + 7 * 24 * 60 * 60 * 1000;
    match upload.client.send(
        &upload.mailbox,
        &upload.sender_mb,
        &upload.credential,
        entry.seq_hint,
        &STANDARD.encode(&entry.encrypted_envelope),
        expiry,
    ) {
        Ok(relay_id) => {
            store.record_sync_wire_attempt(
                &upload.recipient,
                &message_id,
                now_ms,
                Some(&relay_id),
                None,
            )?;
        }
        // 推不出去不影响上传本身：outbox 里还有这行，下一拍会重发。真正会卡住
        // 的场景（接收端等不到信令）由下一拍的重发自然解决。
        Err(error) => {
            let failure = super::deliver::classify_delivery_failure(&error);
            store.record_sync_wire_attempt(
                &upload.recipient,
                &message_id,
                now_ms,
                None,
                Some(failure),
            )?;
            super::deliver::wire_log(
                "snapshot-start-deferred",
                &message_id,
                entry.encrypted_envelope.len(),
                failure.code(),
            );
        }
    }
    Ok(())
}

/// 接收方已确认到第几片（不含）。relay 上看不到未 ack 的片时，视作全部已确认。
fn ack_state(
    client: &RelayClient,
    mailbox: &str,
    sender_mb: &str,
    credential: &[u8; 32],
    blob_id: &str,
) -> Result<Option<BlobState>> {
    client.blob_state(mailbox, blob_id, credential, Some(sender_mb))
}

fn current_ack(state: &Option<BlobState>, uploaded: usize) -> usize {
    match state {
        // 还没有任何片在 relay 上：要么一片没传，要么全被 ack 了。
        Some(state) if state.present.is_empty() => uploaded,
        Some(state) => state.acked_upto().min(uploaded),
        None => 0,
    }
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
) -> Result<bool> {
    let tx = store.begin_write_transaction()?;
    let published = store.publish_snapshot(id, sender, key_version, bytes, key, public)?;
    if published {
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
    Ok(published)
}

/// 接收端从 blob 拉片的通道（owner 凭据：读/确认自己的信箱是 owner 权限）。
pub(crate) struct BlobDownload<'a> {
    pub(crate) client: &'a RelayClient,
    pub(crate) mailbox: &'a str,
    pub(crate) owner_credential: &'a [u8; 32],
    pub(crate) source: &'a str,
    pub(crate) key_version: u64,
    pub(crate) key: &'a ContentKey,
    pub(crate) public: &'a [u8; 32],
}

/// 从 blob 上把在途快照的分片拉下来：一窗一窗拉，**每窗确认释放**（`blob_ack`），
/// 所以 relay 上最多同时存在一窗 + 发送端还没被 ack 的那点尾巴，而不是整份快照。
///
/// 返回 `true` 表示这一轮发布了一份快照（调用方可以提前收工）。
pub(crate) fn import_blob_snapshots(
    store: &Store,
    download: &BlobDownload<'_>,
    rounds: usize,
) -> Result<bool> {
    for import in store.pending_blob_imports(download.source)? {
        ensure!(
            import.key_version == download.key_version,
            "sync_snapshot_identity_conflict"
        );
        let mut from = import.highest.map_or(0, |highest| highest + 1);
        // 诊断：blob 拉取只在 ELSEWHEN_DEBUG 下打进度（片号/长度，不打正文）。
        let trace = std::env::var("ELSEWHEN_DEBUG").is_ok();
        for _ in 0..rounds {
            let window = download.client.blob_get_window(
                download.mailbox,
                download.owner_credential,
                &import.blob_id,
                from,
                WINDOW_CHUNKS,
            )?;
            let Some(start) = window.from else {
                // relay 上没有这一片起的东西：发送端还没传到，或整份被回收了。
                // 两种都只能等下一拍；真正作废由 GC/重快照兜底。
                break;
            };
            if window.chunks.is_empty() || start > from {
                // 缺口（前面被 ack 掉了还没重传）：不能跳着放，等发送端补。
                break;
            }
            let tx = store.begin_write_transaction()?;
            for (offset, bytes) in window.chunks.iter().enumerate() {
                store.stage_snapshot_chunk(
                    &import.id,
                    start + offset,
                    download.source,
                    download.key_version,
                    bytes,
                    download.key,
                    download.public,
                )?;
            }
            if let Some(tx) = tx {
                tx.commit()?;
            }
            let upto = start + window.chunks.len() - 1;
            // 先落盘后确认：本地事务已提交，才敢让 relay 删掉这一窗。
            download.client.blob_ack(
                download.mailbox,
                download.owner_credential,
                &import.blob_id,
                upto,
            )?;
            from = upto + 1;
            if trace {
                super::deliver::wire_log(
                    "blob-import",
                    &import.id[..8.min(import.id.len())],
                    window.chunks.iter().map(|c| c.len()).sum::<usize>(),
                    &format!(
                        "window {}..{} next {}",
                        start,
                        start + window.chunks.len() - 1,
                        from
                    ),
                );
            }
            let Some(manifest) = store.staged_snapshot_manifest(&import.id)? else {
                continue;
            };
            if publish_and_receipt(
                store,
                &import.id,
                download.source,
                download.key_version,
                &manifest,
                download.key,
                download.public,
            )? {
                // 发布成功：整块回收，不再占 relay 空间（对齐「close 后残留 0」）。
                // 回收失败**不算导入失败**：快照已经发布、回执也排上了，重试这一拍
                // 什么都不会重做。失败只是让 blob 在 relay 上多待一会儿，下一拍
                // `recycle_published_blobs` 接着关（close 在 relay 侧是幂等的）。
                if let Err(error) = close_published_blob(store, download, &import.id, &import.blob_id, "done")
                {
                    super::deliver::wire_log(
                        "blob-recycle",
                        &import.id[..8.min(import.id.len())],
                        0,
                        &format!("deferred {error:#}"),
                    );
                }
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// 关掉一份快照在 relay 上的整块 blob，并记下「已回收」。
///
/// **先 close 后落标记**：反过来会在 close 失败时留下「本地以为回收了、relay 上还
/// 占着」的永久谎言；而顺序正确的失败只是「多待一拍」，下一拍重关即可。
///
/// `status` 由快照状态决定：`aborted` 是撤回（`cancel`），其余是消费完成（`done`）。
fn close_published_blob(
    store: &Store,
    download: &BlobDownload<'_>,
    id: &str,
    blob_id: &str,
    status: &str,
) -> Result<()> {
    download.client.blob_close(
        download.mailbox,
        download.owner_credential,
        blob_id,
        status,
        None,
    )?;
    store.mark_snapshot_blob_recycled(id, chrono::Utc::now().timestamp_millis())
}

/// 补关以前失败过的 blob（review m4）。
///
/// 与 `import_blob_snapshots` 分开的原因是两者门控条件不同：这里处理的行已经没有
/// 在途分片了（`state='applied'`），`snapshot_import_pending()` 对它们是 false ——
/// 把回收塞进导入流程，最后一块 blob 就再也等不到下一拍。
///
/// 一轮里每行只试一次：close 失败留在表里，下一拍再来（relay 侧 30 分钟 TTL 仍是
/// 最终兜底，不会无限堆积）。
pub(crate) fn recycle_published_blobs(
    store: &Store,
    download: &BlobDownload<'_>,
) -> Result<usize> {
    let mut recycled = 0;
    for (id, blob_id, state) in store.pending_blob_recycles()? {
        // 作废的那份要 `cancel`（撤回），已发布的要 `done`（消费完成）。
        let status = if state == "aborted" { "cancel" } else { "done" };
        if close_published_blob(store, download, &id, &blob_id, status).is_ok() {
            recycled += 1;
        } else {
            super::deliver::wire_log(
                "blob-recycle",
                &id[..8.min(id.len())],
                0,
                "still pending",
            );
        }
    }
    Ok(recycled)
}

pub(super) fn accept_snapshot(
    store: &Store,
    bytes: &[u8],
    header: &EnvelopeHeader,
    key: &ContentKey,
    public: &[u8; 32],
    plaintext: &[u8],
) -> Result<bool> {
    if let Some(id) = header.message_id.strip_prefix("snapshot-start:") {
        // 只是「分片在哪个 blob 上」的提前线索：真正发布仍由 manifest 决定。
        validate_id(id)?;
        let start: SnapshotStart = serde_json::from_slice(&plaintext)?;
        ensure!(start.version == 1, "sync_snapshot_start_version");
        // 「记下 blob 位置」与「排回执」必须同一个事务：否则写成功但回执丢，
        // 发送端会永远重投 start（它是 outbox 里退不掉的那一行）。
        let tx = store.begin_write_transaction()?;
        store.begin_blob_snapshot_import(
            id,
            &header.sender_device_id,
            header.key_version,
            &start.blob_id,
        )?;
        // 已收口也要回执：对端要的只是「我收到了这条通知」，不是「你再传一遍」。
        store.queue_sync_wire_receipt(
            &header.sender_device_id,
            &header.message_id,
            &crate::storage::sync::hash(&plaintext),
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        return Ok(true);
    }
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
