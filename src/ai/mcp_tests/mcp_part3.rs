use super::*;

/// 用真子进程走一遍协议：python3 起一个最小 jsonrpc echo server。
/// 无 python3 的环境跳过（CI 兜底）。
#[test]
fn stdio_jsonrpc_roundtrip_against_real_child_process() {
    let python = match which_python() {
        Some(p) => p,
        None => {
            eprintln!("跳过：找不到 python3");
            return;
        }
    };
    let script = r#"
import json, sys
for line in sys.stdin:
    msg = json.loads(line)
    if 'id' not in msg:
        continue
    method = msg.get('method', '')
    if method == 'initialize':
        result = {"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {"name": "test", "version": "0"}}
    elif method == 'tools/list':
        result = {"tools": [{"name": "echo", "description": "回声", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}}]}
    elif method == 'tools/call':
        result = {"content": [{"type": "text", "text": "echo: " + msg['params']['arguments']['text']}], "isError": False}
    else:
        result = {}
    print(json.dumps({"jsonrpc": "2.0", "id": msg['id'], "result": result}), flush=True)
"#;
    let dir = std::env::temp_dir().join(format!("elsewhen-mcp-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script_path = dir.join("mcp_echo.py");
    std::fs::write(&script_path, script).unwrap();
    let server = crate::storage::McpServerConfig {
        id: "test-server".into(),
        name: "testsrv".to_string(),
        keywords: "回声,echo".to_string(),
        platform: "all".to_string(),
        transport: "stdio".into(),
        command: python,
        args_json: serde_json::to_string(&[script_path.to_string_lossy().to_string()]).unwrap(),
        env_json: "{}".into(),
        url: String::new(),
        headers_json: "{}".into(),
        enabled: true,
        trusted: false,
        allow_readonly_auto: false,
        created_at: String::new(),
        updated_at: String::new(),
    };
    // 无 store 依赖：list_tools_for_server / call_tool 只吃配置与会话。
    let tools = list_tools_for_server(&server).unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, format!("{MCP_PREFIX}testsrv__echo"));
    assert_eq!(tools[0].keywords, "回声,echo");

    // connect 复用缓存会话：第二次调用不再 spawn 新进程
    let result = {
        let mut map = connect(&server).unwrap();
        let conn = map.get_mut("test-server").unwrap();
        conn.request(
            "tools/call",
            json!({"name": "echo", "arguments": {"text": "你好"}}),
            Duration::from_secs(30),
        )
        .unwrap()
    };
    assert_eq!(extract_text(&result).as_deref(), Some("echo: 你好"));

    // 从存储经 tools/list、缓存到 catalog：编辑关键词下一轮生效，
    // 停用一个服务后，其工具/意图不得从另一服务的缓存中复活。
    let db = dir.join("keywords.db");
    let store = Store::open(&db).unwrap();
    Store::mark_initialized(&db).unwrap();
    let mut ids = Vec::new();
    for name in ["keyword-first", "keyword-second"] {
        ids.push(
            store
                .save_mcp_server(
                    None,
                    name,
                    "初始意图",
                    "all",
                    "stdio",
                    &server.command,
                    &server.args_json,
                    "{}",
                    "",
                    "{}",
                    true,
                    false,
                )
                .unwrap(),
        );
    }
    let (first_fetch, errors) = available_tools(&store);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(first_fetch.len(), 2);
    let (cached, errors) = available_tools(&store);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(cached.len(), 2, "缓存必须只保存各自服务的工具");
    store
        .save_mcp_server(
            Some(&ids[0]),
            "keyword-first",
            "小红书,种草",
            "all",
            "stdio",
            &server.command,
            &server.args_json,
            "{}",
            "",
            "{}",
            true,
            false,
        )
        .unwrap();
    let (updated, errors) = available_tools(&store);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        updated
            .iter()
            .find(|tool| tool.server == "keyword-first")
            .unwrap()
            .keywords,
        "小红书,种草"
    );
    let mut registry = crate::ai::tool::ToolRegistry::deferred_runtime();
    registry.set_mcp_tools(updated);
    assert!(
        registry
            .meta_spec()
            .unwrap()
            .description
            .contains("小红书/种草")
    );
    store.set_mcp_server_enabled(&ids[0], false).unwrap();
    let (remaining, errors) = available_tools(&store);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].server, "keyword-second");
    for id in ids {
        invalidate_server(&id);
    }
    drop(store);
    invalidate_server("test-server");
    std::fs::remove_dir_all(&dir).ok();
}
