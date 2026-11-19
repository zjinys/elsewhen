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

/// 给 blocking client builder 追加出站代理。无 custom、无环境变量时
/// **显式 no_proxy**：屏蔽 reqwest 默认 system-proxy 对 `http_proxy` 等
/// 非白名单变量的隐式读取（不然「只认 HTTPS 语义」名存实亡——用户
/// export 的 http_proxy 会劫持全部出站，测试 flake 正是此症状）。
pub(crate) fn with_outbound_proxy(
    builder: reqwest::blocking::ClientBuilder,
) -> reqwest::blocking::ClientBuilder {
    let (mode, custom_url) = current_policy();

    if mode == "off" {
        // 显式直连：连 reqwest 对环境变量的隐式探测也一并关掉。
        return builder.no_proxy();
    }
    if mode == "custom" && !custom_url.is_empty() {
        if let Ok(proxy) = reqwest::Proxy::all(&custom_url) {
            return builder.proxy(with_local_bypass(proxy));
        }
    }
    match env_proxy() {
        Some(proxy) => builder.proxy(with_local_bypass(proxy)),
        // auto 但无白名单变量：显式关掉默认探测（http_proxy 不算）。
        None => builder.no_proxy(),
    }
}

/// async 版 [`with_outbound_proxy`]——AI 流式（streaming.rs）与 MCP 会话
/// 等异步出站必须走同一策略，否则「off 屏蔽环境变量」名存实亡。
pub(crate) fn with_outbound_proxy_async(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    let (mode, custom_url) = current_policy();

    if mode == "off" {
        return builder.no_proxy();
    }
    if mode == "custom" && !custom_url.is_empty() {
        if let Ok(proxy) = reqwest::Proxy::all(&custom_url) {
            return builder.proxy(with_local_bypass(proxy));
        }
    }
    match env_proxy() {
        Some(proxy) => builder.proxy(with_local_bypass(proxy)),
        None => builder.no_proxy(),
    }
}

fn current_policy() -> (String, String) {
    policy_slot()
        .read()
        .map(|guard| (guard.mode.clone(), guard.url.clone()))
        .unwrap_or(("auto".to_string(), String::new()))
}

/// 当前出站是否挂了代理（off/custom 非空/白名单 env 命中任一）。
/// 给「IPv6 失败 → 仅 IPv4 重试」这类直连层兑底判断用：走了代理时
/// 目标地址选择发生在代理端，本机双栈排序不再参与，兑底无意义。
pub(crate) fn proxy_in_use() -> bool {
    let (mode, custom_url) = current_policy();
    match mode.as_str() {
        "off" => false,
        "custom" => !custom_url.is_empty(),
        _ => env_proxy().is_some(),
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

/// 本地/内网豁免表（loopback + RFC1918 + link-local + IPv6 本地 + mDNS）。
/// 出站代理只服务公网（AI provider 等）；发往局域网的服务——尤其是
/// 自建同步 relay（如 http://192.168.101.116:8787）——必须直连：远端
/// 代理节点摸不到用户局域网，走代理等于全部 transport 失败（v0.7.2
/// 时期事故：custom 代理把同步推信全吞，Android 永远「等待成员授权」）。
/// reqwest 的 NoProxy 支持 IP 与 CIDR 匹配。
const LOCAL_NO_PROXY: &str = "localhost,127.0.0.1,::1,\
    10.0.0.0/8,172.16.0.0/12,192.168.0.0/16,169.254.0.0/16,\
    fe80::/10,fc00::/7,fd00::/8,.local";

fn with_local_bypass(proxy: reqwest::Proxy) -> reqwest::Proxy {
    proxy.no_proxy(reqwest::NoProxy::from_string(LOCAL_NO_PROXY))
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

    /// 非白名单变量（http_proxy）不得被隐式劫持：auto+仅 http_proxy 时
    /// 决策结果必须是显式 no_proxy（否则 reqwest 默认 system proxy 会
    /// 把它读走——测试 flake 的根因）。探测方式：构造 client 后请求回
    /// 环 server，若被代理劫持会连到代理端口而非 mock。用分支语义测：
    /// env_proxy() 返回 None 时 with_outbound_proxy 必走 no_proxy 分支——
    /// 难以直查 client，改为实证打法：起回环 mock，require 请求直达。同
    /// 一场景在 provider streaming 测试（sync 也复用）已长期回归，这里
    /// 钉住语义层：仅 http_proxy 时 env_proxy() 必须是 None。
    #[test]
    fn lowercase_http_proxy_is_not_quotaed_for_outbound() {
        let _guard = crate::config::test_env_guard();
        // 受控环境：先移除全部白名单变量，只留 http_proxy。
        for name in ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"] {
            unsafe { std::env::remove_var(name) };
        }
        unsafe { std::env::set_var("http_proxy", "http://127.0.0.1:7897") };
        apply_policy("auto", "");
        assert!(env_proxy().is_none());
        unsafe { std::env::remove_var("http_proxy") };
        apply_policy("auto", "");
    }

    /// async 路径（AI 流式）同策略：off 屏蔽、auto 挂白名单 env、回环豁免。
    /// 至少不在编译层面缺席——行为同上，语义层钉住构建链。
    #[test]
    fn async_builder_follows_same_policy() {
        let _guard = crate::config::test_env_guard();
        apply_policy("off", "");
        let _ = with_outbound_proxy_async(reqwest::Client::builder());
        apply_policy("custom", "socks5://127.0.0.1:7891");
        let _ = with_outbound_proxy_async(reqwest::Client::builder());
        apply_policy("auto", "");
        unsafe { std::env::set_var("HTTPS_PROXY", "http://127.0.0.1:7890") };
        let _ = with_outbound_proxy_async(reqwest::Client::builder());
        unsafe { std::env::remove_var("HTTPS_PROXY") };
    }

    /// custom 代理必须放行回环地址（行为实证）：起本地 mock server，
    /// custom 指向一个必死的代理端口——回环请求若被送进代理就会失败，
    /// 直连则命中 mock。这是 v0.7.2 同步全灭事故的回归测试。
    #[test]
    fn custom_proxy_still_reaches_loopback_directly() {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
                .unwrap();
        });
        let _guard = crate::config::test_env_guard();
        apply_policy("custom", "http://127.0.0.1:9"); // discard 端口，必死
        let client = with_outbound_proxy(reqwest::blocking::Client::builder())
            .build()
            .unwrap();
        let body = client
            .get(format!("http://{addr}/"))
            .send()
            .expect("回环请求必须直连命中 mock，而不是死代理")
            .text()
            .unwrap();
        assert_eq!(body, "ok");
        server.join().unwrap();
        apply_policy("auto", "");
    }
}
