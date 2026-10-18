//! MCP（Model Context Protocol）客户端：stdio + streamable-http 双传输的
//! jsonrpc-2.0 最小实现。
//!
//! 设计要点：
//! - 懒连接：首个工具调用/清单拉取时才 spawn 子进程（stdio）或建 HTTP
//!   会话（streamable-http）；会话按 server id 缓存，配置变更时
//!   `invalidate_server` 作废，下次重拉。
//! - stdio 本机子进程（桌面 only）；streamable-http 远程端点
//!   （全平台可用，含移动端），会话以 `Mcp-Session-Id` header 维持。
//! - 超时全部有界（握手/清单 30s，工具调用 120s），server 卡死不拖垮对话链。
//! - 失败不上抛到生成主链路：工具调用错误以结果文本返回（与内置工具一致，
//!   模型能看到错误并自行调整）。
//! - 工具名合成 `mcp__<server>__<tool>`（Anthropic 生态惯例），server 名在
//!   存储层已限制为 `[A-Za-z0-9_-]+`，不产生歧义分隔。

use crate::storage::Store;
use anyhow::{anyhow, bail, Context as _, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Sender};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(120);
const TOOLS_LIST_TIMEOUT: Duration = Duration::from_secs(30);
/// 单 server 工具数上限：防失控 server 挤爆工具清单预算。
const MAX_TOOLS_PER_SERVER: usize = 128;
/// 单次工具结果文本上限：与内置工具同口径的护栏。
const MAX_RESULT_CHARS: usize = 64 * 1024;

pub const MCP_PREFIX: &str = "mcp__";

#[derive(Debug, Clone, serde::Serialize)]
pub struct McpToolInfo {
    /// 合成名：`mcp__<server>__<tool>`
    pub name: String,
    pub server: String,
    pub server_id: String,
    pub raw_name: String,
    pub description: String,
    pub parameters: Value,
}

struct McpSession {
    child: Child,
    stdin: Sender<String>,
    /// 读线程与 request 共享的应答路由表：id → 该请求的回传通道。
    pending: Arc<Mutex<HashMap<i64, Sender<Value>>>>,
    next_id: AtomicI64,
}

impl McpSession {
    fn spawn(command: &str, args: &[String], env: &HashMap<String, String>) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            // 脱离当前进程组：server 崩溃不连带，Ctrl+C 也不误伤工具进程。
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let mut child = cmd.spawn().map_err(|e| anyhow!("MCP 进程启动失败（{command}）：{e}"))?;
        let stdin = child.stdin.take().context("stdin 缺失")?;
        let stdout = child.stdout.take().context("stdout 缺失")?;

        let (stdin_tx, stdin_rx) = channel::<String>();
        std::thread::Builder::new()
            .name("mcp-stdin".into())
            .spawn(move || {
                let mut stdin = stdin;
                for line in stdin_rx {
                    if stdin.write_all(line.as_bytes()).is_err() {
                        break;
                    }
                    if stdin.flush().is_err() {
                        break;
                    }
                }
            })
            .ok();

        let (notify_tx, notify_rx) = channel::<Value>();
        let pending: Arc<Mutex<HashMap<i64, Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let reader_pending = Arc::clone(&pending);
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
                if let Some(id) = message.get("id").and_then(Value::as_i64) {
                    // 直接路由到等待中的 request；超时的请求已摘牌，send
                    // 失败即静默丢弃（副本回复，无泄漏）。
                    let sender = reader_pending
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .remove(&id);
                    if let Some(sender) = sender {
                        let _ = sender.send(message);
                    }
                } else if notify_tx.send(message).is_err() {
                    break;
                }
            }
        });
        std::thread::Builder::new()
            .name("mcp-notify".into())
            .spawn(move || {
                for _ in notify_rx {
                    // 通知/日志留观测口，暂不消费。
                }
            })
            .ok();

        Ok(Self {
            child,
            stdin: stdin_tx,
            pending,
            next_id: AtomicI64::new(0),
        })
    }

    fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let (tx, rx) = channel();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, tx);
        let message = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        if self.stdin.send(message.to_string() + "\n").is_err() {
            self.pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id);
            bail!("MCP server 的 stdin 已关闭");
        }
        let reply = match rx.recv_timeout(timeout) {
            Ok(reply) => reply,
            Err(_) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&id);
                bail!("MCP 请求超时（{method}，{timeout:?}）")
            }
        };
        if let Some(error) = reply.get("error") {
            bail!("MCP 错误响应：{error}");
        }
        Ok(reply.get("result").cloned().unwrap_or(Value::Null))
    }

    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn sessions() -> &'static Mutex<HashMap<String, McpConn>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, McpConn>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 一条活跃的 MCP 连接。stdio 走子进程，streamable-http 走远程端点。
