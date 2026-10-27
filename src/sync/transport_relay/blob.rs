//! relay v3 blob 数据面客户端：快照密文的分片级上传与拉取。
//!
//! 为什么快照不能走信件：一封信一封密文，relay 上的峰值就是整份快照；丢一次
//! 响应就重投一份整份副本，实测把同一份负载放大到 4.4×。blob 面把「身份」钉在
//! `(blob_id, index)` 上，重投同一片只是原地覆盖，且接收方 ack 一片就立即释放
//! 一片 —— 两端在途量都降到「窗口」这个量级。
//!
//! 盲性：这里上传的是快照分片的**密文信封**（与信件路径同一套 `crypto::seal`），
//! relay 只见到 `blob_id`、分片序号与长度。签名域与 relay 侧一一对应，六个域各自
//! 独立（`blob-open`/`blob-put`/`blob-state`/`blob-get`/`blob-ack`/`blob-close`），
//! 任何一个域的签名都不能拿去顶另一个端点。

use super::*;

/// 单片上限。relay 侧同样卡 1 MiB。
///
/// 片大小的真实影响看下面的 `WINDOW_CHUNKS` 注释：256 KiB 的明文片封成信封后
/// 实测约 683 KiB，不是「360 KiB」——两处写两套数会让人按旧数算出 32 片窗口，
/// 而 32 × 683 KiB = 21 MiB 直接撞穿 relay 的 16 MB 响应上限。
const CHUNK_BYTES_MAX: usize = 1024 * 1024;
/// 一次 GET 最多取几片 = 在途窗口的片上界。
///
/// 实测：256 KiB 的明文片封成信封后约 683 KiB（记录帧先 base64 一次，信封里再
/// base64 一次）。所以「32 片 = 8 MiB」的算法是错的 —— 32 片实际是 21 MiB，直接
/// 撞穿 relay 的 16 MB 响应上限，接收端永远拿不到完整一窗。
/// 这里按**最坏情况**定：`WINDOW_CHUNKS * CHUNK_BYTES_MAX = 10 MiB ≤ 16 MB`，
/// 无论片多大都不会溢出；真实片大下 10 片 ≈ 6.8 MiB，加上发送端没被 ack 的
/// 那一片，relay 峰值仍在一窗量级。
pub(crate) const WINDOW_CHUNKS: usize = 10;
/// 与 relay 侧 `BLOB_ID` 正则一致：URL 上的身份，先卡长度与字符集。
fn validate_blob_id(value: &str) -> Result<()> {
    ensure!(
        (16..=64).contains(&value.len())
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'),
        "invalid blob id"
    );
    Ok(())
}

/// `blob_open` 的结论：`created` 为真表示 relay 上刚建了一份**全新的空** blob
/// （原来的丢了或从没开过），续传必须从第 0 片重来；否则 `have` 就是仍在上面的片。
#[derive(Debug, Default)]
pub(crate) struct BlobOpen {
    pub created: bool,
    pub have: Vec<usize>,
}

/// 一个 blob 的进度视图。总量不在这里：片数/总字节/摘要由端到端签名的
/// manifest 决定，relay 不知道也不需要知道。
#[derive(Debug, Clone, Default)]
pub(crate) struct BlobState {
    /// 仍在 relay 上、尚未被接收方 ack 的分片序号（升序）。
    pub present: Vec<usize>,
    /// 收到的最高分片序号，空 blob 为 None。
    pub highest: Option<usize>,
    pub stored_bytes: i64,
}

impl BlobState {
    /// 接收方已经确认到第几片（不含）。上传方靠它推算在途窗口。
    pub(crate) fn acked_upto(&self) -> usize {
        self.present.first().copied().unwrap_or(usize::MAX)
    }
}

/// 各 relay「有没有 blob 数据面」的本地答案，键是 `base_url`。
///
/// 为什么要有这份缓存（review 2026-10-25 M2）：`snapshot_upload` 每拍都要回答
/// 「这次导出走数据面还是信件面」，不打缓存就是每拍一次 `/status`。键是
/// `base_url` 而不是「全局布尔」：换一台 relay 自然重新探测。
static BLOB_CAPABILITY: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, bool>>,
> = std::sync::OnceLock::new();

fn blob_capability_map() -> &'static std::sync::Mutex<std::collections::HashMap<String, bool>> {
    BLOB_CAPABILITY.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// 已经问出答案就返回它；`None` 表示「还没问过」。
