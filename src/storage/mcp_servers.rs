//! MCP 服务器配置的存储层（迁移 44 stdio；迁移 45 streamable-http）。
//!
//! 用户自定义外部工具源：stdio 子进程（本机）或 streamable-http 远程
//! 端点。保存时做基础校验；进程/HTTP 会话管理在 `crate::ai::mcp`，
//! 此处只管配置事实。headers_json 里常放 Bearer token，属密钥级敏感
//! 面，表已显式排除出同步域。

use super::Store;
use anyhow::{bail, Context, Result};
use rusqlite::params;

#[derive(Debug, Clone, serde::Serialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub command: String,
    pub args_json: String,
    pub env_json: String,
    /// streamable-http 端点（stdio 传输为空）。
    pub url: String,
    /// 透传 header（如 Authorization）；键值均为字符串。
    pub headers_json: String,
    pub enabled: bool,
    /// 免审批开关（迁移 47）：true 时该 server 的工具调用直接执行，
    /// 不再创建 pending-action 审批卡。MCP server 是第三方代码，开启
    /// 即视为用户自担风险（设置页有明示）。
    pub trusted: bool,
    pub created_at: String,
    pub updated_at: String,
}

fn validate_args(args: &serde_json::Value) -> Result<()> {
    if !args.is_array() {
        bail!("args 必须是字符串数组");
    }
    if args.as_array().unwrap().iter().any(|v| !v.is_string()) {
        bail!("args 数组元素必须是字符串");
    }
    Ok(())
}

fn validate_env(env: &serde_json::Value) -> Result<()> {
    if !env.is_object() {
        bail!("env 必须是字符串到字符串的对象");
    }
    if env.as_object().unwrap().iter().any(|(k, v)| !v.is_string() || k.is_empty()) {
        bail!("env 必须是字符串到字符串的对象");
    }
    Ok(())
}

fn validate_headers(headers: &serde_json::Value) -> Result<()> {
    if !headers.is_object() {
        bail!("headers 必须是字符串到字符串的对象");
    }
    if headers.as_object().unwrap().iter().any(|(k, v)| {
        !v.is_string() || k.is_empty()
    }) {
        bail!("headers 必须是字符串到字符串的对象");
    }
    Ok(())
}

impl Store {
    pub fn list_mcp_servers(&self) -> Result<Vec<McpServerConfig>> {
        let mut statement = self.connection.prepare(
            "SELECT id,name,transport,command,args_json,env_json,url,headers_json,enabled,trusted,created_at,updated_at
             FROM mcp_servers ORDER BY created_at ASC, id ASC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(McpServerConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                transport: row.get(2)?,
                command: row.get(3)?,
                args_json: row.get(4)?,
                env_json: row.get(5)?,
                url: row.get(6)?,
                headers_json: row.get(7)?,
                enabled: row.get::<_, i64>(8)? != 0,
                trusted: row.get::<_, i64>(9)? != 0,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("读取 MCP 服务器配置")
    }

    pub fn list_enabled_mcp_servers(&self) -> Result<Vec<McpServerConfig>> {
        Ok(self
            .list_mcp_servers()?
            .into_iter()
            .filter(|s| s.enabled)
            .collect())
    }

    pub fn get_mcp_server(&self, id: &str) -> Result<Option<McpServerConfig>> {
        Ok(self
            .list_mcp_servers()?
            .into_iter()
            .find(|s| s.id == id))
    }

    /// 新建或更新。id 为 None 则新建；name 全局唯一（同一 server 改名不冲突）。
    pub fn save_mcp_server(
        &self,
        id: Option<&str>,
        name: &str,
        transport: &str,
        command: &str,
        args_json: &str,
        env_json: &str,
        url: &str,
        headers_json: &str,
        enabled: bool,
        trusted: bool,
    ) -> Result<String> {
        let name = name.trim();
        if name.is_empty() {
            bail!("名称不能为空");
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            bail!("名称只能用字母/数字/短横线/下划线（工具名会拼成 mcp__{name}__…）");
        }
        let url = url.trim().to_string();
        let headers: serde_json::Value =
            serde_json::from_str(headers_json).context("headers 不是合法 JSON")?;
        validate_headers(&headers)?;
        match transport {
            "stdio" => {}
            "streamable-http" => {
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    bail!("streamable-http 传输需要 http(s):// 端点 URL");
                }
                if !command.trim().is_empty() {
                    bail!("streamable-http 传输不使用本地命令");
                }
            }
            _ => bail!("支持的传输：stdio（本机进程）、streamable-http（远程端点）"),
        }
        let command = command.trim().to_string();
        if transport == "stdio" && command.is_empty() {
            bail!("命令不能为空");
        }
        let args: serde_json::Value =
            serde_json::from_str(args_json).context("args 不是合法 JSON")?;
        validate_args(&args)?;
        let env: serde_json::Value =
            serde_json::from_str(env_json).context("env 不是合法 JSON")?;
        validate_env(&env)?;
        let now = chrono::Utc::now().to_rfc3339();
        match id {
            Some(existing) if !existing.is_empty() => {
                let updated = self.connection.execute(
                    "UPDATE mcp_servers SET name=?2,transport=?3,command=?4,args_json=?5,env_json=?6,url=?7,headers_json=?8,enabled=?9,trusted=?10,updated_at=?11
                     WHERE id=?1",
                    params![existing, name, transport, command, args_json, env_json, url, headers_json, enabled as i64, trusted as i64, now],
                )?;
                if updated == 0 {
                    bail!("未找到要更新的 MCP 服务器（id={existing}）");
                }
                // 配置变了，缓存的进程会话与工具清单一并作废（下次调用重拉）。
                super::super::ai::mcp::invalidate_server(existing);
                Ok(existing.to_string())
            }
            _ => {
                let new_id = uuid::Uuid::new_v4().to_string();
                self.connection.execute(
                    "INSERT INTO mcp_servers (id,name,transport,command,args_json,env_json,url,headers_json,enabled,trusted,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)",
                    params![new_id, name, transport, command, args_json, env_json, url, headers_json, enabled as i64, trusted as i64, now],
                )?;
                Ok(new_id)
            }
        }
    }

    pub fn delete_mcp_server(&self, id: &str) -> Result<()> {
        let deleted = self
            .connection
            .execute("DELETE FROM mcp_servers WHERE id=?1", [id])?;
        if deleted == 0 {
            bail!("未找到要删除的 MCP 服务器（id={id}）");
        }
        super::super::ai::mcp::invalidate_server(id);
        Ok(())
    }

    pub fn set_mcp_server_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let updated = self.connection.execute(
            "UPDATE mcp_servers SET enabled=?2, updated_at=?3 WHERE id=?1",
            params![id, enabled as i64, chrono::Utc::now().to_rfc3339()],
        )?;
        if updated == 0 {
            bail!("未找到 MCP 服务器（id={id}）");
        }
        super::super::ai::mcp::invalidate_server(id);
        Ok(())
    }

