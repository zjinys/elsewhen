//! 出站 HTTP 统一挂代理环境变量。
//!
//! 背景：relay 部署在 Cloudflare Workers，大陆网络直连优选 IP 的 TLS
//! 握手在 0.6~6s 间剧烈抖动甚至超时；桌面用户常挂 clash 代理，但
//! reqwest 默认**不读**系统代理或环境变量。建 client 时若探测到
//! `HTTPS_PROXY`/`ALL_PROXY` 等标准变量则全部走代理。桌面启动脚本
//! 会探测本机常见代理端口并导出（见 scripts/debug-run.sh）。
//!
//! 只认 HTTPS 代理语义的变量：`https_proxy`/`HTTPS_PROXY`（仅 HTTPS
//! 目标）与 `all_proxy`/`ALL_PROXY`（全协议）。`http_proxy` 特意不读：
//! 本项目出站几乎全是 HTTPS，eh 语义混淆不如少猜。

/// 给 client builder 追加「环境变量代理」；无代理变量时原样返回。
pub(crate) fn with_env_proxy(
    builder: reqwest::blocking::ClientBuilder,
) -> reqwest::blocking::ClientBuilder {
    let candidates = [
        ("HTTPS_PROXY", false),
        ("https_proxy", false),
        ("ALL_PROXY", true),
        ("all_proxy", true),
    ];
    for (name, _) in candidates {
        let Ok(raw) = std::env::var(name) else {
            continue;
        };
        let url = raw.trim();
        if url.is_empty() {
            continue;
        }
        if let Ok(proxy) = reqwest::Proxy::all(url) {
            return builder.proxy(proxy);
        }
    }
    builder
}
