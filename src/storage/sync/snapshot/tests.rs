use super::*;
use crate::storage::sync::ReplicaIdentity;
use crate::sync::engine;
use uuid::Uuid;

struct Directory(std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Database {
    store: Store,
    _directory: Directory,
}
impl Database {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("elsewhen-snapshot-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Self {
            store: Store::open(&dir.join("test.db")).unwrap(),
            _directory: Directory(dir),
        }
    }
    fn enable(&self, device: &str) {
        self.store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "dataset".into(),
                device_id: device.into(),
                stream_id: format!("stream-{device}"),
                processor_device_id: "a".into(),
            })
            .unwrap();
    }
    fn count(&self, table: &str) -> i64 {
        self.store
            .connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
}
fn pair(a: &Database, b: &Database) {
    a.store.authorize_sync_member("b", "stream-b").unwrap();
    b.store.authorize_sync_member("a", "stream-a").unwrap();
}
fn export(
    source: &Store,
    target: &str,
    id: &str,
    key: &ContentKey,
    signer: &DeviceIdentity,
) -> (Vec<Vec<u8>>, Vec<u8>) {
    let mut exporter = SnapshotExporter::new(source, target, id, 1).unwrap();
    let mut chunks = Vec::new();
    while let Some(chunk) = exporter.next_encrypted_chunk(key, signer).unwrap() {
        chunks.push(chunk)
    }
    (chunks, exporter.finish(key, signer).unwrap())
}
fn stage(
    target: &Store,
    id: &str,
    source: &str,
    chunks: &[Vec<u8>],
    key: &ContentKey,
    public: &[u8; 32],
) {
    for (index, chunk) in chunks.iter().enumerate().rev() {
        target
            .stage_snapshot_chunk(id, index, source, 1, chunk, key, public)
            .unwrap();
    }
}

#[test]
fn encrypted_snapshot_survives_restart_and_preserves_provider_and_concurrent_source_writes() {
    let a = Database::new();
    let b = Database::new();
    a.store
        .submit_input_as_event(&"large original ".repeat(70_000), "capture", None)
        .unwrap();
    let provider = a
        .store
        .save_ai_provider_config(
            None,
            "main",
            "openai-compatible",
            "https://synthetic.invalid/v1",
            "model",
            "SNAPSHOT_SYNTHETIC_KEY",
            0.5,
            Some(2048),
        )
        .unwrap();
    a.store
        .connection
        .execute(
            "UPDATE ai_provider_configs SET context_window=65536 WHERE id=?1",
            [&provider],
        )
        .unwrap();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    let conversation = a.store.create_conversation(Some("before"), None).unwrap();
    a.store
        .submit_conversation_input(&conversation, "before snapshot", None)
        .unwrap();
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let mut exporter = SnapshotExporter::new(&a.store, "b", &id, 1).unwrap();
    // A held source read view must not block unrelated capture (WAL).
    a.store
        .submit_input_as_event("after snapshot", "capture", None)
        .unwrap();
    let mut chunks = Vec::new();
    while let Some(chunk) = exporter.next_encrypted_chunk(&key, &signer).unwrap() {
        chunks.push(chunk)
    }
    assert!(chunks.len() > 1);
    let manifest = exporter.finish(&key, &signer).unwrap();
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    b.store
        .stage_snapshot_chunk(&id, 0, "a", 1, &chunks[0], &key, &signer.public_key())
        .unwrap();
    assert!(
        !b.store
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    assert_eq!(b.count("events"), 0);
    assert_eq!(b.count("ai_provider_configs"), 0);
    let reopened = Store::open(&b.store.path).unwrap();
    stage(&reopened, &id, "a", &chunks, &key, &signer.public_key());
    assert!(
        reopened
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    assert_eq!(b.count("events"), 2);
    assert_eq!(b.count("analysis_jobs"), 0);
    let restored = reopened.active_ai_provider_config().unwrap().unwrap();
    assert_eq!(restored.api_key, "SNAPSHOT_SYNTHETIC_KEY");
    assert_eq!(restored.context_window, Some(65536));
    assert_eq!(restored.id, provider);
    assert!(
        reopened
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    assert_eq!(b.count("events"), 2);
    // Covered operations verify their hashes and regenerate receipts, not rows.
    for op in a.store.pending_sync_operations("b", 100).unwrap() {
        reopened
            .receive_sync_operation("a", &serde_json::to_vec(&op).unwrap())
            .unwrap();
    }
    assert_eq!(b.count("events"), 3);
    assert_eq!(b.count("messages"), 1);
    assert_eq!(reopened.pending_sync_receipts("a", 100).unwrap().len(), 3);
    assert_eq!(b.count("analysis_jobs"), 0);
    let stored: Vec<u8> = reopened
        .connection
        .query_row(
            "SELECT encrypted_chunk FROM sync_snapshot_chunks LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!String::from_utf8_lossy(&stored).contains("SNAPSHOT_SYNTHETIC_KEY"));
    drop(reopened);
}

#[test]
fn bad_manifest_rolls_back_every_domain_write_and_key() {
    let a = Database::new();
    let b = Database::new();
    a.store
        .submit_input_as_event("keep source", "capture", None)
        .unwrap();
    a.store
        .save_ai_provider_config(
            None,
            "main",
            "openai-compatible",
            "https://synthetic.invalid",
            "model",
            "KEY_IN_SOURCE",
            0.7,
            None,
        )
        .unwrap();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    let header = EnvelopeHeader::new("dataset", "a", "b", 1, format!("snapshot-end:{id}")).unwrap();
    let mut wrong: SnapshotManifest = serde_json::from_slice(
        &crypto::open(
            &EncryptedEnvelope::from_bytes(&manifest).unwrap(),
            &header,
            &key,
            &signer.public_key(),
        )
        .unwrap(),
    )
    .unwrap();
    wrong.digest = "0".repeat(64);
    let bad = crypto::seal(header, &key, &signer, &serde_json::to_vec(&wrong).unwrap())
        .unwrap()
        .to_bytes()
        .unwrap();
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &bad, &key, &signer.public_key())
            .is_err()
    );
    assert_eq!(b.count("events"), 0);
    assert_eq!(b.count("ai_provider_configs"), 0);
    assert_eq!(b.count("sync_checkpoint_operations"), 0);
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    assert_eq!(b.count("events"), 1);
}

#[test]
fn newer_request_fences_late_chunks_and_existing_user_data_is_never_replaced() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let old = Uuid::new_v4().to_string();
    let new = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &old, &key, &signer);
    b.store.begin_snapshot_import(&old, "a", 1).unwrap();
    b.store.begin_snapshot_import(&new, "a", 1).unwrap();
    assert!(
        b.store
            .stage_snapshot_chunk(&old, 0, "a", 1, &chunks[0], &key, &signer.public_key())
            .is_err()
    );
    assert!(
        b.store
            .publish_snapshot(&old, "a", 1, &manifest, &key, &signer.public_key())
            .is_err()
    );
    assert!(b.store.begin_snapshot_import(&old, "a", 1).is_err());
    b.store
        .submit_input_as_event("local data must stay", "capture", None)
        .unwrap();
    assert!(
        b.store
            .begin_snapshot_import(&Uuid::new_v4().to_string(), "a", 1)
            .is_err()
    );
    assert_eq!(
        b.store.list_events().unwrap()[0].raw_text,
        "local data must stay"
    );
}

