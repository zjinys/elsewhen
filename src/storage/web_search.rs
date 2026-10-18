//! 外网搜索与 DoH 网络配置存取（migration 25 的 `web_search_configs` / `network_settings`）。
//!
//! 两张表都是单行（singleton=1）。读取用 `INSERT OR IGNORE` 保证行存在后再 SELECT，
//! 这样「从未配置过」也返回默认值而不是「无行」。

use anyhow::Result;
use rusqlite::params;

use super::Store;

/// 外网搜索配置（对应 `web_search_configs`）。
#[derive(Debug, Clone, PartialEq)]
pub struct WebSearchConfig {
    pub enabled: bool,
    pub engine: String,
    pub base_url: String,
    pub api_key: String,
    pub max_results: i64,
}

/// DoH 启用策略三态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DohMode {
    /// 强制系统直连，忽略已填的 DoH 地址。
    Off,
    /// 优先 DoH+ECH，连不上自动回退直连（默认）。
    Auto,
    /// 只用 DoH+ECH，失败即报错、不降级。
    Force,
}

impl DohMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DohMode::Off => "off",
            DohMode::Auto => "auto",
            DohMode::Force => "force",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "off" => DohMode::Off,
            "force" => DohMode::Force,
            _ => DohMode::Auto,
        }
    }
}

/// DoH 网络配置（对应 `network_settings`）。
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkSettings {
    /// 自定义 DoH 服务地址（与启用策略解耦：地址可留着，由 doh_mode 控制行为）。
    pub doh_uri: String,
    /// DoH 作用域：'search_only' | 'global'（global 本版仅预留）。
    pub doh_scope: String,
    /// 启用策略：off | auto | force。
    pub doh_mode: DohMode,
    /// 出站代理策略：off（强制直连）| auto（跟随环境变量，默认）| custom。
    pub proxy_mode: String,
    /// custom 模式的代理地址：http:// 或 socks5:// 开头。
    pub proxy_url: String,
}

impl NetworkSettings {
    /// 当前是否应尝试走 DoH（off 直接否；auto/force 需已填地址）。
    pub fn should_use_doh(&self) -> bool {
        self.doh_mode != DohMode::Off && !self.doh_uri.trim().is_empty()
    }
    /// DoH 失败后是否允许回退直连（仅 auto 允许；force 与「off 本就不走 DoH」都不回退）。
    pub fn allow_direct_fallback(&self) -> bool {
        self.doh_mode == DohMode::Auto
    }
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: "tavily".to_string(),
            base_url: "https://api.tavily.com/search".to_string(),
            api_key: String::new(),
            max_results: 5,
        }
    }
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            doh_uri: String::new(),
            doh_scope: "search_only".to_string(),
            doh_mode: DohMode::Auto,
            proxy_mode: "auto".to_string(),
            proxy_url: String::new(),
        }
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

impl Store {
    /// 读取外网搜索配置（无行时先补默认行）。
    pub fn web_search_config(&self) -> Result<WebSearchConfig> {
        self.connection.execute(
            "INSERT OR IGNORE INTO web_search_configs(singleton,enabled,engine,base_url,api_key,max_results,created_at,updated_at)
             VALUES(1,0,'tavily','https://api.tavily.com/search','',5,?1,?1)",
            params![now_iso()],
        )?;
        let cfg = self.connection.query_row(
            "SELECT enabled,engine,base_url,api_key,max_results FROM web_search_configs WHERE singleton=1",
            [],
            |r| {
                Ok(WebSearchConfig {
                    enabled: r.get::<_, i64>(0)? != 0,
                    engine: r.get(1)?,
                    base_url: r.get(2)?,
                    api_key: r.get(3)?,
                    max_results: r.get(4)?,
                })
            },
        )?;
        Ok(cfg)
    }

    /// 保存外网搜索配置。
    pub fn save_web_search_config(
        &self,
        enabled: bool,
        engine: &str,
        base_url: &str,
        api_key: &str,
        max_results: i64,
    ) -> Result<()> {
        let now = now_iso();
        self.connection.execute(
            "INSERT INTO web_search_configs(singleton,enabled,engine,base_url,api_key,max_results,created_at,updated_at)
             VALUES(1,?1,?2,?3,?4,?5,?6,?6)
             ON CONFLICT(singleton) DO UPDATE SET enabled=?1,engine=?2,base_url=?3,api_key=?4,max_results=?5,updated_at=?6",
            params![enabled as i64, engine, base_url, api_key, max_results, now],
        )?;
        Ok(())
    }

    /// 读取 DoH 网络配置（无行时先补默认行）。
    pub fn network_settings(&self) -> Result<NetworkSettings> {
        self.connection.execute(
            "INSERT OR IGNORE INTO network_settings(singleton,doh_uri,doh_scope,doh_mode,created_at,updated_at)
             VALUES(1,'','search_only','auto',?1,?1)",
            params![now_iso()],
        )?;
        let s = self.connection.query_row(
            "SELECT doh_uri,doh_scope,doh_mode,proxy_mode,proxy_url FROM network_settings WHERE singleton=1",
            [],
            |r| {
                Ok(NetworkSettings {
                    doh_uri: r.get(0)?,
                    doh_scope: r.get(1)?,
                    doh_mode: DohMode::from_str(&r.get::<_, String>(2)?),
                    proxy_mode: r.get(3)?,
                    proxy_url: r.get(4)?,
                })
            },
        )?;
        Ok(s)
    }

    /// 保存出站代理策略（mode: off|auto|custom）。校验 custom 必带 URL。
    pub fn save_network_proxy(&self, proxy_mode: &str, proxy_url: &str) -> Result<()> {
        let now = now_iso();
        anyhow::ensure!(
            matches!(proxy_mode, "off" | "auto" | "custom"),
            "未知代理策略: {proxy_mode}"
        );
        let url = proxy_url.trim();
        anyhow::ensure!(
            proxy_mode != "custom"
                || (!url.is_empty() && url.starts_with("http://") || url.starts_with("socks5://")),
            "自定义代理地址需以 http:// 或 socks5:// 开头"
        );
        self.connection.execute(
            "UPDATE network_settings SET proxy_mode=?1, proxy_url=?2, updated_at=?3 WHERE singleton=1",
            params![proxy_mode, url, now],
        )?;
        Ok(())
    }

    /// 保存 DoH 服务地址与启用策略（scope 本版固定 search_only，不在这里改）。
    pub fn save_doh_settings(&self, doh_uri: &str, doh_mode: DohMode) -> Result<()> {
        let now = now_iso();
        self.connection.execute(
            "INSERT INTO network_settings(singleton,doh_uri,doh_scope,doh_mode,created_at,updated_at)
             VALUES(1,?1,'search_only',?2,?3,?3)
             ON CONFLICT(singleton) DO UPDATE SET doh_uri=?1,doh_mode=?2,updated_at=?3",
            params![doh_uri, doh_mode.as_str(), now],
        )?;
        Ok(())
    }
}
