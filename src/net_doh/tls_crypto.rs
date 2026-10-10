//! TLS 连接层：cipher suite 指纹对齐 Chrome（抗 JA3 检测）、ECH/普通 TLS connector、
//! TCP+TLS 连接与一次最小 HTTP/1.1 GET。
//!
//! 参照 fluxdo_doh `ech.rs` / `tls_crypto.rs` 重写，简化点：
//! * 不做 TLS config 缓存 / 会话恢复（搜索是低频请求，单次连接开销可接受）；
//! * 不做多 IP 交错并行竞速（逐 IP 顺序尝试即可，配合上层的 IP 列表）；
//! * HTTP 层手写最小 HTTP/1.1 GET，不引入 hyper——搜索只需拿到响应体文本。

use anyhow::{Context, Result, anyhow};
use rustls::client::EchMode;
use rustls::pki_types::{EchConfigListBytes, ServerName};
use rustls::{CipherSuite, ClientConfig, RootCertStore};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

/// 构建 TLS 加密提供者，cipher suite 顺序对齐 Chrome/BoringSSL。
///
/// rustls 默认（ring/aws-lc）把 ChaCha20 排在前面，JA3 指纹明显不同于真实浏览器，
/// 可能被 Cloudflare Bot Management 标记为非浏览器客户端。重排为 Chrome 顺序以降低
/// 被识别概率（与 fluxdo `tls_crypto.rs` 同一目的）。
fn build_provider() -> Arc<rustls::crypto::CryptoProvider> {
    let mut provider = rustls::crypto::aws_lc_rs::default_provider();
    provider.cipher_suites = reorder_cipher_suites_chrome_style(provider.cipher_suites);
    Arc::new(provider)
}

fn reorder_cipher_suites_chrome_style(
    suites: Vec<rustls::SupportedCipherSuite>,
) -> Vec<rustls::SupportedCipherSuite> {
    let priority = |suite: &rustls::SupportedCipherSuite| -> u32 {
        match suite.suite() {
            CipherSuite::TLS13_AES_128_GCM_SHA256 => 0,
            CipherSuite::TLS13_AES_256_GCM_SHA384 => 1,
            CipherSuite::TLS13_CHACHA20_POLY1305_SHA256 => 2,
            CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256 => 10,
            CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 => 11,
            CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384 => 12,
            CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384 => 13,
            CipherSuite::TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256 => 14,
            CipherSuite::TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256 => 15,
            _ => 100,
        }
    };
    let mut sorted = suites;
    sorted.sort_by_key(priority);
    sorted
}

