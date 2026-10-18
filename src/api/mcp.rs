//! MCP 服务器管理（用户自定义外部工具源，与 user_skills 同层级的扩展面）。
//!
//! 配置事实存 `mcp_servers` 表（迁移 44）；进程拉起与协议握手在
//! `crate::ai::mcp`（懒启动、会话缓存）。保存/删除/停用即时生效：配置
//! 变更会作废缓存的进程会话，下次工具调用重拉。

use super::*;

#[derive(Debug, Deserialize, Serialize)]
pub struct McpServerDto {
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// 首期仅 "stdio"；现写死校验，后续演进 streamable-http 再放行。
    pub transport: String,
    pub command: String,
    /// 字符串数组 JSON 文本（如 `["-y","@modelcontextprotocol/server-…"]`）。
    pub args_json: String,
    /// 字符串对象 JSON 文本（如 `{"NODE_ENV":"production"}`）。
    pub env_json: String,
    /// streamable-http 端点（stdio 传输留空）。
    #[serde(default)]
    pub url: String,
    /// 透传 header JSON 文本（如 `{"Authorization":"Bearer …"}`）。
    #[serde(default)]
    pub headers_json: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 免审批开关（迁移 47）：true 时工具调用直接执行，不再弹审批卡。
    #[serde(default)]
    pub trusted: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize)]
pub struct McpToolDto {
    /// 合成名：`mcp__<server>__<tool>`。
    pub name: String,
    pub server: String,
    pub raw_name: String,
    pub description: String,
    /// 参数 JSON Schema 的 JSON 文本（frb 界面不传动态 Value，展示/透传用）。
    pub parameters: String,
}

fn store_from_config() -> Result<crate::storage::Store> {
    let config = crate::config::AppConfig::load()?;
    let store = crate::storage::Store::open(&config.database_path)?;
    Ok(store)
}

pub fn list_mcp_servers() -> Result<Vec<McpServerDto>> {
    let store = store_from_config()?;
    Ok(store
        .list_mcp_servers()?
        .into_iter()
        .map(|s| McpServerDto {
            id: s.id,
            name: s.name,
            transport: s.transport,
            command: s.command,
            args_json: s.args_json,
            env_json: s.env_json,
            url: s.url,
            headers_json: s.headers_json,
            enabled: s.enabled,
            trusted: s.trusted,
        })
        .collect())
}

pub fn save_mcp_server(server: McpServerDto) -> Result<String> {
    let store = store_from_config()?;
    let id = store.save_mcp_server(
        Some(server.id.as_str()).filter(|s| !s.is_empty()),
        &server.name,
        &server.transport,
        &server.command,
        &server.args_json,
        &server.env_json,
        &server.url,
        &server.headers_json,
        server.enabled,
        server.trusted,
    )?;
    Ok(id)
}

pub fn delete_mcp_server(id: String) -> Result<()> {
    let store = store_from_config()?;
    store.delete_mcp_server(&id)
}

pub fn set_mcp_server_enabled(id: String, enabled: bool) -> Result<()> {
    let store = store_from_config()?;
    store.set_mcp_server_enabled(&id, enabled)
}

pub fn set_mcp_server_trusted(id: String, trusted: bool) -> Result<()> {
    let store = store_from_config()?;
    store.set_mcp_server_trusted(&id, trusted)
}

/// 连通性测试：发起 stdio 会话并拉取工具清单。将一并验证用户的命令/
/// 参数/env 配置是否真的能起进程、过握手。UI 的「测试连接」入口用。
pub fn test_mcp_server(server: McpServerDto) -> Result<Vec<McpToolDto>> {
    let config = crate::storage::McpServerConfig {
        id: if server.id.is_empty() {
            "test-probe".to_string()
        } else {
            server.id.clone()
        },
        name: server.name.clone(),
        transport: server.transport.clone(),
        command: server.command.clone(),
        args_json: server.args_json.clone(),
        env_json: server.env_json.clone(),
        url: server.url.clone(),
        headers_json: server.headers_json.clone(),
        enabled: true,
        trusted: false,
        created_at: String::new(),
        updated_at: String::new(),
    };
    // 未保存的配置探测：先invalidate 避免串到旧 id 的缓存会话。
    crate::ai::mcp::invalidate_server(&config.id);
    let tools = crate::ai::mcp::list_tools_for_server(&config)
        .context("MCP 连接失败（进程启动/握手/工具清单任一环节出错均为同一报告）")?;
    Ok(tools
        .into_iter()
        .map(|t| McpToolDto {
            name: t.name,
            server: t.server,
            raw_name: t.raw_name,
            description: t.description,
            parameters: t.parameters.to_string(),
        })
        .collect())
}
