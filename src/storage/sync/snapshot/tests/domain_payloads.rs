use super::export;
use super::pair;
use super::stage;
use super::*;

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
        .create_todo("快照待办", "high", None, None, None, None, false)
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
fn snapshot_carries_negative_knowledge_to_new_devices() {
    // 摘录孵化闭环（migration 50）：负知识（identity + 否定原因）必须跨
    // 设备——「你否定过这条来源」的提示在新设备上仍然成立。
    let a = Database::new();
    let b = Database::new();
    a.enable("a");
    b.enable("b");
    pair(&a, &b);
    let page = a
        .store
        .upsert_wiki_page(
            &crate::storage::WikiPageDraft {
                slug: "source-neg".into(),
                kind: "source".into(),
                title: "跑步治愈关节炎".into(),
                summary: "跑步治愈关节炎".into(),
                content_md: "跑步治愈关节炎".into(),
                tags: vec![],
                source_event_ids: vec![],
                status: "active".into(),
                reason: "test import".into(),
                source_url: Some("https://example.com/neg".into()),
            },
            crate::storage::ContentPolicy::Always,
        )
        .unwrap()
        .page;
    a.store
        .set_wiki_opinion(&page.slug, Some("reject"))
        .unwrap();
    a.store
        .connection
        .execute(
            "UPDATE knowledge_sources SET reject_reason='伪科学，已辟谣' WHERE id IN
             (SELECT source_id FROM knowledge_source_pages WHERE page_id=?1)",
            [&page.id],
        )
        .unwrap();

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
    let rows: Vec<(String, Option<String>, Option<String>)> = b
        .store
        .connection
        .prepare(
            "SELECT identity,opinion,reject_reason FROM knowledge_sources WHERE opinion='reject'",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(
        rows.iter()
            .any(|(_, _, reason)| reason.as_deref() == Some("伪科学，已辟谣")),
        "负知识原因必须随快照到达新设备；实际 {rows:?}"
    );
    // wiki_pages 行也断层台判定源：同步后的 B 端能指导台账提示
    let hit = b
        .store
        .rejection_for_source(Some("https://example.com/neg"), "跑步治愈关节炎")
        .unwrap();
    assert_eq!(hit.as_deref(), Some("伪科学，已辟谣"));
}
