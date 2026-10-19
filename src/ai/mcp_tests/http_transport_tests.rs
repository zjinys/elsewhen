//! MCP streamable HTTP 传输层往返测试（原 mcp.rs inline mod http_transport_tests 外移，行为零变化）。

use super::*;

/// mock 一个 streamable-http jsonrpc server：
/// - initialize 回 MCP-Session-Id 响应头 + SSE 封装
/// - tools/call 要求带会话头与 Bearer，回 JSON 封装
#[test]
fn streamable_http_roundtrip_with_session_id_and_headers() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let http_port = std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let mut buf = [0u8; 65536];
                let len = stream.read(&mut buf).unwrap();
                let raw = String::from_utf8_lossy(&buf[..len]).to_string();
                // 解析请求头与体
                let (head, body) = raw.split_once("\r\n\r\n").unwrap();
                let has_session = head.to_lowercase().contains("mcp-session-id:");
                let auth_ok = head.to_lowercase().contains("authorization: bearer tok-1");
                let msg: Value = serde_json::from_str(body).unwrap();
                let id = msg.get("id").cloned();
                let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
                let result = match method {
                    "initialize" => {
                        let payload = json!({
                            "protocolVersion": "2024-11-05",
                            "capabilities": {},
                            "serverInfo": {"name": "http", "version": "0"}
                        });
                        let response = if let Some(id) = id {
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nMcp-Session-Id: sess-42\r\n\r\nevent: message\r\ndata: {}\r\n\r\n",
                                json!({"jsonrpc":"2.0","id":id,"result":payload})
                            )
                        } else {
                            return;
                        };
                        stream.write_all(response.as_bytes()).unwrap();
                        return;
                    }
                    "tools/list" if has_session => {
                        json!({"tools":[{"name":"http_echo","description":"d","inputSchema":{"type":"object","properties":{}}}]})
                    }
                    "tools/call" if has_session && auth_ok => {
                        let text = msg["params"]["arguments"]["text"].as_str().unwrap_or("");
                        json!({"content":[{"type":"text","text":format!("http: {text}")}],"isError":false})
                    }
                    _ => {
                        let response = if let Some(id) = id {
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}",
                                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":"bad request"}})
                            )
                        } else {
                            "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n".to_string()
                        };
                        stream.write_all(response.as_bytes()).unwrap();
                        return;
                    }
                };
                if let Some(id) = id {
                    let payload = json!({"jsonrpc":"2.0","id":id,"result":result});
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{payload}"
                    );
                    stream.write_all(response.as_bytes()).unwrap();
                }
            });
        }
    });
    let _ = http_port;

    let server = crate::storage::McpServerConfig {
        id: "http-server".into(),
        name: "httpsrv".to_string(),
        platform: "all".to_string(),
        transport: "streamable-http".into(),
        command: String::new(),
        args_json: "[]".into(),
        env_json: "{}".into(),
        url: format!("http://{addr}/mcp"),
        headers_json: r#"{"Authorization":"Bearer tok-1"}"#.into(),
        enabled: true,
        trusted: false,
        allow_readonly_auto: false,
        created_at: String::new(),
        updated_at: String::new(),
    };

    let tools = list_tools_for_server(&server).unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].raw_name, "http_echo");
    assert_eq!(tools[0].name, format!("{MCP_PREFIX}httpsrv__http_echo"));

    // tools/call 走同会话（session id 已缓存）+ Bearer 透传；call_tool
    // 从 store 查启用 server，先落库
    let db = std::env::temp_dir().join(format!("elsewhen-mcp-http-{}.db", std::process::id()));
    let store = crate::storage::Store::open(&db).unwrap();
    crate::storage::Store::mark_initialized(&db).unwrap();
    store
        .save_mcp_server(
            None,
            &server.name,
            "all",
            &server.transport,
            &server.command,
            &server.args_json,
            &server.env_json,
            &server.url,
            &server.headers_json,
            true,
            false,
        )
        .unwrap();
    let result = call_tool(
        &store,
        &format!("{MCP_PREFIX}httpsrv__http_echo"),
        &json!({"text": "hi http"}),
    )
    .unwrap_or_else(|e| panic!("call_tool 失败：{e:#}"));
    assert_eq!(result, "http: hi http");
    invalidate_server(&server.id);
    std::fs::remove_file(&db).ok();
}
