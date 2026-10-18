//! 外网搜索抽象层：引擎无关的结果类型 + 引擎 trait + 多引擎实现 + 统一传输。
//!
//! 设计要点：
//! * **引擎是选项，不是写死**：Tavily / Brave / SearxNG 首版三选，trait + 工厂
//!   （`build_engine`）让后续好加；各引擎只负责「请求长什么样 + 响应怎么解析」，
//!   解析做成纯函数便于单测。
//! * **传输与引擎解耦**：`Transport::fetch` 统一处理「直连（reqwest）vs DoH+ECH
//!   （`net_doh`）」，引擎不再各自背两套网络代码。
//! * SearxNG 无需 API key（自托管），是国内用户低成本起步的选项。

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::time::Duration;

/// 一条搜索结果。
#[derive(Debug, Clone, PartialEq)]
pub struct WebSearchHit {
    pub title: String,
    pub url: String,
    /// 引擎返回的相关性评分（0~1，部分引擎没有）。
    pub score: Option<f64>,
    /// 内容摘要（已按 `SNIPPET_CAP` 截断）。
    pub snippet: String,
}

/// 引擎无关的搜索接口。
pub trait WebSearchEngine: Send + Sync {
    fn search(&self, query: &str, max_results: u32) -> Result<Vec<WebSearchHit>>;
}

/// 摘要单条截断上限（字符）。
const SNIPPET_CAP: usize = 200;

/// 按字符数截断（中文按字算，避免按字节截断破坏 UTF-8）。
fn truncate_chars(s: &str, cap: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= cap {
        return trimmed.to_string();
    }
    let cut: String = trimmed.chars().take(cap).collect();
    format!("{cut}…")
}

// ── 传输层 ─────────────────────────────────────────────────────

/// 一次 HTTP 请求的规格（引擎填好，交给传输层执行）。
pub struct SearchRequest {
    pub url: String,
    pub method: &'static str, // "GET" | "POST"
    /// 额外表头，每项是 `(名, 值)`。
    pub headers: Vec<(String, String)>,
    /// POST 的 JSON 请求体（GET 时为 None）。
    pub json_body: Option<serde_json::Value>,
}

/// 统一传输：直连或 DoH+ECH，对引擎透明。DoH 三态（off/auto/force）在这里落地。
pub enum Transport {
    /// 系统直连（reqwest blocking）。
    Direct { timeout: Duration },
    /// 经 DoH+ECH 隧道。`fallback_direct`：auto 模式下 DoH 失败时是否回退直连。
    Doh {
        doh_uri: String,
        timeout: Duration,
        fallback_direct: bool,
    },
}

impl Transport {
    /// 按 DoH 三态构建传输。off 一律直连；auto/force 在填了地址时走 DoH（auto 允许回退，
    /// force 不回退）；auto/force 但没填地址时也直连（等价 off，不算错误）。
    pub fn new(settings: &crate::storage::NetworkSettings, timeout: Duration) -> Self {
        if settings.should_use_doh() {
            Transport::Doh {
                doh_uri: settings.doh_uri.clone(),
                timeout,
                fallback_direct: settings.allow_direct_fallback(),
            }
        } else {
            Transport::Direct { timeout }
        }
    }

    pub fn fetch(&self, req: &SearchRequest) -> Result<String> {
        match self {
            Transport::Direct { timeout } => fetch_direct(req, *timeout),
            Transport::Doh {
                doh_uri,
                timeout,
                fallback_direct,
            } => match fetch_via_doh(req, doh_uri, *timeout) {
                Ok(body) => Ok(body),
                Err(e) if *fallback_direct => {
                    eprintln!("[web_search] DoH 传输失败，回退直连: {e:#}");
                    fetch_direct(req, *timeout)
                        .with_context(|| format!("DoH 失败且直连回退也失败（DoH 错: {e:#}）"))
                }
                Err(e) => Err(e),
            },
        }
    }
}

