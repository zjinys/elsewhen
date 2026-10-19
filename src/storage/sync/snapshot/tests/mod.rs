use super::*;
use crate::storage::sync::ReplicaIdentity;
use crate::sync::engine;
use uuid::Uuid;

// 公共测试辅助（原文件平移，pub(super) 供分组子文件复用）。
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

pub(super) fn pair(a: &Database, b: &Database) {
    a.store.authorize_sync_member("b", "stream-b").unwrap();
    b.store.authorize_sync_member("a", "stream-a").unwrap();
}

pub(super) fn export(
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

pub(super) fn stage(
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

pub(super) fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

mod lifecycle;

mod fencing_checkpoints;

mod frame_completeness;

mod domain_payloads;

mod skills_mcp;
