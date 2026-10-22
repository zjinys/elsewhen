//! 快照导入入口：staging 数据库校验、分片暂存与发布。

use super::*;
use crate::sync::protocol::validate_id;

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
                "UPDATE sync_snapshots SET manifest=?2 WHERE id=?1",
                params![id, bytes],
            )?;
        }
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
