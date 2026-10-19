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
mod cache;
mod runtime;
mod tools;
mod transport;

pub(crate) use cache::{mcp_server_fingerprint, mcp_tools_cache_lookup, mcp_tools_cache_store};
#[cfg(test)]
pub(crate) use runtime::connect;
pub(crate) use runtime::invalidate_server;
pub use runtime::list_tools_for_server;
pub(crate) use runtime::record_exec_audit;
pub use tools::{available_tools, call_tool};
#[cfg(test)]
pub(crate) use tools::{extract_text, truncate_result};
pub(crate) use transport::HttpSession;

#[cfg(test)]
#[path = "mcp_tests/mod.rs"]
mod tests;

#[cfg(test)]
#[path = "mcp_tests/approval_tests.rs"]
mod approval_tests;

#[cfg(test)]
#[path = "mcp_tests/http_transport_tests.rs"]
mod http_transport_tests;
