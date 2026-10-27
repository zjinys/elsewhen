//! 内存里的快照数据面，只给测试用。
//!
//! 为什么不直接依赖真 relay：删除 mail 分片路径之后，单测里再没有「不需要网络也能
//! 跑一份快照」的通道 —— 剩下的只有 `#[ignore]` 的冒烟用例，等于把窗口流控、
//! 逐窗 ack、manifest 时序的 CI 覆盖一起删掉。这个小实现语义上照着 relay 的
//! `/blob/*`（开/写/看进度/取一窗/确认释放/关掉），让收发两端在测试里跑**同一段
//! 生产代码**。
//!
//! 它故意不模拟 TTL、配额、404 降级这些东西：那些属于 `transport_relay` 的
//! 用例（`blob.rs`）与真机冒烟，不是这里的职责。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, ensure};

use crate::sync::crypto::ContentKey;
use crate::sync::engine::snapshot::{BlobDownload, BlobUpload};
use crate::sync::transport_relay::{BlobOpen, BlobSink, BlobSource, BlobState, BlobWindow};

#[derive(Debug, Default)]
struct Blob {
    chunks: BTreeMap<usize, Vec<u8>>,
    /// 被 `truncate` 藏起来的片：模拟「relay 上这会儿还没有」。`restore` 放回来。
    hidden: BTreeMap<usize, Vec<u8>>,
    /// 接收方已确认到哪一片（`None` = 还没确认过任何片）。
    acked_upto: Option<usize>,
    /// 关掉时带的状态（`done`/`cancel`）。关掉后片就不再占空间。
    closed: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct MemBlob {
    inner: Mutex<BTreeMap<String, Blob>>,
}

impl MemBlob {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 只留前 `keep` 片，其余藏起来：模拟「relay 上这会儿只到了一半」。
    pub(crate) fn truncate(&self, blob_id: &str, keep: usize) {
        self.with(blob_id, |blob| {
            let tail: Vec<usize> = blob.chunks.range(keep..).map(|(index, _)| *index).collect();
            for index in tail {
                if let Some(bytes) = blob.chunks.remove(&index) {
                    blob.hidden.insert(index, bytes);
                }
            }
        });
    }

    /// 把藏起来的片放回去（窗口补齐）。
    pub(crate) fn restore(&self, blob_id: &str) {
        self.with(blob_id, |blob| {
            for (index, bytes) in std::mem::take(&mut blob.hidden) {
                blob.chunks.insert(index, bytes);
            }
        });
    }

    fn with<T>(&self, blob_id: &str, f: impl FnOnce(&mut Blob) -> T) -> T {
        let mut all = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        f(all.entry(blob_id.to_string()).or_default())
    }

    /// 当前仍占着空间的片数（诊断断言用）。
    pub(crate) fn live_chunks(&self, blob_id: &str) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(blob_id)
            .filter(|blob| blob.closed.is_none())
            .map_or(0, |blob| blob.chunks.len())
    }

    pub(crate) fn close_status(&self, blob_id: &str) -> Option<String> {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(blob_id)
            .and_then(|blob| blob.closed.clone())
    }
}

fn present(blob: &Blob) -> Vec<usize> {
    if blob.closed.is_some() {
        return Vec::new();
    }
    blob.chunks.keys().copied().collect()
}

impl BlobSink for MemBlob {
    fn open(&self, blob_id: &str) -> Result<BlobOpen> {
        Ok(self.with(blob_id, |blob| {
            // 已关掉的 blob 再开就是「新的」：relay 上关掉等于释放，同 id 重开
            // 从第 0 片开始。发送端据此决定是否重传整份。
            if blob.closed.is_some() {
                *blob = Blob::default();
                return BlobOpen {
                    created: true,
                    have: Vec::new(),
                };
            }
            let created = blob.chunks.is_empty() && blob.acked_upto.is_none();
            BlobOpen {
                created,
                have: present(blob),
            }
        }))
    }

    fn state(&self, blob_id: &str) -> Result<Option<BlobState>> {
        let all = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let Some(blob) = all.get(blob_id) else {
            // relay 上从来没有过这份 blob。
            return Ok(None);
        };
        let present = present(blob);
        Ok(Some(BlobState {
            highest: present.last().copied(),
            stored_bytes: blob.chunks.values().map(|bytes| bytes.len() as i64).sum(),
            present,
        }))
    }

    fn put(&self, blob_id: &str, index: usize, payload: &[u8]) -> Result<()> {
        ensure!(!payload.is_empty(), "empty blob chunk");
        self.with(blob_id, |blob| {
            if blob.closed.is_some() {
                *blob = Blob::default();
            }
            blob.chunks.insert(index, payload.to_vec());
            Ok(())
        })
    }
}

impl BlobSource for MemBlob {
    fn get_window(&self, blob_id: &str, from: usize, count: usize) -> Result<BlobWindow> {
        let all = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let Some(blob) = all.get(blob_id) else {
            return Ok(BlobWindow::default());
        };
        if blob.closed.is_some() {
            return Ok(BlobWindow::default());
        }
        // 与 relay 一致：只返回**从 `from` 开始连续**的那一段。第一个缺口就停，
        // 绝不跳着给 —— 跳过缺口会让接收端把后面的片错位落盘。
        let mut chunks = Vec::new();
        let mut index = from;
        while chunks.len() < count {
            let Some(bytes) = blob.chunks.get(&index) else {
                break;
            };
            chunks.push(bytes.clone());
            index += 1;
        }
        if chunks.is_empty() {
            return Ok(BlobWindow::default());
        }
        Ok(BlobWindow {
            from: Some(from),
            chunks,
        })
    }

    fn ack(&self, blob_id: &str, upto: usize) -> Result<()> {
        self.with(blob_id, |blob| {
            blob.chunks.retain(|index, _| *index > upto);
            blob.acked_upto = Some(upto);
            Ok(())
        })
    }

    fn close(&self, blob_id: &str, status: &str) -> Result<()> {
        self.with(blob_id, |blob| {
            blob.chunks.clear();
            blob.hidden.clear();
            blob.closed = Some(status.to_string());
            Ok(())
        })
    }
}

/// 一份指向内存数据面的上传上下文（`signal: None`：信令只进 outbox）。
pub(crate) fn mem_upload(plane: &Arc<MemBlob>, recipient: &str, key_version: u64) -> BlobUpload {
    BlobUpload {
        sink: Box::new(plane.clone()),
        signal: None,
        recipient: recipient.to_string(),
        key_version,
        window_wait_ms: 50,
    }
}

/// 一份指向内存数据面的下载通道。
pub(crate) fn mem_download<'a>(
    plane: &'a Arc<MemBlob>,
    from: &'a str,
    key_version: u64,
    key: &'a ContentKey,
    public: &'a [u8; 32],
) -> BlobDownload<'a> {
    BlobDownload {
        source: plane.as_ref(),
        from,
        key_version,
        key,
        public,
    }
}
