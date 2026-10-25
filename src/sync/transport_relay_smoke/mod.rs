//! Explicit, loopback-only Rust ↔ production Worker interoperability test.
//! The Node runtime test starts an isolated Worker and supplies its random port.

use crate::sync::transport_relay::RelayClient;
use ring::rand::{SecureRandom, SystemRandom};

fn key() -> [u8; 32] {
    let mut bytes = [0; 32];
    SystemRandom::new().fill(&mut bytes).unwrap();
    bytes
}

fn synthetic_provider() -> (
    String,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
    std::thread::JoinHandle<()>,
) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = calls.clone();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        loop {
            let mut buffer = [0; 4096];
            let n = socket.read(&mut buffer).unwrap();
            assert!(n > 0 && request.len() + n <= 65536);
            request.extend_from_slice(&buffer[..n]);
            if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                if request.len() >= end + 4 + length {
                    assert!(
                        headers.contains("authorization: bearer snapshot_worker_synthetic_key")
                    );
                    break;
                }
            }
        }
        count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"configured on new device"}}]}"#;
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    (url, calls, server)
}

#[test]
#[ignore = "requires an explicitly launched isolated relay; run npm test in relay/"]
fn end_to_end_send_fetch_ack_against_real_worker() {
    let url = std::env::var("ELSEWHEN_RELAY_TEST_URL")
        .expect("test harness must provide ELSEWHEN_RELAY_TEST_URL");
    let parsed = reqwest::Url::parse(&url).unwrap();
    // 允许 loopback 与私网（NAS 自建中继）；公网域名仍禁止，防止误打生产。
    assert!(
        parsed.host_str().is_some_and(|host| host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| match ip {
                    std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private(),
                    std::net::IpAddr::V6(v6) => {
                        v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00
                    }
                })),
        "interoperability tests may only access loopback or private addresses"
    );
    let client = RelayClient::new(&url).unwrap();
    assert!(client.health().unwrap());
    let owner_a = key();
    let owner_b = key();
    let sender = client.create_mailbox(&owner_a).unwrap();
    let mailbox = client.create_mailbox(&owner_b).unwrap();
    assert_ne!(sender, mailbox);
    let credential = RelayClient::derive_sender_credential(&key(), &sender, &mailbox).unwrap();
    let expiry = chrono::Utc::now().timestamp_millis() + 3_600_000;
    assert_eq!(
        client
            .register_sender(&mailbox, &owner_b, &sender, &credential, expiry, 0)
            .unwrap(),
        1
    );
    assert_eq!(
        client
            .register_sender(&mailbox, &owner_b, &sender, &credential, expiry, 0)
            .unwrap(),
        1,
        "lost response retries preserve the original grant revision"
    );
    assert!(
        client
            .register_sender(&mailbox, &owner_a, &sender, &credential, expiry, 0)
            .is_err()
    );
    assert_eq!(
        client
            .sender_status(&mailbox, &owner_b, &sender)
            .unwrap()
            .revision,
        1
    );

    let mut ids = Vec::new();
    for seq in 1..=3 {
        ids.push(
            client
                .send(&mailbox, &sender, &credential, seq, "c3ludGhldGlj", expiry)
                .unwrap(),
        );
    }
    assert!(client.fetch(&mailbox, &credential, "0", 10).is_err());
    assert!(client.ack(&mailbox, &credential, &ids).is_err());
    assert_eq!(client.status(&mailbox, &owner_b).unwrap().count, 3);
    let mut after = "0".to_string();
    for (index, id) in ids.iter().enumerate() {
        let page = client.fetch_page(&mailbox, &owner_b, &after, 1).unwrap();
        assert_eq!(page.envelopes.len(), 1);
        assert_eq!(&page.envelopes[0].id, id);
        assert_eq!(page.envelopes[0].ciphertext, "c3ludGhldGlj");
        assert_eq!(page.has_more, index != 2);
        after = page.next_cursor;
    }
    assert_eq!(
        client
            .revoke_sender(&mailbox, &owner_b, &sender, 1)
            .unwrap(),
        2
    );
    assert_eq!(
        client
            .revoke_sender(&mailbox, &owner_b, &sender, 1)
            .unwrap(),
        2
    );
    assert!(
        client
            .register_sender(&mailbox, &owner_b, &sender, &credential, expiry, 0)
            .is_err()
    );
    assert!(
        client
            .send(&mailbox, &sender, &credential, 4, "YQ==", expiry)
            .is_err()
    );
    let replacement = key();
    assert_eq!(
        client
            .rotate_owner(&mailbox, &owner_b, &replacement)
            .unwrap(),
        2
    );
    assert!(client.status(&mailbox, &owner_b).is_err());
    assert_eq!(client.ack(&mailbox, &replacement, &ids).unwrap(), 3);
    assert_eq!(client.status(&mailbox, &replacement).unwrap().count, 0);

    // 阶段二：真实加密事件 + 应用层回执往返。
    application_roundtrip(&client, &sender, &mailbox, owner_a, replacement, expiry);

    // 阶段三：blob 数据面 —— 分片流式上传 / 窗口拉取 / 逐窗确认 / close 精确回收。
    // 复用一个新信箱，避免与前面已轮换的 owner 状态纠缠。
    let blob_owner = key();
    let blob_sender = key();
    let blob_mailbox = client.create_mailbox(&blob_owner).unwrap();
    let blob_sender_mb = client.create_mailbox(&blob_sender).unwrap();
    let blob_cred = RelayClient::derive_sender_credential(&blob_sender, &blob_sender_mb, &blob_mailbox).unwrap();
    let expiry2 = chrono::Utc::now().timestamp_millis() + 3_600_000;
    client
        .register_sender(&blob_mailbox, &blob_owner, &blob_sender_mb, &blob_cred, expiry2, 0)
        .unwrap();
    let blob_id = crate::sync::transport_relay::random_hex(24);
    blob_roundtrip(
        &client,
        &blob_owner,
        &blob_mailbox,
        &blob_sender_mb,
        &blob_cred,
        &blob_id,
    );

    // R5-1 `/drop`：放弃的信必须真的从 relay 消失（否则死信堆到 MAX_MAILS），
    // 但走的是独立签名域 —— 客户端不可能用「已消费」的调用表达「已丢弃」。
    let abandoned = client
        .send(&blob_mailbox, &blob_sender_mb, &blob_cred, 900, "Z2FyYmFnZQ==", expiry2)
        .unwrap();
    assert_eq!(client.status(&blob_mailbox, &blob_owner).unwrap().count, 1);
    assert_eq!(
        client.drop(&blob_mailbox, &blob_owner, &[abandoned.clone()]).unwrap(),
        1
    );
    assert_eq!(client.status(&blob_mailbox, &blob_owner).unwrap().count, 0);
    // 幂等：同一封再 drop 一次得 0，不是错误（丢响应重试的常态）。
    assert_eq!(client.drop(&blob_mailbox, &blob_owner, &[abandoned]).unwrap(), 0);

    // 阶段四：窗口化快照（R4）——分片经 blob 传输，接收端边拉边删，
    // relay 峰值必须是一窗量级，close 后归零。
    let snapshot_dir = std::env::temp_dir().join(format!(
        "elsewhen-relay-blob-snapshot-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&snapshot_dir).unwrap();
    blob_snapshot_roundtrip(&client, &snapshot_dir);

    // 阶段五：blob 缺口的边界 —— 不能跳着放，也不能报错（R4-6）。
    blob_gap_import(&client, &snapshot_dir);

    // 阶段六：abort 之后同 id 续传并发布（review 2026-10-25 M1）。
    blob_abort_resume(&client, &snapshot_dir);
}

#[path = "../transport_relay_smoke_tests/roundtrip.rs"]
mod roundtrip;
#[path = "../transport_relay_smoke_tests/blob.rs"]
mod blob_roundtrip;
#[path = "../transport_relay_smoke_tests/snapshot_blob.rs"]
mod snapshot_blob;

use blob_roundtrip::blob_roundtrip;
use roundtrip::application_roundtrip;
use snapshot_blob::aborted_export_resumes_and_publishes as blob_abort_resume;
use snapshot_blob::gap_import_breaks_without_error as blob_gap_import;
use snapshot_blob::roundtrip as blob_snapshot_roundtrip;
