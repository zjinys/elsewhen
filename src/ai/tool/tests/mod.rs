//! AI 工具行为与提议提交边界测试（从 mod.rs 内联外移）。

use super::*;

// 公共测试辅助（原文件平移，pub(super) 供分组子文件复用）。
pub(super) fn temp_db() -> (Store, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "elsewhen-tool-test-{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Store::open(&path).unwrap();
    (store, path)
}

// ── R2A：MCP 工具延迟注入（catalog + enabled 过滤 + enable 登记） ──

fn mcp_tool(server: &str, raw: &str, desc_len: usize) -> crate::ai::mcp::McpToolInfo {
    crate::ai::mcp::McpToolInfo {
        name: format!("mcp__{server}__{raw}"),
        server: server.to_string(),
        server_id: format!("{server}-id"),
        raw_name: raw.to_string(),
        description: format!(
            "列出仓库 issue 并返回分页结果。{}",
            "细节说明。".repeat(desc_len)
        ),
        parameters: serde_json::json!({
            "type":"object",
            "properties":{"owner":{"type":"string"},"repo":{"type":"string"}},
            "required":["owner","repo"]
        }),
        read_only_hint: false,
    }
}

mod project_reports;

mod registry_dispatch;

mod record_event;

mod knowledge_draft;

mod misc_tools;

mod wiki_tools;

mod mcp_tools;
