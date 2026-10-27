//! C2：扫描循环里「不可用的邮件」必须显式表态（计数 + relay `drop`），
//! 而不是静默跳过；游标必须越过它，后续 peer 邮件照常处理。
//!
//! 现场证据（roadmap §一 F9）：桌面信箱残留一封 10-24 wipe 前的配对信，
//! 同一封被重扫 1239 次、`sync_relay_cursors.after_id` 永远回 0 —— 旧代码里
//! 这类邮件走的是 `else { false }`：不 accept、不 ack、不 drop、不计数。
//!
//! 这里用一台**假 relay**（只实现 `/fetch`、`/ack`、`/drop`）+ 真实
//! `pull_transfer` 跑三圈，把「计数 → 阈值 → 显式 drop → 审计行关闭」整条链
//! 走完，同时验证同一页里排在它后面的 peer 分片仍然被应用。

use super::*;
use crate::sync::transport_relay::RelayClient;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Mail {
    id: String,
    sender: String,
    ciphertext: String,
}

#[derive(Default)]
struct RelayState {
    mails: Vec<Mail>,
    /// 每次 `/ack` 收到的 id 列表（按请求顺序）。
    acked: Vec<Vec<String>>,
    /// 每次 `/drop` 收到的 id 列表（按请求顺序）。
    dropped: Vec<Vec<String>>,
}

struct FakeRelay {
    url: String,
    state: Arc<Mutex<RelayState>>,
    /// 单页上限（生产是 100）：想验证「翻页途中出错也要先推游标」时传 1。
    page_limit: usize,
}

impl FakeRelay {
    fn start(mails: Vec<Mail>) -> Self {
        Self::start_with_page_limit(mails, 100)
    }

    fn start_with_page_limit(mails: Vec<Mail>, page_limit: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(RelayState {
            mails,
            ..RelayState::default()
        }));
        let shared = state.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut raw = Vec::new();
                let mut buffer = [0u8; 8192];
                loop {
                    let Ok(n) = stream.read(&mut buffer) else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    raw.extend_from_slice(&buffer[..n]);
                    let text = String::from_utf8_lossy(&raw).to_string();
                    if let Some((head, body)) = text.split_once("\r\n\r\n") {
                        let length = head
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|value| value.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if body.len() >= length {
                            break;
                        }
                    }
                }
                let text = String::from_utf8_lossy(&raw).to_string();
                let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
                let request_line = head.lines().next().unwrap_or_default().to_string();
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or_default().to_string();
                let target = parts.next().unwrap_or_default().to_string();
                let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
                let response = {
                    let mut guard = shared.lock().unwrap();
                    match (method.as_str(), path) {
                        ("GET", "/fetch") => {
                            let after = query
                                .split('&')
                                .find_map(|pair| pair.strip_prefix("after="))
                                .unwrap_or("0")
                                .to_string();
                            let after_id: i64 = after.parse().unwrap();
                            let remaining: Vec<Mail> = guard
                                .mails
                                .iter()
                                .filter(|mail| mail.id.parse::<i64>().unwrap() > after_id)
                                .cloned()
                                .collect();
                            let has_more = remaining.len() > page_limit;
                            let page: Vec<Mail> = remaining.into_iter().take(page_limit).collect();
                            let next_cursor = page
                                .last()
                                .map(|mail| mail.id.clone())
                                .unwrap_or(after.clone());
                            serde_json::json!({
                                "has_more": has_more,
                                "envelopes": page.iter().map(|mail| serde_json::json!({
                                    "id": mail.id,
                                    "sender_mb": mail.sender,
                                    "seq_hint": 1,
                                    "ciphertext": mail.ciphertext,
                                    "created_at": 1,
                                    "expires_at": i64::MAX / 2,
                                })).collect::<Vec<_>>(),
                                "next_cursor": next_cursor,
                            })
                        }
                        ("POST", "/ack") | ("POST", "/drop") => {
                            let ids: Vec<String> = serde_json::from_str::<serde_json::Value>(body)
                                .ok()
                                .and_then(|value| value.get("ids").cloned())
                                .and_then(|value| serde_json::from_value(value).ok())
                                .unwrap_or_default();
                            if path == "/ack" {
                                guard.acked.push(ids.clone());
                            } else {
                                guard.dropped.push(ids.clone());
                            }
                            guard.mails.retain(|mail| !ids.contains(&mail.id));
                            serde_json::json!({ "deleted": ids.len() })
                        }
                        _ => serde_json::json!({}),
                    }
                };
                let body = response.to_string();
                let out = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(out.as_bytes());
                let _ = stream.flush();
            }
        });
        Self {
            url,
            state,
            page_limit,
        }
    }
}

