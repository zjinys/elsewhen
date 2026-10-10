//! DoH 解析器：手工构造 wire-format DNS 查询，经本模块的 TLS 连接 POST 给自定义
//! DoH 服务，手工解析响应——A 记录取干净 IP、HTTPS 记录（type 65）取 ECH 配置。
//!
//! 为什么不用 hickory-resolver：hickory 0.24 内部锁定 rustls 0.21 / tokio-rustls 0.24 /
//! `http 0.2` 一套旧依赖，实测连用户自建 DoH 服务时返回「invalid data」（其 TLS/HTTP
//! 层与该服务的握手不兼容）。改用本模块自己的 `tls_crypto` TLS 连接后完全可控，
//! 且与 ECH 连接复用同一套 TLS 指纹处理，更内聚。

use super::tls_crypto;
use anyhow::{Context, Result, anyhow};
use rustls::pki_types::EchConfigListBytes;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

/// 一次域名解析的结果：干净 IP 列表 + 可选的 ECH 配置。
pub struct DohLookup {
    pub ips: Vec<IpAddr>,
    pub ech_config: Option<EchConfigListBytes<'static>>,
}

const RT_A: u16 = 1;
const RT_AAAA: u16 = 28;
const RT_HTTPS: u16 = 65;

/// 基于自定义 DoH 服务的解析器。
pub struct DohResolver {
    /// DoH 服务器主机名（SNI / Host）。
    host: String,
    /// DoH 服务器 IP（系统 DNS 引导解析）。
    ips: Vec<IpAddr>,
    port: u16,
    /// DoH 查询路径（标准 /dns-query）。
    path: String,
    timeout: Duration,
}

impl DohResolver {
    /// `doh_uri` 形如 `https://doh.example.com/dns-query`。路径会被规范化为 `/dns-query`
    /// （标准 DoH），非标准路径仅作警告——实测自建 DoH 在标准路径上同样响应。
    pub fn new(doh_uri: &str, timeout: Duration) -> Result<Self> {
        let uri: http::Uri = doh_uri.parse().context("DoH 地址不是合法 URL")?;
        let host = uri
            .host()
            .ok_or_else(|| anyhow!("DoH 地址缺少主机名"))?
            .to_string();
        let port = uri.port_u16().unwrap_or(443);
        let path = uri.path().to_string();
        if !path.is_empty() && path != "/dns-query" {
            eprintln!("[net_doh] DoH 路径 '{path}' 非标准，将使用 /dns-query");
        }
        let ips: Vec<IpAddr> = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port))
            .context("解析 DoH 服务器地址失败")?
            .map(|a| a.ip())
            .collect();
        if ips.is_empty() {
            return Err(anyhow!("DoH 服务器 {host} 解析不到 IP"));
        }
        Ok(Self {
            host,
            ips,
            port,
            path: "/dns-query".to_string(),
            timeout,
        })
    }

    /// 解析域名：先取 A 记录的干净 IP，再尽力取 HTTPS 记录里的 ECH 配置。
    /// ECH 取不到不视为失败（很多域名本就没有），只降级为普通 TLS。
    pub async fn lookup(&self, domain: &str) -> Result<DohLookup> {
        let ips = self.query_ips(domain).await?;
        let ech_config = self.query_ech(domain).await;
        Ok(DohLookup { ips, ech_config })
    }

    async fn query_ips(&self, domain: &str) -> Result<Vec<IpAddr>> {
        let mut ips = self
            .query(domain, RT_A)
            .await?
            .into_iter()
            .filter_map(|r| match r {
                RData::A(ip) => Some(IpAddr::V4(ip)),
                _ => None,
            })
            .collect::<Vec<_>>();
        // AAAA 尽力而为：失败/为空不影响（IPv4 已够）。
        if let Ok(v6) = self.query(domain, RT_AAAA).await {
            ips.extend(v6.into_iter().filter_map(|r| match r {
                RData::AAAA(ip) => Some(IpAddr::V6(ip)),
                _ => None,
            }));
        }
        if ips.is_empty() {
            return Err(anyhow!("{domain} 解析结果为空"));
        }
        Ok(ips)
    }

    async fn query_ech(&self, domain: &str) -> Option<EchConfigListBytes<'static>> {
        let records = self.query(domain, RT_HTTPS).await.ok()?;
        for r in records {
            if let RData::Https(ech) = r {
                return Some(EchConfigListBytes::from(ensure_ech_list_len_prefix(ech)));
            }
        }
        None
    }

    /// 发一次 DoH 查询并解析响应记录。
    async fn query(&self, domain: &str, rtype: u16) -> Result<Vec<RData>> {
        let wire = build_query(domain, rtype);
        let connector = tls_crypto::build_connector(None)?; // 连 DoH 服务器本身不用 ECH
        let mut last_err: Option<anyhow::Error> = None;
        for ip in &self.ips {
            match self.query_via(&connector, *ip, &wire).await {
                Ok(records) => return Ok(records),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("DoH 服务器无可用 IP")))
    }

    async fn query_via(
        &self,
        connector: &tokio_rustls::TlsConnector,
        ip: IpAddr,
        wire: &[u8],
    ) -> Result<Vec<RData>> {
        let tls =
            tls_crypto::connect_tls(connector, &self.host, ip, self.port, self.timeout).await?;
        let body = tls_crypto::http_post_over_tls(
            tls,
            &self.host,
            &self.path,
            "application/dns-message",
            wire,
            self.timeout,
        )
        .await?;
        parse_response(wire, &body)
    }
}