#[test]
fn provider_dirty_revision_must_be_durably_prepared_before_checkpoint() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    a.store
        .save_ai_provider_config(
            None,
            "main",
            "openai-compatible",
            "https://synthetic.invalid",
            "model",
            "synthetic",
            0.7,
            None,
        )
        .unwrap();
    let id = Uuid::new_v4().to_string();
    assert!(SnapshotExporter::new(&a.store, "b", &id, 1).is_err());
    let bundle = a.store.current_provider_bundle().unwrap();
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    assert!(
        a.store
            .mark_provider_revision_prepared(&bundle.revision_id)
            .is_err()
    );
    engine::stage_provider_bundle(&a.store, &bundle, "b", 1, &key, &signer).unwrap();
    a.store
        .mark_provider_revision_prepared(&bundle.revision_id)
        .unwrap();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    let header = EnvelopeHeader::new(
        "dataset",
        "a",
        "b",
        1,
        format!("provider:{}", bundle.revision_id),
    )
    .unwrap();
    let old = crypto::seal(
        header.clone(),
        &key,
        &signer,
        &serde_json::to_vec(&bundle).unwrap(),
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    assert_eq!(
        b.store
            .apply_encrypted_provider_bundle(&old, &header, &key, &signer.public_key())
            .unwrap(),
        crate::storage::sync::providers::ProviderApplyResult::Duplicate
    );
}

