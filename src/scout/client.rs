//! Scout 情报服务消费端（提案 2026-11-15 §4）。
//!
//! scout 是独立采集服务（CF Workers + D1，`scout-cf/`）；本模块把它的
//! findings 镜像进本地库，供 UI（情报三页）与今天栏卡片消费。服务端是
//! 权威数据源，本地镜像可随时清空重建；已读态（read_at）归本地管。
//!
//! 网络纪律照 `sync::transport_relay::RelayClient`：HTTPS 强制（私网可
//! HTTP）、禁凭据/查询串进 base url、禁重定向、30s 超时、响应上限、
//! 走 [`crate::http_proxy::with_outbound_proxy`] 的统一出站代理策略。

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug)]
pub(crate) struct LegacyScout;
impl std::fmt::Display for LegacyScout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("scout_upgrade_required")
    }
}
impl std::error::Error for LegacyScout {}

/// scout API 的 finding 原始形状（/findings 响应行）。
/// payload 是随源变化的 JSON 对象，客户端不建模，序列化成字符串上交。
#[derive(Deserialize)]
struct RawScoutFinding {
    id: i64,
    source_id: String,
    snapshot_id: i64,
    kind: String,
    item_key: String,
    payload: serde_json::Value,
    /// 仅 changed 有：字段级 from/to/diff。服务端 diff.ts 计算。
    #[serde(default)]
    delta: Option<serde_json::Value>,
    detected_at: String,
}

/// 解码后的 finding（payload_json 已定形，进镜像层）。
#[derive(Clone, Debug)]
pub struct ScoutFinding {
    pub id: i64,
    pub source_id: String,
    pub snapshot_id: i64,
    pub kind: String,
    pub item_key: String,
    pub payload_json: String,
    /// 字段级变化（仅 changed），已定形为 JSON 字符串；None = 无变化明细。
    pub delta_json: Option<String>,
    pub detected_at: String,
}

/// scout API 的源状态形状（/sources 响应，镜像展示用，不落库）。
#[derive(Clone, Debug, Deserialize)]
pub struct ScoutSource {
    pub id: String,
    pub name: String,
    pub interval_minutes: i64,
    pub enabled: i64,
    #[serde(default)]
    pub last_status: Option<String>,
    #[serde(default)]
    pub last_run_at: Option<String>,
}

#[derive(Deserialize)]
pub struct FindingPage {
    pub version: i64,
    pub instance_id: String,
    pub upper: i64,
    pub next: i64,
    pub has_more: bool,
    items: Vec<RawScoutFinding>,
}
impl FindingPage {
    pub fn findings(&self) -> Result<Vec<ScoutFinding>> {
        self.items
            .iter()
            .map(|f| {
                Ok(ScoutFinding {
                    id: f.id,
                    source_id: f.source_id.clone(),
                    snapshot_id: f.snapshot_id,
                    kind: f.kind.clone(),
                    item_key: f.item_key.clone(),
                    payload_json: serde_json::to_string(&f.payload)?,
                    delta_json: f.delta.as_ref().map(serde_json::to_string).transpose()?,
                    detected_at: f.detected_at.clone(),
                })
            })
            .collect()
    }
    pub fn validate(&self, after: i64, upper: Option<i64>) -> Result<()> {
        anyhow::ensure!(
            self.version == 1 && !self.instance_id.is_empty(),
            "unsupported_scout_protocol"
        );
        anyhow::ensure!(
            self.upper >= after && upper.is_none_or(|u| u == self.upper),
            "invalid_scout_upper"
        );
        let mut previous = after;
        for item in &self.items {
            anyhow::ensure!(
                item.id > previous && item.id <= self.upper,
                "invalid_scout_page_order"
            );
            previous = item.id;
        }
        anyhow::ensure!(
            self.next == previous || (!self.has_more && self.next == self.upper),
            "invalid_scout_next"
        );
        anyhow::ensure!(
            self.has_more == (self.next < self.upper) && (!self.has_more || self.next > after),
            "invalid_scout_progress"
        );
        Ok(())
    }
}

