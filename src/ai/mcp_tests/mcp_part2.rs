use super::*;

/// 缓存行往返：成功行返回工具、失败行把显式错误透传给装配层
/// （红线上抛优于静默——绝不把失败行读成空清单）。
#[test]
fn cache_roundtrip_keeps_error_rows_explicit() {
    let key = "fp-roundtrip-error".to_string();
    assert!(mcp_tools_cache_lookup(&key).is_none(), "未写入前未命中");
    mcp_tools_cache_store(key.clone(), vec![sample_tool("echo")], None);
    let (tools, error) = mcp_tools_cache_lookup(&key).expect("成功行命中");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].raw_name, "echo");
    assert!(error.is_none());

    let fail_key = "fp-roundtrip-fail".to_string();
    let message = "MCP「srv」：连接被拒".to_string();
    mcp_tools_cache_store(fail_key.clone(), Vec::new(), Some(message.clone()));
    let (tools, error) = mcp_tools_cache_lookup(&fail_key).expect("失败行也命中");
    assert!(tools.is_empty());
    assert_eq!(error.as_deref(), Some(message.as_str()), "错误必须显式透传");
}