/// 解析出的记录数据（只保留 elsewhen 需要的）。
enum RData {
    A(Ipv4Addr),
    AAAA(Ipv6Addr),
    /// HTTPS 记录里提取的 ECH 配置字节。
    Https(Vec<u8>),
}

/// 构造标准 DNS 查询（单问题、RD 置位）。
fn build_query(domain: &str, rtype: u16) -> Vec<u8> {
    let mut q = Vec::new();
    let id: u16 = 0x1a2b;
    q.extend_from_slice(&id.to_be_bytes()); // ID
    q.extend_from_slice(&0x0100u16.to_be_bytes()); // flags: RD
    q.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    q.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
    q.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    q.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
    for label in domain.trim_end_matches('.').split('.') {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.push(0); // 域名结束
    q.extend_from_slice(&rtype.to_be_bytes()); // QTYPE
    q.extend_from_slice(&1u16.to_be_bytes()); // QCLASS IN
    q
}

/// 解析 DNS 响应，返回 answer 段的记录。
fn parse_response(query: &[u8], resp: &[u8]) -> Result<Vec<RData>> {
    if resp.len() < 12 {
        return Err(anyhow!("DNS 响应过短"));
    }
    if resp[0] != query[0] || resp[1] != query[1] {
        return Err(anyhow!("DNS 响应 ID 不匹配"));
    }
    let ancount = u16::from_be_bytes([resp[6], resp[7]]) as usize;
    let mut pos = 12;
    skip_question(resp, &mut pos)?;
    let mut out = Vec::new();
    for _ in 0..ancount {
        skip_name(resp, &mut pos)?;
        if pos + 10 > resp.len() {
            return Err(anyhow!("DNS 记录截断"));
        }
        let rtype = u16::from_be_bytes([resp[pos], resp[pos + 1]]);
        let rdlen = u16::from_be_bytes([resp[pos + 8], resp[pos + 9]]) as usize;
        pos += 10;
        if pos + rdlen > resp.len() {
            return Err(anyhow!("DNS rdata 截断"));
        }
        let rdata = &resp[pos..pos + rdlen];
        match rtype {
            RT_A if rdlen == 4 => out.push(RData::A(Ipv4Addr::new(
                rdata[0], rdata[1], rdata[2], rdata[3],
            ))),
            RT_AAAA if rdlen == 16 => {
                let mut b = [0u8; 16];
                b.copy_from_slice(rdata);
                out.push(RData::AAAA(Ipv6Addr::from(b)));
            }
            RT_HTTPS => {
                if let Some(ech) = extract_ech_from_https(resp, pos, rdlen) {
                    out.push(RData::Https(ech));
                }
            }
            _ => {}
        }
        pos += rdlen;
    }
    Ok(out)
}

/// 跳过 question 段（一个 qname + 4 字节）。
fn skip_question(resp: &[u8], pos: &mut usize) -> Result<()> {
    skip_name(resp, pos)?;
    *pos += 4; // QTYPE + QCLASS
    if *pos > resp.len() {
        return Err(anyhow!("DNS question 截断"));
    }
    Ok(())
}

/// 跳过一个域名（处理 0xC0 压缩指针）。
fn skip_name(resp: &[u8], pos: &mut usize) -> Result<()> {
    loop {
        if *pos >= resp.len() {
            return Err(anyhow!("DNS name 截断"));
        }
        let len = resp[*pos];
        if len & 0xC0 == 0xC0 {
            *pos += 2; // 压缩指针占 2 字节
            return Ok(());
        }
        if len == 0 {
            *pos += 1;
            return Ok(());
        }
        *pos += 1 + len as usize;
    }
}

/// 从 HTTPS（SVCB）记录的 rdata 中提取 ECH 配置（SvcParamKey=5）。
fn extract_ech_from_https(resp: &[u8], rdata_start: usize, rdlen: usize) -> Option<Vec<u8>> {
    let rdata = &resp[rdata_start..rdata_start + rdlen];
    if rdata.len() < 3 {
        return None;
    }
    // 前 2 字节 priority，随后 target name（可能含压缩指针，逐个 label 跳过）
    let mut i = 2;
    loop {
        if i >= rdata.len() {
            return None;
        }
        let len = rdata[i];
        if len & 0xC0 == 0xC0 {
            i += 2;
            break;
        }
        if len == 0 {
            i += 1;
            break;
        }
        i += 1 + len as usize;
    }
    // 之后是 SvcParam 列表：(key u16, len u16, value)
    while i + 4 <= rdata.len() {
        let key = u16::from_be_bytes([rdata[i], rdata[i + 1]]);
        let vlen = u16::from_be_bytes([rdata[i + 2], rdata[i + 3]]) as usize;
        i += 4;
        if i + vlen > rdata.len() {
            return None;
        }
        if key == 5 {
            // SvcParamKey::EchConfig
            return Some(rdata[i..i + vlen].to_vec());
        }
        i += vlen;
    }
    None
}

/// 补齐 EchConfigList 的 2 字节总长度前缀（rustls 要求）。
fn ensure_ech_list_len_prefix(bytes: Vec<u8>) -> Vec<u8> {
    if bytes.len() >= 2 {
        let declared = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
        if declared == bytes.len().saturating_sub(2) {
            return bytes;
        }
    }
    if bytes.len() > u16::MAX as usize {
        return bytes;
    }
    let mut prefixed = Vec::with_capacity(bytes.len() + 2);
    prefixed.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    prefixed.extend_from_slice(&bytes);
    prefixed
}