#[test]
fn checkpointed_history_can_be_used_as_a_new_snapshot_source() {
    let a = Database::new();
    let b = Database::new();
    let c = Database::new();
    a.enable("a");
    b.enable("b");
    c.enable("c");
    pair(&a, &b);
    a.store
        .submit_input_as_event("one fact", "capture", None)
        .unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let key = ContentKey::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    b.store
        .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
        .unwrap();
    b.store.authorize_sync_member("c", "stream-c").unwrap();
    c.store.authorize_sync_member("b", "stream-b").unwrap();
    let id = Uuid::new_v4().to_string();
    let signer = DeviceIdentity::generate().unwrap();
    let (chunks, manifest) = export(&b.store, "c", &id, &key, &signer);
    c.store.begin_snapshot_import(&id, "b", 1).unwrap();
    stage(&c.store, &id, "b", &chunks, &key, &signer.public_key());
    assert!(
        c.store
            .publish_snapshot(&id, "b", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    assert_eq!(c.count("events"), 1);
    assert_eq!(c.count("sync_checkpoint_operations"), 1);
}

#[test]
fn completed_snapshot_identity_cannot_be_reused_for_other_content() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    b.store
        .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
        .unwrap();
    let expected =
        EnvelopeHeader::new("dataset", "a", "b", 1, format!("snapshot-end:{id}")).unwrap();
    let mut changed: SnapshotManifest = serde_json::from_slice(
        &crypto::open(
            &EncryptedEnvelope::from_bytes(&manifest).unwrap(),
            &expected,
            &key,
            &signer.public_key(),
        )
        .unwrap(),
    )
    .unwrap();
    changed.records += 1;
    let changed = crypto::seal(
        expected,
        &key,
        &signer,
        &serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &changed, &key, &signer.public_key())
            .is_err()
    );
    let chunk = SnapshotChunk {
        id: id.clone(),
        index: chunks.len(),
        data: STANDARD.encode(b"extra"),
    };
    let expected = EnvelopeHeader::new(
        "dataset",
        "a",
        "b",
        1,
        format!("snapshot:{id}:{}", chunks.len()),
    )
    .unwrap();
    let extra = crypto::seal(
        expected,
        &key,
        &signer,
        &serde_json::to_vec(&chunk).unwrap(),
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    assert!(
        b.store
            .stage_snapshot_chunk(
                &id,
                chunks.len(),
                "a",
                1,
                &extra,
                &key,
                &signer.public_key()
            )
            .is_err()
    );
}

#[test]
fn snapshot_uses_hash_checkpoints_instead_of_resending_deleted_chat_bodies() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    let conversation = a
        .store
        .create_conversation(Some("removed title sentinel"), None)
        .unwrap();
    a.store
        .send_message(&conversation, "assistant", "removed reply sentinel", None)
        .unwrap();
    a.store
        .set_conversation_archived(&conversation, true)
        .unwrap();
    a.store.delete_archived_conversation(&conversation).unwrap();
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    let mut stream = Vec::new();
    for (index, bytes) in chunks.iter().enumerate() {
        let expected =
            EnvelopeHeader::new("dataset", "a", "b", 1, format!("snapshot:{id}:{index}")).unwrap();
        let chunk: SnapshotChunk = serde_json::from_slice(
            &crypto::open(
                &EncryptedEnvelope::from_bytes(bytes).unwrap(),
                &expected,
                &key,
                &signer.public_key(),
            )
            .unwrap(),
        )
        .unwrap();
        stream.extend(STANDARD.decode(chunk.data).unwrap());
    }
    assert!(!String::from_utf8_lossy(&stream).contains("removed reply sentinel"));
    assert!(!String::from_utf8_lossy(&stream).contains("removed title sentinel"));
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    assert_eq!(b.count("messages"), 0);
    assert_eq!(b.count("sync_checkpoint_operations"), 4);
    for operation in a.store.pending_sync_operations("b", 100).unwrap() {
        b.store
            .receive_sync_operation("a", &serde_json::to_vec(&operation).unwrap())
            .unwrap();
    }
    assert_eq!(b.count("messages"), 0);
}

#[test]
fn frame_decoder_rejects_unknown_record_kinds_and_oversized_lengths() {
    let mut decoder = FrameDecoder::default();
    assert!(decoder.push(&u32::MAX.to_be_bytes(), |_| Ok(())).is_err());
    let bytes = br#"{"kind":"arbitrary_sql","statement":"not executable"}"#;
    let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(bytes);
    assert!(FrameDecoder::default().push(&frame, |_| Ok(())).is_err());
}

#[test]
fn snapshot_carries_every_table_or_explicitly_excludes_it() {
    let db = Database::new();
    let mut stmt = db
        .store
        .connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .unwrap();
    let tables: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    drop(stmt);
    assert!(tables.len() > 20, "schema 必须真实加载");
    for table in &tables {
        let carried = SNAPSHOT_CARRIED_TABLES.contains(&table.as_str());
        let excluded = SNAPSHOT_EXCLUDED_TABLES.iter().any(|(n, _)| n == table);
        assert!(
            carried ^ excluded,
            "表 {table} 未在快照携带/排除清单登记：新增表必须显式归类，否则快照恢复断档"
        );
    }
    // 携带清单必须在 QUERIES 里被真实引用（provider bundle 承载的表除外）。
    for table in SNAPSHOT_CARRIED_TABLES {
        assert!(
            QUERIES.iter().any(|q| q.contains(table)) || *table == "ai_provider_configs",
            "表 {table} 在携带清单里但没有对应的导出查询"
        );
    }
}

