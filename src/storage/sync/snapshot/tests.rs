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
    // 目录段同哲学：文件枚举只允许触达登记过的目录（§31b）。
    assert!(
        !SNAPSHOT_CARRIED_DIRS.is_empty(),
        "目录携带清单不得为空：新目录必须显式登记，否则快照文件段永不外发"
    );
    for dir in SNAPSHOT_CARRIED_DIRS {
        assert!(
            !dir.contains("..") && !dir.starts_with('/'),
            "目录携带清单必须是数据目录内的相对路径：{dir}"
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

#[test]
fn snapshot_carries_knowledge_reviews_and_reading_states() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    // ---- 源端：一个知识页 + 评审行 + 阅读状态 ----
    a.store
        .connection
        .execute(
            "INSERT INTO wiki_pages(id,slug,kind,title,summary,content_md,tags,source_event_ids,evidence_count,first_seen_at,last_seen_at,status,created_at,updated_at,area) VALUES('wpage-k','topic/snap','topic','快照知识页','','正文','[]','[]',1,'t1','t1','active','t1','t1','insight')",
            [],
        )
        .unwrap();
    {
        let tx = a.store.begin_write_transaction().unwrap();
        a.store.sync_record_wiki_page("wpage-k", false).unwrap();
        a.store
            .connection
            .execute(
                "INSERT INTO knowledge_maintenance_reviews(fingerprint,page_id,resolution,created_at,issue_description) VALUES('fp-snap','wpage-k','dismissed','2026-01-01T00:00:00Z','上游已变更')",
                [],
            )
            .unwrap();
        a.store.sync_record_knowledge_review("fp-snap").unwrap();
        a.store
            .connection
            .execute(
                "INSERT INTO knowledge_reading_states(page_id,state,updated_at) VALUES('wpage-k','valuable','2026-01-02T00:00:00Z')",
                [],
            )
            .unwrap();
        a.store.sync_record_reading_state("wpage-k").unwrap();
        if let Some(tx) = tx {
            tx.commit().unwrap();
        }
    }
    // ---- 快照导出 → 空库导入 ----
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
    assert_eq!(b.count("wiki_pages"), 1);
    assert_eq!(b.count("knowledge_maintenance_reviews"), 1);
    assert_eq!(b.count("knowledge_reading_states"), 1);
    let review: (String, String) = b
        .store
        .connection
        .query_row(
            "SELECT page_id,resolution FROM knowledge_maintenance_reviews WHERE fingerprint='fp-snap'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(review, ("wpage-k".into(), "dismissed".into()));
    let reading: (String, String) = b
        .store
        .connection
        .query_row(
            "SELECT state,updated_at FROM knowledge_reading_states WHERE page_id='wpage-k'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(reading, ("valuable".into(), "2026-01-02T00:00:00Z".into()));

    // ---- 快照 bootstrap 后增量继续：A 端新评审 B 正常收到 ----
    let reopened = Store::open(&b.store.path).unwrap();
    {
        let tx = a.store.begin_write_transaction().unwrap();
        a.store
            .connection
            .execute(
                "INSERT INTO knowledge_maintenance_reviews(fingerprint,page_id,resolution,created_at,issue_description) VALUES('fp-snap-2','wpage-k','acknowledged','2026-01-03T00:00:00Z',NULL)",
                [],
            )
            .unwrap();
        a.store.sync_record_knowledge_review("fp-snap-2").unwrap();
        if let Some(tx) = tx {
            tx.commit().unwrap();
        }
    }
    for operation in a.store.pending_sync_operations("b", 100).unwrap() {
        reopened
            .receive_sync_operation("a", &serde_json::to_vec(&operation).unwrap())
            .unwrap();
    }
    let incremental: i64 = reopened
        .connection
        .query_row(
            "SELECT count(*) FROM knowledge_maintenance_reviews",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(incremental, 2, "评审行随流到达，无冲突立案");
}

#[test]
fn snapshot_carries_wiki_assets_and_recipient_self_heals_image_bytes() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    // ---- 源端：导入完成形态——正文占位符 + 本机文件 ----
    let rel = "tweet_images/2099471972589576451/img_1.jpg";
    let bytes: Vec<u8> = (0..64u32).map(|i| (i % 251) as u8).collect();
    let data_dir = a.store.path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(data_dir.join("tweet_images/2099471972589576451")).unwrap();
    std::fs::write(data_dir.join(rel), &bytes).unwrap();
    let draft = crate::storage::WikiPageDraft {
        slug: "tweet-2099471972589576451".into(),
        kind: "source".into(),
        title: "带图推文".into(),
        summary: "…".into(),
        content_md: format!("正文。\n\n![]({{{{tweet-image:{rel}}}}})\n"),
        tags: vec!["tweet".into()],
        source_event_ids: vec![],
        status: "active".into(),
        reason: "URL 导入".into(),
        source_url: Some("https://x.com/u/status/2099471972589576451".into()),
    };
    a.store
        .upsert_wiki_page(&draft, crate::storage::ContentPolicy::Always)
        .unwrap();
    // 保存即收资产（字节入 DB，占位符原样入库）。
    assert_eq!(a.count("wiki_assets"), 1, "保存路径收集图片字节");
    let stored: String = a
        .store
        .connection
        .query_row(
            "SELECT content_md FROM wiki_pages WHERE slug='tweet-2099471972589576451'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        stored.contains("{{tweet-image:"),
        "入库是占位符而非本机路径"
    );
    assert!(
        !stored.contains(&data_dir.display().to_string()),
        "本机绝对路径不得入库"
    );

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

    assert_eq!(b.count("wiki_assets"), 1, "台账随快照 v3 落地");
    let (sha_db, size_db): (String, i64) = b
        .store
        .connection
        .query_row(
            "SELECT sha256,size FROM wiki_assets WHERE rel=?1",
            [rel],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(size_db, bytes.len() as i64);
    assert_eq!(sha_db.len(), 64);

    // ---- 接收端：文件段直达磁盘（不再经 DB 自愈）----
    let b_dir = b.store.path.parent().unwrap();
    let expected_path = b_dir.join(rel);
    assert!(
        expected_path.exists(),
        "快照 apply 直接把文件写到接收端数据目录"
    );
    assert_eq!(
        std::fs::read(&expected_path).unwrap(),
        bytes,
        "落盘字节与源一致"
    );
    let resolved = crate::storage::wiki_images::resolve_wiki_image_refs(&b.store, &stored);
    assert!(
        resolved.contains(&expected_path.display().to_string()),
        "解析指向接收端自己的数据目录：{resolved}"
    );

    // ---- apply 层负向：rel / mime / hash 三道闸门各拒一次 ----
    let empty_manifest = || super::SnapshotManifest {
        version: super::SNAPSHOT_VERSION,
        id: id.clone(),
        dataset_id: "dataset".into(),
        source_device_id: "a".into(),
        recipient_device_id: "b".into(),
        processor_device_id: "a".into(),
        key_version: 1,
        chunks: 0,
        bytes: 0,
        records: 0,
        digest: String::new(),
        frontiers: Default::default(),
        logical_clock: 0,
    };
    let real_sha = {
        use ring::digest::{Digest, SHA256};
        let d: Digest = ring::digest::digest(&SHA256, &bytes);
        d.as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let bytes_hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let file_record = |rel: &str, mime: &str, sha: &str, hex: &str| super::Record::WikiFile {
        rel: rel.into(),
        sha256: sha.into(),
        mime: mime.into(),
        size: bytes.len() as i64,
        bytes_hex: hex.into(),
    };
    let ledger_row = |rel: &str, mime: &str, sha: &str| super::Record::WikiAsset {
        rel: rel.into(),
        sha256: sha.into(),
        mime: mime.into(),
        size: bytes.len() as i64,
    };
    let mut seen = false;
    // 正向：合法记录可 apply（直接对 b 再来一次，幂等 OR REPLACE）。
    b.store
        .apply_snapshot_record(
            &id,
            &empty_manifest(),
            &file_record(rel, "image/jpeg", &real_sha, &bytes_hex),
            &mut seen,
        )
        .unwrap();
    // rel 形态不合法。
    let err = b
        .store
        .apply_snapshot_record(
            &id,
            &empty_manifest(),
            &file_record(
                "tweet_images/../etc/img_1.jpg",
                "image/jpeg",
                &real_sha,
                &bytes_hex,
            ),
            &mut seen,
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("sync_snapshot_wiki_file_invalid"),
        "{err}"
    );
    // mime 不在白名单。
    let err = b
        .store
        .apply_snapshot_record(
            &id,
            &empty_manifest(),
            &file_record(rel, "image/svg+xml", &real_sha, &bytes_hex),
            &mut seen,
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("sync_snapshot_wiki_file_invalid"),
        "{err}"
    );
    // 字节与 sha256 不一致（传输位翻转逃过加密也会在此被拦）。
    let mut tampered = bytes_hex.clone();
    let last = tampered.len() - 2;
    tampered.replace_range(last.., &format!("{:02x}", bytes[bytes.len() - 1] ^ 0xff));
    let err = b
        .store
        .apply_snapshot_record(
            &id,
            &empty_manifest(),
            &file_record(rel, "image/jpeg", &real_sha, &tampered),
            &mut seen,
        )
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("sync_snapshot_wiki_file_hash_mismatch"),
        "{err}"
    );
    // 台账行：mime 白名单同样生效（hash 一致性由 WikiFile 闸门验）。
    let err = b
        .store
        .apply_snapshot_record(
            &id,
            &empty_manifest(),
            &ledger_row(rel, "image/svg+xml", &real_sha),
            &mut seen,
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("sync_snapshot_wiki_asset_invalid"),
        "{err}"
    );
}

#[test]
fn snapshot_applies_legacy_absolute_path_wiki_rows_as_placeholders() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    // 旧 v2 代的行形态：正文里是发送端本机绝对路径（未折叠存量）。
    a.store
        .connection
        .execute(
            "INSERT INTO wiki_pages(id,slug,kind,title,summary,content_md,tags,source_event_ids,evidence_count,first_seen_at,last_seen_at,status,created_at,updated_at,area) \
             VALUES('wp-old','tweet-42','source','旧端页','',?1,'[]','[]',1,'t1','t1','active','t1','t1','imported')",
            ["看这张 ![](/home/old-user/.local/share/elsewhen/tweet_images/42/img_1.jpg)"],
        )
        .unwrap();
    {
        let tx = a.store.begin_write_transaction().unwrap();
        a.store.sync_record_wiki_page("wp-old", false).unwrap();
        if let Some(tx) = tx {
            tx.commit().unwrap();
        }
    }
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
    let content: String = b
        .store
        .connection
        .query_row(
            "SELECT content_md FROM wiki_pages WHERE id='wp-old'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        content.contains("![]({{tweet-image:tweet_images/42/img_1.jpg}})")
            && !content.contains("/home/old-user"),
        "接收端落地必须已是占位符：{content}"
    );
}

#[test]
fn skill_distribution_travels_inside_snapshot() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    // A 端：技能发布两文件 + 一条墓碑（模拟曾卸载过的旧技能）。
    a.store
        .connection
        .execute(
            "INSERT INTO skill_files(skill_id,rel,bytes,sha256,size,updated_at)
             VALUES('proxy-pick','SKILL.md',?1,?2,3,'2026-10-18T08:00:00Z'),
                   ('proxy-pick','scripts/pick.py',x'70697961',?3,4,'2026-10-18T08:00:00Z')",
            rusqlite::params![
                b"abc".as_slice(),
                {
                    use ring::digest::{SHA256, digest};
                    let d = digest(&SHA256, b"abc");
                    hex_of(d.as_ref())
                },
                {
                    use ring::digest::{SHA256, digest};
                    let d = digest(&SHA256, b"piya");
                    hex_of(d.as_ref())
                },
            ],
        )
        .unwrap();
    a.store
        .connection
        .execute(
            "INSERT INTO skill_tombstones(skill_id,updated_at) VALUES('old-skill','2026-10-17T00:00:00Z')",
            [],
        )
        .unwrap();

    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    b.store
        .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
        .unwrap();

    // 技能文件与墓碑随快照到达（分发的「随整库走」在此断言，不许再静默断档）。
    assert_eq!(b.count("skill_files"), 2, "两枚技能文件应随快照到达 B");
    assert_eq!(b.count("skill_tombstones"), 1, "墓碑应随快照到达 B");
    let bytes: Vec<u8> = b
        .store
        .connection
        .query_row(
            "SELECT bytes FROM skill_files WHERE skill_id='proxy-pick' AND rel='scripts/pick.py'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bytes, b"piya", "字节 hex 往返应逐位一致");
}

fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn mcp_server_snapshot_strips_credentials_and_trust_and_local_rows_win() {
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    // 发送端：linux stdio（本机绑定命令）+ all 远程，都带鉴权头与 trusted。
    let make = |name: &str, platform: &str, transport: &str, command: &str, url: &str| {
        a.store
            .save_mcp_server(
                None, name, platform, transport, command, "[]", "{}", url,
                r#"{"Authorization":"Bearer SECRET"}"#, true, true,
            )
            .unwrap()
    };
    let _local = make("fs-variant", "linux", "stdio", "npx", "");
    let remote = make("remote-mcp", "all", "streamable-http", "", "https://mcp.example.com/mcp");
    let fresh = make("fresh-remote", "all", "streamable-http", "", "https://fresh.example.com/mcp");
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let id = Uuid::new_v4().to_string();
    let (chunks, manifest) = export(&a.store, "b", &id, &key, &signer);
    // 接收端先自建同 (name,platform) 行（trusted=1、自己的 URL）——本机获胜。
    b.store
        .save_mcp_server(
            None, "remote-mcp", "all", "streamable-http", "", "[]", "{}",
            "https://my-own.example.com/mcp", "{}", true, true,
        )
        .unwrap();
    b.store.begin_snapshot_import(&id, "a", 1).unwrap();
    stage(&b.store, &id, "a", &chunks, &key, &signer.public_key());
    assert!(
        b.store
            .publish_snapshot(&id, "a", 1, &manifest, &key, &signer.public_key())
            .unwrap()
    );
    // 三铁律（无本机对照的 fresh 行）：行到达、headers 不搭车（凭据不离
    // 本机）、trusted 强制 0（信任是本机事实，同步到达的 server 回到逐次
    // 审批）。
    let servers = b.store.list_mcp_servers().unwrap();
    let restored = servers.iter().find(|s| s.id == fresh).unwrap();
    assert_eq!(restored.platform, "all");
    assert_eq!(restored.headers_json, "{}");
    assert!(!restored.trusted, "trusted 不得搭车");
    // remote-mcp 被本机获胜占用：远端行不落表。
    assert!(servers.iter().all(|s| s.id != remote));
    let local_row = servers.iter().find(|s| s.name == "fs-variant").unwrap();
    assert_eq!(local_row.platform, "linux");
    assert_eq!(local_row.headers_json, "{}");
    assert!(!local_row.trusted);
    // 本机获胜：同名同平台只有一条，且是接收端自己那行。
    let mine: Vec<_> = servers.iter().filter(|s| s.name == "remote-mcp").collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].url, "https://my-own.example.com/mcp");
    assert!(mine[0].trusted, "接收端本机行不受快照影响");
    // 同步到达的行不含 headers（fresh 行的对照面已验证 trusted=0）。
    let all_synced = servers.iter().find(|s| s.id == fresh).unwrap();
    assert_eq!(all_synced.url, "https://fresh.example.com/mcp");
}
