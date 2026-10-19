//! tools/list 进程内缓存：指纹键、命中/写入与 TTL 常量。

use super::*;

/// tools/list 结果按 server 行的进程内缓存（proposal
/// 2026-10-19-mcp-tools-deferred-injection §3.2）。键 = server 全量连接
/// 配置的内容指纹：配置增删/启停/transport/命令/URL 任一变更，指纹即变，
/// 旧行自然失效，无需显式失效钩子。不进同步域（本机运行时状态）。
#[derive(Clone)]
pub(crate) struct McpToolsCacheEntry {
    tools: Vec<McpToolInfo>,
    /// Some=上次拉取失败的显式错误（装配层继续走 mcp_errors 通道上报，
    /// 绝不静默降级为空清单——红线：宁可显式失败不静默少工具）。
    error: Option<String>,
    fetched_at: std::time::Instant,
}

/// 成功行有效期：无 ToolsChanged 事件机制，TTL 是事件缺失下的保守兜底
/// （远端新增工具最迟 5 分钟可见；指纹变更即时可见）。
const MCP_TOOLS_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);
/// 失败行短 TTL：错误也缓存以免每轮生成都重打一遍挂掉的 server，
/// 但很快放行重试，故障恢复不被长缓存拖住。
const MCP_TOOLS_CACHE_ERROR_TTL: std::time::Duration = std::time::Duration::from_secs(30);

static MCP_TOOLS_CACHE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, McpToolsCacheEntry>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

pub(crate) fn mcp_server_fingerprint(server: &crate::storage::McpServerConfig) -> String {
    crate::storage::knowledge::content_hash(&format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        server.id,
        server.name,
        server.platform,
        server.transport,
        server.command,
        server.args_json,
        server.env_json,
        server.url,
        server.headers_json,
        server.enabled
    ))
}

pub(crate) fn mcp_tools_cache_lookup(
    fingerprint: &str,
) -> Option<(Vec<McpToolInfo>, Option<String>)> {
    let cache = MCP_TOOLS_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let entry = cache.get(fingerprint)?;
    let ttl = if entry.error.is_some() {
        MCP_TOOLS_CACHE_ERROR_TTL
    } else {
        MCP_TOOLS_CACHE_TTL
    };
    if entry.fetched_at.elapsed() > ttl {
        return None;
    }
    Some((entry.tools.clone(), entry.error.clone()))
}

pub(crate) fn mcp_tools_cache_store(
    fingerprint: String,
    tools: Vec<McpToolInfo>,
    error: Option<String>,
) {
    MCP_TOOLS_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(
            fingerprint,
            McpToolsCacheEntry {
                tools,
                error,
                fetched_at: std::time::Instant::now(),
            },
        );
}