enum McpConn {
    Stdio(Arc<Mutex<McpSession>>),
    Http(Box<HttpSession>),
}

impl McpConn {
    fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        match self {
            McpConn::Stdio(session) => {
                let guard = session.lock().unwrap_or_else(|p| p.into_inner());
                guard.request(method, params, timeout)
            }
            McpConn::Http(session) => session.request(method, params, timeout),
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        match self {
            McpConn::Stdio(session) => {
                let guard = session.lock().unwrap_or_else(|p| p.into_inner());
                let _ = guard
                    .stdin
                    .send(json!({"jsonrpc":"2.0","method":method,"params":params}).to_string()
                        + "\n");
                Ok(())
            }
            McpConn::Http(session) => session.notify(method, params),
        }
    }

    /// 活性检查：stdio 看子进程，HTTP 恒真（服务器自会报错）。
    fn is_alive(&mut self) -> bool {
        match self {
            McpConn::Stdio(session) => {
                session.lock().unwrap_or_else(|p| p.into_inner()).is_alive()
            }
            McpConn::Http(_) => true,
        }
    }
}

/// streamable-http 会话：initialize 拿回 `Mcp-Session-Id` 后续请求都带上。
/// 每个请求一次 POST，响应按 content-type 兼容 JSON 与 SSE 两种封装；
/// 不开 GET 常驻 SSE 流（客户端拉模式足够，server 主动推送无消费者）。
struct HttpSession {
    client: reqwest::blocking::Client,
    url: String,
    session_id: Option<String>,
    headers: Vec<(String, String)>,
    next_id: AtomicI64,
}

/// 单个 HTTP 响应体上限（SSE 无 content-length，必须有界）。
const MAX_HTTP_BODY_BYTES: usize = 16 * 1024 * 1024;

impl HttpSession {
    fn new(server: &crate::storage::McpServerConfig) -> Result<Self> {
        if server.url.is_empty() {
            bail!("streamable-http 服务器缺少 URL");
        }
        // 与 provider 同一共享 client（带用户代理配置），长工具调用单独超时。
        let client = crate::ai::provider::shared_blocking_client(300)?;
        let headers: Value = serde_json::from_str(&server.headers_json).unwrap_or(json!({}));
        let mut parsed = Vec::new();
        if let Some(map) = headers.as_object() {
            for (k, v) in map {
                let value = v.as_str().context("header 值必须是字符串")?;
                parsed.push((k.clone(), value.to_string()));
            }
        }
        Ok(Self {
            client,
            url: server.url.clone(),
            session_id: None,
            headers: parsed,
            next_id: AtomicI64::new(0),
        })
    }

    fn build_request(&self, body: &Value, timeout: Duration) -> Result<reqwest::blocking::RequestBuilder> {
        let mut req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2025-06-18")
            .timeout(timeout)
            .json(body);
        if let Some(sid) = &self.session_id {
            req = req.header("Mcp-Session-Id", sid);
        }
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        Ok(req)
    }

    /// POST 一条 jsonrpc 消息。返回 Ok(None) 表示 202/空体（通知或非文本响应）。
    fn post(&mut self, body: Value, timeout: Duration) -> Result<Option<Value>> {
        let response = self
            .build_request(&body, timeout)?
            .send()
            .map_err(|e| anyhow!("MCP HTTP 请求失败：{e}"))?;
        let status = response.status();
        if let Some(sid) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            self.session_id = Some(sid.to_string());
        }
        if status == reqwest::StatusCode::ACCEPTED {
            return Ok(None);
        }
        if !status.is_success() {
            bail!("MCP HTTP 响应状态异常（{}）", status)
        }
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body_text = response
            .bytes()
            .map_err(|e| anyhow!("MCP HTTP 响应读取失败：{e}"))?;
        if body_text.len() > MAX_HTTP_BODY_BYTES {
            bail!("MCP HTTP 响应体超过 {MAX_HTTP_BODY_BYTES} 字节，拒绝处理");
        }
        let body_text = String::from_utf8_lossy(&body_text);
        if body_text.trim().is_empty() {
            return Ok(None);
        }
        if content_type.contains("text/event-stream") {
            // SSE 封装：找第一条带 id 的 data 消息（我们是同步拉模式，
            // 不消费 server 主动推送）。
            for line in body_text.lines() {
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let Ok(message) = serde_json::from_str::<Value>(data.trim()) else {
                    continue;
                };
                if message.get("id").is_some() {
                    return Ok(Some(message));
                }
            }
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&body_text).map_err(|e| {
            anyhow!("MCP HTTP 响应不是合法 JSON：{e}")
        })?))
    }

    fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let reply = self
            .post(
                json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
                timeout,
            )?
            .ok_or_else(|| anyhow!("MCP 服务器未返回响应（{method}）"))?;
        if let Some(error) = reply.get("error") {
            bail!("MCP 错误响应：{error}");
        }
        Ok(reply.get("result").cloned().unwrap_or(Value::Null))
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.post(json!({"jsonrpc":"2.0","method":method,"params":params}), Duration::from_secs(30))?;
        Ok(())
    }
}

