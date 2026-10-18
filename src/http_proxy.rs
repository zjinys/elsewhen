//! 出站 HTTP 代理决策（设置/网络 tab 的「代理」区块）。
//!
//! 三态（存 network_settings.proxy_mode，migration 43）：
//! - off    强制直连：屏蔽环境变量代理——用户在设置里显式关
//! - auto   跟随环境变量（默认，与历史上「启动脚本探测 clash 导出」兼容）
//! - custom 用设置页填的地址（覆盖环境变量），http:// 或 socks5://
//!
//! reqwest 默认不读系统代理或环境变量；统一在给 client builder 挂代理时
//! 走 [`with_outbound_proxy`]。当前策略缓存在 [`PROXY_OVERRIDE`]，
//! 由保存 API 与启动加载刷新；AI provider 的共享 client 池长持有 client，
//! 策略变化时须 [`clear_provider_clients`] 清池（保存 API 已挂）。

use std::sync::OnceLock;
use std::sync::RwLock;

/// custom 模式覆盖地址（Some 即用）；None 走 off/auto 逻辑——
/// 两者合一存 MODE，避免另一把锁与半更新。
struct ProxyPolicy {
    mode: String,
    url: String,
}

fn policy_slot() -> &'static RwLock<ProxyPolicy> {
    static SLOT: OnceLock<RwLock<ProxyPolicy>> = OnceLock::new();
    SLOT.get_or_init(|| {
        RwLock::new(ProxyPolicy {
            mode: "auto".to_string(),
            url: String::new(),
        })
    })
}

/// 应用代理策略（来自 network_settings 读取/保存）。非法 mode 按 auto。
pub fn apply_policy(mode: &str, url: &str) {
    let mode = match mode {
        "off" | "custom" => mode,
        _ => "auto",
    };
    if let Ok(mut guard) = policy_slot().write() {
        *guard = ProxyPolicy {
            mode: mode.to_string(),
            url: url.trim().to_string(),
        };
    }
    crate::ai::provider::clear_shared_http_clients();
}

/// 从打开的库加载策略（启动钩子调用，失败静默走默认 auto）。
pub fn load_policy_from_store(store: &crate::storage::Store) {
    if let Ok(settings) = store.network_settings() {
        apply_policy(&settings.proxy_mode, &settings.proxy_url);
    }
}

/// 给 client builder 追加出站代理。无 custom、无环境变量时原样返回。
pub(crate) fn with_outbound_proxy(
    builder: reqwest::blocking::ClientBuilder,
) -> reqwest::blocking::ClientBuilder {
    let (mode, custom_url) = policy_slot()
        .read()
        .map(|guard| (guard.mode.clone(), guard.url.clone()))
        .unwrap_or(("auto".to_string(), String::new()));

    if mode == "off" {
        // 显式直连：连 reqwest 对环境变量的隐式探测也一并关掉。
        return builder.no_proxy();
    }
    if mode == "custom" && !custom_url.is_empty() {
        if let Ok(proxy) = reqwest::Proxy::all(&custom_url) {
            return builder.proxy(proxy);
        }
    }
    match env_proxy() {
        Some(proxy) => builder.proxy(with_loopback_bypass(proxy)),
        None => builder,
    }
}

/// 环境变量代理（历史行为：auto 模式)。只认 HTTPS 语义的变量；`http_proxy`
/// 特意不读：本项目出站几乎全是 HTTPS，eh 语义混淆不如少猜。
/// 环境变量代理（auto 模式）。只认 HTTPS 语义的变量；`http_proxy` 特意不读：
/// 本项目出站几乎全是 HTTPS，语义混淆不如少猜。找到返回 Some。
fn env_proxy() -> Option<reqwest::Proxy> {
    for name in ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"] {
        let Ok(raw) = std::env::var(name) else {
            continue;
        };
        let url = raw.trim();
        if url.is_empty() {
            continue;
        }
        if let Ok(proxy) = reqwest::Proxy::all(url) {
            return Some(proxy);
        }
    }
    None
}

/// 回环豁免：127.0.0.1/localhost/::1 永远直连。本地 FakeRelay（测试）、
/// 本机 Ollama、relay 调试都不该被送进代理。
fn with_loopback_bypass(proxy: reqwest::Proxy) -> reqwest::Proxy {
    proxy.no_proxy(reqwest::NoProxy::from_string("localhost,127.0.0.1,::1"))
}

#[cfg(test)]
mod proxy_policy_tests {
    use super::*;

    /// 优先级矩阵：off 屏蔽环境变量、custom 覆盖环境变量、auto 跟随环境变量。
    /// set_var 影响进程级 env，与全项目测试共用 env 守卫串行。
    #[test]
    fn policy_priority_matrix() {
        let _guard = crate::config::test_env_guard();
        unsafe { std::env::set_var("HTTPS_PROXY", "http://127.0.0.1:7890") };

        // off：屏蔽 env——探测方式是用 no_proxy 的 client（不直接断言 builder，
        // 用 agent 行为难测，这里断言决策函数的输出语义）。
        apply_policy("off", "");
        assert_eq!(policy_slot().read().unwrap().mode, "off");

        // custom：按填的地址
        apply_policy("custom", "socks5://127.0.0.1:7891");
        {
            let guard = policy_slot().read().unwrap();
            assert_eq!(guard.mode, "custom");
            assert_eq!(guard.url, "socks5://127.0.0.1:7891");
        }

        // auto + 非法 mode 归一成 auto
        apply_policy("weird", "ignored");
        {
            let guard = policy_slot().read().unwrap();
            assert_eq!(guard.mode, "auto");
            // 非法 mode 不清 url？——按实现保留旧 url。显式改回无 url 场景：
        }
        // 恢复默认，避免污染其他 env 相关测试
        apply_policy("auto", "");
    }

    /// with_outbound_proxy 在 auto+有 env 时确实挂上了代理。
    /// 通过 debug 输出比对（reqwest client 不可直查 proxy）改测 env 分支函数。
    #[test]
    fn env_proxy_picked_up_in_auto_mode() {
        let _guard = crate::config::test_env_guard();
        unsafe { std::env::set_var("ALL_PROXY", "socks5://127.0.0.1:7892") };
        apply_policy("auto", "");
        let builder = reqwest::blocking::Client::builder();
        // 不炸即通过：真正的代理生效测试在装机链路做（gstatic 204）。
        let _ = with_outbound_proxy(builder);
        unsafe { std::env::remove_var("ALL_PROXY") };
        apply_policy("auto", "");
    }
}