    /// 切换免审批开关。trusted=true 后该 server 的工具调用直接执行。
    pub fn set_mcp_server_trusted(&self, id: &str, trusted: bool) -> Result<()> {
        let updated = self.connection.execute(
            "UPDATE mcp_servers SET trusted=?2, updated_at=?3 WHERE id=?1",
            params![id, trusted as i64, chrono::Utc::now().to_rfc3339()],
        )?;
        if updated == 0 {
            bail!("未找到 MCP 服务器（id={id}）");
        }
        super::super::ai::mcp::invalidate_server(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Store {
        let path = std::env::temp_dir().join(format!("elsewhen-mcp-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path).unwrap();
        Store::mark_initialized(&path).unwrap();
        store
    }

    #[test]
    fn mcp_server_crud_roundtrip() {
        let store = fixture();
        let id = store
            .save_mcp_server(
                None,
                "filesystem",
                "stdio",
                "npx",
                r#"["-y","@modelcontextprotocol/server-filesystem","/tmp"]"#,
                "{}",
                "",
                "{}",
                true,
                false,
            )
            .unwrap();
        let listed = store.list_mcp_servers().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "filesystem");
        assert!(listed[0].enabled);

        store
            .save_mcp_server(
                Some(&id),
                "filesystem2",
                "stdio",
                "npx",
                "[]",
                "{}",
                "",
                "{}",
                false,
                false,
            )
            .unwrap();
        let updated = store.get_mcp_server(&id).unwrap().unwrap();
        assert_eq!(updated.name, "filesystem2");
        assert!(!updated.enabled);

        store.set_mcp_server_enabled(&id, true).unwrap();
        assert_eq!(store.list_enabled_mcp_servers().unwrap().len(), 1);

        store.delete_mcp_server(&id).unwrap();
        assert!(store.get_mcp_server(&id).unwrap().is_none());
    }

    #[test]
    fn mcp_server_validation_rejects_bad_input() {
        let store = fixture();
        // 名称带非法字符（会污染合成工具名）。
        assert!(store
            .save_mcp_server(None, "bad name", "stdio", "cmd", "[]", "{}", "", "{}", true, false)
            .is_err());
        // transport 只认 stdio / streamable-http。
        assert!(store
            .save_mcp_server(
                None, "ok-name", "websocket", "cmd", "[]", "{}", "", "{}", true, false
            )
            .is_err());
        // args 不是字符串数组。
        assert!(store
            .save_mcp_server(
                None, "ok-name", "stdio", "cmd", r#"["-y",3]"#, "{}", "", "{}", true, false
            )
            .is_err());
        // stdio 命令为空。
        assert!(store
            .save_mcp_server(
                None, "ok-name", "stdio", "  ", "[]", "{}", "", "{}", true, false
            )
            .is_err());
        // streamable-http 需要 http(s) URL。
        assert!(store
            .save_mcp_server(
                None, "ok-name", "streamable-http", "", "[]", "{}", "ftp://x", "{}", true, false
            )
            .is_err());
        // streamable-http 不吃本地命令。
        assert!(store
            .save_mcp_server(
                None, "ok-name", "streamable-http", "npx", "[]", "{}",
                "https://mcp.example.com/mcp", "{}", true, false
            )
            .is_err());
        // 合法远程配置落地且 url 原样保存。
        let id = store
            .save_mcp_server(
                None, "remote", "streamable-http", "", "[]", "{}",
                "https://mcp.example.com/mcp", r#"{"Authorization":"Bearer t"}"#, true, false,
            )
            .unwrap();
        let listed = store.get_mcp_server(&id).unwrap().unwrap();
        assert_eq!(listed.transport, "streamable-http");
        assert_eq!(listed.url, "https://mcp.example.com/mcp");
        assert_eq!(listed.headers_json, r#"{"Authorization":"Bearer t"}"#);
        assert!(store.list_mcp_servers().unwrap().len() == 1);
    }
}
