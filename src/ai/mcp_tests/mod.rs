use super::*;

// 公共测试辅助（原 mod tests 内嵌段平移，pub(super) 供分组子文件复用）。
pub(super) fn sample_tool(name: &str) -> McpToolInfo {
    McpToolInfo {
        name: format!("mcp__srv__{name}"),
        server: "srv".into(),
        server_id: "srv-id".into(),
        raw_name: name.into(),
        description: "demo".into(),
        parameters: serde_json::json!({"type": "object"}),
        read_only_hint: false,
    }
}

pub(super) fn sample_server(url: &str) -> crate::storage::McpServerConfig {
    crate::storage::McpServerConfig {
        id: "srv-id".into(),
        name: "srv".into(),
        platform: "all".into(),
        transport: "streamable-http".into(),
        command: String::new(),
        args_json: "[]".into(),
        env_json: "{}".into(),
        url: url.into(),
        headers_json: "{}".into(),
        enabled: true,
        trusted: false,
        allow_readonly_auto: false,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

pub(super) fn which_python() -> Option<String> {
    ["python3", "python"].iter().find_map(|bin| {
        std::process::Command::new(bin)
            .arg("--version")
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|_| bin.to_string())
    })
}

mod mcp_part1;
mod mcp_part2;
mod mcp_part3;
mod mcp_part4;
