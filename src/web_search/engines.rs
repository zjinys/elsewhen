//! 三个搜索引擎实现：Tavily / Brave / SearxNG 的请求与响应解析及工厂。

use super::*;

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
