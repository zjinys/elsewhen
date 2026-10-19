//! Bounded OpenAI-compatible SSE framing. Network chunks are not UTF-8 or JSON boundaries.
use super::{AiReply, ProviderEvent, TokenUsage};
use crate::ai::tool::ToolCall;
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::BTreeMap;

const MAX_EVENT_BYTES: usize = 1024 * 1024;
const MAX_REPLY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
struct Function {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
pub(super) struct Decoder {
    pending: Vec<u8>,
    data: Vec<u8>,
    content: String,
    reasoning: String,
    tools: BTreeMap<u64, Function>,
    model: Option<String>,
    usage: Option<TokenUsage>,
    finish_reason: Option<String>,
    done: bool,
    bytes: usize,
}

impl Decoder {
    /// Returns only content deltas; callers must still filter protocols and validate the final answer.
    pub(super) fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>> {
        let mut deltas = Vec::new();
        for &byte in bytes {
            if self.done {
                break;
            }
            if byte == b'\n' {
                let mut line = std::mem::take(&mut self.pending);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if line.is_empty() {
                    if !self.data.is_empty() {
                        let data = std::mem::take(&mut self.data);
                        if let Some(delta) = self.event(&data)? {
                            deltas.push(delta);
                        }
                    }
                } else if let Some(mut data) = line.strip_prefix(b"data:") {
                    if data.first() == Some(&b' ') {
                        data = &data[1..];
                    }
                    if self.data.len() + data.len() + 1 > MAX_EVENT_BYTES {
                        bail!("SSE event exceeds limit");
                    }
                    if !self.data.is_empty() {
                        self.data.push(b'\n');
                    }
                    self.data.extend_from_slice(data);
                }
            } else {
                if self.pending.len() >= MAX_EVENT_BYTES {
                    bail!("SSE line exceeds limit");
                }
                self.pending.push(byte);
            }
        }
        Ok(deltas)
    }

    fn event(&mut self, data: &[u8]) -> Result<Option<String>> {
        if data == b"[DONE]" {
            self.done = true;
            return Ok(None);
        }
        let event: Value = serde_json::from_slice(data).context("Invalid SSE JSON")?;
        if event.get("error").is_some() {
            bail!("Provider reported a streaming error");
        }
        self.bytes += data.len();
        if self.bytes > MAX_REPLY_BYTES {
            bail!("Stream reply exceeds limit");
        }
        if let Some(model) = event["model"].as_str() {
            self.model = Some(model.into());
        }
        if let Some(usage) = event.get("usage").filter(|v| v.is_object()) {
            // Repeated cumulative usage chunks replace one another, never add.
            if let (Some(prompt), Some(completion)) = (
                usage["prompt_tokens"].as_u64(),
                usage["completion_tokens"].as_u64(),
            ) {
                // deepseek: prompt_cache_hit_tokens；OpenAI: cached_tokens。
                let cached = usage["prompt_cache_hit_tokens"]
                    .as_u64()
                    .or_else(|| usage["prompt_tokens_details"]["cached_tokens"].as_u64());
                self.usage = Some(TokenUsage {
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                    total_tokens: usage["total_tokens"]
                        .as_u64()
                        .unwrap_or(prompt.saturating_add(completion)),
                    prompt_cached_tokens: cached,
                });
            }
        }
        let Some(choice) = event["choices"].as_array().and_then(|choices| {
            choices
                .iter()
                .find(|c| c["index"].as_u64().unwrap_or(0) == 0)
        }) else {
            return Ok(None);
        };
        if let Some(reason) = choice["finish_reason"].as_str() {
            self.finish_reason = Some(reason.into());
        }
        let delta = &choice["delta"];
        if let Some(reasoning) = delta["reasoning_content"].as_str() {
            self.reasoning.push_str(reasoning);
        }
        if let Some(calls) = delta["tool_calls"].as_array() {
            for call in calls {
                let index = call["index"]
                    .as_u64()
                    .context("Missing stream tool index")?;
                if index >= 128 {
                    bail!("Too many stream tool calls");
                }
                let function = self.tools.entry(index).or_default();
                if let Some(id) = call["id"].as_str() {
                    function.id.push_str(id);
                }
                if let Some(name) = call["function"]["name"].as_str() {
                    function.name.push_str(name);
                }
                if let Some(arguments) = call["function"]["arguments"].as_str() {
                    function.arguments.push_str(arguments);
                }
            }
        }
        if let Some(function) = delta.get("function_call") {
            let call = self.tools.entry(0).or_default();
            if call.id.is_empty() {
                call.id = format!("call_{}", uuid::Uuid::new_v4().simple());
            }
            if let Some(name) = function["name"].as_str() {
                call.name.push_str(name);
            }
            if let Some(arguments) = function["arguments"].as_str() {
                call.arguments.push_str(arguments);
            }
        }
        if let Some(content) = delta["content"].as_str() {
            if !content.is_empty() {
                self.content.push_str(content);
                return Ok(Some(content.into()));
            }
        }
        Ok(None)
    }

