//! 快照导入入口：staging 数据库校验、分片暂存与发布。

use super::*;
use crate::sync::protocol::validate_id;

/// 一份在途的 blob 导入。
#[derive(Debug, Clone)]
pub(crate) struct BlobImport {
    pub(crate) id: String,
    pub(crate) key_version: u64,
    pub(crate) blob_id: String,
    /// 已落盘的最高片号（相对 0 起）。
    pub(crate) highest: Option<usize>,
}

// `snapshot-start` 的处理结论在调用侧没有任何分支需要知道：「新记下」与「已收口」
// 接下来都是「回执 + `Ok(true)`」。所以这里不返回枚举 —— 一个没有消费者的三态枚举
// 就是新死代码（正是 review 拿来批评 `is_blocking` 的那种形态）。区别只体现在
// **行为**上：已收口的快照不会被改动任何一行。
//
// 但「已收口」必须能与「出错」区分开：`snapshot-start` 是**通知**不是数据，对端
// 因为丢了回执而重投是正常现象；把它当错误会让接收端按 `mail_id` 累计拒收，三次后
// drop 掉那封信，发送端再发又是一封新 `mail_id`、计数归零 —— 无限循环。

fn validate_blob_id(blob_id: &str) -> Result<()> {
    ensure!(
        (16..=64).contains(&blob_id.len())
            && blob_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'),
        "sync_snapshot_blob_id_invalid"
    );
    Ok(())
}

