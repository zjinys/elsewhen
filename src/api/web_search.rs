//! 外网搜索 + DoH 配置 FRB 门面（设置页「外网搜索」区块用）。

use super::*;

/// 外网搜索配置 DTO。
#[derive(Clone, Debug)]
pub struct WebSearchConfigDto {
    pub enabled: bool,
    pub engine: String,
    pub base_url: String,
    /// 明文 API key 只在保存时上行；读取时回传空串，用 has_api_key 判断是否已配置。
    pub api_key: String,
    pub has_api_key: bool,
    pub max_results: i64,
}

/// DoH 网络配置 DTO。
#[derive(Clone, Debug)]
pub struct NetworkSettingsDto {
    pub doh_uri: String,
    pub doh_scope: String,
    /// 启用策略：off | auto | force。
    pub doh_mode: String,
    /// off（强制直连）| auto（跟环境变量，默认）| custom（用 proxy_url）。
    pub proxy_mode: String,
    /// custom 模式的代理地址（http:// 或 socks5://）。
    pub proxy_url: String,
}

/// 读取外网搜索配置（设置页预填用；不回传明文 key）。
pub fn get_web_search_config() -> Result<WebSearchConfigDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let c = store.web_search_config()?;
    Ok(WebSearchConfigDto {
        enabled: c.enabled,
        engine: c.engine,
        base_url: c.base_url,
        api_key: String::new(),
        has_api_key: !c.api_key.trim().is_empty(),
        max_results: c.max_results,
    })
}

/// 保存外网搜索配置。`api_key` 传空串表示「不改动已存的 key」，
/// 只有传非空才覆盖——避免设置页每次保存都把已配置的 key 清掉。
pub fn save_web_search_config(
    enabled: bool,
    engine: String,
    base_url: String,
    api_key: String,
    max_results: i64,
) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let existing = store.web_search_config()?;
    let key = if api_key.trim().is_empty() {
        existing.api_key
    } else {
        api_key
    };
    store.save_web_search_config(enabled, &engine, &base_url, &key, max_results)
}

/// 读取 DoH 网络配置。
pub fn get_network_settings() -> Result<NetworkSettingsDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let s = store.network_settings()?;
    Ok(NetworkSettingsDto {
        doh_uri: s.doh_uri,
        doh_scope: s.doh_scope,
        doh_mode: s.doh_mode.as_str().to_string(),
        proxy_mode: s.proxy_mode,
        proxy_url: s.proxy_url,
    })
}

/// 保存 DoH 服务地址与启用策略。scope 本版固定 search_only。
/// `doh_mode` 取值：off | auto | force（其他值按 auto 处理）。
pub fn save_doh_settings(doh_uri: String, doh_mode: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.save_doh_settings(&doh_uri, crate::storage::DohMode::from_str(&doh_mode))
}

/// 保存出站代理策略并即时生效（清 AI client 池）。`proxy_mode`：off|auto|custom。
pub fn save_network_proxy(proxy_mode: String, proxy_url: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.save_network_proxy(&proxy_mode, proxy_url.trim())?;
    crate::http_proxy::apply_policy(&proxy_mode, proxy_url.trim());
    Ok(())
}

/// 经指定代理（空=直连）访问一个 204 端点，返回可读结果。供「测试」按钮。
pub fn test_proxy_connection(proxy_url: String) -> Result<String> {
    let url = "https://www.gstatic.com/generate_204";
    let mut builder = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none());
    let trimmed = proxy_url.trim();
    if !trimmed.is_empty() {
        builder = builder.proxy(reqwest::Proxy::all(trimmed).context("代理地址格式不合法")?);
    }
    let client = builder.build().context("创建测试客户端失败")?;
    let started = std::time::Instant::now();
    let resp = client
        .get(url)
        .send()
        .context("请求失败（地址不可达或代理拒绝）")?;
    let ms = started.elapsed().as_millis();
    Ok(format!("HTTP {} · {} ms", resp.status().as_u16(), ms))
}

/// 测试 DoH+ECH 连通性：经当前配置的 DoH 访问一个 Cloudflare trace 端点，
/// 返回可读结果（含 SNI 是否加密，即 ECH 是否生效）。供设置页「测试连接」按钮。
pub fn test_doh_connection(doh_uri: String) -> Result<String> {
    if doh_uri.trim().is_empty() {
        anyhow::bail!("请先填写 DoH 服务地址");
    }
    let body = crate::net_doh::https_get_via_doh(
        "https://www.cloudflare.com/cdn-cgi/trace",
        &doh_uri,
        std::time::Duration::from_secs(15),
    )?;
    let sni = body
        .lines()
        .find(|l| l.starts_with("sni="))
        .map(|l| l.trim_start_matches("sni="))
        .unwrap_or("unknown");
    let ip = body
        .lines()
        .find(|l| l.starts_with("ip="))
        .map(|l| l.trim_start_matches("ip="))
        .unwrap_or("unknown");
    let ech_ok = sni == "encrypted";
    Ok(format!(
        "连接成功。出口 IP：{ip}；ECH（SNI 加密）：{}",
        if ech_ok {
            "已生效 ✅"
        } else {
            "未生效（降级普通 TLS）"
        }
    ))
}
