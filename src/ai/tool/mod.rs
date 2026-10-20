//! 工具调用层：把系统能力抽象成可被 AI 调用的工具。
//!
//! 设计目标：
//! - 一切除对话外的能力（抓取/检索/统计/写库）都能注册为工具；
//! - 未来 skill / MCP 提供的新能力以同样的 `Tool` trait 接入（实现 `Tool` 后注册进 `ToolRegistry` 即可）；
//! - 事件记录（`record_event`）为 `ToolPolicy::WriteDirect`：调用即写入 events 真源；
//! - 知识页保存（`save_knowledge_draft`，影响面更大）为 `ToolPolicy::WriteConfirm`：
//!   只生成「待确认动作」，用户确认后才碰真源，守住「只有确认才入库」的铁律。

use anyhow::Result;

use self::dispatch::user_skills_roster;
use self::dispatch::{ImportDirectoryFilesTool, ImportDirectoryProjectTool};
use self::query_tools::ListEventsByDateTool;
use self::query_tools::{ArchiveConversationsByTitleTool, ImportUrlToWikiTool, RenameWikiPageTool};
use self::read_tools::{
    DailyTokenUsageTool, FetchPageTool, FetchTweetTool, GetWikiPageTool, ListRulesTool,
    ListWikiPagesTool, SearchKnowledgeBaseTool, WebSearchTool,
};
use self::revision_tools::SaveWikiRevisionTool;
use self::todo_contact_tools::{
    BatchExtractPeopleRelationsTool, CreateReminderTool, CreateTodoTool, ListTodosTool,
    ProposePeopleRelationsTool,
};
use self::write_tools::RecordEventTool;
use self::write_tools::SaveKnowledgeDraftTool;
use crate::storage::Store;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

/// AI 发起的一次工具调用请求
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    /// OpenAI 原生协议下 arguments 的原始字面字符串。
    /// 回传 assistant tool_calls 时 OpenAI 要求与模型返回逐字节一致，故必须保留原文。
    pub raw_arguments: Option<String>,
}

impl ToolCall {
    pub fn new(name: impl Into<String>, arguments: Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            arguments,
            raw_arguments: None,
        }
    }

    /// 从文本信封 JSON（`{"name":..., "arguments":{...}}`）解析工具调用
    pub fn from_json(parsed: &Value) -> Option<Self> {
        let name = parsed.get("name")?.as_str()?.to_string();
        Some(ToolCall::new(
            name,
            parsed.get("arguments").cloned().unwrap_or(Value::Null),
        ))
    }
}

/// 工具清单中单项的规范描述（传给模型 / 序列化进原生 tools 字段）
#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// 技能包：把低频工具按任务场景打包，附方法论说明。
/// L1（title/tags/brief）常驻在 enable_tools 描述里供模型判断，
/// L2（instructions + 工具完整 schema）在 load 时才注入。
/// 不变量：每个 deferred() 工具至少被一个内建 skill 覆盖（有测试断言）。
pub struct SkillDefinition {
    pub id: &'static str,
    pub title: &'static str,
    pub tags: &'static [&'static str],
    pub brief: &'static str,
    pub instructions: &'static str,
    pub tools: &'static [&'static str],
}

