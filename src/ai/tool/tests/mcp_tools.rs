use super::*;

/// 未启用的 MCP 工具不进 tools 字段、不进常驻集（tool_reserve 回落）；
/// enable 登记全名后下一轮放量。
#[test]
fn mcp_tools_deferred_until_enabled() {
    let mut registry = ToolRegistry::deferred_runtime();
    registry.set_mcp_tools(vec![mcp_tool("gitea-mcp", "list_issues", 2)]);
    let empty = std::collections::HashSet::new();
    // 常驻集不含 MCP
    assert!(
        registry
            .eager_specs(true)
            .iter()
            .all(|s| !s.name.starts_with("mcp__"))
    );
    // 未启用：不出现在注入集
    assert!(
        registry
            .provider_specs_partitioned(true, &empty)
            .iter()
            .all(|s| !s.name.starts_with("mcp__"))
    );
    // 登记后放量
    let enabled: std::collections::HashSet<String> =
        ["mcp__gitea-mcp__list_issues".to_string()].into();
    let specs = registry.provider_specs_partitioned(true, &enabled);
    assert_eq!(
        specs
            .iter()
            .filter(|s| s.name == "mcp__gitea-mcp__list_issues")
            .count(),
        1
    );
}

/// catalog 渲染：按 server 分组、签名行 + 首行描述截断、2000 token 预算截断。
#[test]
fn mcp_catalog_groups_and_budgets() {
    let mut registry = ToolRegistry::deferred_runtime();
    let tools: Vec<_> = (0..53)
        .map(|i| mcp_tool("gitea-mcp", &format!("tool_{i:02}"), 30))
        .chain(std::iter::once(mcp_tool("fs-mcp", "read_file", 1)))
        .collect();
    registry.set_mcp_tools(tools);
    let catalog = registry.mcp_catalog_text();
    assert!(catalog.contains("gitea-mcp（53 个工具）"));
    assert!(catalog.contains("fs-mcp（1 个工具）"));
    assert!(catalog.contains("tool_00"), "工具应按行出现在 catalog 中");
    // 行内描述截断到 120 字符：不应出现整段长描述
    assert!(
        catalog
            .lines()
            .filter(|l| l.contains("// "))
            .all(|l| l.chars().count() < 180)
    );
    // 2000 token 预算装不下 54 行中文描述 → 截断进计数行，总量贴近预算
    assert!(
        catalog.contains("个工具未列出"),
        "超预算应截断并为模型留计数提示"
    );
    let catalog_tokens = crate::ai::budget::text_tokens("deepseek-flash", &catalog);
    assert!(
        (1_500..=2_300).contains(&catalog_tokens),
        "实际 {catalog_tokens}"
    );
    // 全量 schema 体积远大于 catalog（瘦身验收的对照面）
    let full: usize = registry
        .mcp_tools()
        .iter()
        .map(|t| {
            crate::ai::budget::text_tokens("deepseek-flash", &serde_json::to_string(t).unwrap())
        })
        .sum();
    assert!(crate::ai::budget::text_tokens("deepseek-flash", &catalog) < full / 4);
}

/// enable_result：MCP 全名登记 + 完整 schema 回传；未知全名走 unknown 报错。
#[test]
fn enable_result_registers_mcp_fullnames() {
    let mut registry = ToolRegistry::deferred_runtime();
    registry.set_mcp_tools(vec![mcp_tool("gitea-mcp", "list_issues", 1)]);
    let mut enabled = std::collections::HashSet::new();
    let text = registry.enable_result(
        &[
            "mcp__gitea-mcp__list_issues".to_string(),
            "mcp__gitea-mcp__nope".to_string(),
        ],
        &mut enabled,
    );
    assert!(enabled.contains("mcp__gitea-mcp__list_issues"));
    assert!(text.contains("mcp__gitea-mcp__list_issues 已启用"));
    assert!(text.contains("JSON Schema"));
    assert!(text.contains("未知的技能包") || text.contains("mcp__gitea-mcp__nope"));
    assert!(!enabled.contains("mcp__gitea-mcp__nope"));
}

/// 短名容错：模型在文本协议下可能输出短名（list_issues）而非全名
/// （mcp__gitea-mcp__list_issues），dispatch 应唯一匹配解析。
#[test]
fn dispatch_resolves_short_name_to_mcp_fullname() {
    let (store, path) = temp_db();
    let mut registry = ToolRegistry::deferred_runtime();
    registry.set_mcp_tools(vec![mcp_tool("gitea-mcp", "list_issues", 1)]);
    // 短名唯一匹配：应解析为全名并走 MCP 分发分支
    // （测试环境无真实 MCP server 配置，走到「服务器未启用」即证明解析成功）
    let call = ToolCall::new("list_issues", json!({}));
    let result = dispatch(&call, &registry, &store, "conv-test");
    drop(store);
    let _ = std::fs::remove_file(path);
    // 解析成功 = 不再是「未知工具」，而是 MCP 服务器层面的错误
    assert!(
        !result.content.contains("未知工具"),
        "应解析为 MCP 全名而非未知工具：{}",
        result.content
    );
    assert!(
        result.content.contains("MCP 服务器") || result.content.contains("尚未执行"),
        "{}",
        result.content
    );
}

/// 短名容错：多个 MCP server 有同名工具时不做猜测，报未知工具。
#[test]
fn dispatch_rejects_ambiguous_short_name() {
    let (store, path) = temp_db();
    let mut registry = ToolRegistry::deferred_runtime();
    registry.set_mcp_tools(vec![
        mcp_tool("gitea-mcp", "list_issues", 1),
        mcp_tool("github-mcp", "list_issues", 1),
    ]);
    let call = ToolCall::new("list_issues", json!({}));
    let result = dispatch(&call, &registry, &store, "conv-test");
    drop(store);
    let _ = std::fs::remove_file(path);
    assert!(result.content.contains("未知工具"), "{}", result.content);
    assert!(result.content.contains("唯一匹配"), "{}", result.content);
}

/// enable_result：服务器名（如 gitea-mcp）整组启用该 server 下全部工具。
/// 复现事故：模型看到 catalog 分组名后传「gitea-mcp」，旧代码报「未知的技能包」。
#[test]
fn enable_result_registers_mcp_server_group() {
    let mut registry = ToolRegistry::deferred_runtime();
    registry.set_mcp_tools(vec![
        mcp_tool("gitea-mcp", "list_issues", 1),
        mcp_tool("gitea-mcp", "search_repos", 1),
        mcp_tool("other-mcp", "read_file", 1),
    ]);
    let mut enabled = std::collections::HashSet::new();
    let text = registry.enable_result(&["gitea-mcp".to_string()], &mut enabled);
    assert!(enabled.contains("mcp__gitea-mcp__list_issues"));
    assert!(enabled.contains("mcp__gitea-mcp__search_repos"));
    // 只启用该组，不波及别的 server
    assert!(!enabled.contains("mcp__other-mcp__read_file"));
    // 组名不是 unknown——不应出现「未知的技能包：gitea-mcp」
    assert!(
        !text.contains("未知的技能包：gitea-mcp"),
        "服务器组名不应报未知：{}",
        text
    );
    assert!(text.contains("mcp__gitea-mcp__list_issues 已启用"));
    assert!(text.contains("mcp__gitea-mcp__search_repos 已启用"));
}