    pub(super) fn is_done(&self) -> bool {
        self.done
    }

    pub(super) fn finish(self) -> Result<AiReply> {
        if !self.done {
            bail!("Provider stream interrupted before DONE");
        }
        let mut tools = Vec::new();
        let mut ids = std::collections::HashSet::new();
        for call in self.tools.into_values() {
            if call.id.is_empty() || call.name.is_empty() || !ids.insert(call.id.clone()) {
                bail!("Invalid streamed tool identity");
            }
            let arguments: Value = serde_json::from_str(&call.arguments)
                .context("Incomplete streamed tool arguments")?;
            if !arguments.is_object() {
                bail!("Streamed tool arguments must be an object");
            }
            tools.push(ToolCall {
                id: call.id,
                name: call.name,
                arguments,
                raw_arguments: Some(call.arguments),
            });
        }
        Ok(AiReply {
            content: self.content,
            tool_calls: tools,
            model: self.model,
            usage: self.usage,
            reasoning_content: if self.reasoning.is_empty() {
                None
            } else {
                Some(self.reasoning)
            },
            finish_reason: self.finish_reason,
        })
    }
}

/// Runs on the existing synchronous generation worker. Dropping a selected request/body
/// future cancels local HTTP work, including while the server sends no bytes.
pub(super) fn generate(
    config: &super::OpenAiCompatibleConfig,
    url: &str,
    mut payload: Value,
    mut policies: crate::ai::budget::RequestPolicies,
    mut window: usize,
    output: usize,
    observer: &mut dyn FnMut(ProviderEvent<'_>) -> Result<()>,
) -> Result<AiReply> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let client = crate::http_proxy::with_outbound_proxy_async(
            reqwest::Client::builder().timeout(std::time::Duration::from_secs(60)),
        )
        .build()?;
        payload["stream"] = Value::Bool(true);
        payload["stream_options"] = serde_json::json!({"include_usage":true});
        let mut budget_retry = false;
        let mut protocol_retry = false;
        let mut attempt = 0;
        let mut trigger = "initial";
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_millis(25));
        loop {
            observer(ProviderEvent::Poll)?;
            attempt += 1;
            let (result, report) =
                policies.fit_reported(&config.model, &mut payload, window, output);
            observer(ProviderEvent::Budget {
                attempt,
                trigger,
                report: &report,
            })?;
            result?;
            let send = client
                .post(url)
                .bearer_auth(&config.api_key)
                .json(&payload)
                .send();
            tokio::pin!(send);
            let mut response = loop {
                tokio::select! {
                    result = &mut send => break result.context("Failed to send streaming request")?,
                    _ = heartbeat.tick() => observer(ProviderEvent::Poll)?,
                }
            };
            let status = response.status();
            let sse = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.starts_with("text/event-stream"));
            let mut decoder = Decoder::default();
            let mut body = Vec::new();
            loop {
                let read = response.chunk();
                tokio::pin!(read);
                let chunk = loop {
                    tokio::select! {
                        result = &mut read => break result.context("Provider stream read failed")?,
                        _ = heartbeat.tick() => observer(ProviderEvent::Poll)?,
                    }
                };
                observer(ProviderEvent::Poll)?;
                let Some(chunk) = chunk else {
                    break;
                };
                if status.is_success() && sse {
                    for delta in decoder.push(&chunk)? {
                        observer(ProviderEvent::TextDelta(&delta))?;
                    }
                    if decoder.is_done() {
                        break;
                    }
                } else {
                    if body.len() + chunk.len() > MAX_REPLY_BYTES {
                        bail!("Provider response exceeds limit");
                    }
                    body.extend_from_slice(&chunk);
                }
            }
            if !status.is_success() {
                let error_body = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
                let code = error_body["error"]["code"].as_str().map(str::to_owned);
                let explicit_tool_rejection = matches!(
                    code.as_deref(),
                    Some("tools_not_supported" | "unsupported_tools")
                ) || error_body["error"]["message"].as_str()
                    == Some("unsupported tool messages");
                if matches!(status.as_u16(), 400 | 422) && explicit_tool_rejection {
                    return Err(super::NativeToolsUnsupported.into());
                }
                if !budget_retry
                    && crate::ai::budget::is_context_overflow(status.as_u16(), &error_body)
                {
                    let used = crate::ai::budget::input_tokens(&config.model, &payload);
                    window = (used + output).min(window) / 2;
                    budget_retry = true;
                    trigger = "upstream_context_window_rejection";
                    continue;
                }
                if crate::ai::budget::is_context_overflow(status.as_u16(), &error_body) {
                    // R4：预算重试（窗口减半重算 fit）仍未通过——配置的
                    // context_window 严重虚高或硬上限就是放不下。用可
                    // 诊断的语义错误替代泛化 HTTP 400，收尾层与人工排查
                    // 才能区分「溢出死了」与「网关坏了」。
                    bail!(
                        "Provider 上下文窗口不足：窗口自适应重试后仍被上游拒绝（HTTP {}）",
                        status.as_u16()
                    );
                }
                // Only an explicit unsupported-stream signal authorizes one fallback.
                if !protocol_retry
                    && matches!(status.as_u16(), 400 | 422)
                    && matches!(
                        code.as_deref(),
                        Some("stream_not_supported" | "unsupported_stream")
                    )
                {
                    payload.as_object_mut().unwrap().remove("stream");
                    payload.as_object_mut().unwrap().remove("stream_options");
                    protocol_retry = true;
                    trigger = "explicit_stream_unsupported";
                    continue;
                }
                bail!("AI provider returned HTTP {}", status.as_u16());
            }
            observer(ProviderEvent::Poll)?;
            if sse {
                return decoder.finish();
            }
            // Some compatible endpoints ignore stream=true and return a normal JSON reply.
            let response: super::OpenAiResponse =
                serde_json::from_slice(&body).context("Invalid provider JSON response")?;
            let choice = response
                .choices
                .first()
                .context("AI provider returned no choices")?;
            return Ok(AiReply {
                content: choice.message.content.clone().unwrap_or_default(),
                tool_calls: super::response_message_tool_calls(&choice.message),
                model: response.model,
                usage: response.usage.and_then(|u| {
                    Some(TokenUsage {
                        prompt_tokens: u.prompt_tokens?,
                        completion_tokens: u.completion_tokens?,
                        total_tokens: u
                            .total_tokens
                            .unwrap_or(u.prompt_tokens?.saturating_add(u.completion_tokens?)),
                        prompt_cached_tokens: u
                            .prompt_cache_hit_tokens
                            .or_else(|| u.prompt_tokens_details.and_then(|d| d.cached_tokens)),
                    })
                }),
                reasoning_content: choice.message.reasoning_content.clone(),
                finish_reason: choice.finish_reason.clone(),
            });
        }
    })
}

