//! dispatch.rs：原 tool/mod.rs 同名分段外移，行为零变化。

use super::Tool;
use super::write_tools::store_create_pending;
use super::{ToolCall, ToolContext, ToolPolicy, ToolRegistry, ToolResultMsg, ToolSpec};
use crate::storage::Store;
use anyhow::{Context, Result};
use serde_json::{Value, json};

pub(super) fn user_skills_roster() -> String {
    let Ok(cfg) = crate::config::AppConfig::load() else {
        return String::new();
    };
    match crate::ai::skills::discover(&cfg.data_dir) {
        Ok(skills) => {
            let active: Vec<_> = skills
                .iter()
                .filter(|s| crate::ai::skills::is_enabled(&cfg.data_dir, &s.id))
                .collect();
            let total = active.len();
            let mut out = String::new();
            for skill in active.iter().take(crate::ai::skills::USER_SKILL_LIMIT) {
                out.push_str(&format!("- {}（用户技能）：{}\n", skill.id, skill.brief));
            }
            if total > crate::ai::skills::USER_SKILL_LIMIT {
                out.push_str(&format!(
                    "（用户技能共 {total} 个，仅列出前 {} 个；其余仍可按名称 enable，但请在设置页建议用户清理不用的技能）\n",
                    crate::ai::skills::USER_SKILL_LIMIT
                ));
            }
            out
        }
        Err(_) => String::new(),
    }
}

pub(super) struct ImportDirectoryProjectTool;
impl Tool for ImportDirectoryProjectTool {
    fn name(&self) -> &'static str {
        "import_directory_as_project"
    }
    fn description(&self) -> &'static str {
        "将用户明确指定的本地目录作为一个项目导入知识库；先用有限证据进行 AI 综合分析，再由用户确认保存一个 project 知识页。项目文件片段可能发送给当前 AI Provider，扫描和分析可能消耗较多 token。"
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"directory":{"type":"string","description":"本地目录绝对路径，必填"}},"required":["directory"],"additionalProperties":false})
    }
    fn deferred(&self) -> bool {
        true
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let directory = arg_str(args, "directory")?;
        if !std::path::Path::new(&directory).is_dir() {
            anyhow::bail!("目录不存在或不可访问：{directory}");
        }
        let preview = crate::local_sources::analyze_project_with_ai(
            ctx.store,
            std::path::Path::new(&directory),
        )?;
        // 把预览阶段的最终报告放入待确认动作，确认时直接保存同一份内容，
        // 避免再次调用 AI 导致前后报告不一致或重复消耗 token。
        let action_args = json!({
            "directory": directory,
            "project_name": preview.project_name,
            "files": preview.files,
            "skipped": preview.skipped,
            "scan_truncated": preview.scan_truncated,
            "content_md": preview.content_md,
        });
        store_create_pending(ctx.store, ctx.conversation_id, self.name(), &action_args)?;
        let excerpt: String = preview.content_md.chars().take(2400).collect();
        Ok(format!(
            "已完成目录 `{directory}` 的只读项目盘点，尚未写入知识库。\n\n{excerpt}\n\n确认后只保存这份项目摘要，不保存完整文件地图；原始目录仍是事实来源。项目证据可能已发送给当前 AI Provider。在审阅卡点击接受后保存。"
        ))
    }
}

