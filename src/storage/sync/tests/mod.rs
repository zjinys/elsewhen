use super::*;
use crate::event::NewEvent;
use rusqlite::OptionalExtension;
use uuid::Uuid;

// 公共测试辅助（原文件平移，pub(super) 供分组子文件复用）。
struct Database {
    store: Store,
    dir: std::path::PathBuf,
}

impl Database {
    fn new(device: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("elsewhen-sync-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "test-dataset".into(),
                device_id: device.into(),
                stream_id: format!("stream-{device}"),
                processor_device_id: "a".into(),
            })
            .unwrap();
        Self { store, dir }
    }
    fn count(&self, table: &str) -> i64 {
        self.store
            .connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub(super) fn pair(a: &Database, b: &Database) {
    let ai = a.store.sync_identity().unwrap().unwrap();
    let bi = b.store.sync_identity().unwrap().unwrap();
    a.store
        .authorize_sync_member(&bi.device_id, &bi.stream_id)
        .unwrap();
    b.store
        .authorize_sync_member(&ai.device_id, &ai.stream_id)
        .unwrap();
}

/// 跳过队首（最低 origin_sequence）只送其余 op：接收端留缺口（gap-wait），
/// 用于驱动「依赖证据未到 → 等待补齐」的接收路径。
fn deliver_except_first(a: &Database, b: &Database) -> Vec<SyncOperation> {
    let ai = a.store.sync_identity().unwrap().unwrap();
    let bi = b.store.sync_identity().unwrap().unwrap();
    let ops = a.store.pending_sync_operations(&bi.device_id, 100).unwrap();
    for op in ops.iter().skip(1) {
        b.store
            .receive_sync_operation(&ai.device_id, &serde_json::to_vec(op).unwrap())
            .unwrap();
    }
    ops
}

pub(super) fn deliver(a: &Database, b: &Database) -> Vec<SyncOperation> {
    let ai = a.store.sync_identity().unwrap().unwrap();
    let bi = b.store.sync_identity().unwrap().unwrap();
    let ops = a.store.pending_sync_operations(&bi.device_id, 100).unwrap();
    for op in &ops {
        b.store
            .receive_sync_operation(&ai.device_id, &serde_json::to_vec(op).unwrap())
            .unwrap();
    }
    ops
}

// ---- P4a: S01 source isolation, idempotency fidelity, long-offline catch-up ----

fn op_state(db: &Database, op_id: &str) -> Option<(String, Option<String>)> {
    db.store
        .connection
        .query_row(
            "SELECT state,error_code FROM sync_operations WHERE op_id=?1",
            [op_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .unwrap()
}

pub(super) fn isolated_count(db: &Database) -> i64 {
    db.store
        .connection
        .query_row(
            "SELECT count(*) FROM sync_operations WHERE state='isolated'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

pub(super) fn capture_wiki(db: &Database, id: &str) {
    let tx = db.store.begin_write_transaction().unwrap();
    db.store.sync_record_wiki_page(id, false).unwrap();
    if let Some(tx) = tx {
        tx.commit().unwrap();
    }
}

pub(super) fn seed_wiki_page(
    db: &Database,
    id: &str,
    slug: &str,
    title: &str,
    content: &str,
    evidence: &[&str],
) {
    let now = chrono::Utc::now().to_rfc3339();
    let mut all_evidence: Vec<String> = evidence.iter().map(|e| e.to_string()).collect();
    all_evidence.sort();
    all_evidence.dedup();
    db.store
        .connection
        .execute(
            "INSERT INTO wiki_pages(id,slug,kind,title,summary,content_md,tags,source_event_ids, \
             evidence_count,first_seen_at,last_seen_at,status,created_at,updated_at,area) \
             VALUES(?1,?2,'topic',?3,'',?4,'[]',?5,?6,'2026-01-01T00:00:00+00:00','2026-01-01T00:00:00+00:00','active','2026-01-01T00:00:00+00:00','2026-01-01T00:00:00+00:00','insight')",
            params![
                id,
                slug,
                title,
                content,
                serde_json::to_string(&all_evidence).unwrap(),
                (all_evidence.len() as i64).max(1),
            ],
        )
        .unwrap();
}

pub(super) fn resolve_kindbinding(
    db: &Database,
    op_id: &str,
    updated_at: Option<&str>,
    accept: bool,
) {
    db.store
        .resolve_object_conflict(op_id, updated_at, accept)
        .unwrap();
}

mod message_conflicts;

mod capture_replay;

mod receipts_deletion;

mod transport_isolation;

mod wiki_pages;

mod conflict_review;

mod domain_objects;
