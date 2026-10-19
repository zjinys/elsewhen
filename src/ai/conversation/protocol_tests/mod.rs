// 公共测试辅助（原文件平移，pub(super) 供分组子文件复用）。
use super::*;
use crate::ai::provider::AiReply;
use serde_json::json;

pub(super) fn missing_call_reply() -> AiReply {
    let mut reply = reply_with(Some("function_call"), "", vec![]);
    reply.usage = Some(TokenUsage {
        prompt_tokens: 200,
        completion_tokens: 43,
        total_tokens: 243,
        prompt_cached_tokens: None,
    });
    reply
}

pub(super) fn protocol_http_fixture(
    replies: Vec<(u16, Value)>,
) -> (String, std::thread::JoinHandle<Vec<Value>>) {
    use std::io::{BufRead, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, reply) in replies {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(std::time::Instant::now() < deadline, "missing HTTP request");
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(e) => panic!("fixture accept: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            requests.push(serde_json::from_slice(&body).unwrap());
            let body = reply.to_string();
            let reason = if status == 200 { "OK" } else { "Bad Request" };
            write!(stream, "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
        requests
    });
    (url, server)
}

mod native_vs_text;

mod budget_repair_flow;
