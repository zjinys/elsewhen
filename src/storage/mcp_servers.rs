//! MCP 服务器配置的存储层（迁移 44 stdio；45 streamable-http；48 平台标签）。
//!
//! 用户自定义外部工具源：stdio 子进程（本机）或 streamable-http 远程
//! 端点。保存时做基础校验；进程/HTTP 会话管理在 `crate::ai::mcp`。
//! 同步语义（迁移 48）：定义随人走，激活看平台——表进快照域（带
//! platform 标签，快照不含 headers 与 trusted），本机执行环境事实由
//! 激活过滤与 trusted 置 0 兑现（见 snapshot apply）。

use super::Store;
use anyhow::{Context, Result, bail};
use rusqlite::params;

#[derive(Debug, Clone, serde::Serialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    /// 平台标签（迁移 48）：all/linux/macos/windows。stdio 跨平台命令
    /// 不通（npx.cmd、PATH），在此标注适用平台；激活只取 all + 当前 OS。
    pub platform: String,
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
    /// 只读免审批白名单（迁移 53）：true 时该 server 下
    /// annotations.readOnlyHint=true 的工具调用跳过逐次审批直执行，
    /// 仍留事件痕迹。默认 0；不进快照同步域（本机策略）。
    pub allow_readonly_auto: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl McpServerConfig {
    /// 行是否适用于当前运行平台。移动端只有 all 行适用（android/ios 不在
    /// 标签值域内，天然只匹配 all）。
    pub fn platform_matches_current(&self) -> bool {
        self.platform == "all" || self.platform == current_platform()
    }
}

/// 当前平台的标签值。android/ios 返回自身名称（不在标签值域），从而只
/// 激活 all 行——移动端不上本地 stdio，与调用层的诚实降级拒绝双重拦截。
pub fn current_platform() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else {
        "unknown"
    }
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
    if env
        .as_object()
        .unwrap()
        .iter()
        .any(|(k, v)| !v.is_string() || k.is_empty())
    {
        bail!("env 必须是字符串到字符串的对象");
    }
    Ok(())
}

fn validate_headers(headers: &serde_json::Value) -> Result<()> {
    if !headers.is_object() {
        bail!("headers 必须是字符串到字符串的对象");
    }
    if headers
        .as_object()
        .unwrap()
        .iter()
        .any(|(k, v)| !v.is_string() || k.is_empty())
    {
        bail!("headers 必须是字符串到字符串的对象");
    }
    Ok(())
}