fn fetch_direct(req: &SearchRequest, timeout: Duration) -> Result<String> {
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

fn fetch_via_doh(req: &SearchRequest, doh_uri: &str, timeout: Duration) -> Result<String> {
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

/// Tavily（https://tavily.com，面向 LLM 的搜索 API，Bearer 鉴权）。
pub struct TavilyEngine {
    pub api_key: String,
    pub base_url: String,
    pub transport: Transport,
}

#[derive(Deserialize)]
struct TavilyResponse {
    #[serde(default)]
    results: Vec<TavilyResult>,
    #[serde(default)]
    detail: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct TavilyResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    score: Option<f64>,
}

pub fn parse_tavily_response(body: &str, max_results: u32) -> Result<Vec<WebSearchHit>> {
    let resp: TavilyResponse = serde_json::from_str(body).context("Tavily 响应不是合法 JSON")?;
    if let Some(detail) = &resp.detail {
        if resp.results.is_empty() {
            return Err(anyhow!("Tavily 返回错误: {detail}"));
        }
    }
    Ok(resp
        .results
        .into_iter()
        .take(max_results as usize)
        .map(|r| WebSearchHit {
            title: r.title,
            url: r.url,
            score: r.score,
            snippet: truncate_chars(&r.content, SNIPPET_CAP),
        })
        .collect())
}

impl WebSearchEngine for TavilyEngine {
    fn search(&self, query: &str, max_results: u32) -> Result<Vec<WebSearchHit>> {
        if self.api_key.trim().is_empty() {
            return Err(anyhow!("Tavily API key 未配置"));
        }
        let req = SearchRequest {
            url: self.base_url.clone(),
            method: "POST",
            headers: vec![(
                "Authorization".to_string(),
                format!("Bearer {}", self.api_key),
            )],
            json_body: Some(serde_json::json!({
                "query": query,
                "max_results": max_results,
                "search_depth": "basic",
                "include_answer": false,
            })),
        };
        let body = self.transport.fetch(&req)?;
        parse_tavily_response(&body, max_results)
    }
}

// ── Brave Search ───────────────────────────────────────────────

/// Brave Search（https://brave.com/search/api，`X-Subscription-Token` 鉴权）。
pub struct BraveEngine {
    pub api_key: String,
    pub base_url: String,
    pub transport: Transport,
}

#[derive(Deserialize)]
struct BraveResponse {
    #[serde(default)]
    web: Option<BraveWeb>,
}

#[derive(Deserialize)]
struct BraveWeb {
    #[serde(default)]
    results: Vec<BraveResult>,
}

#[derive(Deserialize)]
struct BraveResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
}

pub fn parse_brave_response(body: &str, max_results: u32) -> Result<Vec<WebSearchHit>> {
    let resp: BraveResponse = serde_json::from_str(body).context("Brave 响应不是合法 JSON")?;
    let results = resp.web.map(|w| w.results).unwrap_or_default();
    Ok(results
        .into_iter()
        .take(max_results as usize)
        .map(|r| WebSearchHit {
            title: r.title,
            url: r.url,
            score: None,
            snippet: truncate_chars(&r.description, SNIPPET_CAP),
        })
        .collect())
}

impl WebSearchEngine for BraveEngine {
    fn search(&self, query: &str, max_results: u32) -> Result<Vec<WebSearchHit>> {
        if self.api_key.trim().is_empty() {
            return Err(anyhow!("Brave API key 未配置"));
        }
        let sep = if self.base_url.contains('?') {
            "&"
        } else {
            "?"
        };
        let url = format!(
            "{}{}q={}&count={}",
            self.base_url,
            sep,
            urlencoding_simple(query),
            max_results
        );
        let req = SearchRequest {
            url,
            method: "GET",
            headers: vec![
                ("X-Subscription-Token".to_string(), self.api_key.clone()),
                ("Accept".to_string(), "application/json".to_string()),
            ],
            json_body: None,
        };
        let body = self.transport.fetch(&req)?;
        parse_brave_response(&body, max_results)
    }
}

// ── SearxNG ────────────────────────────────────────────────────

/// SearxNG（自托管元搜索，`format=json`，无需 API key）。
pub struct SearxngEngine {
    pub base_url: String,
    pub transport: Transport,
}

#[derive(Deserialize)]
struct SearxngResponse {
    #[serde(default)]
    results: Vec<SearxngResult>,
}

#[derive(Deserialize)]
struct SearxngResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    score: Option<f64>,
}