pub(crate) fn blob_capability(base_url: &str) -> Option<bool> {
    blob_capability_map().lock().ok()?.get(base_url).copied()
}

pub(crate) fn set_blob_capability(base_url: &str, supported: bool) {
    if let Ok(mut map) = blob_capability_map().lock() {
        map.insert(base_url.to_string(), supported);
    }
}

impl RelayClient {
    /// 开户。幂等：同一个 `blob_id` 重复开只续期并回报已有的分片序号。
    pub(crate) fn blob_open(
        &self,
        mailbox: &str,
        sender_mb: &str,
        sender_cred: &[u8; 32],
        blob_id: &str,
    ) -> Result<BlobOpen> {
        validate_mailbox(mailbox)?;
        validate_mailbox(sender_mb)?;
        validate_blob_id(blob_id)?;
        let (ts, nonce) = request_stamp();
        let sig = sign(
            sender_cred,
            "blob-open",
            &[sender_mb, mailbox, blob_id],
            ts,
            &nonce,
        );
        let result: serde_json::Value = self
            .post(
                "blob/open",
                &serde_json::json!({
                    "version": VERSION, "mailbox_key": mailbox, "sender_mb": sender_mb,
                    "blob_id": blob_id, "ts": ts, "nonce": nonce, "sig": sig,
                }),
            )
            // 老 relay 上 `/blob/*` 是路由级 404：不是「网络抖一下」，而是
            // 「这台 relay 没有 blob 数据面」（review N1）。`/blob/open` 在**有**
            // blob 面的 relay 上永不 404（不存在的 blob 会被建成新的），所以这个
            // 404 是结论性的 —— 顺手把这台 relay 降级成「没有数据面」，下一拍
            // 直接以 `sync_protocol_unsupported` 停手，不再每拍撞一次 404。
            .map_err(|error| {
                let error = blob_unavailable(error);
                if error.is::<RelayBlobUnavailable>() {
                    set_blob_capability(&self.base_url, false);
                }
                error
            })?;
        let have = result
            .get("have")
            .and_then(|value| value.as_array())
            .context("relay response missing blob progress")?;
        let mut indices = Vec::with_capacity(have.len());
        for value in have {
            let index = value.as_u64().context("invalid blob chunk index")? as usize;
            // 上界跟 relay 的 `MAX_BLOB_CHUNKS`（4096）不是同一个数，但**故意更严**：
            // 2560 = WINDOW_CHUNKS(10) × 256，是本客户端能接受的最大片号，用来把
            // 「对端/中继返回了离谱片号」在解析层就拦住，而不是等组装时才炸。
            // 两个数都不随对方变化，所以不需要协议协商。
            ensure!(index < WINDOW_CHUNKS * 256, "blob chunk index out of range");
            indices.push(index);
        }
        indices.sort_unstable();
        Ok(BlobOpen {
            created: result
                .get("created")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            have: indices,
        })
    }

    /// 写一片。body 是裸二进制（不套 base64），签名把这一片的确切字节钉住。
    pub(crate) fn blob_put(
        &self,
        mailbox: &str,
        sender_mb: &str,
        sender_cred: &[u8; 32],
        blob_id: &str,
        index: usize,
        payload: &[u8],
    ) -> Result<()> {
        validate_mailbox(mailbox)?;
        validate_mailbox(sender_mb)?;
        validate_blob_id(blob_id)?;
        ensure!(!payload.is_empty(), "empty blob chunk");
        ensure!(payload.len() <= CHUNK_BYTES_MAX, "blob chunk too large");
        validate_integer(index as i64)?;
        let (ts, nonce) = request_stamp();
        // 与 `send` 同一约定：内容的 sha256 放在 ts/nonce **之后**（relay 侧
        // `senderOperation` 收到 digest 也是往 signed(...) 尾部追加）。放错位置
        // 会 HMAC 全挂，这是和真实 relay 对跑时抓出来的。
        let sig = hmac_sign_hex(
            sender_cred,
            &[
                "relay-v2-blob-put",
                sender_mb,
                mailbox,
                blob_id,
                &index.to_string(),
                &ts.to_string(),
                &nonce,
                &sha256_hex(payload),
            ],
        );
        let response = self
            .http
            .put(format!("{}/blob/chunk", self.base_url))
            .query(&[
                ("version", VERSION.to_string()),
                ("mailbox_key", mailbox.to_string()),
                ("sender_mb", sender_mb.to_string()),
                ("blob_id", blob_id.to_string()),
                ("index", index.to_string()),
                ("ts", ts.to_string()),
                ("nonce", nonce),
                ("sig", sig),
            ])
            .body(payload.to_vec())
            .send()
            .context("relay blob PUT failed")?;
        // 404 = 这份 blob 真的没了（被配额/TTL 收掉）：要从第 0 片重开，
        // 而不是按 5xx 退避十分钟。
        let _: serde_json::Value = response_json(response).map_err(blob_unavailable)?;
        Ok(())
    }