#[test]
fn snapshot_carries_wiki_todos_goals_rules_and_continues_incremental() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    // ---- 源端：五类对象 + 全部同步轨迹 ----
    let conversation = a.store.create_conversation(Some("工作"), None).unwrap();
    a.store
        .submit_conversation_input(&conversation, "上下文消息", None)
        .unwrap();
    // wiki 页：直接落行 + 事务内捕获（真实事件 id 证据）。
    a.store
        .connection
        .execute(
            "INSERT INTO wiki_pages(id,slug,kind,title,summary,content_md,tags,source_event_ids,evidence_count,first_seen_at,last_seen_at,status,created_at,updated_at,area) VALUES('wpage-1','topic/snap','topic','快照页','','v1 正文','[]','[]',1,'t1','t1','active','t1','t1','insight')",
            [],
        )
        .unwrap();
    {
        let tx = a.store.begin_write_transaction().unwrap();
        a.store.sync_record_wiki_page("wpage-1", false).unwrap();
        if let Some(tx) = tx {
            tx.commit().unwrap();
        }
    }
    a.store
        .save_wiki_page_content("topic/snap", "v2 正文", "[human] 修订", None)
        .unwrap();
    let todo = a
        .store
        .create_todo("快照待办", "high", None, None, None, None)
        .unwrap()
        .id;
    a.store
        .update_todo(&todo, "快照待办·改", None, Some("high"), None)
        .unwrap();
    a.store
        .create_goal("快照目标一", crate::storage::GoalPhase::Near)
        .unwrap();
    a.store
        .create_goal("快照目标二", crate::storage::GoalPhase::Mid)
        .unwrap();
    a.store
        .add_rule("快照规则", crate::storage::RuleStatus::Active, None)
        .unwrap();

    // ---- 快照导出 → 新 staging 导入 ----
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    // 五类对象全部落地。
    assert_eq!(b.count("wiki_pages"), 1);
    assert_eq!(b.count("todos"), 1);
    assert_eq!(b.count("goals"), 2);
    assert_eq!(b.count("rules"), 1);
    let title: String = b
        .store
        .connection
        .query_row("SELECT title FROM todos WHERE id=?1", [&todo], |r| r.get(0))
        .unwrap();
    assert_eq!(title, "快照待办·改", "按 head 采纳的最新行落地");
    let content: String = b
        .store
        .connection
        .query_row(
            "SELECT content_md FROM wiki_pages WHERE id='wpage-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(content, "v2 正文");
    // head/足迹种类的完备性。
    let kinds: Vec<(String, i64)> = {
        let mut stmt = b
            .store
            .connection
            .prepare("SELECT object_kind,count(*) FROM sync_object_heads GROUP BY object_kind ORDER BY object_kind")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    assert_eq!(
        kinds,
        vec![
            ("conversation".into(), 1),
            ("goal".into(), 2),
            ("rule".into(), 1),
            ("todo".into(), 1),
            ("wiki_page".into(), 1),
        ],
        "五类对象的 head 全量重建"
    );
    let slug_seen: i64 = b
        .store
        .connection
        .query_row(
            "SELECT count(*) FROM sync_wiki_slug_seen WHERE slug='topic/snap'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(slug_seen, 1, "足迹随页面行重建，会话引用锚不悬空");
    assert!(b.count("wiki_revisions") >= 1, "页面修订随行携带");
    assert!(b.count("sync_known_revisions") >= 6);

    // ---- 恢复后增量继续：A 再改待办，B 正常收到且无冲突 ----
    let reopened_store = Store::open(&b.store.path).unwrap();
    a.store
        .update_todo(&todo, "快照后增量修改", None, Some("high"), None)
        .unwrap();
    for operation in a.store.pending_sync_operations("b", 100).unwrap() {
        reopened_store
            .receive_sync_operation("a", &serde_json::to_vec(&operation).unwrap())
            .unwrap();
    }
    let incremental: String = reopened_store
        .connection
        .query_row("SELECT title FROM todos WHERE id=?1", [&todo], |r| r.get(0))
        .unwrap();
    assert_eq!(incremental, "快照后增量修改");
    assert_eq!(reopened_store.list_object_conflicts().unwrap().len(), 0);
}
