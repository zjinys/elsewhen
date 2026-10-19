use super::*;

/// 指纹必须覆盖全部连接配置：任一字段变更（启停/url/命令/环境）都
/// 产生新键，旧缓存自然失效——这是「配置变更即失效」的实现机制。
#[test]
fn fingerprint_changes_when_connection_config_changes() {
    let base = mcp_server_fingerprint(&sample_server("http://a"));
    assert_eq!(
        base,
        mcp_server_fingerprint(&sample_server("http://a")),
        "同配置指纹稳定"
    );
    let variants = [
        sample_server("http://b"),
        {
            let mut s = sample_server("http://a");
            s.enabled = false;
            s
        },
        {
            let mut s = sample_server("http://a");
            s.headers_json = r#"{"Authorization":"Bearer x"}"#.into();
            s
        },
    ];
    for variant in variants {
        assert_ne!(
            base,
            mcp_server_fingerprint(&variant),
            "配置变更必须改变指纹"
        );
    }
}
