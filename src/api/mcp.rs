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
    /// 平台标签（迁移 48）：all/linux/macos/windows；空串按 all 处理。
    #[serde(default)]
    pub platform: String,
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
            platform: s.platform,
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

/// MCP 生命周期审计：与 skills 侧同模型（信任墙 #4）——新增/删除/启停/
/// 免审批开关都落在 `<data_dir>/mcp/.audit.log`（JSONL）。故意不进用户
/// 事件流：那是个人记录，不应混入系统操作，但免审批开关这类放宽信任
/// 边界的动作必须有本机留痕。
fn record_mcp_audit(action: &str, detail: &str) {
    use std::io::Write;
    let Ok(config) = crate::config::AppConfig::load() else {
        return;
    };
    let dir = config.data_dir.join("mcp");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let line = serde_json::json!({
        "ts": chrono::Utc::now().to_rfc3339(),
        "action": action,
        "detail": detail,
    });
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(".audit.log"))
    {
        let _ = writeln!(f, "{line}");
    }
}

pub fn save_mcp_server(server: McpServerDto) -> Result<String> {
    let store = store_from_config()?;
    let is_new = server.id.is_empty();
    // 未标记平台的老客户端（字段是 serde default）归入 all。
    let platform = if server.platform.is_empty() {
        "all"
    } else {
        server.platform.as_str()
    };
    let id = store.save_mcp_server(
        Some(server.id.as_str()).filter(|s| !s.is_empty()),
        &server.name,
        platform,
        &server.transport,
        &server.command,
        &server.args_json,
        &server.env_json,
        &server.url,
        &server.headers_json,
        server.enabled,
        server.trusted,
    )?;
    record_mcp_audit(
        if is_new { "create" } else { "update" },
        &format!("name={} platform={platform} trusted={}", server.name, server.trusted),
    );
    Ok(id)
}

pub fn delete_mcp_server(id: String) -> Result<()> {
    let store = store_from_config()?;
    store.delete_mcp_server(&id)?;
    record_mcp_audit("delete", &id);
    Ok(())
}

pub fn set_mcp_server_enabled(id: String, enabled: bool) -> Result<()> {
    let store = store_from_config()?;
    store.set_mcp_server_enabled(&id, enabled)?;
    record_mcp_audit(
        if enabled { "enable" } else { "disable" },
        &id,
    );
    Ok(())
}

pub fn set_mcp_server_trusted(id: String, trusted: bool) -> Result<()> {
    let store = store_from_config()?;
    store.set_mcp_server_trusted(&id, trusted)?;
    // 免审批开关最该留痕：这是对第三方代码信任边界的显式放宽。
    record_mcp_audit(
        if trusted { "trust" } else { "untrust" },
        &id,
    );
    Ok(())
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
        platform: server.platform.clone(),
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