    /// 进度。`sender` 给出时用 sender 凭据（上传方维持窗口），否则用 owner 凭据。
    ///
    /// **不会 404**：relay 的 `/blob/state` 对不存在的 blob 返回 `200 + present:[]`
    /// （`expires_at:0`、`highest:-1`、`stored_bytes:0`），语义是「没有进度」而不是
    /// 「出错了」—— 上传方据此判「blob 被 TTL 收掉了，从 0 重传」。真正会 404 的是
    /// `blob_put` / `blob_get_window`（那份 blob 真的没了）。
    pub(crate) fn blob_state(
        &self,
        mailbox: &str,
        blob_id: &str,
        credential: &[u8; 32],
        sender: Option<&str>,
    ) -> Result<Option<BlobState>> {
        validate_mailbox(mailbox)?;
        validate_blob_id(blob_id)?;
        let (ts, nonce) = request_stamp();
        let sig = sign(credential, "blob-state", &[mailbox, blob_id], ts, &nonce);
        let mut query = vec![
            ("version", VERSION.to_string()),
            ("mailbox_key", mailbox.to_string()),
            ("blob_id", blob_id.to_string()),
            ("ts", ts.to_string()),
            ("nonce", nonce),
            ("sig", sig),
        ];
        if let Some(sender) = sender {
            validate_mailbox(sender)?;
            query.push(("sender_mb", sender.to_string()));
        }
        match self.get::<serde_json::Value>("blob/state", &query) {
            Ok(result) => {
                let present = result
                    .get("present")
                    .and_then(|value| value.as_array())
                    .context("relay response missing blob progress")?;
                let mut indices = Vec::with_capacity(present.len());
                for value in present {
                    indices.push(value.as_u64().context("invalid blob chunk index")? as usize);
                }
                indices.sort_unstable();
                let stored = result
                    .get("stored_bytes")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(0);
                Ok(Some(BlobState {
                    highest: indices.last().copied(),
                    present: indices,
                    stored_bytes: stored,
                }))
            }
            Err(error) if is_not_found(&error) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// 拉一窗分片：返回 `(起始序号, 下一片序号, 各片密文)`。
    /// body 是 `[4 字节大端长度][密文]` 的连续帧，省掉 base64 的 1.37× 膨胀。
    pub(crate) fn blob_get_window(
        &self,
        mailbox: &str,
        owner_cred: &[u8; 32],
        blob_id: &str,
        from: usize,
        limit: usize,
    ) -> Result<BlobWindow> {
        validate_mailbox(mailbox)?;
        validate_blob_id(blob_id)?;
        ensure!((1..=WINDOW_CHUNKS).contains(&limit), "invalid blob window");
        validate_integer(from as i64)?;
        let (ts, nonce) = request_stamp();
        let sig = sign(
            owner_cred,
            "blob-get",
            &[mailbox, blob_id, &from.to_string(), &limit.to_string()],
            ts,
            &nonce,
        );
        let response = self
            .http
            .get(format!("{}/blob/chunk", self.base_url))
            .query(&[
                ("version", VERSION.to_string()),
                ("mailbox_key", mailbox.to_string()),
                ("blob_id", blob_id.to_string()),
                ("from", from.to_string()),
                ("limit", limit.to_string()),
                ("ts", ts.to_string()),
                ("nonce", nonce),
                ("sig", sig),
            ])
            .send()
            .context("relay blob GET failed")?;
        if !response.status().is_success() {
            if is_not_found_status(response.status()) {
                return Ok(BlobWindow::default());
            }
            return Err(RelayHttpError(response.status()).into());
        }
        let start = response
            .headers()
            .get("x-blob-from")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(-1);
        let mut body = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut body)
            .context("read relay blob response")?;
        ensure!(
            body.len() as u64 <= MAX_RESPONSE_BYTES,
            "relay blob response too large"
        );
        let chunks = parse_frames(&body)?;
        ensure!(chunks.len() <= limit, "relay returned too many chunks");
        // 有分片却没有游标 = 协议违约（中间的代理把 `x-blob-from` 吃了，或 relay
        // 漏了这个头）。这时默默返回空窗会让接收端永远“还差一片”，而发送端
        // 永远等不到 ack —— 面板就停在「等待成员初始同步」上，跟事故现场一模一样。
        ensure!(
            chunks.is_empty() || start >= 0,
            "relay blob window missing cursor"
        );
        Ok(BlobWindow {
            from: usize::try_from(start).ok(),
            chunks,
        })
    }

    /// 逐片释放：接收方确认「序号 ≤ upto 已落盘」后，relay 立即删掉这些分片。
    pub(crate) fn blob_ack(
        &self,
        mailbox: &str,
        owner_cred: &[u8; 32],
        blob_id: &str,
        upto: usize,
    ) -> Result<usize> {
        validate_mailbox(mailbox)?;
        validate_blob_id(blob_id)?;
        validate_integer(upto as i64)?;
        let (ts, nonce) = request_stamp();
        let sig = sign(
            owner_cred,
            "blob-ack",
            &[mailbox, blob_id, &upto.to_string()],
            ts,
            &nonce,
        );
        let result: serde_json::Value = self.post(
            "blob/ack",
            &serde_json::json!({
                "version": VERSION, "mailbox_key": mailbox, "blob_id": blob_id,
                "upto": upto, "ts": ts, "nonce": nonce, "sig": sig,
            }),
        )?;
        Ok(integer_field(&result, "deleted")? as usize)
    }

    /// 结束并精确回收。`status` 只用 `done`/`cancel`；owner 或开户 sender 都能关。
    pub(crate) fn blob_close(
        &self,
        mailbox: &str,
        credential: &[u8; 32],
        blob_id: &str,
        status: &str,
        sender: Option<&str>,
    ) -> Result<usize> {
        validate_mailbox(mailbox)?;
        validate_blob_id(blob_id)?;
        ensure!(
            matches!(status, "done" | "cancel"),
            "invalid blob close status"
        );
        let (ts, nonce) = request_stamp();
        let sig = sign(
            credential,
            "blob-close",
            &[mailbox, blob_id, status],
            ts,
            &nonce,
        );
        let mut body = serde_json::json!({
            "version": VERSION, "mailbox_key": mailbox, "blob_id": blob_id, "status": status,
            "ts": ts, "nonce": nonce, "sig": sig,
        });
        if let Some(sender) = sender {
            validate_mailbox(sender)?;
            body["sender_mb"] = sender.into();
        }
        let result: serde_json::Value = self.post("blob/close", &body)?;
        Ok(integer_field(&result, "freed")? as usize)
    }
}

/// 一窗下载的结果。
#[derive(Debug, Default)]
pub(crate) struct BlobWindow {
    /// relay 上实际起始的片号；空窗（缺口之后没有片）为 None。
    pub from: Option<usize>,
    pub chunks: Vec<Vec<u8>>,
}

/// 发送端眼里的快照数据面：开一份 blob、看它的进度、放一片。
///
/// 为什么有这层 trait（2026-10-26 C5-A）：mail 分片路径删除后，「不需要 relay
/// 也能跑快照」的单元测试通道就没了 —— 只剩 `#[ignore]` 的真 relay 冒烟用例，
/// 等于把窗口/ack/发布时序的 CI 覆盖一起删掉。两个三方法接缝让测试用内存数据面
/// 跑**同一段生产代码**，而不是留一条测试专用的快照旁路。
pub(crate) trait BlobSink {
    /// 幂等开户：`created` 为真表示从第 0 片重来。
    fn open(&self, blob_id: &str) -> Result<BlobOpen>;
    /// 仍未被接收方 ack 的片。`None` 表示 relay 上已经看不到这份 blob。
    fn state(&self, blob_id: &str) -> Result<Option<BlobState>>;
    fn put(&self, blob_id: &str, index: usize, payload: &[u8]) -> Result<()>;
}

/// 接收端眼里的快照数据面：取一窗、确认释放、关掉。
pub(crate) trait BlobSource {
    fn get_window(&self, blob_id: &str, from: usize, count: usize) -> Result<BlobWindow>;
    /// 确认到第 `upto` 片（含）已落盘，relay 据此释放这一窗。
    fn ack(&self, blob_id: &str, upto: usize) -> Result<()>;
    fn close(&self, blob_id: &str, status: &str) -> Result<()>;
}

/// 共享一份数据面（`Arc<MemBlob>`、只读的 `Arc<RelaySink>` 都靠它）：让
/// `Box<dyn BlobSink>` / `&dyn BlobSource` 可以从共享所有权构造出来。
impl<T: BlobSink + ?Sized> BlobSink for std::sync::Arc<T> {
    fn open(&self, blob_id: &str) -> Result<BlobOpen> {
        (**self).open(blob_id)
    }
    fn state(&self, blob_id: &str) -> Result<Option<BlobState>> {
        (**self).state(blob_id)
    }
    fn put(&self, blob_id: &str, index: usize, payload: &[u8]) -> Result<()> {
        (**self).put(blob_id, index, payload)
    }
}

impl<T: BlobSource + ?Sized> BlobSource for std::sync::Arc<T> {
    fn get_window(&self, blob_id: &str, from: usize, count: usize) -> Result<BlobWindow> {
        (**self).get_window(blob_id, from, count)
    }
    fn ack(&self, blob_id: &str, upto: usize) -> Result<()> {
        (**self).ack(blob_id, upto)
    }
    fn close(&self, blob_id: &str, status: &str) -> Result<()> {
        (**self).close(blob_id, status)
    }
}

/// 生产实现：把身份（信箱 + 凭据）绑在结构里，让调用点不必每片传五个参数。
pub(crate) struct RelaySink {
    pub(crate) client: RelayClient,
    pub(crate) mailbox: String,
    pub(crate) sender_mb: String,
    pub(crate) credential: [u8; 32],
}

impl BlobSink for RelaySink {
    fn open(&self, blob_id: &str) -> Result<BlobOpen> {
        self.client
            .blob_open(&self.mailbox, &self.sender_mb, &self.credential, blob_id)
    }

