//! MCP 审批闭环测试（原 mcp.rs inline mod approval_tests 外移，行为零变化）。

    use super::*;
    use crate::ai::tool::{ToolCall, ToolRegistry, dispatch};

    /// 审批闭环：dispatch MCP 工具不直接执行 → 创建 pending_action →
    /// 用户确认后 execute_pending_action 才真正发起 tools/call。
    /// 用真子进程 echo server 验证全程（与协议测试同 fixture）。
    #[test]
    fn dispatch_creates_pending_action_and_execute_confirmed_runs_it() {
        let python = match super::tests::which_python() {
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
        result = {"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {"name": "t", "version": "0"}}
    elif method == 'tools/call':
        result = {"content": [{"type": "text", "text": "echo: " + msg['params']['arguments']['text']}], "isError": False}
    else:
        result = {}
    print(json.dumps({"jsonrpc": "2.0", "id": msg['id'], "result": result}), flush=True)
"#;
        let dir =
            std::env::temp_dir().join(format!("elsewhen-mcp-approval-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("mcp_echo.py");
        std::fs::write(&script_path, script).unwrap();
        let server = crate::storage::McpServerConfig {
            id: "approval-server".into(),
            name: "approvalsrv".to_string(),
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

        let db = dir.join("test.db");
        let store = crate::storage::Store::open(&db).unwrap();
        crate::storage::Store::mark_initialized(&db).unwrap();
        // call_tool 执行时从 mcp_servers 表查配置，先把 server 落库
        store
            .save_mcp_server(
                None,
                &server.name,
                "all",
                &server.transport,
                &server.command,
                &server.args_json,
                &server.env_json,
                "",
                "{}",
                true,
                false,
            )
            .unwrap();
        let conversation_id = store.create_conversation(None, None).unwrap();

        // dispatch 一个 MCP 工具调用（registry 为空 → 走 MCP sidecar 路由）
        let call = ToolCall {
            id: "call-1".into(),
            name: format!("{MCP_PREFIX}approvalsrv__echo"),
            arguments: serde_json::json!({ "text": "hi" }),
            raw_arguments: None,
        };
        let mut registry = ToolRegistry::deferred_runtime();
        let msg = dispatch(&call, &registry, &store, &conversation_id);
        // 不直接执行：结果文本告知待确认，工具输出不出现
        let text = format!("{msg:?}");
        assert!(text.contains("待用户确认"), "got: {text}");
        assert!(!text.contains("echo: hi"));

        // pending_action 已创建，参数封存
        let pending = store
            .pending_actions_for_conversation(&conversation_id)
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].action, "mcp_tool_call");
        let args: serde_json::Value = serde_json::from_str(&pending[0].args_json).unwrap();
        assert_eq!(args["full_name"], call.name);
        assert_eq!(args["arguments"]["text"], "hi");

        // 用户在审阅卡点接受 → 真正执行
        let out = crate::ai::tool::execute_pending_action(&store, &pending[0]).unwrap();
        assert_eq!(out, "echo: hi");

        invalidate_server(&server.id);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// trusted=1 的 server：dispatch 直接执行，不创建 pending_action。
    /// 与上面审批闭环同一 echo fixture，仅 trusted 标志不同。
    #[test]
    fn trusted_server_dispatch_executes_directly_without_pending_action() {
        let python = match super::tests::which_python() {
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
        result = {"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {"name": "t", "version": "0"}}
    elif method == 'tools/call':
        result = {"content": [{"type": "text", "text": "echo: " + msg['params']['arguments']['text']}], "isError": False}
    else:
        result = {}
    print(json.dumps({"jsonrpc": "2.0", "id": msg['id'], "result": result}), flush=True)
"#;
        let dir =
            std::env::temp_dir().join(format!("elsewhen-mcp-trusted-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("mcp_echo.py");
        std::fs::write(&script_path, script).unwrap();

        let db = dir.join("test.db");
        let store = crate::storage::Store::open(&db).unwrap();
        crate::storage::Store::mark_initialized(&db).unwrap();
        // trusted=true 落库
        store
            .save_mcp_server(
                None,
                "trustedsrv",
                "all",
                "stdio",
                &python,
                &serde_json::to_string(&[script_path.to_string_lossy().to_string()]).unwrap(),
                "{}",
                "",
                "{}",
                true,
                true,
            )
            .unwrap();
        let conversation_id = store.create_conversation(None, None).unwrap();

        let call = ToolCall {
            id: "call-1".into(),
            name: format!("{MCP_PREFIX}trustedsrv__echo"),
            arguments: serde_json::json!({ "text": "hi" }),
            raw_arguments: None,
        };
        let registry = ToolRegistry::deferred_runtime();
        let msg = dispatch(&call, &registry, &store, &conversation_id);
        // 直接执行：工具输出出现在结果里，没有审批暂存
        let text = format!("{msg:?}");
        assert!(text.contains("echo: hi"), "got: {text}");
        assert!(!text.contains("待用户确认"), "got: {text}");
        let pending = store
            .pending_actions_for_conversation(&conversation_id)
            .unwrap();
        assert!(pending.is_empty(), "trusted 不应创建 pending_action");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 只读免审批（迁移 53）：server 显式开 allow_readonly_auto 且上游
    /// 标注 readOnlyHint=true → 跳过 pending 直执行；免审批仍留执行审计。
    /// 不盲信单一来源：关掉任一边（开关未开 / 注解缺失）都回审批流。
    #[test]
    fn readonly_auto_allowlist_executes_directly_only_when_both_sides_agree() {
        let python = match super::tests::which_python() {
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
        result = {"protocolVersion": "2024-11-05", "capabilities": {}, "serverInfo": {"name": "t", "version": "0"}}
    elif method == 'tools/call':
        result = {"content": [{"type": "text", "text": "echo: " + msg['params']['arguments']['text']}], "isError": False}
    else:
        result = {}
    print(json.dumps({"jsonrpc": "2.0", "id": msg['id'], "result": result}), flush=True)
"#;
        let dir = std::env::temp_dir().join(format!(
            "elsewhen-mcp-readonly-auto-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("mcp_echo.py");
        std::fs::write(&script_path, script).unwrap();

        let readonly_tool = |hint: bool| crate::ai::mcp::McpToolInfo {
            name: format!("{MCP_PREFIX}roa__echo"),
            server: "roa".into(),
            server_id: "roa-id".into(),
            raw_name: "echo".into(),
            description: "echo".into(),
            parameters: serde_json::json!({
                "type":"object",
                "properties":{"text":{"type":"string"}},
                "required":["text"]
            }),
            read_only_hint: hint,
        };

        // 场景一：白名单开 + 注解 true → 直执行，无 pending。
        let db = dir.join("allow.db");
        let store = crate::storage::Store::open(&db).unwrap();
        crate::storage::Store::mark_initialized(&db).unwrap();
        store
            .save_mcp_server(
                None,
                "roa",
                "all",
                "stdio",
                &python,
                &serde_json::to_string(&[script_path.to_string_lossy().to_string()]).unwrap(),
                "{}",
                "",
                "{}",
                true,
                false,
            )
            .unwrap();
        let server_id = store
            .list_mcp_servers()
            .unwrap()
            .into_iter()
            .find(|s| s.name == "roa")
            .map(|s| s.id)
            .unwrap();
        store
            .set_mcp_server_allow_readonly_auto(&server_id, true)
            .unwrap();
        let conversation_id = store.create_conversation(None, None).unwrap();
        let call = ToolCall {
            id: "call-1".into(),
            name: format!("{MCP_PREFIX}roa__echo"),
            arguments: serde_json::json!({ "text": "hi" }),
            raw_arguments: None,
        };
        let mut registry = ToolRegistry::deferred_runtime();
        registry.set_mcp_tools(vec![readonly_tool(true)]);
        let msg = dispatch(&call, &registry, &store, &conversation_id);
        let text = format!("{msg:?}");
        assert!(text.contains("echo: hi"), "got: {text}");
        assert!(!text.contains("待用户确认"), "got: {text}");
        assert!(
            store
                .pending_actions_for_conversation(&conversation_id)
                .unwrap()
                .is_empty(),
            "只读免审批不应创建 pending_action"
        );

        // 场景二：注解 true 但白名单关（默认态）→ 走审批（pending 登记）。
        let db2 = dir.join("closed.db");
        let store2 = crate::storage::Store::open(&db2).unwrap();
        crate::storage::Store::mark_initialized(&db2).unwrap();
        store2
            .save_mcp_server(
                None,
                "roa",
                "all",
                "stdio",
                &python,
                &serde_json::to_string(&[script_path.to_string_lossy().to_string()]).unwrap(),
                "{}",
                "",
                "{}",
                true,
                false,
            )
            .unwrap();
        let conversation_id2 = store2.create_conversation(None, None).unwrap();
        let mut registry2 = ToolRegistry::deferred_runtime();
        registry2.set_mcp_tools(vec![readonly_tool(true)]);
        let msg2 = dispatch(&call, &registry2, &store2, &conversation_id2);
        let text2 = format!("{msg2:?}");
        assert!(
            text2.contains("待用户确认"),
            "白名单默认关闭应走审批: {text2}"
        );
        assert!(
            !store2
                .pending_actions_for_conversation(&conversation_id2)
                .unwrap()
                .is_empty(),
            "白名单关闭应登记 pending_action"
        );

        // 场景三：白名单开但工具无 readOnlyHint（写注解缺失）→ 走审批。
        let mut registry3 = ToolRegistry::deferred_runtime();
        registry3.set_mcp_tools(vec![readonly_tool(false)]);
        let msg3 = dispatch(&call, &registry3, &store, &conversation_id);
        let text3 = format!("{msg3:?}");
        assert!(
            text3.contains("待用户确认"),
            "无 readOnlyHint 注解的工具不得借白名单免审批: {text3}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