/// 配对帧形态的载荷：base64 解码后是随机字节（首字节不是 `{`），
/// 正是 `looks_like_dataplane_envelope` 判为「不属于这条收件路径」的那类。
fn foreign_frame() -> String {
    STANDARD.encode([0x9au8; 96])
}

fn pull(f: &Pair, role: &str, client: &RelayClient) -> anyhow::Result<()> {
    let loan = f.loan(role);
    let store = loan.store().unwrap();
    let vault = store
        .load_sync_vault(&loan.installation, &loan.master, loan.revision)
        .unwrap();
    let record = loan.record.as_ref().unwrap();
    let (_, receive, _) = vault
        .peer_keys(&record.dataset_id, &record.peer.device_id)
        .unwrap();
    let result = super::super::flow::pull_transfer(&loan, client, record, &receive);
    // 租约必须显式回收，否则下一次 `begin` 会撞上未释放的工作区。
    drop(loan);
    result
}

fn rejection(store: &Store, mailbox: &str, mail_id: &str) -> Option<(i64, Option<i64>, String)> {
    store
        .connection
        .query_row(
            "SELECT attempts,dropped_at,last_error FROM sync_wire_rejections WHERE mail_id=?1 AND mailbox_id=?2",
            rusqlite::params![mail_id, mailbox],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok()
}

#[test]
fn unroutable_mail_is_counted_dropped_and_does_not_block_the_peer() {
    let f = Pair::new();
    f.channels();
    for i in 0..700 {
        let payload = format!("bulk-{}-{}", i, "x".repeat(1024));
        f.owner
            .store
            .submit_input_as_event(&payload, "capture", None)
            .unwrap();
    }
    let offer = f.export();
    f.select(&offer);

    // 交接跑在**暂存数据集**上：断言必须打开同一个库（`loan.database`），
    // 不能拿 `config.database_path`（那是加入前的原始库）。
    let (mailbox, peer_mailbox, database) = {
        let joiner = f.loan("joiner");
        let record = joiner.record.as_ref().unwrap();
        let route = (
            record.route.local_mailbox.clone(),
            record.route.peer_mailbox.clone(),
            joiner.database.clone(),
        );
        drop(joiner);
        route
    };
    // 快照线上唯一的一封信：manifest（`snapshot-end:`）。分片走 blob 数据面，
    // 不经过信箱（mail 分片路径 2026-10-26 C5-A 已删除）。
    let manifest_id = {
        let owner = f.loan("owner");
        let owner_store = owner.store().unwrap();
        let id: String = owner_store
            .connection
            .query_row(
                // `sync_snapshot_export_wires` 里还挂着这次配对的 roster（membership）
                // 信 —— 那份由成员广播负责，不是快照线。快照线只有 manifest。
                "SELECT message_id FROM sync_snapshot_export_wires WHERE snapshot_id=?1 AND message_id LIKE 'snapshot-end:%' ORDER BY message_id LIMIT 1",
                [&offer.snapshot_id],
                |r| r.get(0),
            )
            .unwrap();
        drop(owner);
        id
    };
    let manifest_wire = f.wire(&manifest_id);

    // id 7 = 不可路由的残留（sender 既不是对端、也不是配对请求方），
    // id 8 = 对端真实的 manifest。旧代码在 id 7 上「悄悄跳过」，然后永远重扫它。
    // relay 的游标契约要求 id 是 16 位十进制（`validate_cursor`）。
    const FOREIGN: &str = "0000000000000007";
    const CHUNK: &str = "0000000000000008";
    let relay = FakeRelay::start(vec![
        Mail {
            id: FOREIGN.into(),
            sender: "f".repeat(64),
            ciphertext: foreign_frame(),
        },
        Mail {
            id: CHUNK.into(),
            sender: peer_mailbox,
            ciphertext: manifest_wire,
        },
    ]);
    let client = RelayClient::new(&relay.url).unwrap();

    // 第一圈：残留信被计数（未到阈值），同一页后排的真实 manifest 仍被应用并 ack。
    let first = pull(&f, "joiner", &client);
    let state = relay.state.lock().unwrap();
    assert!(
        state.acked.iter().flatten().any(|id| id == CHUNK),
        "同一页里排在残留信后面的 peer 信令必须照常应用并 ack: {:?}",
        state.acked
    );
    assert!(
        state.dropped.is_empty(),
        "第一次扫描还没到阈值，不能直接丢: {:?}",
        state.dropped
    );
    assert!(
        state.mails.iter().any(|mail| mail.id == FOREIGN),
        "未到阈值的残留信仍在 relay 上（留给下一圈计数）"
    );
    drop(state);
    assert!(
        first.is_err(),
        "第一次见到不可用邮件必须冒泡一次，否则排障只能靠猜"
    );
    let attempts = {
        let store = Store::open(&database).unwrap();
        let row = rejection(&store, &mailbox, FOREIGN).expect("拒收必须落库");
        assert_eq!(row.0, 1, "第一次扫描应记 1 次");
        assert_eq!(row.1, None, "没到阈值就不能标记已放弃");
        row.0
    };
    assert_eq!(attempts, 1);

    // 第二圈：不再重复冒泡（同一封信只报一次），计数继续涨。
    assert!(pull(&f, "joiner", &client).is_ok(), "之后不该每拍都刷错误");

    // 第三圈：达到阈值 → 在 relay 上显式 drop（不是 ack），本地留审计行。
    assert!(pull(&f, "joiner", &client).is_ok());
    {
        let state = relay.state.lock().unwrap();
        assert_eq!(
            state.dropped,
            vec![vec![FOREIGN.to_string()]],
            "到阈值必须用 drop 显式放弃，且只丢这一封"
        );
        assert!(
            state.acked.iter().flatten().all(|id| id != FOREIGN),
            "丢弃不能走 ack —— ack 的语义是「已应用」"
        );
        assert!(
            state.mails.is_empty(),
            "drop 成功后 relay 上不该再留着它: {:?}",
            state.mails.iter().map(|m| m.id.clone()).collect::<Vec<_>>()
        );
    }
    let store = Store::open(&database).unwrap();
    let row = rejection(&store, &mailbox, FOREIGN).unwrap();
    assert_eq!(row.0, 3, "第三次扫描时计数应为 3");
    assert!(row.1.is_some(), "drop 成功后必须落 dropped_at 审计");
    assert!(
        row.2.contains("sync_enrollment_unroutable_mail"),
        "审计行要写清楚为什么放弃: {}",
        row.2
    );
    // manifest 确实落盘：这一页并非「什么都没做」。
    let staged: i64 = store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sync_snapshots WHERE id=?1 AND manifest IS NOT NULL",
            [&offer.snapshot_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(staged, 1, "被 ack 的 manifest 必须真的落库");
    let _ = manifest_id;
}

/// C2-2 的守卫：**先推进游标、再冒泡错误**。
///
/// 旧代码在循环末尾 `return Err(error)`，游标推进在它后面 —— 于是一封不可应用的
/// 信就让同一页下一拍从同一处重来，永远排在最前面（现场 `after_id` 卡在 0 的
/// 直接机制）。这里让 relay 还有下一页（`has_more=true`）且唯一一封不可路由，
/// 断言「虽然这一拍报错，游标已经越过它」。
#[test]
fn a_rejected_mail_advances_the_scan_cursor_before_the_error_bubbles() {
    let f = Pair::new();
    f.channels();
    let (mailbox, database) = {
        let joiner = f.loan("joiner");
        let record = joiner.record.as_ref().unwrap();
        let pair = (record.route.local_mailbox.clone(), joiner.database.clone());
        drop(joiner);
        pair
    };
    const FOREIGN: &str = "0000000000000007";
    const CHUNK: &str = "0000000000000008";
    // 单页 1 封：第一页只有那封不可路由的信，且 `has_more=true`。
    let relay = FakeRelay::start_with_page_limit(
        vec![
            Mail {
                id: FOREIGN.into(),
                sender: "f".repeat(64),
                ciphertext: foreign_frame(),
            },
            Mail {
                id: CHUNK.into(),
                sender: "e".repeat(64),
                ciphertext: foreign_frame(),
            },
        ],
        1,
    );
    let client = RelayClient::new(&relay.url).unwrap();
    assert!(
        pull(&f, "joiner", &client).is_err(),
        "第一次见到不可用邮件必须冒泡一次"
    );
    let store = Store::open(&database).unwrap();
    let cursor = store
        .relay_scan_cursor(&super::super::flow::scan_key(&mailbox))
        .unwrap();
    assert_eq!(
        cursor, FOREIGN,
        "报错的那一拍也必须把游标推过这封信，否则同一页会永远从同一处重来"
    );
}