    fn state(&self, blob_id: &str) -> Result<Option<BlobState>> {
        self.client.blob_state(
            &self.mailbox,
            blob_id,
            &self.credential,
            Some(&self.sender_mb),
        )
    }

    fn put(&self, blob_id: &str, index: usize, payload: &[u8]) -> Result<()> {
        self.client.blob_put(
            &self.mailbox,
            &self.sender_mb,
            &self.credential,
            blob_id,
            index,
            payload,
        )
    }
}

/// 生产实现（接收端）：读/确认自己的信箱是 owner 权限。
pub(crate) struct RelaySource {
    pub(crate) client: RelayClient,
    pub(crate) mailbox: String,
    pub(crate) owner_credential: [u8; 32],
}

impl BlobSource for RelaySource {
    fn get_window(&self, blob_id: &str, from: usize, count: usize) -> Result<BlobWindow> {
        self.client
            .blob_get_window(&self.mailbox, &self.owner_credential, blob_id, from, count)
    }

    fn ack(&self, blob_id: &str, upto: usize) -> Result<()> {
        self.client
            .blob_ack(&self.mailbox, &self.owner_credential, blob_id, upto)?;
        Ok(())
    }

    fn close(&self, blob_id: &str, status: &str) -> Result<()> {
        self.client
            .blob_close(&self.mailbox, &self.owner_credential, blob_id, status, None)?;
        Ok(())
    }
}

/// 把 blob 端点的 404 从「HTTP 状态错」里挑出来单列成 `RelayBlobUnavailable`。
/// 只用于 `open` / `put`：`get_window` 的 404 有另一层语义（「发送端还没传到
/// 这一片」），它在调用点自己返回空窗，绝不能变成错误。
fn blob_unavailable(error: anyhow::Error) -> anyhow::Error {
    match error.downcast_ref::<RelayHttpError>() {
        Some(RelayHttpError(status)) if is_not_found_status(*status) => RelayBlobUnavailable.into(),
        _ => error,
    }
}

fn is_not_found_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::NOT_FOUND
}

