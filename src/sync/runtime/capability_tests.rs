//! 数据面能力协商（review 2026-10-25 M2）：`snapshot_upload` 不能只信环境变量。
//!
//! 这里用一个只会回 `/status` 的最小 HTTP 服务验证两件事：
//! ① 老 relay（`/status` 里没有 `capabilities`）必须被当成「没有数据面」；
//! ② 结论按 `base_url` 缓存 —— 每拍多打一次 `/status` 是纯浪费。

use super::relay_supports_blob;
use crate::sync::transport_relay::RelayClient;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// 一个只会回 `/status` 的最小 HTTP 服务。
fn fake_status_relay(body: &'static str) -> (String, Arc<AtomicUsize>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            counter.fetch_add(1, Ordering::SeqCst);
            let mut buffer = [0u8; 4096];
            let _ = std::io::Read::read(&mut stream, &mut buffer);
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = std::io::Write::write_all(&mut stream, response.as_bytes());
        }
    });
    (url, hits)
}

fn probe(url: &str) -> bool {
    let client = RelayClient::new(url).unwrap();
    relay_supports_blob(&client, &"ab".repeat(32), &[7u8; 32])
}

/// 老 relay：`/status` 没有 `capabilities` 字段 → 空清单 → 走信件面。
///
/// 默认「有」才是危险方向：那会让发送端对着一个没有 `/blob/*` 路由的 relay
/// 上传，每拍两次 404，配对面板上表现为「快照同步永远不完成」。
#[test]
fn relay_without_blob_capability_falls_back_to_mail() {
    let (url, hits) = fake_status_relay(
        r#"{"bytes":0,"count":0,"quota":1048576,"earliest_expiry":null}"#,
    );
    assert!(!probe(&url), "老 relay 必须被判成没有 blob 数据面");
    // 结论缓存住：同一台 relay 不再问第二次。
    assert!(!probe(&url));
    assert_eq!(hits.load(Ordering::SeqCst), 1, "能力只该探一次");
    assert_eq!(
        crate::sync::transport_relay::blob_capability(&url),
        Some(false),
        "结论要落进缓存，供下一拍直接走 mail"
    );
}

/// 新 relay 自报 `blob` → 走数据面。
#[test]
fn relay_with_blob_capability_uses_the_dataplane() {
    let (url, _hits) = fake_status_relay(
        r#"{"bytes":0,"count":0,"quota":1048576,"earliest_expiry":null,"blob_bytes":0,"blobs":0,"capabilities":["blob","drop"]}"#,
    );
    assert!(probe(&url));
    assert_eq!(
        crate::sync::transport_relay::blob_capability(&url),
        Some(true)
    );
}

/// 探测本身失败（relay 连不上）**不缓存**：一次抖动不该让本进程永久退回信件面。
#[test]
fn failed_probe_is_not_cached() {
    // 拿一个刚关掉的端口：连上去必失败。
    let url = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        url
    };
    assert!(!probe(&url), "探测失败时保守走信件面");
    assert_eq!(
        crate::sync::transport_relay::blob_capability(&url),
        None,
        "失败不是结论，不能缓存 —— 否则一次超时就永久失去数据面"
    );
}