/// 配置变更后作废会话（下次调用重拉进程与工具清单）。
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
fn connect(
    server: &crate::storage::McpServerConfig,
) -> Result<std::sync::MutexGuard<'static, HashMap<String, McpConn>>> {
    let mut map = sessions().lock().unwrap_or_else(|p| p.into_inner());
    if !map.get_mut(&server.id).map(McpConn::is_alive).unwrap_or(false) {
        map.remove(&server.id);
        let conn = match server.transport.as_str() {
            "stdio" => {
                let args: Vec<String> =
                    serde_json::from_str(&server.args_json).unwrap_or_default();
                let env: HashMap<String, String> =
                    serde_json::from_str(&server.env_json).unwrap_or_default();
                McpConn::Stdio(Arc::new(Mutex::new(McpSession::spawn(
                    &server.command, &args, &env,
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
            parameters: tool.get("inputSchema").cloned().unwrap_or_else(|| json!({"type":"object"})),
        });
    }
    Ok(out)
}

/// 汇总全部启用 server 的工具清单；单个 server 失败不拖垮其它 server，
/// 失败原因进错误列表由装配层决定上报口径（红线上抛优于静默）。
pub fn available_tools(store: &Store) -> (Vec<McpToolInfo>, Vec<String>) {
    let mut tools = Vec::new();
    let mut errors = Vec::new();
    match store.list_enabled_mcp_servers() {
        Ok(servers) => {
            for server in servers {
                match list_tools_for_server(&server) {
                    Ok(mut list) => tools.append(&mut list),
                    Err(error) => errors.push(format!("MCP「{}」：{error:#}", server.name)),
                }
            }
        }
        Err(error) => errors.push(format!("MCP 配置读取失败：{error:#}")),
    }
    (tools, errors)
}

/// 分发一次 MCP 工具调用。`full_name` 为合成名。
pub fn call_tool(store: &Store, full_name: &str, arguments: &Value) -> Result<String> {
    let raw = full_name
        .strip_prefix(MCP_PREFIX)
        .ok_or_else(|| anyhow!("不是 MCP 工具名：{full_name}"))?;
    let (server_name, raw_name) = raw
        .split_once("__")
        .ok_or_else(|| anyhow!("MCP 工具名缺少 server 前缀：{full_name}"))?;
    let server = store
        .list_enabled_mcp_servers()?
        .into_iter()
        .find(|s| s.name == server_name)
        .ok_or_else(|| anyhow!("MCP 服务器「{server_name}」未启用或不存在"))?;
    let mut map = connect(&server)?;
    let conn = map.get_mut(&server.id).expect("connect ensured");
    let result = conn.request(
        "tools/call",
        json!({"name": raw_name, "arguments": arguments}),
        TOOL_CALL_TIMEOUT,
    )?;
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        let text = extract_text(&result).unwrap_or_default();
        bail!("MCP 工具返回错误：{text}");
    }
    let text = extract_text(&result)
        .unwrap_or_else(|| "(工具无文本输出)".to_string());
    Ok(truncate_result(text))
}

fn extract_text(result: &Value) -> Option<String> {
    let content = result.get("content")?.as_array()?;
    let mut out = String::new();
    for item in content {
        if item.get("type").and_then(Value::as_str) == Some("text") {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(text);
            }
        }
    }
    Some(out).filter(|s| !s.is_empty())
}

fn truncate_result(text: String) -> String {
    if text.chars().count() <= MAX_RESULT_CHARS {
        return text;
    }
    let head: String = text.chars().take(MAX_RESULT_CHARS).collect();
    format!("{head}\n\n[输出超过 {MAX_RESULT_CHARS} 字符，已截断；如需完整内容请让工具分页返回]")
}

#[cfg(test)]
mod tests {
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
            transport: "stdio".into(),
            command: python,
            args_json: serde_json::to_string(&[script_path.to_string_lossy().to_string()]).unwrap(),
            env_json: "{}".into(),
            url: String::new(),
            headers_json: "{}".into(),
            enabled: true,
            trusted: false,
            created_at: String::new(),
            updated_at: String::new(),
        };
        // 无 store 依赖：list_tools_for_server / call_tool 只吃配置与会话。
        let tools = list_tools_for_server(&server).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, format!("{MCP_PREFIX}testsrv__echo"));

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
        invalidate_server("test-server");
        std::fs::remove_dir_all(&dir).ok();
    }

    pub(super) fn which_python() -> Option<String> {
        ["python3", "python"].iter().find_map(|bin| {
            std::process::Command::new(bin)
                .arg("--version")
                .output()
                .ok()
                .filter(|out| out.status.success())
                .map(|_| bin.to_string())
        })
    }

    #[test]
    fn oversized_tool_result_is_truncated_with_marker() {
        let big = "好".repeat(MAX_RESULT_CHARS + 10);
        let truncated = truncate_result(big);
        assert!(truncated.chars().count() < MAX_RESULT_CHARS + 100);
        assert!(truncated.contains("已截断"));
    }
}