fn is_not_found(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<RelayHttpError>()
        .is_some_and(|err| is_not_found_status(err.0))
}

/// 解析裸帧：每片前缀 4 字节大端长度。长度不自洽就整窗作废，绝不"尽力而为"
/// 拼接 —— 错位拼出来的分片会被接收端当成本片内容写进快照。
fn parse_frames(body: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut chunks = Vec::new();
    let mut offset = 0;
    while offset < body.len() {
        ensure!(
            body.len() - offset >= 4,
            "relay blob frame header truncated"
        );
        let size = u32::from_be_bytes([
            body[offset],
            body[offset + 1],
            body[offset + 2],
            body[offset + 3],
        ]) as usize;
        offset += 4;
        ensure!(
            size > 0 && size <= CHUNK_BYTES_MAX && body.len() - offset >= size,
            "relay blob frame length invalid"
        );
        chunks.push(body[offset..offset + size].to_vec());
        offset += size;
    }
    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frames_reads_length_prefixed_chunks_and_rejects_truncation() {
        let mut body = Vec::new();
        for chunk in [b"first".as_slice(), b"second".as_slice()] {
            body.extend_from_slice(&(chunk.len() as u32).to_be_bytes());
            body.extend_from_slice(chunk);
        }
        assert_eq!(
            parse_frames(&body).unwrap(),
            vec![b"first".to_vec(), b"second".to_vec()]
        );
        assert!(
            parse_frames(&body[..body.len() - 1]).is_err(),
            "截断必须报错"
        );
        assert!(parse_frames(&body[..2]).is_err(), "长度前缀不完整必须报错");
        assert!(parse_frames(&[]).unwrap().is_empty(), "空窗是合法的");
    }

    #[test]
    fn blob_id_validation_rejects_path_and_length_tricks() {
        for bad in ["short", "../etc", "aaaaaaaaaaaaaaaa/../b", ""] {
            assert!(validate_blob_id(bad).is_err(), "{bad} 必须被拒");
        }
        assert!(validate_blob_id(&"a".repeat(48)).is_ok());
    }

    /// 404 必须从「HttpError(404)」里单列（review 2026-10-25 N1）。
    ///
    /// `/blob/chunk` 的 PUT 对已回收的 blob 回 404，`/blob/*` 在老 relay 上是
    /// 路由级 404。两者的正确反应都是「重开一份、从 0 重传」，而 5xx 的反应是
    /// 「等一等再发同一片」—— 混在一类里会让本该重传的错白等十分钟。
    #[test]
    fn blob_404_is_not_a_generic_http_error() {
        let error = blob_unavailable(RelayHttpError(reqwest::StatusCode::NOT_FOUND).into());
        assert!(
            error.is::<RelayBlobUnavailable>(),
            "404 必须被单列成「blob 没了/没这能力」"
        );
        // 5xx 照旧是 HTTP 错误：这条路要退避，不是要重传。
        let error =
            blob_unavailable(RelayHttpError(reqwest::StatusCode::SERVICE_UNAVAILABLE).into());
        assert!(
            !error.is::<RelayBlobUnavailable>(),
            "5xx 不能被当成 blob 没了"
        );
        // 与 blob 无关的错误原样穿过去，不改语义。
        let error = blob_unavailable(anyhow::anyhow!("connection reset"));
        assert!(!error.is::<RelayBlobUnavailable>());
    }

    /// `blob_open` 收到 404 = 这台 relay 没有 blob 数据面（有数据面的 relay 上
    /// 不存在的 blob 会被建成新的）。这一下必须顺手把该 relay 降级成「没有
    /// 数据面」，否则下一拍还会再撞一次。
    #[test]
    fn blob_open_404_downgrades_that_relay_to_no_dataplane() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buffer = [0u8; 4096];
                let _ = std::io::Read::read(&mut stream, &mut buffer);
                let _ = std::io::Write::write_all(
                    &mut stream,
                    b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                );
            }
        });
        assert_eq!(super::blob_capability(&url), None, "一开始还没问过");
        let client = RelayClient::new(&url).unwrap();
        let error = client
            .blob_open(
                &"ab".repeat(32),
                &"cd".repeat(32),
                &[7u8; 32],
                "0123456789abcdef",
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("relay blob is gone or unsupported"),
            "错误文本必须能让人看出是数据面缺失：{error}"
        );
        assert_eq!(
            super::blob_capability(&url),
            Some(false),
            "404 是结论性的 —— 立刻把该 relay 降级，别每拍再撞"
        );
    }

    #[test]
    fn acked_upto_is_the_first_present_chunk() {
        let state = BlobState {
            present: vec![3, 4, 5],
            highest: Some(5),
            stored_bytes: 3,
        };
        assert_eq!(state.acked_upto(), 3);
        assert_eq!(BlobState::default().acked_upto(), usize::MAX);
    }
}