pub(super) struct ImportDirectoryFilesTool;
impl Tool for ImportDirectoryFilesTool {
    fn name(&self) -> &'static str {
        "import_directory_files"
    }
    fn description(&self) -> &'static str {
        "将用户明确指定的本地目录中的可读文件逐个导入知识库，每个文件生成一张 source 知识页。文件较多时可能消耗较多 token，需要用户确认后才执行。"
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"directory":{"type":"string","description":"本地目录绝对路径，必填"}},"required":["directory"],"additionalProperties":false})
    }
    fn deferred(&self) -> bool {
        true
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let directory = arg_str(args, "directory")?;
        if !std::path::Path::new(&directory).is_dir() {
            anyhow::bail!("目录不存在或不可访问：{directory}");
        }
        let preview =
            crate::local_sources::preview_file_import(ctx.store, std::path::Path::new(&directory))?;
        let details = preview
            .entries
            .iter()
            .take(6)
            .map(|e| {
                format!(
                    "{}\n{}",
                    e.locator,
                    e.previous_excerpt.clone().unwrap_or_else(|| format!(
                        "新文件：{}",
                        e.content.chars().take(240).collect::<String>()
                    ))
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let action_args = json!({"preview":preview});
        store_create_pending(ctx.store, ctx.conversation_id, self.name(), &action_args)?;
        Ok(format!(
            "已读取目录 `{directory}` 的 {} 个文件原文，跳过 {} 个超限或不可读文件；尚未写入知识库。以下为前 6 个文件的内容/差异预览：\n{details}\n确认后保存本批读取到的原文，旧版本保留。在审阅卡点击接受保存，在审阅卡点击拒绝放弃。",
            preview.entries.len(),
            preview.skipped.len()
        ))
    }
}

impl ToolRegistry {
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|t| t.name() == name)
            .map(|b| b.as_ref())
    }

    pub fn names(&self) -> Vec<&str> {
        self.tools.iter().map(|t| t.name()).collect()
    }

    /// 全部可用工具名（内置 + MCP 合成名），enable_tools 登记与文本协议用。
    pub fn all_names(&self) -> Vec<String> {
        let mut out: Vec<String> = self.tools.iter().map(|t| t.name().to_string()).collect();
        out.extend(self.mcp_tools.iter().map(|t| t.name.clone()));
        out
    }

    /// 原生 tool-calling 用的工具清单
    pub fn provider_specs(&self) -> Vec<ToolSpec> {
        self.provider_specs_for(true)
    }

    pub fn provider_specs_for(&self, allow_record_event: bool) -> Vec<ToolSpec> {
        self.tools
            .iter()
            .filter(|tool| allow_record_event || tool.name() != "record_event")
            .map(|t| ToolSpec {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .chain(self.mcp_tools.iter().map(|t| ToolSpec {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            }))
            .collect()
    }

    /// 注入 system prompt 的简短文本清单（文本协议兜底时模型也能知道可用工具）
    pub fn prompt_block(&self) -> String {
        self.prompt_block_for(true)
    }

    pub fn prompt_block_for(&self, allow_record_event: bool) -> String {
        Self::prompt_for_specs(&self.provider_specs_for(allow_record_event))
    }

    pub fn prompt_for_specs(specs: &[ToolSpec]) -> String {
        let mut out = String::from("可用工具（name：用途）：\n");
        for spec in specs {
            out.push_str(&format!(
                "- {}：{}\n参数 JSON Schema：{}\n",
                spec.name, spec.description, spec.parameters
            ));
        }
        out
    }
}

/// 分发一次工具调用；不 panic，失败以结果消息形式返回
pub fn dispatch(
    call: &ToolCall,
    registry: &ToolRegistry,
    store: &Store,
    conversation_id: &str,
) -> ToolResultMsg {
    dispatch_with_knowledge(call, registry, store, conversation_id, &[])
}

pub fn dispatch_with_knowledge(
    call: &ToolCall,
    registry: &ToolRegistry,
    store: &Store,
    conversation_id: &str,
    knowledge_candidates: &[crate::knowledge::KnowledgeCitation],
) -> ToolResultMsg {
    match registry.get(&call.name) {
        Some(tool) => {
            let schema = tool.parameters_schema();
            let missing: Vec<_> = schema["required"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|key| call.arguments.get(*key).is_none_or(Value::is_null))
                .collect();
            if !missing.is_empty() {
                return ToolResultMsg::err(
                    call,
                    format!(
                        "缺少必填参数：{}；请按工具的参数 JSON Schema 一次补全，本次未执行。",
                        missing.join("、")
                    ),
                );
            }
            let ctx = ToolContext {
                store,
                conversation_id,
                knowledge_candidates,
                source_event_id: store
                    .latest_event_id_for_conversation(conversation_id)
                    .ok()
                    .flatten(),
            };
            match tool.run(&call.arguments, &ctx) {
                Ok(text) => ToolResultMsg::ok(call, text),
                Err(e) => ToolResultMsg::err(call, e),
            }
        }
        None => {
            // 内置注册表未命中：MCP 工具（mcp__<server>__<tool>）走外部
            // 分发；错误同样以结果文本返回，不炸生成主链路。
            // R2A catalog 显示短名（list_issues），模型在文本协议下可能
            // 直接输出短名而非全名（mcp__gitea-mcp__list_issues）——
            // 此处做短名→全名容错解析，避免「未知工具」假失败。
            let resolved_name = if call.name.starts_with(crate::ai::mcp::MCP_PREFIX) {
                Some(call.name.clone())
            } else {
                // 短名容错：在 MCP 工具列表里按短名唯一匹配
                let matches: Vec<_> = registry
                    .mcp_tools()
                    .iter()
                    .filter(|t| {
                        t.name
                            .rsplit("__")
                            .next()
                            .map_or(false, |short| short == call.name)
                    })
                    .collect();
                if matches.len() == 1 {
                    Some(matches[0].name.clone())
                } else {
                    None
                }
            };
            if let Some(full_name) = resolved_name {
                let call = ToolCall {
                    id: call.id.clone(),
                    name: full_name,
                    arguments: call.arguments.clone(),
                    raw_arguments: call.raw_arguments.clone(),
                };
                let call = &call;
                // MCP server 是第三方代码，默认走 pending-action 审批——
                // 用户确认卡接受后才真正发起 tools/call，参数以确认时原样
                // 执行（杜绝确认前后偷换内容）。拒绝则丢弃，不执行。
                // platform 标签（迁移 48）统一裁决可达性：不匹配当前平台的行
                // 不注入工具，查不到即不可达；stdio/stdhttp 的设备适用性都由
                // 行上的标签表达（桌面多平台/移动端各取所需）。
                let server_name = call
                    .name
                    .strip_prefix(crate::ai::mcp::MCP_PREFIX)
                    .and_then(|rest| rest.split("__").next())
                    .unwrap_or_default();
                let server_info = store.list_active_mcp_servers().ok().and_then(|servers| {
                    servers
                        .into_iter()
                        .find(|s| s.name == server_name)
                        .map(|s| (s.transport, s.trusted, s.allow_readonly_auto))
                });
                if server_info.is_none() {
                    return ToolResultMsg::err(
                        call,
                        format!(
                            "MCP 服务器「{server_name}」未启用、删除或未适用当前平台，本次调用未执行。"
                        ),
                    );
                }
                // trusted=1：跳过审批直接执行，错误同样以结果文本返回。
                // 只读白名单（迁移 53）：server 显式开了 allow_readonly_auto
                // 且该工具带上游 annotations.readOnlyHint 才免逐次审批——
                // 不盲信任一单一来源；免审批不等于无痕，仍记事件。
                let auto_readonly = server_info.as_ref().map_or(false, |(_, _, allow)| {
                    *allow
                        && registry
                            .mcp_tools()
                            .iter()
                            .any(|t| t.name == call.name && t.read_only_hint)
                });
                if server_info.map_or(false, |(_, trusted, _)| trusted) || auto_readonly {
                    // 免审批不等于无痕：先写独立执行审计（会话外留痕）。
                    crate::ai::mcp::record_exec_audit(
                        if auto_readonly {
                            "readonly-auto-execute"
                        } else {
                            "trusted-execute"
                        },
                        &format!(
                            "tool={} conversation={} readonly_hint={}",
                            call.name, conversation_id, auto_readonly
                        ),
                    );
                    return match crate::ai::mcp::call_tool(store, &call.name, &call.arguments) {
                        Ok(output) => ToolResultMsg::ok(call, output),
                        Err(e) => ToolResultMsg::err(call, e),
                    };
                }
                {
                    let action_args = serde_json::json!({
                        "full_name": call.name,
                        "arguments": call.arguments,
                    });
                    match store_create_pending(
                        store,
                        conversation_id,
                        "mcp_tool_call",
                        &action_args,
                    ) {
                        Ok(_) => ToolResultMsg::err(
                            call,
                            format!(
                                "尚未执行，等待确认：工具 {} 的执行请求已登记待用户确认（MCP 服务器为第三方代码，执行前需要逐次审批）。用户在审阅卡点击接受后才会真正执行；在收到执行结果前，不得声称该调用已成功或已完成。",
                                call.name
                            ),
                        ),
                        Err(e) => ToolResultMsg::err(call, e),
                    }
                }
            } else {
                ToolResultMsg::err(
                    call,
                    format!(
                        "未知工具：{}；第三方 MCP 工具请先 enable_tools 登记全名（格式：mcp__<server>__<tool>），或使用 catalog 中显示的短名（需唯一匹配）",
                        call.name
                    ),
                )
            }
        }
    }
}

// ── 参数解析小工具 ──────────────────────────────────────────────

pub(super) fn arg_str(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .with_context(|| format!("缺少字符串参数 {key}"))
}

pub(super) fn arg_str_opt(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub(super) fn arg_i64(args: &Value, key: &str, default: i64) -> i64 {
    args.get(key).and_then(|v| v.as_i64()).unwrap_or(default)
}