impl Store {
    /// Only a fresh staging database may become an initial replica. The App
    /// keeps its existing local dataset active until this one is fully published.
    pub(crate) fn begin_snapshot_import(
        &self,
        id: &str,
        source: &str,
        key_version: u64,
    ) -> Result<()> {
        validate_id(id)?;
        validate_id(source)?;
        ensure!(
            (1..=i64::MAX as u64).contains(&key_version),
            "sync_snapshot_key_version_invalid"
        );
        let tx = self.begin_write_transaction()?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        ensure!(source != identity.device_id, "sync_snapshot_self_import");
        self.snapshot_source_authorized(source)?;
        let existing: Option<(String, i64, String)> = self
            .connection
            .query_row(
                "SELECT source_device_id,key_version,state FROM sync_snapshots WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((peer, version, state)) = existing {
            ensure!(
                peer == source && version == key_version as i64 && state != "aborted",
                "sync_snapshot_identity_conflict"
            );
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(());
        }
        self.ensure_empty_snapshot_target()?;
        self.connection.execute(
            "UPDATE sync_snapshots SET state='aborted' WHERE state='staging'",
            [],
        )?;
        self.connection.execute("DELETE FROM sync_snapshot_chunks WHERE snapshot_id IN (SELECT id FROM sync_snapshots WHERE state='aborted')",[])?;
        self.connection.execute(
            "INSERT INTO sync_snapshots(id,source_device_id,key_version) VALUES(?1,?2,?3)",
            params![id, source, key_version as i64],
        )?;
        self.connection.execute("INSERT INTO sync_snapshot_target(singleton,snapshot_id) VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET snapshot_id=excluded.snapshot_id",[id])?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 一份正在从 blob 拉取的导入：分片在 relay 的哪个 blob 上、已经收到第几片。
    /// `highest` 是已经**落盘**的最高片号（`None` = 一片都没收）。
    ///
    /// 这里**不看** `manifest`：窗口流控的意义就是「manifest 还没生成时就开始拉」，
    /// 否则发送端等 ack、接收端等 manifest，两边一起卡到超时。分片本身是自验证的
    /// （`snapshot:{id}:{index}` 头 + 内容密钥），不必等 manifest 才敢落盘。
    pub(crate) fn pending_blob_imports(&self, source: &str) -> Result<Vec<BlobImport>> {
        let mut statement = self.connection.prepare(
            "SELECT s.id,s.key_version,s.blob_id,
                    (SELECT MAX(chunk_index) FROM sync_snapshot_chunks c WHERE c.snapshot_id=s.id)
             FROM sync_snapshots s JOIN sync_snapshot_target t ON t.snapshot_id=s.id
             WHERE s.state='staging' AND s.transport='blob' AND s.blob_id IS NOT NULL
               AND s.source_device_id=?1
             ORDER BY s.rowid",
        )?;
        let rows = statement
            .query_map([source], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, key_version, blob_id, highest)| {
                Some(BlobImport {
                    id,
                    key_version: u64::try_from(key_version).ok()?,
                    blob_id,
                    highest: highest.and_then(|h| usize::try_from(h).ok()),
                })
            })
            .collect())
    }

    /// 已发布、但 relay 上的整块 blob 还没回收的快照（review m4）。
    ///
    /// 为什么需要这张清单：发布与回执在同一个事务里提交之后，行就不再满足
    /// `pending_blob_imports`（那是 `state='staging'`），于是那一刻 `blob_close`
    /// 一失败就再也没有下一拍去关它 —— relay 上那块空间只能等 30 分钟 TTL。
    /// `blob_closed_at` 才是「已经回收了」的事实，与「还需不需要拉片」正交。
    ///
    /// 返回第三个元素是快照状态：`aborted`（作废）要用 `cancel` 关，
    /// 已发布会用 `done` 关 —— 两者在 relay 侧语义不同（一个是撤回、一个是完成），
    /// 但对空间的效果一样：立刻清零。
    pub(crate) fn pending_blob_recycles(&self) -> Result<Vec<(String, String, String)>> {
        let mut statement = self.connection.prepare(
            "SELECT id,blob_id,state FROM sync_snapshots
             WHERE transport='blob' AND blob_id IS NOT NULL AND blob_closed_at IS NULL
             ORDER BY rowid",
        )?;
        let rows = statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// 有没有待回收的 blob。给调用方一个便宜的探测：回收与「还有没有在途导入」
    /// 是两件事，用同一个条件门控会让最后一块 blob 永远等不到回收。
    pub(crate) fn blob_recycle_pending(&self) -> Result<bool> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_snapshots
               WHERE transport='blob' AND blob_id IS NOT NULL AND blob_closed_at IS NULL)",
            [],
            |r| r.get(0),
        )?)
    }

    /// 记下「relay 上那块 blob 已经关掉了」。
    ///
    /// 只改有 `blob_id` 的行：mail 传输的导出没有 blob，不该被写成「已回收」。
    pub(crate) fn mark_snapshot_blob_recycled(&self, id: &str, now_ms: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE sync_snapshots SET blob_closed_at=?2 WHERE id=?1 AND blob_id IS NOT NULL",
            params![id, now_ms],
        )?;
        Ok(())
    }

    /// `snapshot-start` 信令：先记下「这份快照走 blob、分片在哪个 blob 上」，
    /// 之后逐片拉取不再依赖 manifest（manifest 要等最后一片才生成，等它就死锁）。
    ///
    /// 幂等：同一 `id` 重复通知（对端没收到回执）直接返回 `Ok` 而不是报错。
    ///
    /// 为什么不能报错：`snapshot-start` 是**通知**不是数据，对端因丢回执重投是正常
    /// 现象；报错会让接收端按 `mail_id` 累计拒收，三次后 drop 掉那封信，发送端再发
    /// 又是一封新 `mail_id`、计数归零 —— 无限循环（review B1 的直接成因）。
    ///
    /// 只有「同一 id 换一个 blob_id」才是真冲突 —— blob 身份一旦定下就不许改，
    /// 换成另一个 blob 等于丢掉已收的分片。
    pub(crate) fn begin_blob_snapshot_import(
        &self,
        id: &str,
        source: &str,
        key_version: u64,
        blob_id: &str,
    ) -> Result<()> {
        validate_blob_id(blob_id)?;
        validate_id(id)?;
        // 先看有没有这一行：已收口的快照不能再走 `begin_snapshot_import`，
        // 它对 `aborted` 会报 identity_conflict、对 `applied` 也过不了后面的
        // `state == staging` 断言。授权仍要在这里自己补一遍，不能因为走上
        // 早退分支就绕开「发送方必须是活跃成员」。
        let existing: Option<(String, i64, String, Option<String>)> = self
            .connection
            .query_row(
                "SELECT source_device_id,key_version,state,blob_id FROM sync_snapshots WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        if let Some((peer, version, state, stored_blob)) = existing {
            ensure!(
                peer == source && version == key_version as i64,
                "sync_snapshot_identity_conflict"
            );
            self.snapshot_source_authorized(source)?;
            ensure!(
                stored_blob.is_none_or(|stored| stored == blob_id),
                "sync_snapshot_blob_conflict"
            );
            if state != "staging" {
                return Ok(());
            }
            let tx = self.begin_write_transaction()?;
            self.connection.execute(
                "UPDATE sync_snapshots SET transport='blob',blob_id=?2 WHERE id=?1",
                params![id, blob_id],
            )?;
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(());
        }
        // 新快照：保留 `begin_snapshot_import` 的空目标校验与「作废旧 staging」。
        self.begin_snapshot_import(id, source, key_version)?;
        let tx = self.begin_write_transaction()?;
        self.connection.execute(
            "UPDATE sync_snapshots SET transport='blob',blob_id=?2 WHERE id=?1",
            params![id, blob_id],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    pub(crate) fn snapshot_source_authorized(&self, source: &str) -> Result<()> {
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
            [source],
            |r| r.get(0),
        )?;
        ensure!(active, "sync_snapshot_source_not_authorized");
        Ok(())
    }
    pub(crate) fn ensure_empty_snapshot_target(&self) -> Result<()> {
        let has_data:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM events UNION ALL SELECT 1 FROM conversations UNION ALL SELECT 1 FROM messages UNION ALL SELECT 1 FROM input_records UNION ALL SELECT 1 FROM ai_provider_configs UNION ALL SELECT 1 FROM wiki_pages UNION ALL SELECT 1 FROM todos UNION ALL SELECT 1 FROM habit_nudges UNION ALL SELECT 1 FROM rules UNION ALL SELECT 1 FROM goals UNION ALL SELECT 1 FROM sync_operations)",[],|r|r.get(0))?;
        ensure!(!has_data, "sync_snapshot_requires_fresh_staging_database");
        Ok(())
    }
    pub(crate) fn selected_snapshot(
        &self,
        id: &str,
        source: &str,
        key_version: u64,
    ) -> Result<String> {
        self.snapshot_source_authorized(source)?;
        let row:Option<(String,i64,String)>=self.connection.query_row("SELECT s.source_device_id,s.key_version,s.state FROM sync_snapshots s JOIN sync_snapshot_target t ON t.snapshot_id=s.id WHERE s.id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let (peer, version, state) = row.context("sync_snapshot_not_selected")?;
        ensure!(
            peer == source && version == key_version as i64 && state != "aborted",
            "sync_snapshot_identity_conflict"
        );
        Ok(state)
    }

    pub(crate) fn stage_snapshot_chunk(
        &self,
        id: &str,
        index: usize,
        source: &str,
        key_version: u64,
        bytes: &[u8],
        key: &ContentKey,
        public_key: &[u8; 32],
    ) -> Result<()> {
        ensure!(index < MAX_CHUNKS, "sync_snapshot_chunk_limit");
        self.require_sync_transport_key(source, public_key, false)?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let expected = EnvelopeHeader::new(
            &identity.dataset_id,
            source,
            &identity.device_id,
            key_version,
            format!("snapshot:{id}:{index}"),
        )?;
        let envelope = EncryptedEnvelope::from_bytes(bytes)?;
        let plaintext = crypto::open(&envelope, &expected, key, public_key)?;
        let chunk: SnapshotChunk = serde_json::from_slice(&plaintext)
            .map_err(|_| anyhow::anyhow!("sync_snapshot_chunk_invalid"))?;
        ensure!(
            chunk.id == id
                && chunk.index == index
                && chunk.data.len() <= CHUNK_BYTES.div_ceil(3) * 4,
            "sync_snapshot_chunk_invalid"
        );
        let data = STANDARD
            .decode(&chunk.data)
            .map_err(|_| anyhow::anyhow!("sync_snapshot_chunk_invalid"))?;
        ensure!(
            !data.is_empty() && data.len() <= CHUNK_BYTES,
            "sync_snapshot_chunk_invalid"
        );
        let digest = super::hash(&data);
        let tx = self.begin_write_transaction()?;
        let state = self.selected_snapshot(id, source, key_version)?;
        let existing:Option<String>=self.connection.query_row("SELECT plaintext_hash FROM sync_snapshot_chunks WHERE snapshot_id=?1 AND chunk_index=?2",params![id,index as i64],|r|r.get(0)).optional()?;
        if let Some(existing) = existing {
            ensure!(existing == digest, "sync_snapshot_chunk_conflict");
        } else {
            ensure!(state != "applied", "sync_snapshot_already_published");
            self.connection.execute("INSERT INTO sync_snapshot_chunks(snapshot_id,chunk_index,encrypted_chunk,plaintext_hash) VALUES(?1,?2,?3,?4)",params![id,index as i64,bytes,digest])?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// False means chunks are missing. No domain data/frontier is committed until
    /// the authenticated manifest, every chunk, and the complete frame stream agree.
    pub(crate) fn publish_snapshot(
        &self,
        id: &str,
        source: &str,
        key_version: u64,
        bytes: &[u8],
        key: &ContentKey,
        public_key: &[u8; 32],
    ) -> Result<bool> {
        self.require_sync_transport_key(source, public_key, false)?;
        let identity = self.sync_identity()?.context("sync_not_initialized")?;
        let expected = EnvelopeHeader::new(
            &identity.dataset_id,
            source,
            &identity.device_id,
            key_version,
            format!("snapshot-end:{id}"),
        )?;
        let manifest: SnapshotManifest = serde_json::from_slice(&crypto::open(
            &EncryptedEnvelope::from_bytes(bytes)?,
            &expected,
            key,
            public_key,
        )?)
        .map_err(|_| anyhow::anyhow!("sync_snapshot_manifest_invalid"))?;
        ensure!(
            manifest.version == SNAPSHOT_VERSION,
            "sync_snapshot_version_unsupported"
        );
        ensure!(
            manifest.id == id
                && manifest.dataset_id == identity.dataset_id
                && manifest.source_device_id == source
                && manifest.recipient_device_id == identity.device_id
                && manifest.key_version == key_version,
            "sync_snapshot_manifest_context_mismatch"
        );
        ensure!(
            manifest.chunks > 0
                && manifest.chunks <= MAX_CHUNKS
                && manifest.bytes > 0
                && manifest.bytes <= MAX_SNAPSHOT_BYTES
                && manifest.frontiers.len() <= 1024
                && (0..=4_000_000_000_000_000).contains(&manifest.logical_clock)
                && valid_hash(&manifest.digest),
            "sync_snapshot_manifest_limit"
        );
        validate_id(&manifest.processor_device_id)?;
        // 传输方式以 **manifest** 为准（它被发送端签名覆盖）：`snapshot-start`
        // 只是提前给接收端一个线索，真伪在这里校正。blob 传输必须带 blob 身份。
        let transport = manifest.transport.as_deref().unwrap_or("mail");
        ensure!(
            matches!(transport, "mail" | "blob"),
            "sync_snapshot_manifest_limit"
        );
        match (transport, manifest.blob_id.as_deref()) {
            ("blob", Some(blob_id)) => validate_blob_id(blob_id)?,
            ("blob", None) => return Err(anyhow::anyhow!("sync_snapshot_blob_id_invalid")),
            _ => {}
        }
        if self.has_sync_authority()? {
            ensure!(
                manifest.processor_device_id == identity.processor_device_id,
                "sync_processing_handoff_required"
            );
        }
        ensure!(
            manifest.processor_device_id != identity.device_id,
            "sync_snapshot_cannot_grant_local_processing"
        );
        let tx = self.begin_write_transaction()?;
        let state = self.selected_snapshot(id, source, key_version)?;
        let old: Option<Vec<u8>> = self.connection.query_row(
            "SELECT manifest FROM sync_snapshots WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        ensure!(
            state != "applied" || old.is_some(),
            "sync_snapshot_manifest_missing"
        );
        if let Some(old) = old {
            let old: SnapshotManifest = serde_json::from_slice(&crypto::open(
                &EncryptedEnvelope::from_bytes(&old)?,
                &expected,
                key,
                public_key,
            )?)
            .map_err(|_| anyhow::anyhow!("sync_snapshot_manifest_invalid"))?;
            ensure!(old == manifest, "sync_snapshot_manifest_conflict");
        } else {
            self.connection.execute(
                "UPDATE sync_snapshots SET manifest=?2,transport=?3,blob_id=?4 WHERE id=?1",
                params![id, bytes, transport, manifest.blob_id],
            )?;
        }
        ensure!(
            manifest.blob_id.is_none()
                || manifest.blob_id.as_deref()
                    == self
                        .connection
                        .query_row("SELECT blob_id FROM sync_snapshots WHERE id=?1", [id], |r| {
                            r.get::<_, Option<String>>(0)
                        })?
                        .as_deref(),
            "sync_snapshot_blob_conflict"
        );
        if state == "applied" {
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(true);
        }
        self.ensure_empty_snapshot_target()?;
        let count: i64 = self.connection.query_row(
            "SELECT count(*) FROM sync_snapshot_chunks WHERE snapshot_id=?1",
            [id],
            |r| r.get(0),
        )?;
        if count < manifest.chunks as i64 {
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Ok(false);
        }
        ensure!(
            count == manifest.chunks as i64,
            "sync_snapshot_extra_chunks"
        );
        let mut digest = DigestContext::new(&SHA256);
        let mut decoded = FrameDecoder::default();
        let mut total = 0;
        let mut records = 0;
        let mut provider_seen = false;
        for index in 0..manifest.chunks {
            let chunk_bytes:Vec<u8>=self.connection.query_row("SELECT encrypted_chunk FROM sync_snapshot_chunks WHERE snapshot_id=?1 AND chunk_index=?2",params![id,index as i64],|r|r.get(0))?;
            let envelope = EncryptedEnvelope::from_bytes(&chunk_bytes)?;
            let payload =
                crypto::open(&envelope, &chunk_header(&manifest, index)?, key, public_key)?;
            let chunk: SnapshotChunk = serde_json::from_slice(&payload)
                .map_err(|_| anyhow::anyhow!("sync_snapshot_chunk_invalid"))?;
            ensure!(
                chunk.id == id && chunk.index == index,
                "sync_snapshot_chunk_invalid"
            );
            let data = STANDARD
                .decode(chunk.data)
                .map_err(|_| anyhow::anyhow!("sync_snapshot_chunk_invalid"))?;
            total += data.len();
            ensure!(total <= MAX_SNAPSHOT_BYTES, "sync_snapshot_size_limit");
            digest.update(&data);
            decoded.push(&data, |record| {
                self.apply_snapshot_record(id, &manifest, &record, &mut provider_seen)?;
                records += 1;
                Ok(())
            })?;
        }
        ensure!(
            decoded.pending.is_empty()
                && total == manifest.bytes
                && records == manifest.records
                && hex(digest.finish().as_ref()) == manifest.digest
                && provider_seen,
            "sync_snapshot_integrity_failed"
        );
        for (stream, sequence) in &manifest.frontiers {
            validate_id(stream)?;
            if self.has_sync_authority()? {
                let known: bool = self.connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sync_member_keys WHERE stream_id=?1)",
                    [stream],
                    |r| r.get(0),
                )?;
                ensure!(known, "sync_snapshot_member_needs_authority");
            }
            ensure!(*sequence >= 0, "sync_snapshot_frontier_invalid");
            let (count,minimum,maximum):(i64,Option<i64>,Option<i64>)=self.connection.query_row("SELECT COUNT(*),MIN(origin_sequence),MAX(origin_sequence) FROM (SELECT origin_sequence FROM sync_operations WHERE origin_stream_id=?1 AND state IN ('applied','conflict') UNION ALL SELECT origin_sequence FROM sync_checkpoint_operations WHERE origin_stream_id=?1)",[stream],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            ensure!(
                count == *sequence
                    && (*sequence == 0 || (minimum == Some(1) && maximum == Some(*sequence))),
                "sync_snapshot_frontier_not_covered"
            );
            ensure!(
                stream != &identity.stream_id || *sequence == 0,
                "sync_snapshot_local_stream_conflict"
            );
            self.connection.execute("INSERT INTO sync_frontiers(origin_stream_id,applied_sequence) VALUES(?1,?2) ON CONFLICT(origin_stream_id) DO UPDATE SET applied_sequence=MAX(applied_sequence,excluded.applied_sequence)",params![stream,sequence])?;
        }
        self.snapshot_source_authorized(source)?;
        let processor_valid: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
            [&manifest.processor_device_id],
            |r| r.get(0),
        )?;
        ensure!(processor_valid, "sync_snapshot_processor_unknown");
        self.connection.execute(
            "UPDATE sync_replica SET logical_clock=MAX(logical_clock,?1),processor_device_id=?2 WHERE singleton=1",
            params![manifest.logical_clock,manifest.processor_device_id],
        )?;
        self.connection.execute(
            "UPDATE sync_snapshots SET state='applied' WHERE id=?1",
            [id],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(true)
    }
}