impl Store {
    pub fn list_mcp_servers(&self) -> Result<Vec<McpServerConfig>> {
        let mut statement = self.connection.prepare(
            "SELECT id,name,platform,transport,command,args_json,env_json,url,headers_json,enabled,trusted,allow_readonly_auto,created_at,updated_at
             FROM mcp_servers ORDER BY created_at ASC, id ASC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(McpServerConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                platform: row.get(2)?,
                transport: row.get(3)?,
                command: row.get(4)?,
                args_json: row.get(5)?,
                env_json: row.get(6)?,
                url: row.get(7)?,
                headers_json: row.get(8)?,
                enabled: row.get::<_, i64>(9)? != 0,
                trusted: row.get::<_, i64>(10)? != 0,
                allow_readonly_auto: row.get::<_, i64>(11)? != 0,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("读取 MCP 服务器配置")
    }

    /// 启用且适用当前平台的 server（工具注册/调用的统一口子）。
    /// 平台不匹配的行只随同步待机，不注入、不连接。
    pub fn list_active_mcp_servers(&self) -> Result<Vec<McpServerConfig>> {
        Ok(self
            .list_mcp_servers()?
            .into_iter()
            .filter(|s| s.enabled && s.platform_matches_current())
            .collect())
    }

    pub fn get_mcp_server(&self, id: &str) -> Result<Option<McpServerConfig>> {
        Ok(self.list_mcp_servers()?.into_iter().find(|s| s.id == id))
    }

    /// 新建或更新。id 为 None 则新建；(name, platform) 唯一（同一名 server
    /// 可有跨平台变体）。platform 缺省语义由调用方定（UI 默认 all；快照
    /// apply 不走此函数）。
    pub fn save_mcp_server(
        &self,
        id: Option<&str>,
        name: &str,
        platform: &str,
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
        match platform {
            "all" | "linux" | "macos" | "windows" => {}
            _ => bail!("平台标签只认 all/linux/macos/windows"),
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
        let env: serde_json::Value = serde_json::from_str(env_json).context("env 不是合法 JSON")?;
        validate_env(&env)?;
        let now = chrono::Utc::now().to_rfc3339();
        match id {
            Some(existing) if !existing.is_empty() => {
                let updated = self.connection.execute(
                    "UPDATE mcp_servers SET name=?2,platform=?3,transport=?4,command=?5,args_json=?6,env_json=?7,url=?8,headers_json=?9,enabled=?10,trusted=?11,updated_at=?12
                     WHERE id=?1",
                    params![existing, name, platform, transport, command, args_json, env_json, url, headers_json, enabled as i64, trusted as i64, now],
                )?;
                if updated == 0 {
                    bail!("未找到要更新的 MCP 服务器（id={existing}）");
                }
                // 配置变了，缓存的进程会话与工具清单一并作废（下次调用重拉）。
                super::super::ai::mcp::invalidate_server(existing);
                Ok(existing.to_string())
            }
            _ => {
                // (name, platform) 唯一：同题另外行存在则拒绝（友好提示优先
                // 于 SQLite 报错；同 id 本身 UPDATE 分支已处理）。
                let existing: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT id FROM mcp_servers WHERE name=?1 AND platform=?2",
                        params![name, platform],
                        |r| r.get(0),
                    )
                    .ok();
                if let Some(other) = existing {
                    if Some(other.as_str()) != id {
                        bail!("已存在同名同平台的 MCP 服务器「{name}」（platform={platform}）");
                    }
                }
                let new_id = uuid::Uuid::new_v4().to_string();
                self.connection.execute(
                    "INSERT INTO mcp_servers (id,name,platform,transport,command,args_json,env_json,url,headers_json,enabled,trusted,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?12)",
                    params![new_id, name, platform, transport, command, args_json, env_json, url, headers_json, enabled as i64, trusted as i64, now],
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

    /// 只读免审批白名单开关（迁移 53；事故 2026-10-19 P0 建议的落地
    /// 前提）。变更后缓存即时失效。
    pub fn set_mcp_server_allow_readonly_auto(&self, id: &str, allow: bool) -> Result<()> {
        let updated = self.connection.execute(
            "UPDATE mcp_servers SET allow_readonly_auto=?2, updated_at=?3 WHERE id=?1",
            params![id, allow as i64, chrono::Utc::now().to_rfc3339()],
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
                "all",
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
                "all",
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
        assert_eq!(store.list_active_mcp_servers().unwrap().len(), 1);

        store.delete_mcp_server(&id).unwrap();
        assert!(store.get_mcp_server(&id).unwrap().is_none());
    }

    #[test]
    fn platform_label_validation_and_variant_uniqueness() {
        let store = fixture();
        let linux = save(&store, "fs-a", "linux", "stdio", "npx", true);
        // 非法平台标签拒绝。
        assert!(
            store
                .save_mcp_server(
                    None, "fs-b", "android", "stdio", "cmd", "[]", "{}", "", "{}", true, false
                )
                .is_err()
        );
        // 同名同平台冲突拒绝。
        assert!(
            store
                .save_mcp_server(
                    None, "fs-a", "linux", "stdio", "other", "[]", "{}", "", "{}", true, false
                )
                .is_err()
        );
        // 同名不同平台变体允许（stdio 各平台命令不同是常态）。
        let win = store
            .save_mcp_server(
                None, "fs-a", "windows", "stdio", "npx.cmd", "[]", "{}", "", "{}", true, false,
            )
            .unwrap();
        assert_ne!(win, linux);
        // 激活过滤：all 行恒适用；停用不激活。当前平台断言与值域一致。
        store.set_mcp_server_enabled(&win, false).unwrap();
        let active = store.list_active_mcp_servers().unwrap();
        assert!(active.iter().all(|s| s.enabled));
        assert!(matches!(
            current_platform(),
            "linux" | "macos" | "windows" | "android" | "ios" | "unknown"
        ));
        let all_id = save(&store, "remote", "all", "streamable-http", "", true);
        let all_row = store.get_mcp_server(&all_id).unwrap().unwrap();
        assert!(all_row.platform_matches_current(), "all 行必须恒适用");
        assert!(
            !active.iter().any(|s| s.id == win),
            "停用/不适用平台的行不得激活"
        );
    }

    fn save(
        store: &Store,
        name: &str,
        platform: &str,
        transport: &str,
        command: &str,
        enabled: bool,
    ) -> String {
        let url = if transport == "streamable-http" {
            "https://mcp.example.com/mcp"
        } else {
            ""
        };
        store
            .save_mcp_server(
                None, name, platform, transport, command, "[]", "{}", url, "{}", enabled, false,
            )
            .unwrap()
    }

    #[test]
    fn mcp_server_validation_rejects_bad_input() {
        let store = fixture();
        // 名称带非法字符（会污染合成工具名）。
        assert!(
            store
                .save_mcp_server(
                    None, "bad name", "all", "stdio", "cmd", "[]", "{}", "", "{}", true, false
                )
                .is_err()
        );
        // transport 只认 stdio / streamable-http。
        assert!(
            store
                .save_mcp_server(
                    None,
                    "ok-name",
                    "all",
                    "websocket",
                    "cmd",
                    "[]",
                    "{}",
                    "",
                    "{}",
                    true,
                    false
                )
                .is_err()
        );
        // args 不是字符串数组。
        assert!(
            store
                .save_mcp_server(
                    None,
                    "ok-name",
                    "all",
                    "stdio",
                    "cmd",
                    r#"["-y",3]"#,
                    "{}",
                    "",
                    "{}",
                    true,
                    false
                )
                .is_err()
        );
        // stdio 命令为空。
        assert!(
            store
                .save_mcp_server(
                    None, "ok-name", "all", "stdio", "  ", "[]", "{}", "", "{}", true, false
                )
                .is_err()
        );
        // streamable-http 需要 http(s) URL。
        assert!(
            store
                .save_mcp_server(
                    None,
                    "ok-name",
                    "all",
                    "streamable-http",
                    "",
                    "[]",
                    "{}",
                    "ftp://x",
                    "{}",
                    true,
                    false
                )
                .is_err()
        );
        // streamable-http 不吃本地命令。
        assert!(
            store
                .save_mcp_server(
                    None,
                    "ok-name",
                    "all",
                    "streamable-http",
                    "npx",
                    "[]",
                    "{}",
                    "https://mcp.example.com/mcp",
                    "{}",
                    true,
                    false
                )
                .is_err()
        );
        // 合法远程配置落地且 url 原样保存。
        let id = store
            .save_mcp_server(
                None,
                "remote",
                "all",
                "streamable-http",
                "",
                "[]",
                "{}",
                "https://mcp.example.com/mcp",
                r#"{"Authorization":"Bearer t"}"#,
                true,
                false,
            )
            .unwrap();
        let listed = store.get_mcp_server(&id).unwrap().unwrap();
        assert_eq!(listed.transport, "streamable-http");
        assert_eq!(listed.url, "https://mcp.example.com/mcp");
        assert_eq!(listed.headers_json, r#"{"Authorization":"Bearer t"}"#);
        assert!(store.list_mcp_servers().unwrap().len() == 1);
    }
}