pub fn parse_searxng_response(body: &str, max_results: u32) -> Result<Vec<WebSearchHit>> {
    let resp: SearxngResponse = serde_json::from_str(body).context("SearxNG 响应不是合法 JSON")?;
    Ok(resp
        .results
        .into_iter()
        .take(max_results as usize)
        .map(|r| WebSearchHit {
            title: r.title,
            url: r.url,
            score: r.score,
            snippet: truncate_chars(&r.content, SNIPPET_CAP),
        })
        .collect())
}

impl WebSearchEngine for SearxngEngine {
    fn search(&self, query: &str, max_results: u32) -> Result<Vec<WebSearchHit>> {
        if self.base_url.trim().is_empty() {
            return Err(anyhow!("SearxNG 地址未配置"));
        }
        let base = self.base_url.trim_end_matches('/');
        let url = format!("{base}/search?q={}&format=json", urlencoding_simple(query));
        let req = SearchRequest {
            url,
            method: "GET",
            headers: vec![("Accept".to_string(), "application/json".to_string())],
            json_body: None,
        };
        let body = self.transport.fetch(&req)?;
        parse_searxng_response(&body, max_results)
    }
}

// ── 工厂与工具 ─────────────────────────────────────────────────

/// 按引擎标识构建引擎实例（统一注入传输层，DoH 三态由 `settings` 决定）。
/// `engine` 取值：`tavily` | `brave` | `searxng`；未知值报错。
pub fn build_engine(
    engine: &str,
    api_key: &str,
    base_url: &str,
    settings: &crate::storage::NetworkSettings,
    timeout: Duration,
) -> Result<Box<dyn WebSearchEngine>> {
    let transport = Transport::new(settings, timeout);
    match engine {
        "tavily" => Ok(Box::new(TavilyEngine {
            api_key: api_key.to_string(),
            base_url: if base_url.trim().is_empty() {
                "https://api.tavily.com/search".to_string()
            } else {
                base_url.to_string()
            },
            transport,
        })),
        "brave" => Ok(Box::new(BraveEngine {
            api_key: api_key.to_string(),
            base_url: if base_url.trim().is_empty() {
                "https://api.search.brave.com/res/v1/web/search".to_string()
            } else {
                base_url.to_string()
            },
            transport,
        })),
        "searxng" => Ok(Box::new(SearxngEngine {
            base_url: base_url.to_string(),
            transport,
        })),
        other => Err(anyhow!("未知的搜索引擎：{other}")),
    }
}

/// 引擎是否需要 API key（SearxNG 自托管不需要）。
pub fn engine_needs_api_key(engine: &str) -> bool {
    matches!(engine, "tavily" | "brave")
}