/// 内建技能包：覆盖全部延迟加载工具。
/// 后续扩展路径：用户自定义 skill 挂知识页（知识库即存储与同步管道），
/// 运行时扫描注册；脚本执行型能力仅桌面端、另行评审。
pub const BUILTIN_SKILLS: &[SkillDefinition] = &[
    SkillDefinition {
        id: "web-search",
        title: "联网检索",
        tags: &["联网", "搜索", "网页", "抓取", "链接"],
        brief: "搜索网页、抓取指定 URL 或推文，获取实时信息",
        instructions: "使用要点：\n\
            - 优先 web_search 找线索，再 fetch_page 读原文；有具体 URL 或推特链接时直接抓取。\n\
            - 回答时引用来源（链接/标题），不要凭记忆转述搜索结果。\n\
            - 抓取失败或页面为空时如实告知，不要编造内容或 URL。\n\
            - 用户只是问事实而你不确定时，先搜再答；闲聊不需要联网。",
        tools: &["web_search", "fetch_page", "fetch_tweet"],
    },
    SkillDefinition {
        id: "wiki-maintenance",
        title: "知识库维护",
        tags: &["改名", "归档", "整理", "导入网页", "wiki"],
        brief: "重命名知识页、按标题归档会话、把网页导入知识库",
        instructions: "使用要点：\n\
            - rename_wiki_page 会同步到所有设备，改名前告知用户影响。\n\
            - archive_conversations_by_title 是批量操作：先向用户复述将归档的清单再执行。\n\
            - import_url_to_wiki 仅在用户明确给出链接并要求保存时使用；导入后提示用户检查内容。\n\
            - 这些操作不可轻易撤销，宁可多确认一次。",
        tools: &[
            "rename_wiki_page",
            "archive_conversations_by_title",
            "import_url_to_wiki",
        ],
    },
    SkillDefinition {
        id: "todo-actions",
        title: "待办管理",
        tags: &["待办", "任务", "清单", "备忘"],
        brief: "创建待办事项",
        instructions: "使用要点：\n\
            - 仅在用户明确要求记录待办、或出现明确的承诺性事项（截止日期、应许动作）时创建。\n\
            - 标题写清动作与对象，避免模糊描述。\n\
            - 一般疑问、想法、临时话题不要建待办；查询已有待办用常驻的 list_todos。\n\
            - 用户要在未来某个时刻被提醒（「X 点提醒我」「明早提醒」）时用 create_reminder，\n\
              它必须带明确到期时间，确认卡片回显已解析时间，确认才建条。",
        tools: &["create_todo", "create_reminder"],
    },
    SkillDefinition {
        id: "people-graph",
        title: "人物关系",
        tags: &["人物", "关系", "人脉"],
        brief: "从对话或知识页提取人物关系并形成提案",
        instructions: "使用要点：\n\
            - propose_people_relations 形成单个提案，batch_extract_people_relations 批量提取。\n\
            - 只提取有证据支撑的关系（对话中明说、知识页可引用），不臆测。\n\
            - 结果是提案，需用户确认后才生效；回复中说明提案数量与依据。",
        tools: &["propose_people_relations", "batch_extract_people_relations"],
    },
    SkillDefinition {
        id: "directory-import",
        title: "目录导入",
        tags: &["目录", "项目", "代码", "导入", "桌面"],
        brief: "把本地目录作为项目或文件导入知识库（桌面端）",
        instructions: "使用要点：\n\
            - 仅桌面端可用；移动端调用会失败。\n\
            - import_directory_as_project 先做有限证据扫描并给出项目分析提案，用户确认后才保存知识页。\n\
            - 目录文件片段会发送给当前 AI Provider，涉及敏感项目时先提醒用户。\n\
            - 用户没有明确指定目录路径时，先问清楚再扫。",
        tools: &["import_directory_as_project", "import_directory_files"],
    },
    SkillDefinition {
        id: "event-record",
        title: "手动记录",
        tags: &["记录", "记住", "记一下", "事件"],
        brief: "把第一人称事实、决定、行动手动记入时间线",
        instructions: "使用要点：\n\
            - 仅记录第一人称的事实、决定、行动或进展；AI 的评价、闲聊不记。\n\
            - 用户说\"记一下/记到知识库/记住这件事\"时使用。\n\
            - occurred_at 可缺省（默认现在），用户提到过去时间时转换成 ISO 8601。",
        tools: &["record_event"],
    },
];

/// 工具执行结果（回喂给 AI 的消息）
#[derive(Debug, Clone)]
pub struct ToolResultMsg {
    pub id: String,
    pub call_name: String,
    pub content: String,
    /// 本次调用是否真实成功（写声明核验用：WriteConfirm 只登记草稿不算写，见 conversation.rs）
    pub success: bool,
}

