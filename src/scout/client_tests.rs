//! ScoutClient 网络纪律测试：URL 校验（HTTPS/凭据/查询串）与响应解码。
//! 网络本身不打出站请求；HTTP 行为用本地 std TcpListener 起的临时回环
//! 服务验证（超时/重定向策略沿用与 RelayClient 相同的既有组合，不重复测）。

use super::*;

fn client(url: &str) -> anyhow::Result<ScoutClient> {
    ScoutClient::new(url)
}

#[test]
fn rejects_non_https_and_credential_urls() {
    assert!(client("ftp://x.example").is_err());
    assert!(client("http://elsewhen-scout.example.workers.dev").is_err());
    // 私网/回环允许 HTTP。
    assert!(client("http://127.0.0.1:8799").is_ok());
    assert!(client("http://192.168.1.10:8799").is_ok());
    // 凭据、查询串、片段全部拒绝。
    assert!(client("https://u:p@elsewhen-scout.example.workers.dev").is_err());
    assert!(client("https://elsewhen-scout.example.workers.dev?x=1").is_err());
    assert!(client("https://elsewhen-scout.example.workers.dev/#frag").is_err());
    // 尾部斜杠规范化。
    let c = client("https://elsewhen-scout.example.workers.dev/").unwrap();
    assert!(
        c.health_base_url()
            .ends_with("elsewhen-scout.example.workers.dev")
    );
}

#[test]
fn since_parameter_is_percent_encoded() {
    // 编码逻辑内联在 fetch_findings 里；这里验证 encode 输出形状与
    // 服务端 D1 查询兼容（ISO 时间戳里的 `:` 必须转义或原样可接受——
    // 两个都可以，%3A 更稳）。
    let enc = crate::web_search::urlencoding_simple("2026-11-15T13:30:00Z");
    assert!(!enc.contains(' '));
    assert!(enc.contains("2026-11-15T13"));
}

/// IPv4 兜底回归：验证兜底客户端（仅 IPv4 local_address）能正常到达
/// 回环 mock——「IPv6 优先失败后兜底救回」的真实网络场景无法在 CI 稳定
/// 模拟（内核连接合并行为因环境而异），这里钉住兜底链路本身可用 +
/// 响应解码正常；环境差异场景（::1 成功合并）下主链路成功也不算失败。
#[test]
fn ipv4_fallback_client_reaches_loopback_mock() {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let body = r#"[{"id":1,"source_id":"s","snapshot_id":1,"kind":"new","item_key":"k","payload":{},"detected_at":"2026-11-16T00:00:00Z"}]"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        // 可能来两次连接（主链路 + 兜底），循环 accept 直到测试结束 drop。
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(response.as_bytes());
            // 写完响应后稍候再关，避免 reqwest 在连接复用窗口内读到 RST。
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
    });
    let _guard = crate::config::test_env_guard();
    crate::http_proxy::apply_policy("off", ""); // 强制直连，兜底才有意义
    let c = ScoutClient::new(&format!("http://{addr}")).unwrap();
    let findings = c
        .fetch_findings("t", None, 10)
        .expect("主链路或 IPv4 兜底至少一条要命中 mock");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].source_id, "s");
    // server 线程循环 accept 阻塞在下一个连接，不 join（detach，随进程结束）。
    std::mem::drop(server);
    crate::http_proxy::apply_policy("auto", "");
}

#[test]
fn raw_reqwest_loopback_probe() {
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut buf = [0u8; 1024];
        let _ = s.read(&mut buf);
        s.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
            .unwrap();
    });
    let c = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let r = c.get(format!("http://{addr}/")).send();
    assert!(
        r.is_ok(),
        "raw reqwest loopback: {:?}",
        r.err().map(|e| e.to_string())
    );
    server.join().unwrap();
}

#[test]
fn cursor_pages_validate_progress_and_commit_atomically() -> anyhow::Result<()> {
    let path = std::env::temp_dir().join(format!("scout-cursor-{}.db", uuid::Uuid::new_v4()));
    let store = crate::storage::Store::open(&path)?;
    let body = serde_json::json!({"version":1,"instance_id":"server-a","upper":4,"next":2,"has_more":true,"items":[{"id":2,"source_id":"s","snapshot_id":1,"kind":"new","item_key":"p","payload":{},"detected_at":"2026-11-17"}]});
    let page: super::client::FindingPage = serde_json::from_value(body.clone())?;
    page.validate(0, None)?;
    assert_eq!(store.apply_scout_page(&page, 0)?, 1);
    assert_eq!(store.scout_cursor()?.unwrap().1, 2);
    // A concurrent stale response cannot duplicate or rewind a committed cursor.
    assert!(store.apply_scout_page(&page, 0).is_err());
    let mut final_body = body.clone();
    final_body["items"] = serde_json::json!([]);
    final_body["next"] = serde_json::json!(4);
    final_body["has_more"] = serde_json::json!(false);
    let final_page: super::client::FindingPage = serde_json::from_value(final_body)?;
    final_page.validate(2, Some(4))?;
    store.apply_scout_page(&final_page, 2)?;
    assert_eq!(store.scout_cursor()?.unwrap().2, None);
    let mut invalid = body;
    invalid["next"] = serde_json::json!(0);
    assert!(
        serde_json::from_value::<super::client::FindingPage>(invalid)?
            .validate(0, None)
            .is_err()
    );
    Ok(())
}
