//! Initial, authenticated checkpoints for a fresh staging database. A pinned WAL
//! read view is streamed as bounded typed records, never as an attached SQLite
//! database. Checkpoints are attested by the explicitly paired source; they do
//! not claim to contain independent historical device signatures.

use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::digest::{Context as DigestContext, SHA256};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::storage::Store;
use crate::sync::{
    crypto::{self, ContentKey, DeviceIdentity, EncryptedEnvelope, EnvelopeHeader},
    protocol::{
        ConversationRecord, EventRecord, InputRecord, MessageRecord, ProviderBundle, SyncOperation,
        validate_id,
    },
};

const CHUNK_BYTES: usize = 256 * 1024;
const MAX_RECORD_BYTES: usize = 20 * 1024 * 1024;
const MAX_SNAPSHOT_BYTES: usize = 256 * 1024 * 1024;
const MAX_CHUNKS: usize = MAX_SNAPSHOT_BYTES / CHUNK_BYTES + 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotChunk {
    id: String,
    index: usize,
    data: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SnapshotManifest {
    pub version: u32,
    pub id: String,
    pub dataset_id: String,
    pub source_device_id: String,
    pub recipient_device_id: String,
    pub processor_device_id: String,
    pub key_version: u64,
    pub chunks: usize,
    pub bytes: usize,
    pub records: usize,
    pub digest: String,
    pub frontiers: BTreeMap<String, i64>,
    pub logical_clock: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MessageOrder {
    origin_stream_id: String,
    origin_sequence: i64,
    logical_clock: i64,
    input_message_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Record {
    Event {
        value: EventRecord,
        status: String,
        processed_at: Option<String>,
        updated_at: String,
    },
    Conversation {
        value: ConversationRecord,
    },
    Message {
        value: MessageRecord,
        order: Option<MessageOrder>,
    },
    Input {
        value: InputRecord,
    },
    Analysis {
        id: String,
        event_id: String,
        prompt_version: String,
        result_json: String,
        created_at: String,
    },
    Classification {
        id: String,
        event_id: String,
        recordable: bool,
        event_kind: String,
        reason: String,
        created_at: String,
    },
    Head {
        object_id: String,
        revision_id: String,
        deleted: bool,
    },
    KnownRevision {
        object_id: String,
        revision_id: String,
    },
    Member {
        device_id: String,
        stream_id: String,
        active: bool,
    },
    Checkpoint {
        op_id: String,
        origin_device_id: String,
        origin_stream_id: String,
        origin_sequence: i64,
        payload_hash: String,
    },
    Operation {
        value: SyncOperation,
        state: String,
        payload_hash: String,
    },
    Conflict {
        op_id: String,
        object_kind: String,
        object_id: String,
        current_revision: Option<String>,
        reason: String,
    },
    ProviderCheckpoint {
        revision_id: String,
        content_hash: String,
    },
    Provider {
        value: ProviderBundle,
    },
}

// rowid is only a cursor inside this one pinned source view, never a replicated
// identity or display-order key. Parent messages exist before their children in
// supported write paths; the importer refuses forward/cyclic parent references.
const QUERIES: &[&str] = &[
    "SELECT rowid,json_object('kind','event','value',json_object('id',id,'occurred_at',occurred_at,'recorded_at',recorded_at,'raw_text',raw_text,'source',source,'created_at',created_at),'status',status,'processed_at',processed_at,'updated_at',updated_at) FROM events WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','conversation','value',json_object('id',id,'title',title,'tag',tag,'archived',json(CASE WHEN archived=1 THEN 'true' ELSE 'false' END),'wiki_page_slug',wiki_page_slug,'assistant_mode',assistant_mode,'created_at',created_at,'updated_at',updated_at)) FROM conversations WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT m.rowid,json_object('kind','message','value',json_object('id',m.id,'conversation_id',m.conversation_id,'parent_message_id',m.parent_message_id,'role',m.role,'content',m.content,'created_at',m.created_at,'input_message_id',(SELECT input_message_id FROM message_response_links WHERE message_id=m.id)),'order',CASE WHEN s.message_id IS NULL THEN NULL ELSE json_object('origin_stream_id',s.origin_stream_id,'origin_sequence',s.origin_sequence,'logical_clock',s.logical_clock,'input_message_id',s.input_message_id) END) FROM messages m JOIN conversations c ON c.id=m.conversation_id LEFT JOIN sync_message_order s ON s.message_id=m.id WHERE m.rowid>?1 ORDER BY m.rowid LIMIT 1",
    "SELECT i.rowid,json_object('kind','input','value',json_object('id',i.id,'raw_text',i.raw_text,'source',i.source,'event_id',i.event_id,'message_id',i.message_id,'created_at',i.created_at)) FROM input_records i WHERE i.rowid>?1 AND i.event_id IS NOT NULL AND (i.message_id IS NULL OR EXISTS(SELECT 1 FROM messages m WHERE m.id=i.message_id)) ORDER BY i.rowid LIMIT 1",
    "SELECT rowid,json_object('kind','analysis','id',id,'event_id',event_id,'prompt_version',prompt_version,'result_json',result_json,'created_at',created_at) FROM event_analyses WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','classification','id',id,'event_id',event_id,'recordable',json(CASE WHEN recordable=1 THEN 'true' ELSE 'false' END),'event_kind',kind,'reason',reason,'created_at',created_at) FROM event_recordability_decisions WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT h.rowid,json_object('kind','head','object_id',object_id,'revision_id',revision_id,'deleted',json(CASE WHEN deleted=1 THEN 'true' ELSE 'false' END)) FROM sync_object_heads h WHERE h.rowid>?1 AND object_kind='conversation' ORDER BY h.rowid LIMIT 1",
    "SELECT r.rowid,json_object('kind','known_revision','object_id',r.object_id,'revision_id',r.revision_id) FROM sync_known_revisions r JOIN sync_object_heads h ON h.object_kind=r.object_kind AND h.object_id=r.object_id WHERE r.rowid>?1 AND r.object_kind='conversation' ORDER BY r.rowid LIMIT 1",
    "SELECT rowid,json_object('kind','member','device_id',device_id,'stream_id',stream_id,'active',json(CASE WHEN active=1 THEN 'true' ELSE 'false' END)) FROM sync_members WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','checkpoint','op_id',op_id,'origin_device_id',origin_device_id,'origin_stream_id',origin_stream_id,'origin_sequence',origin_sequence,'payload_hash',payload_hash) FROM sync_operations WHERE rowid>?1 AND state='applied' ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','checkpoint','op_id',op_id,'origin_device_id',origin_device_id,'origin_stream_id',origin_stream_id,'origin_sequence',origin_sequence,'payload_hash',payload_hash) FROM sync_checkpoint_operations WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','operation','value',json(payload),'state',state,'payload_hash',payload_hash) FROM sync_operations WHERE rowid>?1 AND state='conflict' ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','conflict','op_id',op_id,'object_kind',object_kind,'object_id',object_id,'current_revision',current_revision,'reason',reason) FROM sync_conflicts WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','provider_checkpoint','revision_id',revision_id,'content_hash',content_hash) FROM sync_checkpoint_providers WHERE rowid>?1 ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','provider_checkpoint','revision_id',revision_id,'content_hash',content_hash) FROM sync_provider_versions WHERE rowid>?1 AND state='applied' ORDER BY rowid LIMIT 1",
    "SELECT rowid,json_object('kind','provider_checkpoint','revision_id',substr(message_id,10),'content_hash',payload_hash) FROM sync_wire_outbox WHERE rowid>?1 AND message_id LIKE 'provider:%' ORDER BY rowid LIMIT 1",
];

pub(crate) struct SnapshotExporter {
    view: Store,
    manifest: SnapshotManifest,
    table: usize,
    cursor: i64,
    frame: Vec<u8>,
    frame_offset: usize,
    hash: DigestContext,
    finished: bool,
}

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
            version: 1,
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
        })
    }

    fn next_record(&mut self) -> Result<Option<Vec<u8>>> {
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
        Ok(None)
    }

    pub(crate) fn next_encrypted_chunk(
        &mut self,
        key: &ContentKey,
        signer: &DeviceIdentity,
    ) -> Result<Option<Vec<u8>>> {
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
        let index = self.manifest.chunks;
        self.manifest.chunks += 1;
        self.manifest.bytes += data.len();
        self.hash.update(&data);
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

    pub(crate) fn finish(mut self, key: &ContentKey, signer: &DeviceIdentity) -> Result<Vec<u8>> {
        ensure!(self.finished, "sync_snapshot_export_incomplete");
        self.manifest.digest = hex(self.hash.finish().as_ref());
        let header = manifest_header(&self.manifest)?;
        crypto::seal(header, key, signer, &serde_json::to_vec(&self.manifest)?)?.to_bytes()
    }
}