impl ToolResultMsg {
    pub fn ok(call: &ToolCall, content: String) -> Self {
        Self {
            id: call.id.clone(),
            call_name: call.name.clone(),
            content,
            success: true,
        }
    }

    pub fn err(call: &ToolCall, err: impl std::fmt::Display) -> Self {
        Self {
            id: call.id.clone(),
            call_name: call.name.clone(),
            content: format!("工具执行失败：{err}"),
            success: false,
        }
    }
}

/// 工具写入策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPolicy {
    /// 只读：执行结果直接回喂给 AI
    Read,
    /// 写操作：立即写入真源，不经过确认门（事件记录等低风险快速记下）
    WriteDirect,
    /// 写操作：先起草为「待确认动作」，用户确认后才真正写入（知识页等影响面大的）
    WriteConfirm,
}

mod history;
mod skill_script;
mod stats;

/// 工具运行上下文
pub struct ToolContext<'a> {
    pub store: &'a Store,
    pub conversation_id: &'a str,
    pub source_event_id: Option<String>,
    pub knowledge_candidates: &'a [crate::knowledge::KnowledgeCitation],
}

/// 一个可被 AI 调用的工具。
/// 未来 skill / MCP 提供的新能力以同样的方式接入。
pub trait Tool {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn parameters_schema(&self) -> Value;
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::Read
    }
    /// 延迟加载：不随请求注入完整定义，模型先 enable_tools 再用。
    /// 名单依据 chat_run_events 实测零/低频 + 破坏性/平台专属候选。
    fn deferred(&self) -> bool {
        false
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String>;
}

/// 工具注册表：持有全部可用工具，供分发、清单注入
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
    defer_tools: bool,
    /// MCP sidecar：外部 server 的动态工具，不符 `Tool` trait（动态名/描述），
    /// 由装配层拉取后注入；分发在 dispatch 的前缀路由完成。
    mcp_tools: Vec<crate::ai::mcp::McpToolInfo>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::eager()
    }
}

impl ToolRegistry {
    /// 全量常驻模式（既旧行为）：所有工具随请求注入；测试与内部工具调用用它。
    pub fn eager() -> Self {
        Self::build_with_deferral(false)
    }

    /// 延迟加载模式（生产默认）：deferred() 工具不注入 schema，模型经
    /// enable_tools 按需启用；省下的 token 预算让位给历史记忆。
    pub fn deferred_runtime() -> Self {
        Self::build_with_deferral(true)
    }

    fn build_with_deferral(defer_tools: bool) -> Self {
        Self {
            tools: Self::all_tools(),
            defer_tools,
            mcp_tools: Vec::new(),
        }
    }

    /// 注入 MCP 动态工具（装配层在拉取清单后调用；空清单 = 未启用 MCP）。
    pub fn set_mcp_tools(&mut self, tools: Vec<crate::ai::mcp::McpToolInfo>) {
        self.mcp_tools = tools;
    }

    pub fn mcp_tools(&self) -> &[crate::ai::mcp::McpToolInfo] {
        &self.mcp_tools
    }

    fn all_tools() -> Vec<Box<dyn Tool>> {
        vec![
            Box::new(ListRulesTool),
            Box::new(history::ReadHistoryTool),
            Box::new(history::SummarySpansTool),
            Box::new(ListWikiPagesTool),
            Box::new(GetWikiPageTool),
            Box::new(DailyTokenUsageTool),
            Box::new(SearchKnowledgeBaseTool),
            Box::new(FetchTweetTool),
            Box::new(FetchPageTool),
            Box::new(WebSearchTool),
            Box::new(RecordEventTool),
            Box::new(SaveKnowledgeDraftTool),
            Box::new(ListTodosTool),
            Box::new(CreateTodoTool),
            Box::new(CreateReminderTool),
            Box::new(ProposePeopleRelationsTool),
            Box::new(BatchExtractPeopleRelationsTool),
            Box::new(ListEventsByDateTool),
            Box::new(stats::QueryEventStatsTool),
            Box::new(ArchiveConversationsByTitleTool),
            Box::new(RenameWikiPageTool),
            Box::new(ImportUrlToWikiTool),
            Box::new(ImportDirectoryProjectTool),
            Box::new(ImportDirectoryFilesTool),
            Box::new(SaveWikiRevisionTool),
            Box::new(skill_script::RunSkillScriptTool),
        ]
    }
}