pub struct ScoutClient {
    base_url: String,
    http: reqwest::blocking::Client,
    /// IPv6 不通时的兑底客户端（仅 IPv4，见 [`Self::get_json_with_fallback`]）。
    http_v4: Option<reqwest::blocking::Client>,
}

/// 单次拉取的响应上限。/findings 默认 limit=200、封顶 1000 条；
/// 一条 finding payload 是单条商品/仓库摘要（几 KB），8MB 足够宽。
const MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;

impl ScoutClient {
    pub fn new(base_url: &str) -> Result<Self> {
        let url = reqwest::Url::parse(base_url).context("parse scout URL")?;
        let local = url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| match ip {
                        std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private(),
                        std::net::IpAddr::V6(v6) => {
                            v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00
                        }
                    })
        });
        anyhow::ensure!(
            url.scheme() == "https" || (url.scheme() == "http" && local),
            "scout URL 必须使用 HTTPS（仅私网/回环地址允许 HTTP）"
        );
        anyhow::ensure!(
            url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "scout URL 不能包含凭据、查询串或片段（路径由客户端拼装）"
        );
        let build = |local_addresses: bool| {
            crate::http_proxy::with_outbound_proxy(reqwest::blocking::Client::builder())
                .timeout(std::time::Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .local_address(local_addresses.then_some(std::net::Ipv4Addr::UNSPECIFIED.into()))
                .build()
        };
        let http = build(false).context("build scout HTTP client")?;
        // 兑底客户端只在主客户端失败时使用；构建失败不阻断主链路。
        let http_v4 = build(true).ok();
        Ok(Self {
            base_url: url.as_str().trim_end_matches('/').to_string(),
            http,
            http_v4,
        })
    }

    /// 双栈宿主的 IPv6 优先排序（RFC 6724）会让解析出的 IPv6 先试；
    /// 部分网络的 IPv6 路由不通（TLS 握手直接 EOF），而 IPv4 正常。
    /// 主客户端请求失败时用仅 IPv4 的客户端重试一次，对调用方透明。
    /// 走出站代理时不需要兑底（地址选择交给代理端）。
    fn get_json_with_fallback<T: DeserializeOwned>(&self, path: &str, token: &str) -> Result<T> {
        match self.get_json(path, token) {
            Ok(v) => Ok(v),
            Err(primary) => {
                if crate::http_proxy::proxy_in_use() {
                    return Err(primary);
                }
                let Some(v4) = self.http_v4.as_ref() else {
                    return Err(primary);
                };
                let url = format!("{}{}", self.base_url, path);
                let response = v4
                    .get(&url)
                    .header("Authorization", format!("Bearer {token}"))
                    .send()
                    .with_context(|| format!("scout request failed (ipv4 fallback): {path}"))?;
                if response.status().is_redirection() {
                    bail!(
                        "scout URL returned redirect ({}); base URL is wrong",
                        response.status()
                    );
                }
                let status = response.status();
                let bytes = {
                    let mut buf = Vec::new();
                    response
                        .take(MAX_RESPONSE_BYTES)
                        .read_to_end(&mut buf)
                        .context("read scout response")?;
                    if buf.len() as u64 >= MAX_RESPONSE_BYTES {
                        bail!("scout response exceeds {} bytes", MAX_RESPONSE_BYTES);
                    }
                    buf
                };
                if !status.is_success() {
                    if status.as_u16() == 404 && path.starts_with("/v1/") {
                        return Err(LegacyScout.into());
                    }
                    bail!("scout {} returned {}", path, status);
                }
                serde_json::from_slice(&bytes)
                    .with_context(|| format!("decode scout response: {path}"))
            }
        }
    }

    /// GET + Bearer + 响应上限。scout API 全部 GET（除 admin PATCH，
    /// 消费端不用），重定向被禁——workers.dev 也不会跳。
    fn get_json<T: DeserializeOwned>(&self, path: &str, token: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .with_context(|| format!("scout request failed: {path}"))?;
        // 禁重定向下 3xx 视为配置错误（base url 写错到跳转地址）。
        if response.status().is_redirection() {
            bail!(
                "scout URL returned redirect ({}); base URL is wrong",
                response.status()
            );
        }
        let status = response.status();
        let bytes = {
            let mut buf = Vec::new();
            response
                .take(MAX_RESPONSE_BYTES)
                .read_to_end(&mut buf)
                .context("read scout response")?;
            if buf.len() as u64 >= MAX_RESPONSE_BYTES {
                bail!("scout response exceeds {} bytes", MAX_RESPONSE_BYTES);
            }
            buf
        };
        if !status.is_success() {
            if status.as_u16() == 404 && path.starts_with("/v1/") {
                return Err(LegacyScout.into());
            }
            bail!("scout {} returned {}", path, status);
        }
        serde_json::from_slice(&bytes).with_context(|| format!("decode scout response: {path}"))
    }

    /// 这台 scout 的规范化基地址（日志与测试用）。
    pub(crate) fn health_base_url(&self) -> &str {
        &self.base_url
    }

    /// 健康检查（/health 免鉴权）。
    pub fn health(&self) -> Result<bool> {
        #[derive(Deserialize)]
        struct Health {
            #[serde(default)]
            status: String,
        }
        let url = format!("{}/health", self.base_url);
        let response = self.http.get(&url).send().context("scout health request")?;
        if !response.status().is_success() {
            return Ok(false);
        }
        let mut buf = Vec::new();
        response.take(MAX_RESPONSE_BYTES).read_to_end(&mut buf)?;
        let h: Health = serde_json::from_slice(&buf).unwrap_or(Health {
            status: String::new(),
        });
        Ok(h.status == "ok")
    }

    /// 增量拉取 findings。`since` 为 ISO 时间戳时只取 detected_at >= since。
    pub fn fetch_findings(
        &self,
        token: &str,
        since: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ScoutFinding>> {
        let mut path = format!("/findings?limit={}", limit.clamp(1, 1000));
        if let Some(since) = since {
            path.push_str("&since=");
            path.push_str(&crate::web_search::urlencoding_simple(since));
        }
        let raw: Vec<RawScoutFinding> = self.get_json_with_fallback(&path, token)?;
        Ok(raw
            .into_iter()
            .map(|f| ScoutFinding {
                id: f.id,
                source_id: f.source_id,
                snapshot_id: f.snapshot_id,
                kind: f.kind,
                item_key: f.item_key,
                payload_json: serde_json::to_string(&f.payload)
                    .unwrap_or_else(|_| "{}".to_string()),
                delta_json: f.delta.and_then(|d| serde_json::to_string(&d).ok()),
                detected_at: f.detected_at,
            })
            .collect())
    }

    pub fn finding_page(&self, token: &str, after: i64, upper: Option<i64>) -> Result<FindingPage> {
        let path = format!(
            "/v1/findings?after={after}&limit=250{}",
            upper.map(|u| format!("&upper={u}")).unwrap_or_default()
        );
        let page: FindingPage = self.get_json_with_fallback(&path, token)?;
        page.validate(after, upper)?;
        Ok(page)
    }
    pub fn research_get(&self, token: &str, path: &str) -> Result<serde_json::Value> {
        anyhow::ensure!(
            path.starts_with("/v1/products"),
            "invalid_scout_research_path"
        );
        self.get_json_with_fallback(path, token)
    }

    /// 源状态列表（数据源页只读展示）。
    pub fn fetch_sources(&self, token: &str) -> Result<Vec<ScoutSource>> {
        self.get_json_with_fallback("/sources", token)
    }

    /// 单条 finding 详情（含服务端算好的历史轨迹）。
    pub fn fetch_finding_detail(&self, token: &str, id: i64) -> Result<serde_json::Value> {
        self.get_json_with_fallback(&format!("/findings/{id}"), token)
    }
}

use serde::de::DeserializeOwned;
use std::io::Read;
