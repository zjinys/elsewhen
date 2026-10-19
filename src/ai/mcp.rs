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
use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{Sender, channel};
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
    /// MCP 注解 `readOnlyHint`（tools/list 的 annotations）：上游自报的
    /// 只读标记。仅当 server 显式开了 allow_readonly_auto 白名单才参与
    /// 免审批判定，不盲信单一来源标注（事故 2026-10-19 P0 前提）。
    pub read_only_hint: bool,
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
        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow!("MCP 进程启动失败（{command}）：{e}"))?;
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
        let pending: Arc<Mutex<HashMap<i64, Sender<Value>>>> = Arc::new(Mutex::new(HashMap::new()));
        let reader_pending = Arc::clone(&pending);
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                let Ok(message) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
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
                let _ = guard.stdin.send(
                    json!({"jsonrpc":"2.0","method":method,"params":params}).to_string() + "\n",
                );
                Ok(())
            }
            McpConn::Http(session) => session.notify(method, params),
        }
    }

    /// 活性检查：stdio 看子进程，HTTP 恒真（服务器自会报错）。
    fn is_alive(&mut self) -> bool {
        match self {
            McpConn::Stdio(session) => session.lock().unwrap_or_else(|p| p.into_inner()).is_alive(),
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

    fn build_request(
        &self,
        body: &Value,
        timeout: Duration,
    ) -> Result<reqwest::blocking::RequestBuilder> {
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
        Ok(Some(
            serde_json::from_str(&body_text)
                .map_err(|e| anyhow!("MCP HTTP 响应不是合法 JSON：{e}"))?,
        ))
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
        self.post(
            json!({"jsonrpc":"2.0","method":method,"params":params}),
            Duration::from_secs(30),
        )?;
        Ok(())
    }
}

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
fn connect(
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

/// tools/list 结果按 server 行的进程内缓存（proposal
/// 2026-10-19-mcp-tools-deferred-injection §3.2）。键 = server 全量连接
/// 配置的内容指纹：配置增删/启停/transport/命令/URL 任一变更，指纹即变，
/// 旧行自然失效，无需显式失效钩子。不进同步域（本机运行时状态）。
#[derive(Clone)]
struct McpToolsCacheEntry {
    tools: Vec<McpToolInfo>,
    /// Some=上次拉取失败的显式错误（装配层继续走 mcp_errors 通道上报，
    /// 绝不静默降级为空清单——红线：宁可显式失败不静默少工具）。
    error: Option<String>,
    fetched_at: std::time::Instant,
}

/// 成功行有效期：无 ToolsChanged 事件机制，TTL 是事件缺失下的保守兜底
/// （远端新增工具最迟 5 分钟可见；指纹变更即时可见）。
const MCP_TOOLS_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);
/// 失败行短 TTL：错误也缓存以免每轮生成都重打一遍挂掉的 server，
/// 但很快放行重试，故障恢复不被长缓存拖住。
const MCP_TOOLS_CACHE_ERROR_TTL: std::time::Duration = std::time::Duration::from_secs(30);

static MCP_TOOLS_CACHE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, McpToolsCacheEntry>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

fn mcp_server_fingerprint(server: &crate::storage::McpServerConfig) -> String {
    crate::storage::knowledge::content_hash(&format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        server.id,
        server.name,
        server.platform,
        server.transport,
        server.command,
        server.args_json,
        server.env_json,
        server.url,
        server.headers_json,
        server.enabled
    ))
}

fn mcp_tools_cache_lookup(fingerprint: &str) -> Option<(Vec<McpToolInfo>, Option<String>)> {
    let cache = MCP_TOOLS_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let entry = cache.get(fingerprint)?;
    let ttl = if entry.error.is_some() {
        MCP_TOOLS_CACHE_ERROR_TTL
    } else {
        MCP_TOOLS_CACHE_TTL
    };
    if entry.fetched_at.elapsed() > ttl {
        return None;
    }
    Some((entry.tools.clone(), entry.error.clone()))
}

fn mcp_tools_cache_store(fingerprint: String, tools: Vec<McpToolInfo>, error: Option<String>) {
    MCP_TOOLS_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(
            fingerprint,
            McpToolsCacheEntry {
                tools,
                error,
                fetched_at: std::time::Instant::now(),
            },
        );
}

/// 汇总全部启用 server 的工具清单；单个 server 失败不拖垮其它 server，
/// 失败原因进错误列表由装配层决定上报口径（红线上抛优于静默）。
/// 每行结果按连接配置指纹缓存：成功行 5 分钟、失败行 30 秒，避免每轮
/// 生成前对全部启用 server 逐个发 tools/list RPC 的固定延迟；server 配置
/// 变更（增删/启停/传输参数）即时失效。
pub fn available_tools(store: &Store) -> (Vec<McpToolInfo>, Vec<String>) {
    let mut tools = Vec::new();
    let mut errors = Vec::new();
    match store.list_active_mcp_servers() {
        Ok(servers) => {
            for server in servers {
                let fingerprint = mcp_server_fingerprint(&server);
                if let Some((cached, error)) = mcp_tools_cache_lookup(&fingerprint) {
                    tools.extend(cached);
                    if let Some(error) = error {
                        errors.push(error);
                    }
                    continue;
                }
                match list_tools_for_server(&server) {
                    Ok(mut list) => {
                        tools.append(&mut list);
                        mcp_tools_cache_store(fingerprint, tools.clone(), None);
                    }
                    Err(error) => {
                        // 失败也留缓存行（带显式错误状态），错误继续上报。
                        let message = format!("MCP「{}」：{error:#}", server.name);
                        mcp_tools_cache_store(fingerprint, Vec::new(), Some(message.clone()));
                        errors.push(message);
                    }
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
        .list_active_mcp_servers()?
        .into_iter()
        .find(|s| s.name == server_name)
        .ok_or_else(|| anyhow!("MCP 服务器「{server_name}」未启用或未适用当前平台"))?;
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
    let text = extract_text(&result).unwrap_or_else(|| "(工具无文本输出)".to_string());
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
#[path = "mcp_tests/mod.rs"]
mod tests;

#[cfg(test)]
#[path = "mcp_tests/approval_tests.rs"]
mod approval_tests;

#[cfg(test)]
#[path = "mcp_tests/http_transport_tests.rs"]
mod http_transport_tests;