/// The native Ollama compatibility path stays non-streaming, but HTTP work is
/// cancellable while waiting for headers or a stalled body, just like SSE.
pub(super) fn cancellable_json(
    url: &str,
    payload: &Value,
    observer: &mut dyn FnMut(ProviderEvent<'_>) -> Result<()>,
) -> Result<Value> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let client = crate::http_proxy::with_outbound_proxy_async(
            reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)),
        )
        .build()?;
        observer(ProviderEvent::Poll)?;
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_millis(25));
        let send = client.post(url).json(payload).send();
        tokio::pin!(send);
        let mut response = loop {
            tokio::select! {
                result = &mut send => break result.context("Provider request failed")?,
                _ = heartbeat.tick() => observer(ProviderEvent::Poll)?,
            }
        };
        if !response.status().is_success() {
            bail!("Provider returned HTTP {}", response.status().as_u16());
        }
        let mut body = Vec::new();
        loop {
            let read = response.chunk();
            tokio::pin!(read);
            let chunk = loop {
                tokio::select! {
                    result = &mut read => break result.context("Provider body read failed")?,
                    _ = heartbeat.tick() => observer(ProviderEvent::Poll)?,
                }
            };
            observer(ProviderEvent::Poll)?;
            let Some(chunk) = chunk else {
                break;
            };
            if body.len() + chunk.len() > MAX_REPLY_BYTES {
                bail!("Provider response exceeds limit");
            }
            body.extend_from_slice(&chunk);
        }
        Ok(serde_json::from_slice(&body).context("Invalid provider JSON response")?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_at(address: std::net::SocketAddr) -> super::super::OpenAiCompatibleProvider {
        super::super::OpenAiCompatibleProvider::new(super::super::OpenAiCompatibleConfig {
            base_url: format!("http://{address}"),
            api_key: "synthetic".into(),
            model: "test".into(),
            temperature: 0.0,
            max_tokens: Some(64),
            context_window: Some(32768),
        })
        .unwrap()
    }

    #[test]
    fn cancellation_interrupts_silent_headers_and_body() {
        use crate::ai::provider::AiProvider;
        use std::io::{Read, Write};
        for headers in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let provider = provider_at(listener.local_addr().unwrap());
            let (release, wait) = std::sync::mpsc::channel();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = [0; 8192];
                let _ = socket.read(&mut request);
                if headers {
                    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap();
                }
                let _ = wait.recv_timeout(std::time::Duration::from_secs(3));
            });
            let start = std::time::Instant::now();
            let error = provider
                .generate_observed(Vec::new(), None, &mut |_| {
                    if start.elapsed() >= std::time::Duration::from_millis(100) {
                        bail!("test cancelled");
                    }
                    Ok(())
                })
                .unwrap_err();
            assert!(format!("{error:#}").contains("test cancelled"));
            assert!(start.elapsed() < std::time::Duration::from_secs(2));
            let _ = release.send(());
            server.join().unwrap();
        }
    }

    #[test]
    fn actual_http_stream_delivers_deltas_and_usage() {
        use crate::ai::provider::AiProvider;
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let provider = provider_at(listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 8192];
            let _ = socket.read(&mut request);
            let body = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"第一段\"}}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":3}}\n\ndata: [DONE]\n\n";
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        });
        let mut visible = String::new();
        let reply = provider
            .generate_observed(Vec::new(), None, &mut |delta| {
                if let ProviderEvent::TextDelta(delta) = delta {
                    visible.push_str(delta);
                }
                Ok(())
            })
            .unwrap();
        assert_eq!(visible, "第一段");
        assert_eq!(reply.content, visible);
        assert_eq!(reply.usage.unwrap().total_tokens, 5);
        server.join().unwrap();
    }

    #[test]
    fn every_utf8_and_crlf_boundary_preserves_content() {
        let wire = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"你好🙂\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n";
        for split in 0..=wire.len() {
            let mut decoder = Decoder::default();
            let mut deltas = decoder.push(&wire.as_bytes()[..split]).unwrap();
            deltas.extend(decoder.push(&wire.as_bytes()[split..]).unwrap());
            assert_eq!(deltas.concat(), "你好🙂");
            assert_eq!(decoder.finish().unwrap().content, "你好🙂");
        }
    }

    #[test]
    fn fragmented_tools_and_cumulative_usage() {
        let chunks = [
            serde_json::json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_a","function":{"name":"lookup","arguments":"{\"q\":"}}]}}]}),
            serde_json::json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"尾部\"}"}}]},"finish_reason":"tool_calls"}]}),
            serde_json::json!({"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":3}}),
            serde_json::json!({"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":3}}),
        ];
        let mut decoder = Decoder::default();
        for chunk in chunks {
            for byte in format!("data: {chunk}\n\n").bytes() {
                decoder.push(&[byte]).unwrap();
            }
        }
        decoder.push(b"data: [DONE]\n\n").unwrap();
        assert!(decoder.is_done());
        let reply = decoder.finish().unwrap();
        assert_eq!(reply.tool_calls[0].arguments["q"], "尾部");
        assert_eq!(reply.usage.unwrap().total_tokens, 13);
        assert_eq!(reply.finish_reason.as_deref(), Some("tool_calls"));
    }

    #[test]
    fn incomplete_stream_is_never_success() {
        let mut decoder = Decoder::default();
        decoder.push(b"data: {\"choices\":[]}\n\n").unwrap();
        assert!(decoder.finish().is_err());
    }

    #[test]
    fn malformed_tools_and_provider_errors_fail_closed() {
        let mut decoder = Decoder::default();
        assert!(
            decoder
                .push(b"data: {\"error\":{\"message\":\"private\"}}\n\n")
                .is_err()
        );
        let mut decoder = Decoder::default();
        decoder.push(b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"a\",\"function\":{\"name\":\"save\",\"arguments\":\"{\"}}]}}]}\n\ndata: [DONE]\n\n").unwrap();
        assert!(decoder.finish().is_err());
    }

    #[test]
    fn reasoning_and_other_choices_are_not_visible_text() {
        let mut decoder = Decoder::default();
        let deltas = decoder.push(b"data: {\"choices\":[{\"index\":1,\"delta\":{\"content\":\"other\"}},{\"index\":0,\"delta\":{\"reasoning_content\":\"private\"}}]}\n\ndata: [DONE]\n\n").unwrap();
        assert!(deltas.is_empty());
        let reply = decoder.finish().unwrap();
        assert!(reply.content.is_empty());
        assert_eq!(reply.reasoning_content.as_deref(), Some("private"));
    }
}
