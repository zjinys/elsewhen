//! MCP 运行时入口：执行审计、会话作废、连接获取与工具清单拉取。

use super::*;

/// 配置变更后作废会话（下次调用重拉进程与工具清单）。
/// MCP 执行审计（与 api::mcp::record_mcp_audit 同一 JSONL 审计面
/// `<data_dir>/mcp/.audit.log`）：只读免审批的工执行不走审批卡，
/// 免审批不等于无痕，这里补独立于会话的执行痕迹。
pub(crate) fn record_exec_audit(action: &str, detail: &str) {
    let Ok(config) = crate::config::AppConfig::load() else {
        return;
    };
    let dir = config.data_dir.join("mcp");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let line = serde_json::json!({
        "ts": chrono::Utc::now().to_rfc3339(),
        "action": action,
        "detail": detail,
    });
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(".audit.log"))
    {
        use std::io::Write;
        let _ = writeln!(f, "{line}");
    }
}

pub fn invalidate_server(server_id: &str) {
    let Some(conn) = sessions()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(server_id)
    else {
        return;
    };
    // 立即杀进程：stdio 会话仅此一处持有。HTTP 会话无本地进程，随 drop 释放。
    if let McpConn::Stdio(session) = conn {
        if let Ok(mut session) = session.lock() {
            let _ = session.child.kill();
            let _ = session.child.wait();
        }
    }
}

/// 获取（或新建）server 的会话。返回借用持有会话表锁：工具调用在
/// 生成侧串行执行，配置 invalidate 最多等一次调用结束，不会死锁。
pub(crate) fn connect(
    server: &crate::storage::McpServerConfig,
) -> Result<std::sync::MutexGuard<'static, HashMap<String, McpConn>>> {
    let mut map = sessions().lock().unwrap_or_else(|p| p.into_inner());
    if !map
        .get_mut(&server.id)
        .map(McpConn::is_alive)
        .unwrap_or(false)
    {
        map.remove(&server.id);
        let conn = match server.transport.as_str() {
            "stdio" => {
                let args: Vec<String> = serde_json::from_str(&server.args_json).unwrap_or_default();
                let env: HashMap<String, String> =
                    serde_json::from_str(&server.env_json).unwrap_or_default();
                McpConn::Stdio(Arc::new(Mutex::new(McpSession::spawn(
                    &server.command,
                    &args,
                    &env,
                )?)))
            }
            _ => McpConn::Http(Box::new(HttpSession::new(server)?)),
        };
        map.insert(server.id.clone(), conn);
    }
    let conn = map.get_mut(&server.id).expect("just ensured alive");
    conn.request(
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "elsewhen", "version": env!("CARGO_PKG_VERSION")}
        }),
        HANDSHAKE_TIMEOUT,
    )
    .context("MCP initialize 握手失败")?;
    let _ = conn.notify("notifications/initialized", json!({}));
    Ok(map)
}

/// 拉取某 server 的工具清单（合成名）。失败返回 Err，由调用方决定降级方式。
pub fn list_tools_for_server(server: &crate::storage::McpServerConfig) -> Result<Vec<McpToolInfo>> {
    let mut map = connect(server)?;
    let conn = map.get_mut(&server.id).expect("connect ensured");
    let result = conn.request("tools/list", json!({}), TOOLS_LIST_TIMEOUT)?;
    let tools = result
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for tool in tools.into_iter().take(MAX_TOOLS_PER_SERVER) {
        let raw_name = match tool.get("name").and_then(Value::as_str) {
            Some(name) => name.to_string(),
            None => continue,
        };
        out.push(McpToolInfo {
            name: format!("{MCP_PREFIX}{}__{raw_name}", server.name),
            server: server.name.clone(),
            server_id: server.id.clone(),
            raw_name,
            description: tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            parameters: tool
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({"type":"object"})),
            read_only_hint: tool
                .get("annotations")
                .and_then(|a| a.get("readOnlyHint"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    }
    Ok(out)
}
