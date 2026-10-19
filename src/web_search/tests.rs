//! web_search 内嵌测试平移。

use super::*;

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

#[test]
fn custom_proxy_suppresses_doh_transport() {
    // 「挂了代理就全走代理」：proxy_mode=custom 时 DoH 即使 force+有地址也
    // 不再构建 DoH 传输，退化 Direct（而 Direct 走代理 client）。
    let mut settings = settings_with(
        crate::storage::DohMode::Force,
        "https://dns.example/dns-query",
    );
    settings.proxy_mode = "custom".to_string();
    settings.proxy_url = "http://127.0.0.1:7890".to_string();
    assert!(matches!(
        Transport::new(&settings, Duration::from_secs(5)),
        Transport::Direct { .. }
    ));

    // 非 custom（auto/off 代理模式）不抑制 DoH。
    let settings = settings_with(
        crate::storage::DohMode::Force,
        "https://dns.example/dns-query",
    );
    assert!(matches!(
        Transport::new(&settings, Duration::from_secs(5)),
        Transport::Doh { .. }
    ));
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