fn chunk_header(manifest: &SnapshotManifest, index: usize) -> Result<EnvelopeHeader> {
    EnvelopeHeader::new(
        &manifest.dataset_id,
        &manifest.source_device_id,
        &manifest.recipient_device_id,
        manifest.key_version,
        format!("snapshot:{}:{index}", manifest.id),
    )
}
fn manifest_header(manifest: &SnapshotManifest) -> Result<EnvelopeHeader> {
    EnvelopeHeader::new(
        &manifest.dataset_id,
        &manifest.source_device_id,
        &manifest.recipient_device_id,
        manifest.key_version,
        format!("snapshot-end:{}", manifest.id),
    )
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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

    fn snapshot_source_authorized(&self, source: &str) -> Result<()> {
        let active: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
            [source],
            |r| r.get(0),
        )?;
        ensure!(active, "sync_snapshot_source_not_authorized");
        Ok(())
    }
    pub(super) fn ensure_empty_snapshot_target(&self) -> Result<()> {
        let has_data:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM events UNION ALL SELECT 1 FROM conversations UNION ALL SELECT 1 FROM messages UNION ALL SELECT 1 FROM input_records UNION ALL SELECT 1 FROM ai_provider_configs UNION ALL SELECT 1 FROM wiki_pages UNION ALL SELECT 1 FROM todos UNION ALL SELECT 1 FROM rules UNION ALL SELECT 1 FROM goals UNION ALL SELECT 1 FROM sync_operations)",[],|r|r.get(0))?;
        ensure!(!has_data, "sync_snapshot_requires_fresh_staging_database");
        Ok(())
    }
    fn selected_snapshot(&self, id: &str, source: &str, key_version: u64) -> Result<String> {
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
            manifest.version == 1
                && manifest.id == id
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

    fn apply_snapshot_record(
        &self,
        snapshot_id: &str,
        manifest: &SnapshotManifest,
        record: &Record,
        provider_seen: &mut bool,
    ) -> Result<()> {
        match record {
            Record::Event {
                value,
                status,
                processed_at,
                updated_at,
            } => {
                validate_id(&value.id)?;
                ensure!(
                    matches!(status.as_str(), "pending" | "processed"),
                    "sync_snapshot_event_status_invalid"
                );
                super::super::events::insert_raw_event_on(
                    &self.connection,
                    &value.id,
                    &value.occurred_at,
                    &value.recorded_at,
                    &value.raw_text,
                    &value.source,
                    &value.created_at,
                )?;
                self.connection.execute(
                    "UPDATE events SET status=?2,processed_at=?3,updated_at=?4 WHERE id=?1",
                    params![value.id, status, processed_at, updated_at],
                )?;
            }
            Record::Conversation { value } => {
                validate_id(&value.id)?;
                if let Some(slug) = &value.wiki_page_slug {
                    self.sync_mark_wiki_slug_seen(slug)?;
                }
                self.connection.execute("INSERT INTO conversations(id,title,tag,archived,wiki_page_slug,assistant_mode,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![value.id,value.title,value.tag,value.archived,value.wiki_page_slug,value.assistant_mode,value.created_at,value.updated_at])?;
            }
            Record::Message { value, order } => {
                validate_id(&value.id)?;
                if let Some(parent) = &value.parent_message_id {
                    let parent: Option<String> = self
                        .connection
                        .query_row(
                            "SELECT conversation_id FROM messages WHERE id=?1",
                            [parent],
                            |r| r.get(0),
                        )
                        .optional()?;
                    ensure!(
                        parent.as_deref() == Some(value.conversation_id.as_str()),
                        "sync_snapshot_parent_missing_or_invalid"
                    );
                }
                self.connection.execute("INSERT INTO messages(id,conversation_id,parent_message_id,role,content,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![value.id,value.conversation_id,value.parent_message_id,value.role,value.content,value.created_at])?;
                if let Some(input) = &value.input_message_id {
                    self.connection.execute("INSERT INTO message_response_links(message_id,input_message_id) VALUES(?1,?2)",params![value.id,input])?;
                }
                if let Some(order) = order {
                    validate_id(&order.origin_stream_id)?;
                    ensure!(
                        order.origin_sequence > 0
                            && order.logical_clock > 0
                            && order.logical_clock <= manifest.logical_clock,
                        "sync_snapshot_message_order_invalid"
                    );
                    self.connection.execute("INSERT INTO sync_message_order(message_id,origin_stream_id,origin_sequence,logical_clock,input_message_id) VALUES(?1,?2,?3,?4,?5)",params![value.id,order.origin_stream_id,order.origin_sequence,order.logical_clock,order.input_message_id])?;
                }
            }
            Record::Input { value } => {
                validate_id(&value.id)?;
                self.connection.execute("INSERT INTO input_records(id,raw_text,source,route_status,event_id,message_id,created_at,updated_at) VALUES(?1,?2,?3,'routed',?4,?5,?6,?6)",params![value.id,value.raw_text,value.source,value.event_id,value.message_id,value.created_at])?;
            }
            Record::Analysis {
                id,
                event_id,
                prompt_version,
                result_json,
                created_at,
            } => {
                validate_id(id)?;
                self.connection.execute("INSERT INTO event_analyses(id,event_id,prompt_version,result_json,created_at) VALUES(?1,?2,?3,?4,?5)",params![id,event_id,prompt_version,result_json,created_at])?;
            }
            Record::Classification {
                id,
                event_id,
                recordable,
                event_kind,
                reason,
                created_at,
            } => {
                validate_id(id)?;
                self.connection.execute("INSERT INTO event_recordability_decisions(id,event_id,recordable,kind,reason,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![id,event_id,recordable,event_kind,reason,created_at])?;
            }
            Record::Head {
                object_id,
                revision_id,
                deleted,
            } => {
                let exists: bool = self.connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1)",
                    [object_id],
                    |r| r.get(0),
                )?;
                ensure!(exists != *deleted, "sync_snapshot_head_mismatch");
                self.sync_set_head("conversation", object_id, revision_id, *deleted)?;
            }
            Record::KnownRevision {
                object_id,
                revision_id,
            } => {
                self.connection.execute("INSERT OR IGNORE INTO sync_known_revisions(object_kind,object_id,revision_id) VALUES('conversation',?1,?2)",params![object_id,revision_id])?;
            }
            Record::Member {
                device_id,
                stream_id,
                active,
            } => {
                validate_id(device_id)?;
                validate_id(stream_id)?;
                if self.signed_snapshot_member(device_id, stream_id)? {
                    // Keep the latest owner-signed permission, not this content
                    // snapshot's possibly older active/revoked projection.
                    return Ok(());
                }
                let identity = self.sync_identity()?.context("sync_not_initialized")?;
                if device_id == &identity.device_id && *active {
                    ensure!(
                        stream_id == &identity.stream_id,
                        "sync_snapshot_local_stream_conflict"
                    );
                }
                if stream_id == &identity.stream_id {
                    ensure!(
                        device_id == &identity.device_id && *active,
                        "sync_snapshot_local_member_invalid"
                    );
                }
                let existing: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT device_id FROM sync_members WHERE stream_id=?1",
                        [stream_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                ensure!(
                    existing.as_deref().is_none_or(|id| id == device_id),
                    "sync_stream_identity_conflict"
                );
                self.connection.execute("INSERT INTO sync_members(device_id,stream_id,active) VALUES(?1,?2,?3) ON CONFLICT(stream_id) DO UPDATE SET active=excluded.active",params![device_id,stream_id,active])?;
            }
            Record::Checkpoint {
                op_id,
                origin_device_id,
                origin_stream_id,
                origin_sequence,
                payload_hash,
            } => {
                validate_id(origin_device_id)?;
                validate_id(origin_stream_id)?;
                self.signed_snapshot_member(origin_device_id, origin_stream_id)?;
                ensure!(
                    *origin_sequence > 0
                        && *op_id == format!("{origin_stream_id}:{origin_sequence}")
                        && manifest
                            .frontiers
                            .get(origin_stream_id)
                            .is_some_and(|n| *n >= *origin_sequence)
                        && valid_hash(payload_hash),
                    "sync_snapshot_checkpoint_invalid"
                );
                let member: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT device_id FROM sync_members WHERE stream_id=?1",
                        [origin_stream_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                ensure!(
                    member.as_deref() == Some(origin_device_id.as_str()),
                    "sync_snapshot_origin_identity_invalid"
                );
                self.connection.execute("INSERT INTO sync_checkpoint_operations(op_id,origin_device_id,origin_stream_id,origin_sequence,payload_hash) VALUES(?1,?2,?3,?4,?5)",params![op_id,origin_device_id,origin_stream_id,origin_sequence,payload_hash])?;
            }
            Record::Operation {
                value,
                state,
                payload_hash,
            } => {
                value.validate()?;
                self.signed_snapshot_member(&value.origin_device_id, &value.origin_stream_id)?;
                ensure!(
                    value.dataset_id == manifest.dataset_id
                        && matches!(state.as_str(), "applied" | "conflict")
                        && manifest
                            .frontiers
                            .get(&value.origin_stream_id)
                            .is_some_and(|n| *n >= value.origin_sequence),
                    "sync_snapshot_operation_invalid"
                );
                let member: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT device_id FROM sync_members WHERE stream_id=?1",
                        [&value.origin_stream_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                ensure!(
                    member.as_deref() == Some(value.origin_device_id.as_str())
                        && value.logical_clock <= manifest.logical_clock,
                    "sync_snapshot_origin_identity_invalid"
                );
                let payload = serde_json::to_string(value)?;
                ensure!(
                    super::hash(payload.as_bytes()) == *payload_hash,
                    "sync_snapshot_operation_hash_mismatch"
                );
                self.connection.execute("INSERT INTO sync_operations(op_id,origin_device_id,origin_stream_id,origin_sequence,payload,payload_hash,local,state) VALUES(?1,?2,?3,?4,?5,?6,0,?7)",params![value.op_id,value.origin_device_id,value.origin_stream_id,value.origin_sequence,payload,payload_hash,state])?;
            }
            Record::Conflict {
                op_id,
                object_kind,
                object_id,
                current_revision,
                reason,
            } => {
                self.connection.execute("INSERT INTO sync_conflicts(op_id,object_kind,object_id,current_revision,reason) VALUES(?1,?2,?3,?4,?5)",params![op_id,object_kind,object_id,current_revision,reason])?;
            }
            Record::ProviderCheckpoint {
                revision_id,
                content_hash,
            } => {
                validate_id(revision_id)?;
                ensure!(valid_hash(content_hash), "sync_snapshot_checkpoint_invalid");
                let existing: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT content_hash FROM sync_checkpoint_providers WHERE revision_id=?1",
                        [revision_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                ensure!(
                    existing.as_deref().is_none_or(|hash| hash == content_hash),
                    "sync_provider_revision_content_conflict"
                );
                self.connection.execute("INSERT OR IGNORE INTO sync_checkpoint_providers(revision_id,content_hash,snapshot_id) VALUES(?1,?2,?3)",params![revision_id,content_hash,snapshot_id])?;
            }
            Record::Provider { value } => {
                ensure!(!*provider_seen, "sync_snapshot_duplicate_provider_bundle");
                value.validate()?;
                self.replace_provider_bundle_on(value)?;
                self.connection.execute("INSERT INTO sync_provider_state(singleton,revision_id,base_revision) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET revision_id=excluded.revision_id,base_revision=excluded.base_revision",params![value.revision_id,value.base_revision])?;
                let digest = super::hash(&serde_json::to_vec(value)?);
                let old: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT content_hash FROM sync_checkpoint_providers WHERE revision_id=?1",
                        [&value.revision_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                ensure!(
                    old.as_deref().is_none_or(|hash| hash == digest),
                    "sync_provider_revision_content_conflict"
                );
                self.connection.execute("INSERT OR IGNORE INTO sync_checkpoint_providers(revision_id,content_hash,snapshot_id) VALUES(?1,?2,?3)",params![value.revision_id,digest,snapshot_id])?;
                *provider_seen = true;
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct FrameDecoder {
    pending: Vec<u8>,
}
impl FrameDecoder {
    fn push(&mut self, bytes: &[u8], mut apply: impl FnMut(Record) -> Result<()>) -> Result<()> {
        self.pending.extend_from_slice(bytes);
        let mut consumed = 0;
        loop {
            let remaining = &self.pending[consumed..];
            if remaining.len() < 4 {
                break;
            }
            let size = u32::from_be_bytes(remaining[..4].try_into().unwrap()) as usize;
            ensure!(
                size > 0 && size <= MAX_RECORD_BYTES,
                "sync_snapshot_record_size_invalid"
            );
            if remaining.len() < size + 4 {
                break;
            }
            let record: Record = serde_json::from_slice(&remaining[4..size + 4])
                .map_err(|_| anyhow::anyhow!("sync_snapshot_record_invalid"))?;
            apply(record)?;
            consumed += size + 4;
        }
        if consumed > 0 {
            self.pending.drain(..consumed);
        }
        ensure!(
            self.pending.len() <= MAX_RECORD_BYTES + 4,
            "sync_snapshot_record_size_invalid"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests;