/// 最小 URL 编码（查询参数值用：空格与保留字转义，中文按 UTF-8 字节 %XX）。
fn urlencoding_simple(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 用户文本是否显式表达了「需要外网搜索」的意图。
///
/// 设计原则（与你的需求讨论一致）：触发来自**用户输入的明确信号**，不靠模型自评
/// 「我不知道」——后者不可靠。三类信号：时效词（最新/最近/新闻/热点）、直接指令
/// （搜一下/网上查/查一下）、明确要外网搜索。拿不准时由模型反问用户是否要搜（见
/// system prompt），而不是自行断定。
pub fn user_text_wants_search(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    // 否定词优先：用户明确说「别搜/不用查」时不触发，即使句里有「最新」。
    const NEGATIVE: &[&str] = &[
        "别搜",
        "不用搜",
        "不要搜",
        "别查",
        "不用查",
        "不要查",
        "不用上网",
        "别上网",
    ];
    if NEGATIVE.iter().any(|w| t.contains(w)) {
        return false;
    }
    // 直接指令：让 AI 去搜/去查。
    const COMMAND: &[&str] = &[
        "搜一下",
        "搜索一下",
        "搜一搜",
        "上网搜",
        "网上搜",
        "网上查",
        "上网查",
        "查一下",
        "帮我搜",
        "帮我查",
        "去搜",
        "去查",
        "搜索",
        "外网",
        "联网搜",
        "联网查",
    ];
    if COMMAND.iter().any(|w| t.contains(w)) {
        return true;
    }
    // 时效/资讯词：暗示需要当下、模型训练数据之外的信息。
    const TIMELY: &[&str] = &[
        "最新",
        "最近",
        "近期",
        "这几天",
        "这两天",
        "今天",
        "昨日",
        "昨天",
        "本周",
        "这周",
        "本月",
        "新闻",
        "热点",
        "时事",
        "头条",
        "刚发布",
        "刚出",
        "现在怎么样",
        "现状",
    ];
    TIMELY.iter().any(|w| t.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Tavily 解析 ──

    #[test]
    fn parse_tavily_basic() {
        let body = r#"{"results":[
            {"title":"标题一","url":"https://a.com","content":"内容一","score":0.9},
            {"title":"标题二","url":"https://b.com","content":"内容二","score":0.7}
        ]}"#;
        let hits = parse_tavily_response(body, 5).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "标题一");
        assert_eq!(hits[0].score, Some(0.9));
        assert_eq!(hits[1].snippet, "内容二");
    }

    #[test]
    fn parse_tavily_respects_max_results() {
        let body = r#"{"results":[
            {"title":"一","url":"https://a.com","content":"x"},
            {"title":"二","url":"https://b.com","content":"y"},
            {"title":"三","url":"https://c.com","content":"z"}
        ]}"#;
        assert_eq!(parse_tavily_response(body, 2).unwrap().len(), 2);
    }

    #[test]
    fn parse_tavily_error_detail() {
        let body = r#"{"detail":{"error":"Invalid API key"}}"#;
        let err = parse_tavily_response(body, 5).unwrap_err();
        assert!(err.to_string().contains("Tavily 返回错误"), "实际: {err}");
    }

    #[test]
    fn parse_tavily_invalid_json() {
        assert!(
            parse_tavily_response("not json", 5)
                .unwrap_err()
                .to_string()
                .contains("不是合法 JSON")
        );
    }

    #[test]
    fn snippet_truncated_by_chars() {
        let long = "字".repeat(500);
        let body =
            format!(r#"{{"results":[{{"title":"t","url":"https://a.com","content":"{long}"}}]}}"#);
        let hits = parse_tavily_response(&body, 5).unwrap();
        assert!(hits[0].snippet.chars().count() <= 201);
        assert!(hits[0].snippet.ends_with('…'));
    }

    #[test]
    fn snippet_short_not_truncated() {
        let body = r#"{"results":[{"title":"t","url":"https://a.com","content":"短内容"}]}"#;
        let hits = parse_tavily_response(body, 5).unwrap();
        assert_eq!(hits[0].snippet, "短内容");
        assert!(!hits[0].snippet.ends_with('…'));
    }

    // ── Brave 解析 ──

    #[test]
    fn parse_brave_basic() {
        let body = r#"{"web":{"results":[
            {"title":"勇敢标题","url":"https://a.com","description":"描述一"},
            {"title":"标题二","url":"https://b.com","description":"描述二"}
        ]}}"#;
        let hits = parse_brave_response(body, 5).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "勇敢标题");
        assert_eq!(hits[0].snippet, "描述一");
        assert_eq!(hits[0].score, None);
    }

    #[test]
    fn parse_brave_empty_web() {
        // 无 web 字段（部分查询 Brave 不返回 web 段）
        let hits = parse_brave_response(r#"{}"#, 5).unwrap();
        assert!(hits.is_empty());
    }

    // ── SearxNG 解析 ──

    #[test]
    fn parse_searxng_basic() {
        let body = r#"{"results":[
            {"title":"自托管标题","url":"https://a.com","content":"内容","score":3.2}
        ]}"#;
        let hits = parse_searxng_response(body, 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "自托管标题");
        assert_eq!(hits[0].score, Some(3.2));
    }

    // ── 工厂 ──

    /// 直连 settings（doh off，空地址）。
    fn direct_settings() -> crate::storage::NetworkSettings {
        crate::storage::NetworkSettings::default()
    }

    /// 走 DoH 的 settings（force 模式 + 指定地址）。
    fn doh_settings(doh_uri: &str) -> crate::storage::NetworkSettings {
        crate::storage::NetworkSettings {
            doh_uri: doh_uri.to_string(),
            doh_scope: "search_only".to_string(),
            doh_mode: crate::storage::DohMode::Force,
            ..crate::storage::NetworkSettings::default()
        }
    }

    #[test]
    fn build_engine_dispatch() {
        let s = direct_settings();
        assert!(build_engine("tavily", "k", "", &s, Duration::from_secs(5)).is_ok());
        assert!(build_engine("brave", "k", "", &s, Duration::from_secs(5)).is_ok());
        assert!(
            build_engine(
                "searxng",
                "",
                "http://localhost:8080",
                &s,
                Duration::from_secs(5)
            )
            .is_ok()
        );
        assert!(build_engine("unknown", "", "", &s, Duration::from_secs(5)).is_err());
    }

    #[test]
    fn build_engine_defaults() {
        let s = direct_settings();
        let t = build_engine("tavily", "k", "", &s, Duration::from_secs(5)).unwrap();
        // 空 base_url 用引擎默认端点（通过 search 时 key 校验前的行为无法直接看 URL，
        // 这里只断言构建成功且类型分派正确——见 dispatch 测试）。
        drop(t);
        assert!(engine_needs_api_key("tavily"));
        assert!(engine_needs_api_key("brave"));
        assert!(!engine_needs_api_key("searxng"));
    }

    // ── 触发信号 ──

    #[test]
    fn wants_search_command() {
        assert!(user_text_wants_search("帮我搜一下 elsewhen 是什么"));
        assert!(user_text_wants_search("上网查一下天气"));
        assert!(user_text_wants_search("搜索最新的新闻"));
    }

    #[test]
    fn wants_search_timely() {
        assert!(user_text_wants_search("最近有什么新闻"));
        assert!(user_text_wants_search("今天的热点事件"));
        assert!(user_text_wants_search("这几天俄罗斯出了什么事"));
    }

    #[test]
    fn no_search_for_timeless() {
        assert!(!user_text_wants_search("帮我记一下明天开会"));
        assert!(!user_text_wants_search("Rust 的所有权是什么"));
        assert!(!user_text_wants_search(""));
    }

    #[test]
    fn negative_overrides_timely() {
        assert!(!user_text_wants_search("不用搜，最新的我知道"));
        assert!(!user_text_wants_search("别查最近的新闻了"));
    }

    #[test]
    fn urlencoding_works() {
        assert_eq!(urlencoding_simple("hello world"), "hello%20world");
        assert_eq!(urlencoding_simple("a+b=c"), "a%2Bb%3Dc");
        // 中文按 UTF-8 字节编码
        assert!(urlencoding_simple("搜索").starts_with('%'));
    }

    // ── DoH 三态决策 ──

    fn settings_with(mode: crate::storage::DohMode, uri: &str) -> crate::storage::NetworkSettings {
        crate::storage::NetworkSettings {
            doh_uri: uri.to_string(),
            doh_scope: "search_only".to_string(),
            doh_mode: mode,
            ..crate::storage::NetworkSettings::default()
        }
    }

    #[test]
    fn should_use_doh_respects_mode_and_uri() {
        use crate::storage::DohMode;
        // off 一律不走 DoH，哪怕填了地址
        assert!(!settings_with(DohMode::Off, "https://d/").should_use_doh());
        // auto/force 填了地址才走
        assert!(settings_with(DohMode::Auto, "https://d/").should_use_doh());
        assert!(settings_with(DohMode::Force, "https://d/").should_use_doh());
        // auto/force 空地址 = 直连（等价 off）
        assert!(!settings_with(DohMode::Auto, "").should_use_doh());
        assert!(!settings_with(DohMode::Force, "  ").should_use_doh());
    }

    #[test]
    fn fallback_only_in_auto() {
        use crate::storage::DohMode;
        assert!(settings_with(DohMode::Auto, "https://d/").allow_direct_fallback());
        assert!(!settings_with(DohMode::Force, "https://d/").allow_direct_fallback());
        assert!(!settings_with(DohMode::Off, "").allow_direct_fallback());
    }

    #[test]
    fn transport_matches_mode() {
        use crate::storage::DohMode;
        // off → Direct
        assert!(matches!(
            Transport::new(
                &settings_with(DohMode::Off, "https://d/"),
                Duration::from_secs(5)
            ),
            Transport::Direct { .. }
        ));
        // force + 地址 → Doh 且不回退
        match Transport::new(
            &settings_with(DohMode::Force, "https://d/"),
            Duration::from_secs(5),
        ) {
            Transport::Doh {
                fallback_direct, ..
            } => assert!(!fallback_direct),
            _ => panic!("force+地址应走 DoH"),
        }
        // auto + 地址 → Doh 且回退
        match Transport::new(
            &settings_with(DohMode::Auto, "https://d/"),
            Duration::from_secs(5),
        ) {
            Transport::Doh {
                fallback_direct, ..
            } => assert!(fallback_direct),
            _ => panic!("auto+地址应走 DoH"),
        }
        // auto + 空地址 → Direct
        assert!(matches!(
            Transport::new(&settings_with(DohMode::Auto, ""), Duration::from_secs(5)),
            Transport::Direct { .. }
        ));
    }

    /// 实网验证：经用户自建 DoH+ECH 传输到达 Tavily API（用一个无效 key，预期拿到
    /// Tavily 的鉴权错误响应——这证明隧道 + 请求格式都对，只差有效 key）。手动跑：
    ///   ELSEWHEN_TEST_DOH=https://your-doh/ cargo test --release web_search -- --ignored --nocapture
    /// 设了真实 TAVILY_API_KEY 时会真正检索并打印命中。
    #[test]
    #[ignore]
    fn tavily_via_doh_real_network() {
        let doh =
            std::env::var("ELSEWHEN_TEST_DOH").expect("设置 ELSEWHEN_TEST_DOH 为你的 DoH 服务地址");
        let key = std::env::var("TAVILY_API_KEY")
            .unwrap_or_else(|_| "invalid-key-for-transport-test".to_string());
        let engine = build_engine(
            "tavily",
            &key,
            "https://api.tavily.com/search",
            &doh_settings(&doh),
            Duration::from_secs(20),
        )
        .unwrap();
        match engine.search("elsewhen 是什么", 3) {
            Ok(hits) => {
                println!("检索成功，{} 条命中", hits.len());
                for h in &hits {
                    println!("- {} | {}", h.title, h.url);
                }
                assert!(!hits.is_empty());
            }
            Err(e) => {
                let msg = format!("{e:#}");
                println!("返回错误（无效 key 时预期是鉴权错误）: {msg}");
                if key == "invalid-key-for-transport-test" {
                    assert!(
                        msg.contains("Tavily")
                            || msg.contains("key")
                            || msg.contains("401")
                            || msg.contains("HTTP"),
                        "无效 key 时应得到 Tavily 的鉴权错误而非传输错误，实际: {msg}"
                    );
                } else {
                    panic!("真实 key 下不应失败: {msg}");
                }
            }
        }
    }
}
