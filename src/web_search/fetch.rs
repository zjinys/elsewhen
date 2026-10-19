//! 统一传输底层：直连 reqwest 与 DoH+ECH(net_doh) 兜底取回响应正文。

use super::*;

pub(crate) fn fetch_direct(req: &SearchRequest, timeout: Duration) -> Result<String> {
    let client = crate::ai::provider::shared_blocking_client(timeout.as_secs())
        .context("创建 HTTP 客户端失败")?;
    let mut rb = match req.method {
        "GET" => client.get(&req.url),
        "POST" => client.post(&req.url),
        m => return Err(anyhow!("不支持的请求方法 {m}")),
    };
    for (k, v) in &req.headers {
        rb = rb.header(k, v);
    }
    if let Some(body) = &req.json_body {
        rb = rb.json(body);
    }
    let resp = rb.send().context("搜索请求发送失败")?;
    let status = resp.status();
    let body = resp.text().context("读取搜索响应失败")?;
    if !status.is_success() {
        return Err(anyhow!("HTTP {status}: {}", &body[..body.len().min(200)]));
    }
    Ok(body)
}

pub(crate) fn fetch_via_doh(
    req: &SearchRequest,
    doh_uri: &str,
    timeout: Duration,
) -> Result<String> {
    let parsed = reqwest::Url::parse(&req.url).context("解析搜索 URL 失败")?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("搜索 URL 缺少主机名"))?;
    let port = parsed.port().unwrap_or(443);
    let path = if parsed.path().is_empty() {
        "/"
    } else {
        parsed.path()
    };
    let extra: Vec<String> = req
        .headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect();
    let body_bytes = req
        .json_body
        .as_ref()
        .map(|b| serde_json::to_vec(b))
        .transpose()
        .context("序列化请求体失败")?;
    crate::net_doh::https_request_via_doh(
        host,
        port,
        req.method,
        path,
        parsed.query(),
        "application/json",
        body_bytes.as_deref(),
        &extra,
        doh_uri,
        timeout,
    )
}

// ── Tavily ─────────────────────────────────────────────────────
