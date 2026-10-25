//! 快照导出器：WAL 只读视图流式切片与信封加密。

use super::*;

impl SnapshotExporter {
    /// Enrollment sends the roster from this exact WAL view before offering the
    /// snapshot, not a roster read before/after a concurrent membership change.
    pub(crate) fn signed_roster(&self) -> Result<crate::sync::membership::SignedRoster> {
        self.view
            .current_signed_roster()?
            .context("sync_snapshot_authority_missing")
    }

    /// The recipient chooses id as a fresh transfer request. A failed export
    /// must get a new request id rather than regenerating different chunks under
    /// a previously used identity.
    pub(crate) fn new(source: &Store, recipient: &str, id: &str, key_version: u64) -> Result<Self> {
        validate_id(id)?;
        validate_id(recipient)?;
        ensure!(
            (1..=i64::MAX as u64).contains(&key_version),
            "sync_snapshot_key_version_invalid"
        );
        source.require_local_sync_membership()?;
        source.ensure_sync_provider_baseline()?;
        let dirty: bool = source.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_provider_dirty)",
            [],
            |r| r.get(0),
        )?;
        ensure!(
            !dirty,
            "sync_provider_delivery_must_be_prepared_before_snapshot"
        );
        let mut view = Store::open(&source.path)?;
        view.provider_master = source.provider_master;
        let journal: String = view
            .connection
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        ensure!(
            journal.eq_ignore_ascii_case("wal"),
            "sync_snapshot_requires_wal"
        );
        view.connection.pragma_update(None, "query_only", true)?;
        view.connection.execute_batch("BEGIN DEFERRED")?;
        let identity = view.sync_identity()?.context("sync_not_initialized")?;
        // The dirty check belongs to the same pinned read view as the provider
        // rows. A settings edit may commit between opening the view and pinning it.
        let dirty: bool = view.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_provider_dirty)",
            [],
            |r| r.get(0),
        )?;
        ensure!(
            !dirty,
            "sync_provider_delivery_must_be_prepared_before_snapshot"
        );
        let active: bool = view.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
            [recipient],
            |r| r.get(0),
        )?;
        ensure!(
            active && recipient != identity.device_id,
            "sync_snapshot_recipient_not_authorized"
        );
        let frontiers = {
            let mut q=view.connection.prepare("SELECT origin_stream_id,applied_sequence FROM sync_frontiers ORDER BY origin_stream_id")?;
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<BTreeMap<_, _>>>()?
        };
        let logical_clock = view.connection.query_row(
            "SELECT logical_clock FROM sync_replica WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        let manifest = SnapshotManifest {
            version: SNAPSHOT_VERSION,
            id: id.into(),
            dataset_id: identity.dataset_id,
            source_device_id: identity.device_id,
            recipient_device_id: recipient.into(),
            processor_device_id: identity.processor_device_id,
            key_version,
            chunks: 0,
            bytes: 0,
            records: 0,
            digest: String::new(),
            frontiers,
            logical_clock,
            transport: None,
            blob_id: None,
        };
        Ok(Self {
            view,
            manifest,
            table: 0,
            cursor: 0,
            frame: Vec::new(),
            frame_offset: 0,
            hash: DigestContext::new(&SHA256),
            finished: false,
            files: Vec::new().into_iter(),
            data_dir: None,
        })
    }

    pub(crate) fn next_record(&mut self) -> Result<Option<Vec<u8>>> {
        while self.table < QUERIES.len() {
            let row: Option<(i64, String)> = self
                .view
                .connection
                .query_row(QUERIES[self.table], [self.cursor], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?;
            if let Some((cursor, json)) = row {
                ensure!(
                    json.len() <= MAX_RECORD_BYTES,
                    "sync_snapshot_record_too_large"
                );
                // Validate the typed schema at the source too. Never forward SQL
                // names or arbitrary object structures from a source database.
                let _: Record = serde_json::from_str(&json)
                    .map_err(|_| anyhow::anyhow!("sync_snapshot_source_record_invalid"))?;
                self.cursor = cursor;
                return Ok(Some(json.into_bytes()));
            }
            self.table += 1;
            self.cursor = 0;
        }
        if self.table == QUERIES.len() {
            self.table += 1;
            let record = Record::Provider {
                value: self.view.current_provider_bundle()?,
            };
            let bytes = serde_json::to_vec(&record)?;
            ensure!(
                bytes.len() <= MAX_RECORD_BYTES,
                "sync_snapshot_record_too_large"
            );
            return Ok(Some(bytes));
        }
        // 目录段：台账 rel 排序定序，逐条读发送端磁盘发字节（§31b）。
        if self.table == QUERIES.len() + 1 {
            self.table += 1;
            let data_dir = self
                .view
                .path
                .parent()
                .map(std::path::Path::to_path_buf)
                .ok_or_else(|| anyhow::anyhow!("sync_snapshot_data_dir_missing"))?;
            let mut statement = self
                .view
                .connection
                .prepare("SELECT rel FROM wiki_assets ORDER BY rel")?;
            let rels = statement
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(statement);
            debug_assert!(
                rels.iter()
                    .all(|rel| SNAPSHOT_CARRIED_DIRS.iter().any(|d| rel.starts_with(d)))
            );
            self.files = rels.into_iter();
            self.data_dir = Some(data_dir);
        }
        if self.table == QUERIES.len() + 2 {
            if let Some(rel) = self.files.next() {
                let data_dir = self
                    .data_dir
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("sync_snapshot_data_dir_missing"))?;
                // 文件缺失/超大一律跳过：页面占位符在接收端保留为可见死文本，
                // 不让一张坏图卡死整份快照（与 ingest 跳过缺失文件的语义一致）。
                let Ok(bytes) = std::fs::read(data_dir.join(&rel)) else {
                    return Ok(self.next_record()?);
                };
                if bytes.len() * 2 + 256 > MAX_RECORD_BYTES {
                    return Ok(self.next_record()?);
                }
                let mime = rel
                    .rsplit('.')
                    .next()
                    .and_then(crate::storage::wiki_images::mime_for_ext)
                    .unwrap_or("application/octet-stream");
                let sha256 = hex(ring::digest::digest(&SHA256, &bytes).as_ref());
                let record = Record::WikiFile {
                    rel,
                    sha256,
                    mime: mime.to_string(),
                    size: bytes.len() as i64,
                    bytes_hex: hex(&bytes),
                };
                let encoded = serde_json::to_vec(&record)?;
                ensure!(
                    encoded.len() <= MAX_RECORD_BYTES,
                    "sync_snapshot_record_too_large"
                );
                return Ok(Some(encoded));
            }
            self.table += 1;
        }
        Ok(None)
    }

    /// 切出下一片明文数据并累加 manifest 账（片数/字节/整流摘要）。
    /// 加密封装拆在 `next_encrypted_chunk`：blob 续传要能"跳过前 N 片"而不付
    /// 每片一次签名的代价，跳过时摘要仍然照算（manifest 摘要必须覆盖全部片）。
    fn next_chunk_data(&mut self) -> Result<Option<Vec<u8>>> {
        if self.finished {
            return Ok(None);
        }
        let mut data = Vec::with_capacity(CHUNK_BYTES);
        while data.len() < CHUNK_BYTES {
            if self.frame_offset == self.frame.len() {
                let Some(record) = self.next_record()? else {
                    self.finished = true;
                    break;
                };
                self.frame = Vec::with_capacity(record.len() + 4);
                self.frame
                    .extend_from_slice(&(record.len() as u32).to_be_bytes());
                self.frame.extend_from_slice(&record);
                self.frame_offset = 0;
                self.manifest.records += 1;
            }
            let count = (CHUNK_BYTES - data.len()).min(self.frame.len() - self.frame_offset);
            data.extend_from_slice(&self.frame[self.frame_offset..self.frame_offset + count]);
            self.frame_offset += count;
        }
        if data.is_empty() {
            return Ok(None);
        }
        ensure!(
            self.manifest.bytes + data.len() <= MAX_SNAPSHOT_BYTES,
            "sync_snapshot_size_limit"
        );
        self.manifest.chunks += 1;
        self.manifest.bytes += data.len();
        self.hash.update(&data);
        Ok(Some(data))
    }

    pub(crate) fn next_encrypted_chunk(
        &mut self,
        key: &ContentKey,
        signer: &DeviceIdentity,
    ) -> Result<Option<Vec<u8>>> {
        let Some(data) = self.next_chunk_data()? else {
            return Ok(None);
        };
        let index = self.manifest.chunks - 1;
        let payload = serde_json::to_vec(&SnapshotChunk {
            id: self.manifest.id.clone(),
            index,
            data: STANDARD.encode(data),
        })?;
        let header = chunk_header(&self.manifest, index)?;
        Ok(Some(
            crypto::seal(header, key, signer, &payload)?.to_bytes()?,
        ))
    }

    /// blob 续传：跳过已经传完的前 `count` 片。数据照读、摘要照算，只是不重封
    /// 不重传（跳一片省一次 Ed25519 签名）。片数不够就是账对不上，直接报错。
    pub(crate) fn skip_chunks(&mut self, count: usize) -> Result<()> {
        for _ in 0..count {
            ensure!(
                self.next_chunk_data()?.is_some(),
                "sync_snapshot_export_short"
            );
        }
        Ok(())
    }

    /// 声明这份快照走 blob 数据面：manifest 里带上传输方式与 blob 身份，接收端
    /// 只依据这两个**发送端签名覆盖**的字段决定去哪拉，不做任何运行时探测。
    pub(crate) fn declare_blob(&mut self, blob_id: &str) {
        self.manifest.transport = Some("blob".to_string());
        self.manifest.blob_id = Some(blob_id.to_string());
    }

    pub(crate) fn finish(&mut self, key: &ContentKey, signer: &DeviceIdentity) -> Result<Vec<u8>> {
        ensure!(self.finished, "sync_snapshot_export_incomplete");
        // `ring` 的 `Context::finish` 吃掉上下文，而 `finish` 现在借 &mut self
        // （blob 续传要能反复切片后再收尾），所以用 clone 取摘要，光标不动。
        self.manifest.digest = hex(ring::digest::Context::clone(&self.hash).finish().as_ref());
        let header = manifest_header(&self.manifest)?;
        crypto::seal(header, key, signer, &serde_json::to_vec(&self.manifest)?)?.to_bytes()
    }
}

pub(crate) fn chunk_header(manifest: &SnapshotManifest, index: usize) -> Result<EnvelopeHeader> {
    EnvelopeHeader::new(
        &manifest.dataset_id,
        &manifest.source_device_id,
        &manifest.recipient_device_id,
        manifest.key_version,
        format!("snapshot:{}:{index}", manifest.id),
    )
}
pub(crate) fn manifest_header(manifest: &SnapshotManifest) -> Result<EnvelopeHeader> {
    EnvelopeHeader::new(
        &manifest.dataset_id,
        &manifest.source_device_id,
        &manifest.recipient_device_id,
        manifest.key_version,
        format!("snapshot-end:{}", manifest.id),
    )
}
pub(crate) fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
