//! HTTP/SSE 传输：HttpSession JSON-RPC over HTTP 客户端。

use super::*;

/// 不开 GET 常驻 SSE 流（客户端拉模式足够，server 主动推送无消费者）。
pub(crate) struct HttpSession {
    client: reqwest::blocking::Client,
    url: String,
    session_id: Option<String>,
    headers: Vec<(String, String)>,
    next_id: AtomicI64,
}

/// 单个 HTTP 响应体上限（SSE 无 content-length，必须有界）。
const MAX_HTTP_BODY_BYTES: usize = 16 * 1024 * 1024;

impl HttpSession {
    pub(crate) fn new(server: &crate::storage::McpServerConfig) -> Result<Self> {
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

    pub(crate) fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value> {
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

    pub(crate) fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.post(
            json!({"jsonrpc":"2.0","method":method,"params":params}),
            Duration::from_secs(30),
        )?;
        Ok(())
    }
}
