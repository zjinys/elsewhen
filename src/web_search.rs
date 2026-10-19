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
    ///
    /// 例外：出站代理为 custom（用户显式挂代理）时 DoH 一律让位——「挂了代理就
    /// 全走代理」，两条隧道不叠用；代理挂了也别指望 DoH 救场，那不是它的职责。
    pub fn new(settings: &crate::storage::NetworkSettings, timeout: Duration) -> Self {
        let proxy_mode_custom = settings.proxy_mode == "custom";
        if settings.should_use_doh() && !proxy_mode_custom {
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

mod engines;
mod fetch;
mod intent;

pub use engines::{BraveEngine, SearxngEngine, TavilyEngine, build_engine, engine_needs_api_key};
#[cfg(test)]
pub(crate) use engines::{parse_brave_response, parse_searxng_response, parse_tavily_response};
pub(crate) use fetch::{fetch_direct, fetch_via_doh};
pub(crate) use intent::urlencoding_simple;
pub use intent::user_text_wants_search;

#[cfg(test)]
mod tests;
