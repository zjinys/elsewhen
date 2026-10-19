//! 工具面：available_tools 汇总、call_tool 调用与输出截断。

use super::runtime::connect;
use super::*;

/// 汇总全部启用 server 的工具清单；单个 server 失败不拖垮其它 server，
/// 失败原因进错误列表由装配层决定上报口径（红线上抛优于静默）。
/// 每行结果按连接配置指纹缓存：成功行 5 分钟、失败行 30 秒，避免每轮
/// 生成前对全部启用 server 逐个发 tools/list RPC 的固定延迟；server 配置
/// 变更（增删/启停/传输参数）即时失效。
pub fn available_tools(store: &Store) -> (Vec<McpToolInfo>, Vec<String>) {
    let mut tools = Vec::new();
    let mut errors = Vec::new();
    match store.list_active_mcp_servers() {
        Ok(servers) => {
            for server in servers {
                let fingerprint = mcp_server_fingerprint(&server);
                if let Some((cached, error)) = mcp_tools_cache_lookup(&fingerprint) {
                    tools.extend(cached);
                    if let Some(error) = error {
                        errors.push(error);
                    }
                    continue;
                }
                match list_tools_for_server(&server) {
                    Ok(mut list) => {
                        tools.append(&mut list);
                        mcp_tools_cache_store(fingerprint, tools.clone(), None);
                    }
                    Err(error) => {
                        // 失败也留缓存行（带显式错误状态），错误继续上报。
                        let message = format!("MCP「{}」：{error:#}", server.name);
                        mcp_tools_cache_store(fingerprint, Vec::new(), Some(message.clone()));
                        errors.push(message);
                    }
                }
            }
        }
        Err(error) => errors.push(format!("MCP 配置读取失败：{error:#}")),
    }
    (tools, errors)
}

/// 分发一次 MCP 工具调用。`full_name` 为合成名。
pub fn call_tool(store: &Store, full_name: &str, arguments: &Value) -> Result<String> {
    let raw = full_name
        .strip_prefix(MCP_PREFIX)
        .ok_or_else(|| anyhow!("不是 MCP 工具名：{full_name}"))?;
    let (server_name, raw_name) = raw
        .split_once("__")
        .ok_or_else(|| anyhow!("MCP 工具名缺少 server 前缀：{full_name}"))?;
    let server = store
        .list_active_mcp_servers()?
        .into_iter()
        .find(|s| s.name == server_name)
        .ok_or_else(|| anyhow!("MCP 服务器「{server_name}」未启用或未适用当前平台"))?;
    let mut map = connect(&server)?;
    let conn = map.get_mut(&server.id).expect("connect ensured");
    let result = conn.request(
        "tools/call",
        json!({"name": raw_name, "arguments": arguments}),
        TOOL_CALL_TIMEOUT,
    )?;
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        let text = extract_text(&result).unwrap_or_default();
        bail!("MCP 工具返回错误：{text}");
    }
    let text = extract_text(&result).unwrap_or_else(|| "(工具无文本输出)".to_string());
    Ok(truncate_result(text))
}

pub(crate) fn extract_text(result: &Value) -> Option<String> {
    let content = result.get("content")?.as_array()?;
    let mut out = String::new();
    for item in content {
        if item.get("type").and_then(Value::as_str) == Some("text") {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(text);
            }
        }
    }
    Some(out).filter(|s| !s.is_empty())
}

pub(crate) fn truncate_result(text: String) -> String {
    if text.chars().count() <= MAX_RESULT_CHARS {
        return text;
    }
    let head: String = text.chars().take(MAX_RESULT_CHARS).collect();
    format!("{head}\n\n[输出超过 {MAX_RESULT_CHARS} 字符，已截断；如需完整内容请让工具分页返回]")
}