impl ToolRegistry {
    /// 按启用状态分区注入：defer_tools=false 时全量（等旧行为）。
    pub fn provider_specs_partitioned(
        &self,
        allow_record_event: bool,
        enabled: &std::collections::HashSet<String>,
    ) -> Vec<ToolSpec> {
        self.tools
            .iter()
            .filter(|tool| allow_record_event || tool.name() != "record_event")
            .filter(|tool| !self.defer_tools || !tool.deferred() || enabled.contains(tool.name()))
            .map(|t| ToolSpec {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            // R2A：MCP 工具与内置延迟工具同构——未启用（enable_tools 登记）
            // 不进 tools 字段，只以 catalog 形式出现在 enable_tools 花名册里；
            // defer_tools=false（旧协议）保持全量注入。
            .chain(
                self.mcp_tools
                    .iter()
                    .filter(|t| !self.defer_tools || enabled.contains(&t.name))
                    .map(|t| ToolSpec {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.parameters.clone(),
                    }),
            )
            .collect()
    }

    /// 常驻集（不含延迟工具）——预算预扣与文本协议按此口径。
    pub fn eager_specs(&self, allow_record_event: bool) -> Vec<ToolSpec> {
        self.provider_specs_partitioned(allow_record_event, &std::collections::HashSet::new())
    }

    /// 常驻（非延迟）工具名集——R2B 工具指南按需渲染的判定面。
    pub fn resident_tool_names(&self) -> std::collections::HashSet<String> {
        self.tools
            .iter()
            .filter(|t| !t.deferred())
            .map(|t| t.name().to_string())
            .collect()
    }

    /// 是否处于延迟加载模式。
    pub fn defer_tools(&self) -> bool {
        self.defer_tools
    }

    pub fn is_deferred(&self, name: &str) -> bool {
        self.get(name).is_some_and(|t| t.deferred())
    }

    /// enable_tools 元工具本身：常驻注入，描述里带延迟工具名单。
    /// 不进 tools 分发表（dispatch 由 conversation 层特判登记）。
    pub fn meta_spec(&self) -> Option<ToolSpec> {
        if !self.defer_tools {
            return None;
        }
        Some(ToolSpec {
            name: "enable_tools".to_string(),
            description: format!(
                "按需加载技能包（一次可多个），调用后返回技能使用说明与其打包工具的完整参数定义，\
                 同一轮稍后即可调用这些工具。可用技能包：\n{}{}{}",
                self.skill_roster_text(),
                user_skills_roster(),
                self.mcp_catalog_text(),
            ),
            parameters: serde_json::json!({
                "type":"object",
                "properties":{
                    "skills":{"type":"array","items":{"type":"string"},"description":"要启用的技能包 id，至少一个，必须来自上面的名单"}
                },
                "required":["skills"],
                "additionalProperties":false
            }),
        })
    }

    /// enable_tools 的模型侧说明：技能包 id + 标题/tags + 一句话用途。
    pub fn skill_roster_text(&self) -> String {
        let mut out = String::new();
        for skill in BUILTIN_SKILLS {
            out.push_str(&format!(
                "- {}（{}）：{}。tags: {}",
                skill.id,
                skill.title,
                skill.brief,
                skill.tags.join("/")
            ));
            out.push('\n');
        }
        out
    }

    /// MCP 工具 catalog（R2A，参考 opencode codemode）：未启用的 MCP 工具
    /// 不进 tools 字段，只在 enable_tools 花名册里按 server 分组渲染
    /// 「签名 // 首行描述（120 字符截断）」，整体设 2000 token 内联预算，
    /// 装不下的只计数（enable 后可看完整定义）。
    const MAX_CATALOG_TOKENS: usize = 2000;

    pub fn mcp_catalog_text(&self) -> String {
        if self.mcp_tools.is_empty() {
            return String::new();
        }
        use std::collections::BTreeMap;
        let mut groups: BTreeMap<&str, Vec<&crate::ai::mcp::McpToolInfo>> = BTreeMap::new();
        for tool in &self.mcp_tools {
            let server = tool
                .name
                .strip_prefix(crate::ai::mcp::MCP_PREFIX)
                .and_then(|rest| rest.split("__").next())
                .unwrap_or_default();
            groups.entry(server).or_default().push(tool);
        }
        let mut out = String::from("\n第三方 MCP 工具（调用前必须先 enable_tools 登记全名）：\n");
        let mut used = super::budget::text_tokens("deepseek-flash", &out);
        let mut truncated = 0usize;
        for (server, tools) in &groups {
            let header = format!("- {server}（{} 个工具）：\n", tools.len());
            used += crate::ai::budget::text_tokens("deepseek-flash", &header);
            out.push_str(&header);
            for tool in tools {
                let line = self.catalog_line(tool);
                let line_cost = super::budget::text_tokens("deepseek-flash", &line);
                if used + line_cost > Self::MAX_CATALOG_TOKENS {
                    truncated += 1;
                    continue;
                }
                used += line_cost;
                out.push_str(&line);
            }
        }
        if truncated > 0 {
            out.push_str(&format!(
                "  …（还有 {truncated} 个工具未列出；enable_tools 登记全名后返回完整定义）\n"
            ));
        }
        out
    }

    /// 单个工具的 catalog 行：短名 + 参数列表 // 首行描述（120 字符截断）。
    fn catalog_line(&self, tool: &crate::ai::mcp::McpToolInfo) -> String {
        let short = tool.name.rsplit("__").next().unwrap_or(&tool.name);
        let params: Vec<String> = tool
            .parameters
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|props| props.keys().cloned().collect())
            .unwrap_or_default();
        let first_line = tool
            .description
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .chars()
            .take(120)
            .collect::<String>();
        format!("  - {}({}) // {}\n", short, params.join(", "), first_line)
    }

    /// 处理 enable_tools 调用：按技能包 id 登记（激活包内全部延迟工具），
    /// 回传方法论说明 + 每个工具的名字/描述/完整参数定义。
    pub fn enable_result(
        &self,
        skill_ids: &[String],
        enabled: &mut std::collections::HashSet<String>,
    ) -> String {
        let mut activated_skills = Vec::new();
        let mut tools_included = Vec::new();
        let mut unknown = Vec::new();
        let mut user_skill_bodies = Vec::new();
        let mut mcp_schemas = Vec::new();
        let data_dir = crate::config::AppConfig::load()
            .map(|cfg| cfg.data_dir)
            .ok();
        for id in skill_ids {
            // 用户安装的技能：SKILL.md 正文 + 文件清单回传，同时激活脚本执行工具
            if let Some(dir) = &data_dir {
                if let Ok(body) = crate::ai::skills::load_body(dir, id) {
                    if crate::ai::skills::is_enabled(dir, id) {
                        enabled.insert("run_skill_script".to_string());
                        user_skill_bodies.push(format!("【用户技能 {id}】\n{body}"));
                        continue;
                    }
                }
            }
        }
        for id in skill_ids {
            // R2A：MCP 全名（mcp__<server>__<tool>）逐个登记，回传完整 schema
            if id.starts_with(crate::ai::mcp::MCP_PREFIX) {
                if let Some(tool) = self.mcp_tools.iter().find(|t| &t.name == id) {
                    enabled.insert(id.clone());
                    mcp_schemas.push(format!(
                        "== MCP 工具 {} 已启用 ==\n{}\n参数 JSON Schema：{}\n\n",
                        tool.name, tool.description, tool.parameters
                    ));
                } else {
                    unknown.push(id.clone());
                }
            }
        }
        for id in skill_ids {
            // MCP 服务器名（如 gitea-mcp）：整组启用该 server 下全部工具。
            // catalog 按 server 分组显示，模型按看到的组名回传属于合理行为。
            if !id.starts_with(crate::ai::mcp::MCP_PREFIX) {
                let group: Vec<_> = self
                    .mcp_tools
                    .iter()
                    .filter(|t| {
                        t.name
                            .strip_prefix(crate::ai::mcp::MCP_PREFIX)
                            .and_then(|rest| rest.split("__").next())
                            == Some(id.as_str())
                    })
                    .collect();
                if !group.is_empty() {
                    for tool in group {
                        enabled.insert(tool.name.clone());
                        mcp_schemas.push(format!(
                            "== MCP 工具 {} 已启用 ==\n{}\n参数 JSON Schema：{}\n\n",
                            tool.name, tool.description, tool.parameters
                        ));
                    }
                    continue;
                }
            }
        }
        for id in skill_ids {
            match BUILTIN_SKILLS.iter().find(|s| s.id == id) {
                Some(skill) => {
                    activated_skills.push(format!("{}（{}）", skill.id, skill.title));
                    for name in skill.tools {
                        if self.is_deferred(name) {
                            enabled.insert((*name).to_string());
                            tools_included.push(*name);
                        }
                    }
                }
                None => unknown.push(id.clone()),
            }
        }
        let mut text = String::new();
        if !activated_skills.is_empty() {
            text.push_str(&format!(
                "已启用技能包：{}。\n\n",
                activated_skills.join("、")
            ));
        }
        for id in skill_ids {
            if let Some(skill) = BUILTIN_SKILLS.iter().find(|s| s.id == id) {
                text.push_str(&format!(
                    "== {}（{}）使用说明 ==\n{}\n",
                    skill.id, skill.title, skill.instructions
                ));
                for name in skill.tools {
                    if let Some(tool) = self.get(name) {
                        text.push_str(&format!(
                            "工具 {}: {}\n参数 JSON Schema：{}\n\n",
                            tool.name(),
                            tool.description(),
                            tool.parameters_schema()
                        ));
                    }
                }
            }
        }
        for body in &user_skill_bodies {
            text.push_str(body);
            text.push('\n');
        }
        for schema in &mcp_schemas {
            text.push_str(schema);
        }
        if !unknown.is_empty() {
            // 过滤掉已按服务器组名启用的 id——它们不是 unknown。
            let truly_unknown: Vec<_> = unknown
                .iter()
                .filter(|id| {
                    !self.mcp_tools.iter().any(|t| {
                        t.name
                            .strip_prefix(crate::ai::mcp::MCP_PREFIX)
                            .and_then(|rest| rest.split("__").next())
                            == Some(id.as_str())
                    })
                })
                .cloned()
                .collect();
            if !truly_unknown.is_empty() {
                text.push_str(&format!(
                    "未知的技能包：{}（可用技能包见 enable_tools 描述；MCP 工具可传服务器名整组启用，或传全名 mcp__<服务器>__<工具> 单个启用）。",
                    truly_unknown.join("、")
                ));
            }
        }
        text
    }
}

pub use dispatch::dispatch;
pub use dispatch::dispatch_with_knowledge;
pub(crate) use read_tools::html_to_text;
/// 用户安装技能的 L1 清单（禁用的不显示）。
pub use revision_tools::execute_pending_action;

pub(crate) mod dispatch;
pub(crate) mod query_tools;
pub(crate) mod read_tools;
pub(crate) mod revision_tools;
pub(crate) mod todo_contact_tools;
pub(crate) mod write_tools;

// 历史内联 tests（目录化）经 super::* 依赖原文件符号：统一转发。
#[cfg(test)]
use self::dispatch::{arg_i64, arg_str, arg_str_opt};
#[cfg(test)]
use self::write_tools::insert_event_from_args;
#[cfg(test)]
pub(crate) use read_tools::strip_blocks;
#[cfg(test)]
pub(crate) use revision_tools::{confirm_knowledge_draft, decline_knowledge_draft};
#[cfg(test)]
pub(crate) use write_tools::store_create_pending;

#[cfg(test)]
mod tests;
