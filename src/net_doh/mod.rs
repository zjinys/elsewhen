//! DoH + ECH 网络传输层：让指定的外发请求在国内网络下也能到达被 DNS 污染 / SNI 封锁的服务。
//!
//! 参照 <https://github.com/Lingyan000/fluxdo_doh>（MIT）的思路**重新实现**，仅保留
//! elsewhen 外网搜索所需的最小子集：
//! * DoH 解析（hickory，取干净 IP）；
//! * 查询域名的 DNS HTTPS 记录拿 ECH 配置，用 aws-lc-rs 的 HPKE 加密 SNI；
//! * ECH 握手失败降级为普通 TLS（仍用 DoH 的干净 IP），再失败才报错。
//!
//! 砍掉的部分：本地 HTTP CONNECT 代理、shadowsocks 隧道、MITM 证书（那是 fluxdo 给
//! 整个 app 过 CF 人机盾用的，搜索场景不需要，引入 MITM 反而是安全隐患）。
//!
//! 作用域：本版**仅外网搜索**使用（`doh_scope='search_only'`）；「全局」仅在
//! `network_settings.doh_scope` 字段预留，未实现。没填 `doh_uri` 时本模块不被调用，
//! 一切走系统直连（现状）。

mod dns;
mod tls_crypto;

use anyhow::{Context, Result, anyhow};
use std::time::Duration;

pub use dns::DohResolver;

/// 一次经由 DoH（+可选 ECH）的 HTTPS GET，返回响应体文本。
///
/// 这是给搜索/抓取用的最小入口：建一个 `current_thread` runtime 跑 async 连接
/// （仿 `ai/provider/streaming.rs` 的既有模式），内部完成 DoH 解析与 ECH 握手。
/// `doh_uri` 必须非空——调用方（搜索工具）应在「用户已配置 DoH」时才走这条路，
/// 否则直接用普通 reqwest 直连，不进入本模块。
pub fn https_get_via_doh(url: &str, doh_uri: &str, timeout: Duration) -> Result<String> {
    if doh_uri.trim().is_empty() {
        return Err(anyhow!("doh_uri 为空，不应进入 DoH 传输路径"));
    }
    let url = url.to_string();
    let doh_uri = doh_uri.to_string();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("创建 DoH 运行时失败")?;
    runtime.block_on(async move { async_https_get(&url, &doh_uri, timeout).await })
}

async fn async_https_get(url: &str, doh_uri: &str, timeout: Duration) -> Result<String> {
    let parsed = reqwest::Url::parse(url).context("解析 URL 失败")?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("URL 缺少主机名"))?
        .to_string();
    let port = parsed.port().unwrap_or(443);
    let path = parsed.path().to_string();
    let query = parsed.query().map(|q| q.to_string());
    let tls = connect_via_doh(&host, port, doh_uri, timeout).await?;
    tls_crypto::http_get_over_tls(tls, &host, &path, query.as_deref(), &[], timeout).await
}

/// 经 DoH（+可选 ECH）下载一个二进制资源（如图片），返回响应体原始字节。
/// 被墙的图床（如 pbs.twimg.com）直连失败时，调用方可用此走 DoH 隧道下载。
pub fn https_get_bytes_via_doh(url: &str, doh_uri: &str, timeout: Duration) -> Result<Vec<u8>> {
    if doh_uri.trim().is_empty() {
        return Err(anyhow!("doh_uri 为空，不应进入 DoH 传输路径"));
    }
    let url = url.to_string();
    let doh_uri = doh_uri.to_string();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("创建 DoH 运行时失败")?;
    runtime.block_on(async move {
        let parsed = reqwest::Url::parse(&url).context("解析 URL 失败")?;
        let host = parsed
            .host_str()
            .ok_or_else(|| anyhow!("URL 缺少主机名"))?
            .to_string();
        let port = parsed.port().unwrap_or(443);
        let path = parsed.path().to_string();
        let query = parsed.query().map(|q| q.to_string());
        let tls = connect_via_doh(&host, port, &doh_uri, timeout).await?;
        tls_crypto::http_get_bytes_over_tls(tls, &host, &path, query.as_deref(), &[], timeout)
            .await
    })
}