/// ECH（有配置时）或普通 TLS 的连接器。ECH 配置解析失败自动降级为普通 TLS——
/// 降级不会让情况比「没装 ECH」更糟，只是失去了 SNI 加密。
pub fn build_connector(ech_config: Option<&EchConfigListBytes<'static>>) -> Result<TlsConnector> {
    let mut root_store = RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = build_provider();

    if let Some(ech_bytes) = ech_config {
        let hpke_suites = rustls::crypto::aws_lc_rs::hpke::ALL_SUPPORTED_SUITES;
        match rustls::client::EchConfig::new(ech_bytes.clone(), hpke_suites) {
            Ok(ech) => {
                let config = ClientConfig::builder_with_provider(provider)
                    .with_ech(EchMode::Enable(ech))
                    .map_err(|e| anyhow!("ECH 配置错误: {e}"))?
                    .with_root_certificates(root_store)
                    .with_no_client_auth();
                return Ok(TlsConnector::from(Arc::new(config)));
            }
            Err(e) => {
                eprintln!("[net_doh] ECH 配置解析失败，降级普通 TLS: {e}");
            }
        }
    }

    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .context("构建 TLS 配置失败")?
        .with_root_certificates(root_store)
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

/// 建立到 `ip:port` 的 TCP 连接并完成 TLS 握手（SNI 用 `host`）。
pub async fn connect_tls(
    connector: &TlsConnector,
    host: &str,
    ip: IpAddr,
    port: u16,
    timeout: Duration,
) -> Result<TlsStream<TcpStream>> {
    let server_name = ServerName::try_from(host.to_string()).context("主机名不合法")?;
    let addr = SocketAddr::new(ip, port);
    let tcp = tokio::time::timeout(timeout, TcpStream::connect(addr))
        .await
        .context("TCP 连接超时")?
        .with_context(|| format!("TCP 连接 {addr} 失败"))?;
    let tls = tokio::time::timeout(timeout, connector.connect(server_name, tcp))
        .await
        .context("TLS 握手超时")?
        .with_context(|| format!("TLS 握手 {host}({addr}) 失败"))?;
    Ok(tls)
}

/// 在已建立的 TLS 流上发一个最小 HTTP/1.1 GET，读取并返回响应体文本。
///
/// 仅支持搜索/抓取所需的常见情况：跟随不了重定向、不解 chunked 之外的编码、
/// 假定响应是文本（JSON/HTML）。这足够 Tavily（返回 JSON）与多数抓取场景；
/// 复杂的 HTTP 语义交给后续如需再接 hyper。
pub async fn http_get_over_tls(
    mut tls: TlsStream<TcpStream>,
    host: &str,
    path: &str,
    query: Option<&str>,
    extra_headers: &[String],
    timeout: Duration,
) -> Result<String> {
    let mut target = if path.is_empty() { "/".to_string() } else { path.to_string() };
    if let Some(q) = query {
        target.push('?');
        target.push_str(q);
    }
    let mut request = format!(
        "GET {target} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: elsewhen/0.1\r\nAccept: */*\r\nConnection: close\r\n"
    );
    for h in extra_headers {
        request.push_str(h);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    tokio::time::timeout(timeout, tls.write_all(request.as_bytes()))
        .await
        .context("发送请求超时")?
        .context("发送请求失败")?;

    let raw = read_body_tolerant(&mut tls, timeout).await?;
    parse_http_response(&raw)
}

/// 在已建立的 TLS 流上发一个最小 HTTP/1.1 GET，读取并返回响应体原始字节。
/// 图片等二进制资源下载用（响应不是文本，故返回字节）。
pub async fn http_get_bytes_over_tls(
    mut tls: TlsStream<TcpStream>,
    host: &str,
    path: &str,
    query: Option<&str>,
    extra_headers: &[String],
    timeout: Duration,
) -> Result<Vec<u8>> {
    let mut target = if path.is_empty() { "/".to_string() } else { path.to_string() };
    if let Some(q) = query {
        target.push('?');
        target.push_str(q);
    }
    let mut request = format!(
        "GET {target} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: elsewhen/0.1\r\nAccept: */*\r\nConnection: close\r\n"
    );
    for h in extra_headers {
        request.push_str(h);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    tokio::time::timeout(timeout, tls.write_all(request.as_bytes()))
        .await
        .context("发送请求超时")?
        .context("发送请求失败")?;
    let raw = read_body_tolerant(&mut tls, timeout).await?;
    parse_http_response_bytes(&raw)
}

/// 在已建立的 TLS 流上发一个最小 HTTP/1.1 POST，读取并返回响应体原始字节。
/// DoH 的 wire-format 查询用（响应是二进制 dns-message，故返回字节而非文本）。
pub async fn http_post_over_tls(
    tls: TlsStream<TcpStream>,
    host: &str,
    path: &str,
    content_type: &str,
    body: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>> {
    http_post(tls, host, path, content_type, body, &[], "application/dns-message", timeout)
        .await
}

/// 带自定义 header 的 HTTP/1.1 POST，返回响应体 UTF-8 文本（搜索 API 返回 JSON 用）。
/// `extra_headers` 为额外表头（如 `Authorization: Bearer xxx`），每项是完整的一行。
pub async fn http_post_text_over_tls(
    tls: TlsStream<TcpStream>,
    host: &str,
    path: &str,
    content_type: &str,
    body: &[u8],
    extra_headers: &[String],
    timeout: Duration,
) -> Result<String> {
    let bytes = http_post(tls, host, path, content_type, body, extra_headers, "application/json", timeout)
        .await?;
    String::from_utf8(bytes).context("响应体不是 UTF-8 文本")
}

/// POST 核心：构造请求、发出、读回、拆 header/body、解 chunked，返回 body 字节。
async fn http_post(
    mut tls: TlsStream<TcpStream>,
    host: &str,
    path: &str,
    content_type: &str,
    body: &[u8],
    extra_headers: &[String],
    accept: &str,
    timeout: Duration,
) -> Result<Vec<u8>> {
    let mut request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: elsewhen/0.1\r\nAccept: {accept}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for h in extra_headers {
        request.push_str(h);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    tokio::time::timeout(timeout, tls.write_all(request.as_bytes()))
        .await
        .context("发送请求超时")?
        .context("发送请求失败")?;
    tokio::time::timeout(timeout, tls.write_all(body))
        .await
        .context("发送请求体超时")?
        .context("发送请求体失败")?;
    let raw = read_body_tolerant(&mut tls, timeout).await?;
    parse_http_response_bytes(&raw)
}

/// 读完整响应体，容忍「对端未发 close_notify 就关连接」的 EOF。
///
/// 不少 HTTP/1.1 服务器（尤其 CDN 与部分 API）在发完响应后直接关 TCP 而不发 TLS
/// close_notify；rustls 会把它报成 `UnexpectedEof`，但数据其实已读全。我们发的是
/// `Connection: close`，对端这样关闭是合法的，故把该 EOF 视为正常结束。
async fn read_body_tolerant(
    tls: &mut TlsStream<TcpStream>,
    timeout: Duration,
) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 16384];
    loop {
        match tokio::time::timeout(timeout, tls.read(&mut buf)).await {
            Err(_) => return Err(anyhow!("读取响应超时")),
            Ok(Ok(0)) => break, // 正常 EOF
            Ok(Ok(n)) => raw.extend_from_slice(&buf[..n]),
            Ok(Err(e)) => {
                // 无 close_notify 的关闭：已读到数据就接受，否则才是错误。
                if e.kind() == std::io::ErrorKind::UnexpectedEof && !raw.is_empty() {
                    break;
                }
                return Err(e).context("读取响应失败");
            }
        }
    }
    Ok(raw)
}

/// 解析最小 HTTP/1.1 响应为原始字节（供 DoH 二进制响应用）。
fn parse_http_response_bytes(raw: &[u8]) -> Result<Vec<u8>> {
    let header_end = find_subsequence(raw, b"\r\n\r\n")
        .ok_or_else(|| anyhow!("响应缺少 header 结束标记"))?;
    let header = String::from_utf8_lossy(&raw[..header_end]).to_string();
    let status = header
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .unwrap_or(0);
    if status >= 400 {
        return Err(anyhow!("HTTP {status}"));
    }
    let body = &raw[header_end + 4..];
    let is_chunked = header.to_ascii_lowercase().contains("transfer-encoding:")
        && header.to_ascii_lowercase().contains("chunked");
    if is_chunked {
        decode_chunked(body)
    } else {
        Ok(body.to_vec())
    }
}

/// 解析最小 HTTP/1.1 响应：拆 header/body，处理 chunked 编码，返回 body 的 UTF-8 文本。
fn parse_http_response(raw: &[u8]) -> Result<String> {
    let header_end = find_subsequence(raw, b"\r\n\r\n")
        .ok_or_else(|| anyhow!("响应缺少 header 结束标记"))?;
    let header = String::from_utf8_lossy(&raw[..header_end]).to_string();
    let status = header
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .unwrap_or(0);
    if status >= 400 {
        return Err(anyhow!("HTTP {status}: {}", &raw[header_end + 4..].iter().take(200).map(|&b| b as char).collect::<String>()));
    }
    let body = &raw[header_end + 4..];
    let is_chunked = header
        .lines()
        .any(|l| l.eq_ignore_ascii_case("transfer-encoding: chunked")
            || l.to_ascii_lowercase().starts_with("transfer-encoding:") && l.to_ascii_lowercase().contains("chunked"));
    let decoded = if is_chunked { decode_chunked(body)? } else { body.to_vec() };
    String::from_utf8(decoded).context("响应体不是 UTF-8 文本")
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// 解 HTTP chunked 传输编码。
fn decode_chunked(mut data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line_end = find_subsequence(data, b"\r\n").ok_or_else(|| anyhow!("chunked 数据损坏"))?;
        let size_str = std::str::from_utf8(&data[..line_end]).context("chunk 大小不是文本")?;
        let size = usize::from_str_radix(size_str.split(';').next().unwrap_or("0").trim(), 16)
            .context("chunk 大小解析失败")?;
        data = &data[line_end + 2..];
        if size == 0 {
            break;
        }
        if data.len() < size + 2 {
            return Err(anyhow!("chunked 数据长度不足"));
        }
        out.extend_from_slice(&data[..size]);
        data = &data[size + 2..]; // 跳过 chunk 末尾的 CRLF
    }
    Ok(out)
}