#[cfg(test)]
mod approval_tests {
    use super::*;
    use crate::ai::tool::{dispatch, ToolCall, ToolRegistry};

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
        let dir = std::env::temp_dir().join(format!(
            "elsewhen-mcp-approval-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let script_path = dir.join("mcp_echo.py");
        std::fs::write(&script_path, script).unwrap();
        let server = crate::storage::McpServerConfig {
            id: "approval-server".into(),
            name: "approvalsrv".to_string(),
            transport: "stdio".into(),
            command: python,
            args_json: serde_json::to_string(&[script_path.to_string_lossy().to_string()]).unwrap(),
            env_json: "{}".into(),
            url: String::new(),
            headers_json: "{}".into(),
            enabled: true,
            trusted: false,
            created_at: String::new(),
            updated_at: String::new(),
        };

        let db = dir.join("test.db");
        let store = crate::storage::Store::open(&db).unwrap();
        crate::storage::Store::mark_initialized(&db).unwrap();
        // call_tool 执行时从 mcp_servers 表查配置，先把 server 落库
        store.save_mcp_server(None, &server.name, &server.transport, &server.command, &server.args_json, &server.env_json, "", "{}", true, false).unwrap();
        let conversation_id = store.create_conversation(None, None).unwrap();

        // dispatch 一个 MCP 工具调用（registry 为空 → 走 MCP sidecar 路由）
        let call = ToolCall {
            id: "call-1".into(),
            name: format!("{MCP_PREFIX}approvalsrv__echo"),
            arguments: serde_json::json!({ "text": "hi" }),
            raw_arguments: None,
        };
        let registry = ToolRegistry::deferred_runtime();
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
        let args: serde_json::Value =
            serde_json::from_str(&pending[0].args_json).unwrap();
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
        let dir = std::env::temp_dir().join(format!(
            "elsewhen-mcp-trusted-test-{}",
            std::process::id()
        ));
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
}

#[cfg(test)]
mod http_transport_tests {
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
            transport: "streamable-http".into(),
            command: String::new(),
            args_json: "[]".into(),
            env_json: "{}".into(),
            url: format!("http://{addr}/mcp"),
            headers_json: r#"{"Authorization":"Bearer tok-1"}"#.into(),
            enabled: true,
            trusted: false,
            created_at: String::new(),
            updated_at: String::new(),
        };

        let tools = list_tools_for_server(&server).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].raw_name, "http_echo");
        assert_eq!(tools[0].name, format!("{MCP_PREFIX}httpsrv__http_echo"));

        // tools/call 走同会话（session id 已缓存）+ Bearer 透传；call_tool
        // 从 store 查启用 server，先落库
        let db = std::env::temp_dir()
            .join(format!("elsewhen-mcp-http-{}.db", std::process::id()));
        let store = crate::storage::Store::open(&db).unwrap();
        crate::storage::Store::mark_initialized(&db).unwrap();
        store
            .save_mcp_server(None, &server.name, &server.transport, &server.command, &server.args_json, &server.env_json, &server.url, &server.headers_json, true, false)
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
}
