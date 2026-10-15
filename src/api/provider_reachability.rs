//! Provider base_url 可达性提示。
//!
//! 设计约束(路线图 119 行):
//! - **只提示,不阻止**:环回/不可达地址可能是用户有意为之(本机代理、
//!   内网服务、测试),保存仍被允许;提示码交给 UI 展示,由用户决定。
//! - **绝不静默改 URL**:本模块返回提示,从不返回「修正后的」地址;
//!   不切换未经选择的 Provider,不探测外部模型(无网络请求)。
//! - 纯函数、无 I/O,便于隔离测试。

/// 单个地址的可达性提示。
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(unexpected_cfgs)] // frb 宏展开使用 frb_expand cfg；仓库未配置 check-cfg 白名单
#[flutter_rust_bridge::frb(non_opaque)]
pub struct ReachabilityHint {
    /// 机器可读的提示码,供 UI 映射为本地化文案。
    /// 取值为固定的五类常量(见 assess_base_url 中的字面量),Dart 侧
    /// 作为普通 String 透传。
    pub code: String,
    /// 触发提示的原始地址(原样回显,不修改)。
    pub address: String,
}

/// 解析并评估一个 base_url,返回 0 或多个提示。
///
/// 不做任何网络请求,只做静态字符串/主机名分析。返回空 vec 表示
/// 「未发现需要提示的情形」(不等于「一定可达」——不探测外部)。
pub fn assess_base_url(base_url: &str) -> Vec<ReachabilityHint> {
    let trimmed = base_url.trim();
    let mut hints = Vec::new();
    if trimmed.is_empty() {
        // 空值由 save 路径的必填校验单独报错,这里不再重复提示。
        return hints;
    }

    // 提取 scheme 与 host。只用字符串切分,不依赖 url crate,避免新增依赖。
    let without_scheme = match trimmed.split_once("://") {
        Some((scheme, rest)) => {
            let scheme = scheme.to_ascii_lowercase();
            if scheme != "http" && scheme != "https" {
                hints.push(ReachabilityHint {
                    code: "provider_url_scheme_unusual".to_string(),
                    address: trimmed.to_string(),
                });
            }
            rest
        }
        None => {
            // 没有 scheme:多数 OpenAI 兼容客户端会失败或按 https 猜测,
            // 属于「可能不可达」的明确提示,但仍由用户决定。
            hints.push(ReachabilityHint {
                code: "provider_url_missing_scheme".to_string(),
                address: trimmed.to_string(),
            });
            trimmed
        }
    };

    // 剥离可能存在的 userinfo (如 user:pass@host)，避免密码中的冒号干扰 host 提取。
    let after_auth = match without_scheme.split_once('@') {
        Some((auth, rest)) if !auth.contains('/') => rest,
        _ => without_scheme,
    };

    // IPv6 采用方括号表示 (如 [::1]:8080)，冒号在括号内不是端口分隔符。
    let host = if after_auth.starts_with('[') {
        if let Some(close_idx) = after_auth.find(']') {
            &after_auth[..=close_idx]
        } else {
            let host_end = after_auth.find('/').unwrap_or(after_auth.len());
            &after_auth[..host_end]
        }
    } else {
        let host_end = after_auth.find(['/', ':']).unwrap_or(after_auth.len());
        &after_auth[..host_end]
    }
    .trim()
    .to_ascii_lowercase();
    if host.is_empty() {
        hints.push(ReachabilityHint {
            code: "provider_url_missing_host".to_string(),
            address: trimmed.to_string(),
        });
        return hints;
    }

    if is_loopback_host(&host) {
        hints.push(ReachabilityHint {
            code: "provider_url_loopback".to_string(),
            address: trimmed.to_string(),
        });
    } else if is_private_host(&host) {
        hints.push(ReachabilityHint {
            code: "provider_url_private_network".to_string(),
            address: trimmed.to_string(),
        });
    }
    hints
}

/// 环回/本机:localhost、127.0.0.0/8、::1、0.0.0.0。
fn is_loopback_host(host: &str) -> bool {
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    if host == "::1" || host == "[::1]" || host == "0.0.0.0" {
        return true;
    }
    // 127.0.0.0/8
    if let Some(rest) = host.strip_prefix("127.") {
        // 粗略确认剩下是数字点分(避免把 "127.example.com" 误判)。
        return rest
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
    }
    false
}

/// 私有/内网网段(RFC1918 + 链路本地 + .local/.internal 等),提示「
/// 仅内网可达,其他设备同步到此配置后可能无法访问」。
fn is_private_host(host: &str) -> bool {
    if host == "169.254.0.0" || host.starts_with("169.254.") {
        return true; // 链路本地
    }
    if host.starts_with("10.") {
        return true;
    }
    if host.starts_with("192.168.") {
        return true;
    }
    // 172.16.0.0/12
    if let Some(rest) = host.strip_prefix("172.")
        && let Some(second) = rest.split('.').next()
        && let Ok(n) = second.parse::<u8>()
        && (16..=31).contains(&n)
    {
        return true;
    }
    host.ends_with(".local") || host.ends_with(".internal") || host.ends_with(".lan")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(url: &str) -> Vec<String> {
        assess_base_url(url)
            .iter()
            .map(|h| h.code.clone())
            .collect()
    }

    fn has(url: &str, code: &str) -> bool {
        assess_base_url(url).iter().any(|h| h.code == code)
    }

    #[test]
    fn loopback_addresses_are_flagged_without_modifying_url() {
        for url in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:8080",
            "https://[::1]/v1",
            "http://0.0.0.0:9000",
            "http://foo.localhost",
        ] {
            assert!(
                has(url, "provider_url_loopback"),
                "{url} should be flagged loopback"
            );
        }
        // 提示原样回显地址,绝不返回修正后的 URL。
        let hints = assess_base_url("http://localhost:11434/v1");
        assert_eq!(hints[0].address, "http://localhost:11434/v1");
    }

    #[test]
    fn private_network_addresses_are_flagged() {
        for url in [
            "http://10.0.0.5/v1",
            "http://192.168.1.10:8000",
            "http://172.16.3.4",
            "http://172.31.255.255",
            "http://169.254.1.1",
            "http://nas.local/v1",
            "http://gateway.internal",
        ] {
            assert!(
                has(url, "provider_url_private_network"),
                "{url} should be flagged private"
            );
        }
        // 172.15 与 172.32 不在 /12 内。
        assert!(!has("http://172.15.0.1", "provider_url_private_network"));
        assert!(!has("http://172.32.0.1", "provider_url_private_network"));
        // "127.example.com" 不是环回。
        assert!(!has("http://127.example.com", "provider_url_loopback"));
    }

    #[test]
    fn missing_or_unusual_scheme_is_flagged_but_public_https_is_clean() {
        assert!(has("api.example.com/v1", "provider_url_missing_scheme"));
        assert!(has("ftp://api.example.com", "provider_url_scheme_unusual"));
        // 公网标准 https:无任何提示(不探测外部,不等于保证可达)。
        assert!(codes("https://api.openai.com/v1").is_empty());
        assert!(codes("https://api.deepseek.com").is_empty());
    }

    #[test]
    fn missing_host_is_flagged() {
        assert!(has("http://", "provider_url_missing_host"));
        assert!(has("https:///path", "provider_url_missing_host"));
    }

    #[test]
    fn empty_input_yields_no_hint() {
        assert!(assess_base_url("").is_empty());
        assert!(assess_base_url("   ").is_empty());
    }
}