/// 建立一条经 DoH（+可选 ECH）到 `host:port` 的 TLS 流，供调用方自由发 HTTP 请求。
/// 这是所有经隧道入口的共用连接逻辑：DoH 解析（带 ECH 配置）→ 建 connector → 逐 IP 试。
async fn connect_via_doh(
    host: &str,
    port: u16,
    doh_uri: &str,
    timeout: Duration,
) -> Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>> {
    let resolver = DohResolver::new(doh_uri, timeout)?;
    let lookup = resolver.lookup(host).await?;
    let connector = tls_crypto::build_connector(lookup.ech_config.as_ref())?;
    let mut last_err: Option<anyhow::Error> = None;
    for ip in &lookup.ips {
        match tls_crypto::connect_tls(&connector, host, *ip, port, timeout).await {
            Ok(tls) => return Ok(tls),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("{host} 没有可用 IP")))
}

/// 经 DoH（+可选 ECH）执行一个自定义 HTTP/HTTPS 请求，返回响应体文本。
/// 这是给搜索引擎等多引擎共用的通用隧道入口：调用方给定 method/path/query/body/表头，
/// 传输层负责 DoH+ECH。`extra_headers` 每项是完整一行（如 `Authorization: Bearer xxx`）。
pub fn https_request_via_doh(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    query: Option<&str>,
    content_type: &str,
    body: Option<&[u8]>,
    extra_headers: &[String],
    doh_uri: &str,
    timeout: Duration,
) -> Result<String> {
    if doh_uri.trim().is_empty() {
        return Err(anyhow!("doh_uri 为空，不应进入 DoH 传输路径"));
    }
    let host = host.to_string();
    let path = path.to_string();
    let query = query.map(|q| q.to_string());
    let ct = content_type.to_string();
    let body = body.map(|b| b.to_vec());
    let extra = extra_headers.to_vec();
    let doh_uri = doh_uri.to_string();
    let method = method.to_ascii_uppercase();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("创建 DoH 运行时失败")?;
    runtime.block_on(async move {
        let tls = connect_via_doh(&host, port, &doh_uri, timeout).await?;
        match (method.as_str(), body) {
            ("GET", None) => {
                tls_crypto::http_get_over_tls(tls, &host, &path, query.as_deref(), &extra, timeout)
                    .await
            }
            ("POST", Some(b)) => {
                tls_crypto::http_post_text_over_tls(tls, &host, &path, &ct, &b, &extra, timeout)
                    .await
            }
            (m, _) => Err(anyhow!("DoH 隧道暂不支持的请求形态: {m}")),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 端到端实网验证：经用户自建 DoH 解析 x.com 并用 ECH 连接。
    /// 依赖网络与用户 DoH 服务，标 `#[ignore]` 不进 CI；手动跑：
    ///   ELSEWHEN_TEST_DOH=https://your-doh/ cargo test --release net_doh -- --ignored --nocapture
    fn probe(url: &str) {
        let doh = std::env::var("ELSEWHEN_TEST_DOH")
            .expect("设置 ELSEWHEN_TEST_DOH 为你的 DoH 服务地址");
        match https_get_via_doh(url, &doh, Duration::from_secs(12)) {
            Ok(text) => println!("OK {url} -> {} bytes: {}", text.len(), &text[..text.len().min(120)]),
            Err(e) => panic!("{url} DoH+ECH 请求失败: {e:#}"),
        }
    }

    /// 端到端实网验证：经用户自建 DoH 解析 + ECH 连接。
    /// 依赖网络与用户 DoH 服务，标 `#[ignore]` 不进 CI；手动跑：
    ///   ELSEWHEN_TEST_DOH=https://your-doh/ cargo test --release net_doh -- --ignored --nocapture
    #[test]
    #[ignore]
    fn doh_plain_tls_tavily() {
        // AWS 上的服务：走 DoH 干净 IP + 普通 TLS（无 ECH）。
        probe("https://api.tavily.com/");
    }

    #[test]
    #[ignore]
    fn doh_ech_cloudflare_trace() {
        // Cloudflare 免费版强制 ECH：cloudflare.com/cdn-cgi/trace 能验证 ECH 是否真生效，
        // 返回体里 sni=encrypted 即 ECH 成功、plaintext 则是降级普通 TLS。
        probe("https://www.cloudflare.com/cdn-cgi/trace");
    }

    #[test]
    #[ignore]
    fn doh_ech_x_com() {
        // x.com 在 Cloudflare 免费版后：硬封但 ECH 可达，是本特性的核心场景。
        probe("https://x.com/");
    }
}
