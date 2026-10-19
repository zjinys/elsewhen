//! transport_relay 内嵌测试平移（URL 校验/签名域/整数界/本地 mock relay 集成）。

use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_validation_rejects_prefix_tricks_and_credentials() {
        for url in [
            "http://localhost.evil.invalid",
            "http://127.0.0.1.evil.invalid",
            "http://localhost@evil.invalid",
            // 私网段之外的明文 http 仍然拒绝。
            "http://8.8.8.8:8787",
            "http://example.com:8787",
            "https://user:secret@relay.invalid",
            "https://relay.invalid?key=secret",
            "https://relay.invalid#fragment",
        ] {
            assert!(RelayClient::new(url).is_err(), "{url}");
        }
        for url in [
            "https://relay.invalid",
            "http://localhost:8787",
            "http://127.0.0.1:8787",
            "http://[::1]:8787",
            // 自建局域网中继（NAS）：私网明文地址放行。
            "http://192.168.1.10:8787",
            "http://10.0.0.2",
            "http://172.16.5.5:8787",
            "http://[fd00::5]:8787",
        ] {
            assert!(RelayClient::new(url).is_ok(), "{url}");
        }
    }

    #[test]
    fn sender_capability_is_direction_and_recipient_bound() {
        let a = RelayClient::mailbox_key(&[1; 32]);
        let b = RelayClient::mailbox_key(&[2; 32]);
        let c = RelayClient::mailbox_key(&[3; 32]);
        assert_ne!(a, b);
        let ab = RelayClient::derive_sender_credential(&[9; 32], &a, &b).unwrap();
        assert_ne!(
            ab,
            RelayClient::derive_sender_credential(&[9; 32], &b, &a).unwrap()
        );
        assert_ne!(
            ab,
            RelayClient::derive_sender_credential(&[9; 32], &a, &c).unwrap()
        );
        assert!(RelayClient::derive_sender_credential(&[9; 32], &a, &a).is_err());
    }

    #[test]
    fn wire_numbers_and_cursors_are_bounded() {
        assert!(validate_integer(MAX_SAFE_INTEGER).is_ok());
        assert!(validate_integer(MAX_SAFE_INTEGER + 1).is_err());
        assert!(validate_cursor("0").is_ok());
        assert!(validate_cursor("0000000000000001").is_ok());
        for value in ["1", "-1", "1|nonce", "9999999999999999", ""] {
            assert!(validate_cursor(value).is_err());
        }
    }

    #[test]
    fn signatures_are_domain_separated_from_v1() {
        assert_ne!(
            hmac_sign_hex(&[7; 32], &["relay-send", "a"]),
            hmac_sign_hex(&[7; 32], &["relay-v2-send", "a"])
        );
        assert_eq!(
            sign(&[7; 32], "status", &["mailbox"], 123, "nonce"),
            hmac_sign_hex(&[7; 32], &["relay-v2-status", "mailbox", "123", "nonce"])
        );
    }
}

#[cfg(test)]
mod release_tests {
    use super::*;

