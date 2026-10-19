//! RelayClient 基础与账户面：构造/健康检查、release 下载、邮箱与发送者凭证管理、轮换。

use super::*;

impl RelayClient {
    pub fn new(base_url: &str) -> Result<Self> {
        let url = reqwest::Url::parse(base_url).context("parse relay URL")?;
        let local = url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| match ip {
                        std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private(),
                        // IPv6：回环或唯一本地段（fc00::/7）。
                        std::net::IpAddr::V6(v6) => {
                            v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00
                        }
                    })
        });
        ensure!(
            url.scheme() == "https" || (url.scheme() == "http" && local),
            "relay URL must use HTTPS (HTTP only for private/loopback addresses)"
        );
        ensure!(
            url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "relay URL must not contain credentials, query or fragment"
        );
        let http = crate::http_proxy::with_outbound_proxy(reqwest::blocking::Client::builder())
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("build relay HTTP client")?;
        Ok(Self {
            base_url: url.as_str().trim_end_matches('/').to_string(),
            http,
        })
    }

    pub fn health(&self) -> Result<bool> {
        let response = self.http.get(format!("{}/health", self.base_url)).send()?;
        if !response.status().is_success() {
            return Ok(false);
        }
        let body: serde_json::Value = response_json(response)?;
        Ok(
            body.get("version").and_then(|v| v.as_u64()) == Some(VERSION.into())
                && body.get("ok").and_then(|v| v.as_bool()) == Some(true),
        )
    }

    /// 拉取应用升级发布清单（/release/latest，见 relay worker +
    /// scripts/publish-release.sh）。失败直接报错（区别于 health 的布尔降
    /// 级）：升级检查需要用户可感知的原因。
    pub fn fetch_release_manifest(&self) -> Result<ReleaseManifest> {
        let response = self
            .http
            .get(format!("{}/release/latest", self.base_url))
            .send()?;
        let manifest: ReleaseManifest = response_json(response)?;
        // 盲中继不承诺清单可信：字段级校验（hash 格式、大小上限）防捏造。
        // build_seq 的方向判断交给调用方（与本地比较），这里只挡明显非法。
        ensure!(
            is_short_hash(&manifest.build_hash)
                && manifest.apk_sha256.len() == 64
                && manifest.apk_sha256.chars().all(|c| c.is_ascii_hexdigit())
                && manifest.apk_size > 0
                && manifest.apk_size as u64 <= RELEASE_APK_MAX_BYTES,
            "release manifest failed validation"
        );
        Ok(manifest)
    }

    /// 下载升级 APK 到目录（/release/apk?h=<hash>），返回最终文件路径。
    /// 先写 .part 再原子改名；同 hash 的 .part 已存在时 Range 续传（直连
    /// CF 的长下载容易被慢窗口打断，重下 60MB 不可接受）。完成后校验
    /// sha256——清单来自不可信中继，指纹对上才算数。
    pub fn download_release_apk(
        &self,
        build_hash: &str,
        apk_sha256: &str,
        apk_size: i64,
        dest_dir: &Path,
    ) -> Result<PathBuf> {
        ensure!(is_short_hash(build_hash), "invalid release hash");
        fs::create_dir_all(dest_dir).context("create update download dir")?;
        let final_path = dest_dir.join(format!("elsewhen-{build_hash}.apk"));
        if fs::metadata(&final_path).is_ok_and(|m| m.len() == apk_size as u64) {
            return Ok(final_path);
        }
        let part = dest_dir.join(format!("elsewhen-{build_hash}.apk.part"));
        let existing = fs::metadata(&part)
            .ok()
            .map(|m| m.len())
            .filter(|len| *len > 0 && *len < apk_size as u64);
        let mut request = self
            .http
            .get(format!("{}/release/apk?h={}", self.base_url, build_hash))
            .header("Accept", "application/vnd.android.package-archive");
        if let Some(offset) = existing {
            request = request.header("Range", format!("bytes={offset}-"));
        }
        let response = request.send()?;
        // 服务器不支持 Range 会回 200 全量；只有 206 才算续传。
        if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            ensure!(
                existing.is_some(),
                "relay returned partial content without a range request"
            );
        } else {
            ensure!(
                response.status().is_success(),
                RelayHttpError(response.status())
            );
            let _ = fs::remove_file(&part);
        }
        let size = apk_size as u64;
        let offset = existing.unwrap_or(0);
        if let Some(length) = response.content_length() {
            ensure!(length <= size - offset, "release apk larger than manifest");
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(existing.is_some())
            .open(&part)
            .context("open update part file")?;
        let mut remaining = size - offset;
        let mut buf = [0u8; 64 * 1024];
        let mut stream = response.take(remaining);
        while remaining > 0 {
            let n = stream.read(&mut buf)?;
            if n == 0 {
                bail!(
                    "release apk stream ended early (got {} of {})",
                    size - remaining,
                    size
                );
            }
            file.write_all(&buf[..n])?;
            remaining -= n as u64;
        }
        file.flush()?;
        drop(file);
        let actual = apk_sha256_hex(&part)?;
        ensure!(
            actual.eq_ignore_ascii_case(apk_sha256),
            "release apk sha256 mismatch"
        );
        fs::rename(&part, &final_path).context("promote downloaded apk")?;
        Ok(final_path)
    }

    /// Initial mailbox identity. Persist it across subsequent owner-key rotation.
    /// Owner keys must be generated independently per device, never pair-shared.
    pub fn mailbox_key(owner_cred: &[u8; 32]) -> String {
        hmac_sign_hex(owner_cred, &["relay-mailbox"])
    }

    /// Legacy derivation, not a device identity or end-to-end encryption key.
    pub fn derive_relay_cred(shared_key: &[u8; 32]) -> [u8; 32] {
        derive_key(shared_key, &["relay-auth"])
    }

    /// Recipient-scoped sender capability; does not grant mailbox owner access.
    pub fn derive_sender_credential(
        shared_key: &[u8; 32],
        sender_mb: &str,
        recipient_mb: &str,
    ) -> Result<[u8; 32]> {
        validate_mailbox(sender_mb)?;
        validate_mailbox(recipient_mb)?;
        ensure!(
            sender_mb != recipient_mb,
            "sender and recipient must differ"
        );
        Ok(derive_key(
            shared_key,
            &["relay-v2-sender", sender_mb, recipient_mb],
        ))
    }

    pub fn create_mailbox(&self, owner_cred: &[u8; 32]) -> Result<String> {
        let mailbox = Self::mailbox_key(owner_cred);
        let (ts, nonce) = request_stamp();
        let sig = sign(owner_cred, "create", &[&mailbox], ts, &nonce);
        let _: serde_json::Value = self.post(
            "create",
            &serde_json::json!({
                "version": VERSION, "mailbox_key": mailbox,
                "owner_cred_hex": bytes_to_hex(owner_cred), "ts": ts,
                "nonce": nonce, "sig": sig,
            }),
        )?;
        Ok(mailbox)
    }

    /// v1 self-registration is unsafe and cannot grant sender access in v2.
    /// Kept as an explicit error rather than silently interpreting owner keys.
    pub fn register_credential(
        &self,
        _recipient_mb: &str,
        _sender_mb: &str,
        _cred: &[u8; 32],
        _expires_at_ms: i64,
    ) -> Result<()> {
        bail!(
            "relay v1 registration removed; use create_mailbox and owner-authorized register_sender"
        )
    }

    /// Create, renew or rotate a sender grant at an explicitly observed revision.
    /// Revision 0 means no prior grant; revoked grants retain their revisions.
    /// Retry lost responses with the same credential, expiry and expected revision;
    /// the Worker accepts only the exact post-state, never a newer conflicting grant.
    pub fn register_sender(
        &self,
        mailbox: &str,
        owner_cred: &[u8; 32],
        sender_mb: &str,
        sender_cred: &[u8; 32],
        expires_at_ms: i64,
        expected_revision: i64,
    ) -> Result<i64> {
        validate_mailbox(mailbox)?;
        validate_mailbox(sender_mb)?;
        ensure!(
            sender_mb != mailbox,
            "sender must differ from mailbox owner"
        );
        ensure!(
            sender_cred != owner_cred,
            "sender credential must differ from owner credential"
        );
        validate_integer(expected_revision)?;
        validate_integer(expires_at_ms)?;
        let (ts, nonce) = request_stamp();
        let credential = bytes_to_hex(sender_cred);
        let sig = sign(
            owner_cred,
            "register",
            &[
                mailbox,
                sender_mb,
                &credential,
                &expires_at_ms.to_string(),
                &expected_revision.to_string(),
            ],
            ts,
            &nonce,
        );
        let result: serde_json::Value = self.post(
            "register",
            &serde_json::json!({
                "version": VERSION, "mailbox_key": mailbox, "sender_mb": sender_mb,
                "cred_hex": credential, "expires_at": expires_at_ms,
                "expected_revision": expected_revision, "ts": ts, "nonce": nonce, "sig": sig,
            }),
        )?;
        integer_field(&result, "revision")
    }

    pub fn sender_status(
        &self,
        mailbox: &str,
        owner_cred: &[u8; 32],
        sender_mb: &str,
    ) -> Result<SenderStatus> {
        validate_mailbox(mailbox)?;
        validate_mailbox(sender_mb)?;
        let (ts, nonce) = request_stamp();
        let sig = sign(owner_cred, "sender", &[mailbox, sender_mb], ts, &nonce);
        self.get(
            "sender",
            &[
                ("version", VERSION.to_string()),
                ("mailbox_key", mailbox.to_string()),
                ("sender_mb", sender_mb.to_string()),
                ("ts", ts.to_string()),
                ("nonce", nonce),
                ("sig", sig),
            ],
        )
    }

    pub fn revoke_sender(
        &self,
        mailbox: &str,
        owner_cred: &[u8; 32],
        sender_mb: &str,
        expected_revision: i64,
    ) -> Result<i64> {
        validate_mailbox(mailbox)?;
        validate_mailbox(sender_mb)?;
        validate_integer(expected_revision)?;
        ensure!(expected_revision > 0, "sender revision must be positive");
        let (ts, nonce) = request_stamp();
        let sig = sign(
            owner_cred,
            "revoke",
            &[mailbox, sender_mb, &expected_revision.to_string()],
            ts,
            &nonce,
        );
        let result: serde_json::Value = self.post(
            "revoke",
            &serde_json::json!({
                "version": VERSION, "mailbox_key": mailbox, "sender_mb": sender_mb,
                "expected_revision": expected_revision, "ts": ts, "nonce": nonce, "sig": sig,
            }),
        )?;
        integer_field(&result, "revision")
    }

    /// The mailbox address stays stable; old owner keys stop authorizing requests.
    pub fn rotate_owner(
        &self,
        mailbox: &str,
        owner_cred: &[u8; 32],
        new_cred: &[u8; 32],
    ) -> Result<i64> {
        validate_mailbox(mailbox)?;
        ensure!(owner_cred != new_cred, "new owner credential must differ");
        let credential = bytes_to_hex(new_cred);
        let (ts, nonce) = request_stamp();
        let sig = sign(owner_cred, "rotate", &[mailbox, &credential], ts, &nonce);
        let result: serde_json::Value = self.post(
            "rotate",
            &serde_json::json!({
                "version": VERSION, "mailbox_key": mailbox, "new_cred_hex": credential,
                "ts": ts, "nonce": nonce, "sig": sig,
            }),
        )?;
        integer_field(&result, "owner_generation")
    }
}
