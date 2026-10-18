//! MCP 服务器配置的存储层（迁移 44）。
//!
//! 用户自定义外部工具源：stdio 子进程传输的 jsonrpc-2.0 服务器。
//! 保存时做基础校验（命令必填、参数必须为数组、env 必须为对象）；
//! 进程实际拉起与握手在 `crate::ai::mcp`，此处只管配置事实。

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
    pub enabled: bool,
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
    Ok(())
}

impl Store {
    pub fn list_mcp_servers(&self) -> Result<Vec<McpServerConfig>> {
        let mut statement = self.connection.prepare(
            "SELECT id,name,transport,command,args_json,env_json,enabled,created_at,updated_at
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
                enabled: row.get::<_, i64>(6)? != 0,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
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
        enabled: bool,
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
        if transport != "stdio" {
            bail!("暂只支持 stdio 传输");
        }
        let command = command.trim();
        if command.is_empty() {
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
                    "UPDATE mcp_servers SET name=?2,transport=?3,command=?4,args_json=?5,env_json=?6,enabled=?7,updated_at=?8
                     WHERE id=?1",
                    params![existing, name, transport, command, args_json, env_json, enabled as i64, now],
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
                    "INSERT INTO mcp_servers (id,name,transport,command,args_json,env_json,enabled,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)",
                    params![new_id, name, transport, command, args_json, env_json, enabled as i64, now],
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
                true,
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
            .save_mcp_server(None, "bad name", "stdio", "cmd", "[]", "{}", true)
            .is_err());
        // transport 只认 stdio。
        assert!(store
            .save_mcp_server(None, "ok-name", "websocket", "cmd", "[]", "{}", true)
            .is_err());
        // args 不是字符串数组。
        assert!(store
            .save_mcp_server(None, "ok-name", "stdio", "cmd", r#"["-y",3]"#, "{}", true)
            .is_err());
        // 命令为空。
        assert!(store
            .save_mcp_server(None, "ok-name", "stdio", "  ", "[]", "{}", true)
            .is_err());
        assert!(store.list_mcp_servers().unwrap().is_empty());
    }
}