    /// 极简回环 HTTP 服务：按给定脚本应答（清单 / APK / 断点响应）。
    struct FakeRelay {
        url: String,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl FakeRelay {
        fn start(script: impl Fn(String, Vec<String>) -> Vec<u8> + Send + 'static) -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let handle = std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut socket) = stream else { break };
                    let mut request = Vec::new();
                    let mut buffer = [0u8; 4096];
                    loop {
                        let n = socket.read(&mut buffer).unwrap();
                        request.extend_from_slice(&buffer[..n]);
                        if request.windows(4).any(|b| b == b"\r\n\r\n") {
                            break;
                        }
                    }
                    let text = String::from_utf8_lossy(&request);
                    let mut lines = text.lines();
                    let first = lines.next().unwrap_or_default().to_string();
                    let headers: Vec<String> = lines
                        .take_while(|l| !l.is_empty())
                        .map(|l| l.to_ascii_lowercase())
                        .collect();
                    let body = script(first, headers);
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(head.as_bytes());
                    let _ = socket.write_all(&body);
                }
            });
            Self {
                url,
                handle: Some(handle),
            }
        }
    }

    impl Drop for FakeRelay {
        fn drop(&mut self) {
            // 连接是短生命周期的；断开 listener 靠进程结束即可（测试线程容忍）。
            let _ = self.handle.take();
        }
    }

    fn manifest_json(changelog: &str) -> String {
        format!(
            r#"{{"version":"1.0.0","buildHash":"abcd123","buildSeq":9,"changelog":"{changelog}","apkSha256":"{}","apkSize":5,"publishedAt":1}}"#,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        )
    }

    fn write_dir() -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("elsewhen-release-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn debug_raw_response() {
        let relay = FakeRelay::start(|_f, _h| manifest_json("ok").into_bytes());
        let client = RelayClient::new(&relay.url).unwrap();
        let resp = client
            .http
            .get(format!("{}/release/latest", client.base_url))
            .send()
            .unwrap();
        eprintln!(
            "status={} ct={:?}",
            resp.status(),
            resp.headers().get("content-type")
        );
        let bytes = resp.bytes().unwrap();
        eprintln!(
            "body(\u{22}raw\u{22}) = {:?}",
            String::from_utf8_lossy(&bytes)
        );
    }

    #[test]
    fn manifest_validates_fields() {
        let relay = FakeRelay::start(|_first, _h| manifest_json("ok").into_bytes());
        let client = RelayClient::new(&relay.url).unwrap();
        let m = client.fetch_release_manifest().unwrap();
        assert_eq!(m.build_hash, "abcd123");
        assert_eq!(m.build_seq, 9);
        assert_eq!(m.changelog, "ok");

        // 非法 hash / 大小直接拒绝。
        let bad =
            manifest_json("x").replace("\"buildHash\":\"abcd123\"", "\"buildHash\":\"AB(..\"");
        let relay_bad = FakeRelay::start(move |_f, _h| bad.clone().into_bytes());
        let client_bad = RelayClient::new(&relay_bad.url).unwrap();
        assert!(client_bad.fetch_release_manifest().is_err());
    }

    #[test]
    fn apk_downloads_verifies_hash() {
        let payload = b"hello";
        let mut hasher = ring::digest::Context::new(&SHA256);
        hasher.update(payload);
        let sha_hex = bytes_to_hex(hasher.finish().as_ref());
        let manifest_line = format!(
            r#"{{"version":"1.0.0","buildHash":"abcd123","buildSeq":9,"changelog":"x","apkSha256":"{sha_hex}","apkSize":5,"publishedAt":1}}"#
        );

        // 成功路径：FakeRelay 对 /release/apk?h=abcd123 回指定 5 字节内容
        //（同一端口先 /release/latest 也回相同正文——仅测试简化）。hash
        // 与清单吻合，rename 后不再有 .part。
        let apk_body = payload.to_vec();
        let manifest_for_latest = manifest_line.clone();
        let relay = FakeRelay::start(move |first, _h| {
            if first.starts_with("GET /release/latest") {
                manifest_for_latest.clone().into_bytes()
            } else {
                apk_body.clone()
            }
        });
        let client = RelayClient::new(&relay.url).unwrap();
        let manifest = client.fetch_release_manifest().unwrap();
        let dir = write_dir();
        let path = client
            .download_release_apk(
                &manifest.build_hash,
                &manifest.apk_sha256,
                manifest.apk_size,
                &dir,
            )
            .unwrap();
        assert_eq!(fs::read(&path).unwrap(), payload);
        assert!(!fs::metadata(&dir.join("elsewhen-abcd123.apk.part")).is_ok());
        let _ = fs::remove_dir_all(&dir);

        // sha256 不符：拒收（不产生 final 文件）。
        let refuse_line = manifest_line.replace(&sha_hex, &"0".repeat(64));
        let relay_bad = FakeRelay::start(move |_f, _h| refuse_line.clone().into_bytes());
        let client_bad = RelayClient::new(&relay_bad.url).unwrap();
        let bad_manifest = client_bad.fetch_release_manifest().unwrap();
        let dir_bad = write_dir();
        assert!(
            client_bad
                .download_release_apk(
                    &bad_manifest.build_hash,
                    &bad_manifest.apk_sha256,
                    bad_manifest.apk_size,
                    &dir_bad
                )
                .is_err()
        );
        assert!(!fs::metadata(dir_bad.join("elsewhen-abcd123.apk")).is_ok());
        let _ = fs::remove_dir_all(&dir_bad);
    }
}
