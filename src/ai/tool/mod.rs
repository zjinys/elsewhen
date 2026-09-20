//! 工具调用层：把系统能力抽象成可被 AI 调用的工具。
//!
//! 设计目标：
//! - 一切除对话外的能力（抓取/检索/统计/写库）都能注册为工具；
//! - 未来 skill / MCP 提供的新能力以同样的 `Tool` trait 接入（实现 `Tool` 后注册进 `ToolRegistry` 即可）；
//! - 事件记录（`record_event`）为 `ToolPolicy::WriteDirect`：调用即写入 events 真源；
//! - 知识页保存（`save_knowledge_draft`，影响面更大）为 `ToolPolicy::WriteConfirm`：
//!   只生成「待确认动作」，用户确认后才碰真源，守住「只有确认才入库」的铁律。

use crate::storage::{PendingAction, Store};
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Value};

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

/// 工具执行结果（回喂给 AI 的消息）
#[derive(Debug, Clone)]
pub struct ToolResultMsg {
    pub id: String,
    pub call_name: String,
    pub content: String,
}

impl ToolResultMsg {
    pub fn ok(call: &ToolCall, content: String) -> Self {
        Self {
            id: call.id.clone(),
            call_name: call.name.clone(),
            content,
        }
    }

    pub fn err(call: &ToolCall, err: impl std::fmt::Display) -> Self {
        Self::ok(call, format!("工具执行失败：{err}"))
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

/// 工具运行上下文
pub struct ToolContext<'a> {
    pub store: &'a Store,
    pub conversation_id: &'a str,
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
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String>;
}

/// 工具注册表：持有全部可用工具，供分发、清单注入
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        let tools: Vec<Box<dyn Tool>> = vec![
            Box::new(ListRulesTool),
            Box::new(ListWikiPagesTool),
            Box::new(GetWikiPageTool),
            Box::new(DailyTokenUsageTool),
            Box::new(SearchKnowledgeBaseTool),
            Box::new(FetchTweetTool),
            Box::new(FetchPageTool),
            Box::new(RecordEventTool),
            Box::new(SaveKnowledgeDraftTool),
            Box::new(ListTodosTool),
            Box::new(CreateTodoTool),
            Box::new(ProposePeopleRelationsTool),
            Box::new(BatchExtractPeopleRelationsTool),
            Box::new(ListEventsByDateTool),
            Box::new(ArchiveConversationsByTitleTool),
            Box::new(RenameWikiPageTool),
            Box::new(ImportUrlToWikiTool),
            Box::new(SaveWikiRevisionTool),
        ];
        Self { tools }
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

    /// 原生 tool-calling 用的工具清单
    pub fn provider_specs(&self) -> Vec<ToolSpec> {
        self.tools
            .iter()
            .map(|t| ToolSpec {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect()
    }

    /// 注入 system prompt 的简短文本清单（文本协议兜底时模型也能知道可用工具）
    pub fn prompt_block(&self) -> String {
        let mut out = String::from("可用工具（name：用途）：\n");
        for spec in self.provider_specs() {
            out.push_str(&format!("- {}：{}\n", spec.name, spec.description));
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
    match registry.get(&call.name) {
        Some(tool) => {
            let ctx = ToolContext {
                store,
                conversation_id,
            };
            match tool.run(&call.arguments, &ctx) {
                Ok(text) => ToolResultMsg::ok(call, text),
                Err(e) => ToolResultMsg::err(call, e),
            }
        }
        None => ToolResultMsg::err(call, format!("未知工具：{}", call.name)),
    }
}

// ── 参数解析小工具 ──────────────────────────────────────────────

fn arg_str(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .with_context(|| format!("缺少字符串参数 {key}"))
}

fn arg_str_opt(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn arg_i64(args: &Value, key: &str, default: i64) -> i64 {
    args.get(key).and_then(|v| v.as_i64()).unwrap_or(default)
}

// ── 只读工具 ────────────────────────────────────────────────────

/// 列出个人规则库中的全部规则
struct ListRulesTool;
impl Tool for ListRulesTool {
    fn name(&self) -> &'static str {
        "list_rules"
    }
    fn description(&self) -> &'static str {
        "列出个人规则库中已生效的全部经验规则。"
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{},"additionalProperties":false})
    }
    fn run(&self, _args: &Value, ctx: &ToolContext) -> Result<String> {
        let rules = ctx.store.list_active_rules()?;
        if rules.is_empty() {
            return Ok("规则库为空".to_string());
        }
        let mut out = String::from("已生效的个人规则：\n");
        for r in rules {
            out.push_str(&format!("- {}\n", r.content));
        }
        Ok(out)
    }
}

/// 列出知识库页面
struct ListWikiPagesTool;
impl Tool for ListWikiPagesTool {
    fn name(&self) -> &'static str {
        "list_wiki_pages"
    }
    fn description(&self) -> &'static str {
        "列出知识库中的页面（可按 kind 过滤），返回标题与摘要。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "kind":{"type":"string","description":"页面类型，可选。已知类型：topic/source/insight/principle/method/case/relationship/decision/habit/project/asset/constraint"}
            },
            "additionalProperties":false
        })
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let kind = arg_str_opt(args, "kind");
        let pages = match kind {
            Some(k) => ctx.store.list_wiki_pages(Some(&k), None)?,
            None => ctx.store.list_wiki_pages(None, None)?,
        };
        if pages.is_empty() {
            return Ok("知识库暂无页面".to_string());
        }
        let mut out = format!("知识库共 {} 页：\n", pages.len());
        for p in pages {
            let status = if p.status == "active" {
                ""
            } else {
                "（非 active）"
            };
            out.push_str(&format!(
                "- {}（{}）{status}：{}\n",
                p.title, p.kind, p.summary
            ));
        }
        Ok(out)
    }
}

/// 获取单个知识库页面全文
struct GetWikiPageTool;
impl Tool for GetWikiPageTool {
    fn name(&self) -> &'static str {
        "get_wiki_page"
    }
    fn description(&self) -> &'static str {
        "按 slug 获取知识库页面全文。先用 list_wiki_pages 拿到 slug。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{"slug":{"type":"string","description":"页面 slug，必填"}},
            "required":["slug"],
            "additionalProperties":false
        })
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let slug = arg_str(args, "slug")?;
        let page = ctx
            .store
            .get_wiki_page(&slug)?
            .context("没有找到该知识页")?;
        Ok(format!(
            "标题：{}\n类型：{}\n摘要：{}\n\n正文：\n{}",
            page.title, page.kind, page.summary, page.content_md
        ))
    }
}

/// 每日 token 用量统计
struct DailyTokenUsageTool;
impl Tool for DailyTokenUsageTool {
    fn name(&self) -> &'static str {
        "get_daily_token_usage"
    }
    fn description(&self) -> &'static str {
        "查询最近 N 天每天调 AI 的 token 用量统计。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{"days":{"type":"integer","description":"查询天数，默认 7"}},
            "additionalProperties":false
        })
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let days = arg_i64(args, "days", 7).clamp(1, 90);
        let daily = ctx.store.daily_token_usage(days as u32)?;
        if daily.is_empty() {
            return Ok("该时间段没有 AI 调用记录".to_string());
        }
        let mut out = String::from("每日 token 用量：\n");
        for d in daily {
            out.push_str(&format!(
                "- {}：{} tokens（{} 次调用）\n",
                d.date, d.total_tokens, d.call_count
            ));
        }
        Ok(out)
    }
}

/// 知识库全文搜索
struct SearchKnowledgeBaseTool;
impl Tool for SearchKnowledgeBaseTool {
    fn name(&self) -> &'static str {
        "search_knowledge_base"
    }
    fn description(&self) -> &'static str {
        "在知识库页面与个人事件记录中按关键词搜索，返回相关片段。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "query":{"type":"string","description":"搜索关键词，必填"},
                "limit":{"type":"integer","description":"最多返回条数，默认 5"}
            },
            "required":["query"],
            "additionalProperties":false
        })
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let query = arg_str(args, "query")?;
        let limit = arg_i64(args, "limit", 5).clamp(1, 20) as usize;
        let hits = ctx.store.search_knowledge_base(&query, limit)?;
        if hits.is_empty() {
            return Ok(format!("没有搜到与「{query}」相关的内容"));
        }
        let mut out = format!("与「{query}」相关的条目：\n");
        for h in hits {
            out.push_str(&format!("- [{}] {}：{}\n", h.kind, h.title, h.snippet));
        }
        Ok(out)
    }
}

/// 抓取推文长文（只解析，不入库）
struct FetchTweetTool;
impl Tool for FetchTweetTool {
    fn name(&self) -> &'static str {
        "fetch_tweet"
    }
    fn description(&self) -> &'static str {
        "通过推文链接抓取长文内容（支持 x.com / twitter.com），返回标题、作者与正文。只获取内容，不写入知识库。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{"url":{"type":"string","description":"推文链接，必填"}},
            "required":["url"],
            "additionalProperties":false
        })
    }
    fn run(&self, args: &Value, _ctx: &ToolContext) -> Result<String> {
        let url = arg_str(args, "url")?;
        let t = crate::wiki::fetch_tweet_text(&url)?;
        let mut out = format!("推文 {}：\n", t.tweet_id);
        if let Some(title) = &t.title {
            out.push_str(&format!("标题：{title}\n"));
        }
        if let Some(author) = &t.author_name {
            out.push_str(&format!("作者：{author}\n"));
        }
        out.push_str(&format!("正文：\n{}", t.text));
        Ok(out)
    }
}

/// 通用网页抓取（未来的「下载 URL」能力；只读取文本，不入库）
struct FetchPageTool;
impl Tool for FetchPageTool {
    fn name(&self) -> &'static str {
        "fetch_page"
    }
    fn description(&self) -> &'static str {
        "抓取任意网页/文章 URL 并提取纯文本内容（去除页面噪声），返回前 4000 字。只读取，不写入任何库。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{"url":{"type":"string","description":"http(s) 网页链接，必填"}},
            "required":["url"],
            "additionalProperties":false
        })
    }
    fn run(&self, args: &Value, _ctx: &ToolContext) -> Result<String> {
        let url = arg_str(args, "url")?;
        fetch_page_plain_text(&url)
    }
}

fn fetch_page_plain_text(url: &str) -> Result<String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        anyhow::bail!("仅支持 http/https 链接");
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .context("创建 HTTP 客户端失败")?;
    let resp = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (elsewhen/0.1)")
        .send()
        .context("抓取页面失败")?;
    if !resp.status().is_success() {
        anyhow::bail!("页面返回状态 {}", resp.status());
    }
    let html = resp.text().context("读取页面内容失败")?;
    let text = html_to_text(&html);
    let text = text.trim();
    let chars: Vec<char> = text.chars().collect();
    let truncated = chars.len() > 4000;
    let body: String = chars.iter().take(4000).collect();
    if body.is_empty() {
        anyhow::bail!("页面没有可读文本内容");
    }
    let mut out = format!("页面地址：{url}\n正文：\n{body}");
    if truncated {
        out.push_str("\n…（内容过长已截断）");
    }
    Ok(out)
}

/// 极简 HTML → 纯文本：去掉 script/style 与标签，压缩空白
pub(crate) fn html_to_text(html: &str) -> String {
    let mut s = html.to_string();
    for start in ["<script", "<style", "<noscript", "<svg", "<head"] {
        let mut lo = start.to_lowercase();
        let mut hi = start.to_uppercase();
        // 同时处理大小写变体（简化：保留原文，先不用 case 变体）
        let _ = (&mut lo, &mut hi);
        s = strip_blocks(&s, start);
    }
    // 去标签
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    // 压缩空白
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_space = false;
    for ch in out.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                collapsed.push(' ');
            }
            prev_space = true;
        } else {
            collapsed.push(ch);
            prev_space = false;
        }
    }
    collapsed.trim().to_string()
}

pub(crate) fn strip_blocks(s: &str, open: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while !rest.is_empty() {
        match rest.to_lowercase().find(open) {
            None => {
                out.push_str(rest);
                break;
            }
            Some(idx) => {
                out.push_str(&rest[..idx]);
                let after = &rest[idx..];
                let close = after.find("</").and_then(|i| {
                    let tag_end = &after[i + 2..];
                    let name: String = tag_end
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric())
                        .collect();
                    Some(
                        after
                            .find(&format!("</{name}>"))
                            .map(|c| c + name.len() + 3),
                    )
                    .flatten()
                });
                match close {
                    Some(end) => rest = &after[end..],
                    None => {
                        out.push_str(open);
                        rest = &after[open.len()..];
                    }
                }
            }
        }
    }
    out
}

// ── 写类工具（事件直接入库；知识页走 WriteConfirm：草拟 → 用户确认 → 执行） ───

/// 记录一条个人事件（直接写入 events 真源，无需确认）
struct RecordEventTool;
impl Tool for RecordEventTool {
    fn name(&self) -> &'static str {
        "record_event"
    }
    fn description(&self) -> &'static str {
        "把一段值得长期回看的客观经历、决定、行动或进展记录为个人事件，立即保存到事件记录（无需确认）。只记录用户自己的事实；不要记录对 AI 回复的评价、对话过程、寒暄、纯提问或闲聊。用户明确说‘记一下/帮我记/存进事件’时，按用户意图记录。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "raw_text":{"type":"string","description":"值得回看的第一人称事实、决定、行动或进展；不要填入对 AI 的评价或闲聊，必填"},
                "occurred_at":{"type":"string","description":"发生时间（ISO 8601，如 2026-09-15T10:30:00Z），可选，默认现在"}
            },
            "required":["raw_text"],
            "additionalProperties":false
        })
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteDirect
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        insert_event_from_args(args, ctx.store)
    }
}

/// 解析事件参数并写入 events 真源。
/// record_event 直接执行时用；历史遗留的 record_event 待确认动作也复用同一逻辑。
fn insert_event_from_args(args: &Value, store: &Store) -> Result<String> {
    let raw_text = arg_str(args, "raw_text")?;
    let occurred_at = match arg_str_opt(args, "occurred_at") {
        Some(iso) => chrono::DateTime::parse_from_rfc3339(&iso)
            .map(|t| t.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        None => chrono::Utc::now(),
    };
    let id = store.insert_event(crate::event::NewEvent {
        raw_text: &raw_text,
        occurred_at,
        recorded_at: chrono::Utc::now(),
        source: "tool",
    })?;
    Ok(format!("已保存事件（{id}）：{raw_text}"))
}

/// 保存一篇知识库草稿（草拟，确认后写入 wiki_pages）
struct SaveKnowledgeDraftTool;
impl Tool for SaveKnowledgeDraftTool {
    fn name(&self) -> &'static str {
        "save_knowledge_draft"
    }
    fn description(&self) -> &'static str {
        "把一段可复用的知识/结论草拟成知识库页面。调用后进入待确认状态，需要用户确认才会真正保存。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "title":{"type":"string","description":"页面标题，必填"},
                "content_md":{"type":"string","description":"正文（Markdown），必填"},
                "kind":{"type":"string","description":"页面类型，可选，默认 topic。已知类型：topic/source/insight/principle/method/case/relationship/decision/habit/project"},
                "tags":{"type":"array","items":{"type":"string"},"description":"标签数组，可选"}
            },
            "required":["title","content_md"],
            "additionalProperties":false
        })
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let title = arg_str(args, "title")?;
        let content_md = arg_str(args, "content_md")?;
        let kind = arg_str_opt(args, "kind").unwrap_or_else(|| "topic".to_string());
        let tags: Vec<String> = args
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let action_args = json!({
            "title": title,
            "content_md": content_md,
            "kind": kind,
            "tags": tags,
        });
        store_create_pending(
            ctx.store,
            ctx.conversation_id,
            "save_knowledge_draft",
            &action_args,
        )?;
        Ok(format!(
            "已为你草拟一篇知识库页面（待确认，尚未保存）：\n标题：{title}\n---\n{content}\n---\n—— 等你确认后才会真正保存，回复「好」即可。",
            content = {
                let mut c = content_md.clone();
                if c.chars().count() > 120 {
                    c = c.chars().take(120).collect::<String>() + "…";
                }
                c
            }
        ))
    }
}

fn store_create_pending(
    store: &Store,
    conversation_id: &str,
    action: &str,
    args: &Value,
) -> Result<String> {
    let args_json = serde_json::to_string(args)?;
    store.create_pending_action(conversation_id, action, &args_json)
}

// ── 个人待办工具 ────────────────────────────────────────────────

/// 列出全部待办（可过滤状态）
struct ListTodosTool;
impl Tool for ListTodosTool {
    fn name(&self) -> &'static str {
        "list_todos"
    }
    fn description(&self) -> &'static str {
        "列出个人待办清单。用户问起待办/任务/要做的事情时使用。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "status":{"type":"string","description":"可选：open/done，缺省列出所有未归档"}
            },
            "additionalProperties":false
        })
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let status = args.get("status").and_then(|v| v.as_str());
        let todos = ctx.store.list_todos(status)?;
        if todos.is_empty() {
            return Ok("当前待办为空".to_string());
        }
        let mut out = String::from("待办清单：\n");
        for t in todos {
            let mark = if t.status.as_str() == "done" {
                "✓"
            } else {
                "☐"
            };
            let due = t
                .due_at
                .as_ref()
                .map(|d| format!(" (截止 {d})"))
                .unwrap_or_default();
            out.push_str(&format!("- {mark} {}{due}\n", t.title));
        }
        Ok(out)
    }
}

/// 提议一条待办（确认后创建）
struct CreateTodoTool;
impl Tool for CreateTodoTool {
    fn name(&self) -> &'static str {
        "create_todo"
    }
    fn description(&self) -> &'static str {
        "为分析出需要后续跟进的事情创建一条待办。调用后进入待确认状态，需要用户确认才会真正建条。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "title":{"type":"string","description":"待办内容，必填"},
                "due_at":{"type":"string","description":"截止时间（ISO 日期），可选"},
                "priority":{"type":"string","description":"优先级：high/normal/low，默认 normal"},
                "related_wiki_slug":{"type":"string","description":"关联知识页 slug，可选"},
                "note":{"type":"string","description":"补充说明，可选"}
            },
            "required":["title"],
            "additionalProperties":false
        })
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let title = arg_str(args, "title")?;
        let action_args = json!({
            "title": title.clone(),
            "due_at": arg_str_opt(args, "due_at"),
            "priority": arg_str_opt(args, "priority").unwrap_or_else(|| "normal".to_string()),
            "related_wiki_slug": arg_str_opt(args, "related_wiki_slug"),
            "note": arg_str_opt(args, "note"),
        });
        store_create_pending(ctx.store, ctx.conversation_id, "create_todo", &action_args)?;
        Ok(format!(
            "已为你草拟一条待办（待确认，尚未创建）：\n「{title}」\n—— 回复「好」即可建条。"
        ))
    }
}

// ── 人物关系工具 ───────────────────────────────────────────────

/// 把对话里识别出的「人物 + 人↔事情/项目」关系草拟下来（确认后才建档存关系）
struct ProposePeopleRelationsTool;
impl Tool for ProposePeopleRelationsTool {
    fn name(&self) -> &'static str {
        "propose_people_relations"
    }
    fn description(&self) -> &'static str {
        "把对话中出现的对用户重要的人物，以及「人物 ↔ 事情/项目」的关系草拟下来。调用后进入待确认状态，用户确认后才建档保存。事件/对话中用户用 @人名 标注的一定是人、#事情/项目 标注的一定是事情，优先纳入草拟。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "people":{
                    "type":"array",
                    "description":"本次要建档的人物（可不填，只补关系时省略）",
                    "items":{
                        "type":"object",
                        "properties":{
                            "name":{"type":"string","description":"人物姓名，必填"},
                            "role_note":{"type":"string","description":"身份/角色/背景一句话，可选"}
                        },
                        "required":["name"],
                        "additionalProperties":false
                    }
                },
                "relations":{
                    "type":"array",
                    "description":"人物与事情/项目的关系（可不填，只建档人物时省略）",
                    "items":{
                        "type":"object",
                        "properties":{
                            "person":{"type":"string","description":"人物姓名（与 people 中的 name 对应，或知识库已有的人物页）"},
                            "target":{"type":"string","description":"事情/项目名称"},
                            "relation":{"type":"string","description":"关系类型：负责/参与/合作/对接/跟进/顾问 等，可选，默认参与"},
                            "note":{"type":"string","description":"补充说明，可选"}
                        },
                        "required":["person","target"],
                        "additionalProperties":false
                    }
                }
            },
            "additionalProperties":false
        })
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        // 把 people/relations 原样转成待确认动作参数
        let people: Vec<Value> = args
            .get("people")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let relations: Vec<Value> = args
            .get("relations")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if people.is_empty() && relations.is_empty() {
            anyhow::bail!("请至少提供一位人物或一条关系");
        }
        let action_args = json!({ "people": people, "relations": relations });
        store_create_pending(
            ctx.store,
            ctx.conversation_id,
            "propose_people_relations",
            &action_args,
        )?;
        let mut lines = Vec::new();
        for p in &people {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let note = p.get("role_note").and_then(|v| v.as_str()).unwrap_or("");
            lines.push(format!(
                "人物：{name}{}",
                if note.is_empty() {
                    String::new()
                } else {
                    format!("（{note}）")
                }
            ));
        }
        for r in &relations {
            let person = r.get("person").and_then(|v| v.as_str()).unwrap_or("");
            let target = r.get("target").and_then(|v| v.as_str()).unwrap_or("");
            let rel = r
                .get("relation")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("参与");
            lines.push(format!("{person} —— {rel} —— {target}"));
        }
        Ok(format!(
            "已为你草拟人物与关系（待确认，尚未保存）：\n{}\n—— 回复「好」即建档保存。",
            lines.join("\n")
        ))
    }
}

// ── 批量提取：事件 → 人物/关系（草拟确认） ─────────────────────

/// 批量提取：扫描全部事件 → `@人名` / `#事情` 标注 + AI 补全 → 人物/关系草拟（待确认）
struct BatchExtractPeopleRelationsTool;
impl Tool for BatchExtractPeopleRelationsTool {
    fn name(&self) -> &'static str {
        "batch_extract_people_relations"
    }
    fn description(&self) -> &'static str {
        "扫描知识库里全部已保存事件，批量提取人物与「人物 ↔ 事情/项目」关系。事件里 @人名 标注的一定是人、#事情/项目 标注的一定是事情（权威实体，必须纳入）；AI 再根据事件上下文补全角色与关系。只产草拟，用户确认后才保存。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, _args: &Value, ctx: &ToolContext) -> Result<String> {
        let action_args = crate::wiki::propose_people_relations_from_events(ctx.store)?;
        let people: Vec<Value> = action_args
            .get("people")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let relations: Vec<Value> = action_args
            .get("relations")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if people.is_empty() && relations.is_empty() {
            anyhow::bail!("扫描事件后没有提取到人物或关系");
        }
        store_create_pending(
            ctx.store,
            ctx.conversation_id,
            "propose_people_relations",
            &action_args,
        )?;
        let mut lines = Vec::new();
        for p in &people {
            let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let note = p.get("role_note").and_then(|v| v.as_str()).unwrap_or("");
            lines.push(format!(
                "人物：{name}{}",
                if note.is_empty() {
                    String::new()
                } else {
                    format!("（{note}）")
                }
            ));
        }
        for r in &relations {
            let person = r.get("person").and_then(|v| v.as_str()).unwrap_or("");
            let target = r.get("target").and_then(|v| v.as_str()).unwrap_or("");
            let rel = r
                .get("relation")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("参与");
            lines.push(format!("{person} —— {rel} —— {target}"));
        }
        Ok(format!(
            "已从事件批量草拟人物与关系（待确认，尚未保存）：\n{}\n—— 回复「好」即建档保存。",
            lines.join("\n")
        ))
    }
}

// ── 查询 / 会话管理工具 ────────────────────────────────────────

/// 查看某一天记录的事件（只读）
struct ListEventsByDateTool;
impl Tool for ListEventsByDateTool {
    fn name(&self) -> &'static str {
        "list_events_by_date"
    }
    fn description(&self) -> &'static str {
        "查看某一天记录的事件清单（只读）。date 必须解析成 YYYY-MM-DD：用户说「今天/昨天/前天」按当前日期推算，「6月20日」这类自然日期补当年份，「2026-06-20」直接用。返回当天每条事件的时刻与内容。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "date":{"type":"string","description":"日期，YYYY-MM-DD，必填"}
            },
            "required":["date"],
            "additionalProperties":false
        })
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::Read
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let date_str = arg_str(args, "date")?;
        let date = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").map_err(|_| {
            anyhow::anyhow!("日期格式应为 YYYY-MM-DD（如 2026-06-20），收到：{date_str}")
        })?;
        let events = ctx.store.events_on_date(date)?;
        if events.is_empty() {
            return Ok(format!("{date_str} 这天没有事件记录。"));
        }
        let mut lines = Vec::new();
        for e in &events {
            let time = chrono::DateTime::parse_from_rfc3339(&e.recorded_at)
                .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
                .unwrap_or_else(|_| "??:??".to_string());
            lines.push(format!("{time} {}", e.raw_text));
        }
        Ok(format!(
            "{date_str} 共有 {} 条事件：\n{}",
            lines.len(),
            lines.join("\n")
        ))
    }
}

/// 按标题归档对话（草拟确认制）
struct ArchiveConversationsByTitleTool;
impl Tool for ArchiveConversationsByTitleTool {
    fn name(&self) -> &'static str {
        "archive_conversations_by_title"
    }
    fn description(&self) -> &'static str {
        "按标题归档对话（只归档主对话列表里的对话，不含知识页内聊天）。「把所有标题为 X 的对话归档」→ title=X 精确匹配；「把所有标题包含 X 的对话归档」→ contains=X 子串匹配（大小写不敏感）。标题为空（界面显示为「新对话」）的会话按标题「新对话」参与匹配。草拟确认制：调用后用户确认才真正归档。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "title":{"type":"string","description":"标题精确匹配（与 contains 至少提供一个）"},
                "contains":{"type":"string","description":"标题包含的子串匹配（与 title 至少提供一个）"}
            },
            "additionalProperties":false
        })
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let title = arg_str_opt(args, "title")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let contains = arg_str_opt(args, "contains")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        if title.is_none() && contains.is_none() {
            anyhow::bail!("请提供 title（标题精确匹配）或 contains（标题包含匹配）");
        }
        let conversations = ctx.store.list_conversations()?; // 已排除知识页内聊天
        let effective = |c: &crate::storage::ConversationSummary| -> String {
            c.title
                .as_deref()
                .filter(|t| !t.trim().is_empty())
                .unwrap_or("新对话")
                .to_string()
        };
        let matches: Vec<crate::storage::ConversationSummary> = conversations
            .iter()
            .filter(|c| {
                let et = effective(c);
                let hit_exact = title
                    .as_ref()
                    .map(|t| et.eq_ignore_ascii_case(t))
                    .unwrap_or(false);
                let hit_contains = contains
                    .as_ref()
                    .map(|n| et.to_lowercase().contains(&n.to_lowercase()))
                    .unwrap_or(false);
                hit_exact || hit_contains
            })
            .cloned()
            .collect();
        if matches.is_empty() {
            let sample: Vec<String> = conversations.iter().take(15).map(effective).collect();
            anyhow::bail!(
                "没有找到标题匹配的对话。当前主对话标题有：{}",
                sample.join("、")
            );
        }
        let ids: Vec<String> = matches.iter().map(|c| c.id.clone()).collect();
        let titles: Vec<String> = matches.iter().map(effective).collect();
        let action_args = json!({ "ids": ids, "titles": titles });
        store_create_pending(
            ctx.store,
            ctx.conversation_id,
            "archive_conversations_by_title",
            &action_args,
        )?;
        Ok(format!(
            "找到 {} 个匹配的对话，将归档：\n{}\n—— 回复「好」即归档。",
            matches.len(),
            titles
                .iter()
                .map(|t| format!("  · {t}"))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    }
}

// ── 知识页重命名工具 ──────────────────────────────────────────

/// 重命名知识页（草拟确认制）：标题 + 页面标识一起改，关系引用与页内聊天会话自动迁移
struct RenameWikiPageTool;
impl Tool for RenameWikiPageTool {
    fn name(&self) -> &'static str {
        "rename_wiki_page"
    }
    fn description(&self) -> &'static str {
        "重命名一个知识页（改标题；person/项目 等带前缀的页面会把唯一标识 slug 一起换成新名字，相关的人物关系引用与页内聊天会话自动迁移）。用户说「把 X 改名为 Y」「这个项目不叫 X，实际叫 Y」时使用。草拟确认制：调用后用户确认才真正改名。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "slug":{"type":"string","description":"当前页面的 slug（唯一标识，如 person/谭俊、topic/付款流程），必填"},
                "new_title":{"type":"string","description":"新的页面标题/名字，必填"},
                "reason":{"type":"string","description":"改名原因说明，可选，会写进修订历史"}
            },
            "required":["slug","new_title"],
            "additionalProperties":false
        })
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let slug = arg_str(args, "slug")?;
        let new_title = arg_str(args, "new_title")?;
        if new_title.trim().is_empty() {
            anyhow::bail!("新标题不能为空");
        }
        let reason = arg_str_opt(args, "reason").unwrap_or_default();
        let existing = ctx
            .store
            .get_wiki_page(&slug)?
            .with_context(|| format!("知识库没有 slug={slug} 的页面"))?;
        if existing.title.trim() == new_title.trim() {
            anyhow::bail!("「{}」本来就是这个标题，不需要改名", existing.title);
        }
        // 只读预告：将被迁移的关系数 / 页内会话
        let relations_moved = ctx
            .store
            .list_relations()?
            .iter()
            .filter(|r| r.from_slug == slug || r.to_slug == slug)
            .count();
        let has_chat = ctx.store.find_wiki_chat_conversation(&slug)?.is_some();
        let action_args = json!({
            "slug": slug,
            "new_title": new_title,
            "reason": reason,
        });
        store_create_pending(
            ctx.store,
            ctx.conversation_id,
            "rename_wiki_page",
            &action_args,
        )?;
        let mut lines = vec![format!(
            "将知识页「{}」（{}）重命名为「{}」",
            existing.title, slug, new_title
        )];
        if relations_moved > 0 {
            lines.push(format!("· 同步迁移 {relations_moved} 条人物关系引用"));
        }
        if has_chat {
            lines.push("· 页内聊天会话一并迁移到新名字下".to_string());
        }
        if !reason.is_empty() {
            lines.push(format!("· 原因：{reason}"));
        }
        lines.push("—— 回复「好」即生效。".to_string());
        Ok(lines.join("\n"))
    }
}

// ── 任意 URL 导入工具 ─────────────────────────────────────────

/// 把任意网址的内容导入知识库（推文走 fxtwitter，其他走网页文本提取）
struct ImportUrlToWikiTool;
impl Tool for ImportUrlToWikiTool {
    fn name(&self) -> &'static str {
        "import_url_to_wiki"
    }
    fn description(&self) -> &'static str {
        "导入一个网址的内容到知识库（x.com/twitter.com 推文或任意网页）。调用后进入待确认状态，确认后才保存。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "url":{"type":"string","description":"完整网址，必填"},
                "tags":{"type":"array","items":{"type":"string"},"description":"附加标签，可选"}
            },
            "required":["url"],
            "additionalProperties":false
        })
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let url = arg_str(args, "url")?;
        if !url.starts_with("http://") && !url.starts_with("https://") {
            anyhow::bail!("仅支持 http/https 链接");
        }
        let c = crate::wiki::fetch_import_url(&url)?;
        let tags: Vec<String> = args
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let action_args = json!({
            "source_url": c.source_url,
            "source_kind": c.source_kind,
            "title": c.title,
            "content_md": c.content_md,
            "tags": tags,
        });
        store_create_pending(
            ctx.store,
            ctx.conversation_id,
            "import_url_to_wiki",
            &action_args,
        )?;
        // 预览：标题 + 正文开头
        let title = c.title.unwrap_or_else(|| "未命名".to_string());
        let preview: String = c
            .content_md
            .chars()
            .take(160)
            .collect::<String>()
            .trim_end()
            .to_string();
        Ok(format!(
            "已抓取「{title}」并草拟保存（待确认）：\n{preview}{}\n—— 回复「好」即导入知识库。",
            if c.content_md.chars().count() > 160 {
                "…"
            } else {
                ""
            }
        ))
    }
}

// ── 知识页修订工具（页内 AI 处理会话用） ─────────────────────────

/// 保存一篇知识页修订（页内 AI 聊天用；确认后写入 wiki_pages + revisions）
struct SaveWikiRevisionTool;
impl Tool for SaveWikiRevisionTool {
    fn name(&self) -> &'static str {
        "save_wiki_revision"
    }
    fn description(&self) -> &'static str {
        "把处理当前知识页得出的新版本保存到知识库。生成类加工（总结/提炼观点/写文案/翻译等）默认保存为**派生产物**（挂在该页下的新页，不改动当前页）；明确要修改页面本身内容时才用 save_as=revision。调用后进入待确认状态，确认后才保存。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "slug":{"type":"string","description":"页面 slug（保持当前页面不变），必填"},
                "title":{"type":"string","description":"保存后的标题；派生产物建议用便于区分的标题（如「{原标题}·总结」），必填"},
                "content_md":{"type":"string","description":"新内容正文（Markdown，生成的总结/文案就是成品；若是修订则保留旧事实），必填"},
                "change_note":{"type":"string","description":"本次操作说明（如：生成总结 / 写抖音文案 / 补充要点），必填"},
                "save_as":{"type":"string","enum":["derivative","revision"],"description":"derivative=保存为派生产物（默认，推荐：总结/提炼/写文案/翻译等生成类操作）；revision=直接修订当前页正文（仅当用户明确要改这页本身、且该页不是素材原文时）"},
                "content_type":{"type":"string","description":"产物类型标签（save_as=derivative 时必填，如：总结/提炼观点/抖音文案/翻译/学习笔记）"}
            },
            "required":["slug","title","content_md","change_note"],
            "additionalProperties":false
        })
    }
    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let slug = arg_str(args, "slug")?;
        let title = arg_str(args, "title")?;
        let content_md = arg_str(args, "content_md")?;
        let change_note = arg_str(args, "change_note")?;
        let save_as = arg_str_opt(args, "save_as").unwrap_or_else(|| "derivative".to_string());
        if !matches!(save_as.as_str(), "derivative" | "revision") {
            anyhow::bail!("save_as 仅支持 derivative 或 revision");
        }
        let content_type = arg_str_opt(args, "content_type").filter(|s| !s.trim().is_empty());
        // 草拟前先确认页面还在：改名/删除后旧 slug 会变成幽灵目标
        if ctx.store.get_wiki_page(&slug)?.is_none() {
            anyhow::bail!(
                "知识库没有 slug={slug} 的页面。如果这个页面刚改过名，请用新名字操作；不确定 slug 时先在对话里列一下知识库页面（slug 形如 person/xx、topic/xx）。"
            );
        }
        let action_args = json!({
            "slug": slug,
            "title": title.clone(),
            "content_md": content_md,
            "change_note": change_note,
            "save_as": save_as,
            "content_type": content_type.clone(),
        });
        store_create_pending(
            ctx.store,
            ctx.conversation_id,
            "save_wiki_revision",
            &action_args,
        )?;
        let mode = if save_as == "derivative" {
            let ct = content_type
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "内容".to_string());
            format!("作为「{ct}」派生产物（原页不改动）")
        } else {
            "直接修订当前页".to_string()
        };
        Ok(format!(
            "已草拟（{mode}）：{change_note}\n—— 回复「好」即保存。"
        ))
    }
}

// ── 待确认动作的真正执行（用户确认后调用） ─────────────────────────

/// 执行一条待确认动作（写真源）。返回给用户的摘要。
pub fn execute_pending_action(store: &Store, pa: &PendingAction) -> Result<String> {
    let args: Value = serde_json::from_str(&pa.args_json)?;
    match pa.action.as_str() {
        // 历史遗留的 record_event 待确认动作（改直接入库前的旧数据）仍可执行
        "record_event" => insert_event_from_args(&args, store),
        "save_knowledge_draft" => {
            let title = arg_str(&args, "title")?;
            let content_md = arg_str(&args, "content_md")?;
            let kind = arg_str_opt(&args, "kind").unwrap_or_else(|| "topic".to_string());
            let tags: Vec<String> = args
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let summary: String = content_md
                .chars()
                .take(80)
                .collect::<String>()
                .trim_end()
                .to_string();
            let slug = kb_slug(&title);
            let draft = crate::storage::WikiPageDraft {
                slug,
                kind,
                title: title.clone(),
                summary,
                content_md,
                tags,
                source_event_ids: vec![],
                status: "active".to_string(),
                reason: "由 AI 工具草拟、用户确认后保存".to_string(),
                source_url: None,
            };
            let outcome = store.upsert_wiki_page(&draft)?;
            Ok(format!(
                "已保存知识页「{}」（{}，slug={}）",
                title,
                if outcome.created {
                    "新创建"
                } else {
                    "已更新"
                },
                outcome.page.slug
            ))
        }
        "create_todo" => {
            let title = arg_str(&args, "title")?;
            let due_at = arg_str_opt(&args, "due_at");
            let priority = arg_str_opt(&args, "priority").unwrap_or_else(|| "normal".to_string());
            let related_wiki_slug = arg_str_opt(&args, "related_wiki_slug");
            let note = arg_str_opt(&args, "note");
            let t = store.create_todo(
                &title,
                &priority,
                due_at.as_deref(),
                None,
                related_wiki_slug.as_deref(),
                note.as_deref(),
            )?;
            let due = due_at.map(|d| format!("，截止 {d}")).unwrap_or_default();
            Ok(format!("已创建待办「{}」{due}。", t.title))
        }
        "propose_people_relations" => {
            crate::wiki::apply_people_relations(&args, store, &pa.conversation_id)
        }
        "archive_conversations_by_title" => {
            let ids: Vec<String> = args
                .get("ids")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let titles: Vec<String> = args
                .get("titles")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let mut archived = 0usize;
            for id in &ids {
                if store.get_conversation(id)?.is_some() {
                    store.set_conversation_archived(id, true)?;
                    archived += 1;
                }
            }
            Ok(format!("已归档 {archived} 个对话：{}", titles.join("、")))
        }
        "rename_wiki_page" => {
            let slug = arg_str(&args, "slug")?;
            let new_title = arg_str(&args, "new_title")?;
            let reason = arg_str_opt(&args, "reason")
                .filter(|s| !s.trim().is_empty())
                .map(|s| format!("重命名：{s}"))
                .unwrap_or_else(|| "重命名：名称更正".to_string());
            let outcome = store.rename_wiki_page(&slug, &new_title, &reason)?;
            if !outcome.changed {
                anyhow::bail!("「{}」本来就是这个标题，未做改动", outcome.old_title);
            }
            let mut parts = vec![format!(
                "知识页已重命名：「{}」→「{}」（slug: {} → {}）",
                outcome.old_title, outcome.new_title, outcome.old_slug, outcome.new_slug
            )];
            if outcome.relations_moved > 0 {
                parts.push(format!("迁移了 {} 条关系引用", outcome.relations_moved));
            }
            if outcome.chats_moved > 0 {
                parts.push(format!("迁移了 {} 个页内聊天会话", outcome.chats_moved));
            }
            Ok(parts.join("；"))
        }
        "import_url_to_wiki" => {
            let source_url = arg_str(&args, "source_url")?;
            let source_kind = arg_str(&args, "source_kind")?;
            let title = arg_str(&args, "title")?;
            let content_md = arg_str(&args, "content_md")?;
            let tags: Vec<String> = args
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let summary: String = content_md
                .chars()
                .take(80)
                .collect::<String>()
                .trim_end()
                .to_string();
            let slug = kb_slug(&title);
            let mut all_tags = tags;
            all_tags.push("import".to_string());
            all_tags.push(
                if source_kind == "tweet" {
                    "tweet"
                } else {
                    "web"
                }
                .to_string(),
            );
            all_tags.sort();
            all_tags.dedup();
            let draft = crate::storage::WikiPageDraft {
                slug,
                kind: "source".to_string(),
                title: title.clone(),
                summary,
                content_md,
                tags: all_tags,
                source_event_ids: vec![],
                status: "active".to_string(),
                reason: format!("从 {source_url} 导入（AI 对话）"),
                source_url: Some(source_url),
            };
            let outcome = store.upsert_wiki_page(&draft)?;
            Ok(format!(
                "已导入知识库：{}（{}，slug={}）",
                title,
                if outcome.created {
                    "新页面"
                } else {
                    "已更新"
                },
                outcome.page.slug
            ))
        }
        "save_wiki_revision" => {
            let slug = arg_str(&args, "slug")?;
            let title = arg_str(&args, "title")?;
            let content_md = arg_str(&args, "content_md")?;
            let change_note = arg_str(&args, "change_note")?;
            let save_as = arg_str_opt(&args, "save_as").unwrap_or_else(|| "derivative".to_string());
            let content_type = arg_str_opt(&args, "content_type").filter(|s| !s.trim().is_empty());
            let existing = store.get_wiki_page(&slug)?;
            let existing = existing.with_context(|| {
                format!(
                    "知识页不存在：{slug}（可能已被改名或删除，不能对旧名做修订；若需新建请用 save_knowledge_draft）"
                )
            })?;
            // 素材原文锁定：对素材库 / 派生产物区域的页面，一律保存为派生产物，绝不覆盖原文。
            let force_derivative = matches!(existing.area.as_str(), "imported" | "derivative");
            if force_derivative && save_as != "derivative" {
                anyhow::bail!(
                    "「{}」是素材库原文（或派生产物），不能直接覆盖；请改用 save_as=derivative 把加工成果保存为派生产物。",
                    existing.title
                );
            }
            if save_as == "derivative" {
                let ct = content_type
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "AI 加工".to_string());
                let page = store.create_derivative(
                    &slug,
                    &ct,
                    &title,
                    &content_md,
                    &format!("AI 加工派生：{change_note}"),
                )?;
                Ok(format!(
                    "已保存「{}」的派生产物（{ct}）：{change_note}。原文未改动。",
                    page.title
                ))
            } else {
                let summary: String = content_md
                    .chars()
                    .take(80)
                    .collect::<String>()
                    .trim_end()
                    .to_string();
                // 保留原页面的 kind、source_url、area 与 tags
                let kind = existing.kind.clone();
                let source_url = existing.source_url.clone();
                let tags = existing.tags.clone();
                let draft = crate::storage::WikiPageDraft {
                    slug,
                    kind,
                    title: title.clone(),
                    summary,
                    content_md,
                    tags,
                    source_event_ids: vec![],
                    status: "active".to_string(),
                    reason: format!("AI 页内修订：{change_note}"),
                    source_url,
                };
                let outcome = store.upsert_wiki_page(&draft)?;
                Ok(format!(
                    "知识页「{}」修订已保存：{change_note}",
                    outcome.page.title
                ))
            }
        }
        other => anyhow::bail!("未知待执行动作：{other}"),
    }
}

/// 由标题生成一个唯一、可读的 slug（保留中文，附加短随机后缀）
fn kb_slug(title: &str) -> String {
    let mut base = String::new();
    for ch in title.chars() {
        if ch.is_alphanumeric() {
            base.push(ch);
        } else if ch.is_whitespace() || ch == '-' || ch == '_' {
            base.push('-');
        }
    }
    let base: String = base.trim_matches('-').chars().take(40).collect();
    let base = if base.is_empty() {
        "knowledge".to_string()
    } else {
        base
    };
    format!("kb-{}-{}", base, &uuid::Uuid::new_v4().to_string()[..8])
}

use uuid::Uuid;

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (Store, std::path::PathBuf) {
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

    #[test]
    fn registry_has_builtin_tools() {
        let registry = ToolRegistry::default();
        let names = registry.names();
        for expected in [
            "list_rules",
            "list_wiki_pages",
            "get_wiki_page",
            "search_knowledge_base",
            "fetch_tweet",
            "fetch_page",
            "record_event",
            "save_knowledge_draft",
            "list_todos",
            "create_todo",
            "propose_people_relations",
            "batch_extract_people_relations",
            "list_events_by_date",
            "archive_conversations_by_title",
            "rename_wiki_page",
            "import_url_to_wiki",
            "save_wiki_revision",
        ] {
            assert!(names.contains(&expected), "缺少工具 {expected}");
        }
        assert_eq!(registry.provider_specs().len(), names.len());
    }

    #[test]
    fn dispatch_unknown_tool_is_error_message() {
        let (store, path) = temp_db();
        let registry = ToolRegistry::default();
        let call = ToolCall::new("not_a_tool", json!({}));
        let result = dispatch(&call, &registry, &store, "conv-1");
        assert!(result.content.contains("未知工具"), "{}", result.content);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn list_rules_read_tool_works() {
        let (store, path) = temp_db();
        store
            .add_rule(
                "和大型企业的人沟通重要事项必须留痕",
                crate::storage::RuleStatus::Active,
                None,
            )
            .unwrap();
        let registry = ToolRegistry::default();
        let call = ToolCall::new("list_rules", json!({}));
        let result = dispatch(&call, &registry, &store, "conv-1");
        assert!(result.content.contains("留痕"), "{}", result.content);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn record_event_writes_directly() {
        let (store, path) = temp_db();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        let registry = ToolRegistry::default();
        let call = ToolCall::new(
            "record_event",
            json!({"raw_text": "昨天和张玮沟通了双链路付款"}),
        );
        let result = dispatch(&call, &registry, &store, &conv);
        assert!(result.content.contains("已保存事件"), "{}", result.content);
        // 直接写入了 events 真源
        assert_eq!(store.list_events().unwrap().len(), 1);
        // 不产生待确认动作
        assert!(store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty());
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn execute_record_event_writes_real_source() {
        let (store, path) = temp_db();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        let action_id = store
            .create_pending_action(
                &conv,
                "record_event",
                &json!({"raw_text": "去健身房练了背部"}).to_string(),
            )
            .unwrap();
        let pa = store.pending_actions_for_conversation(&conv).unwrap();
        let summary = execute_pending_action(&store, &pa[0]).unwrap();
        assert!(summary.contains("已保存"), "{summary}");
        assert_eq!(store.list_events().unwrap().len(), 1);
        let _ = action_id;
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn execute_knowledge_draft_creates_wiki_page() {
        let (store, path) = temp_db();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        store
            .create_pending_action(
                &conv,
                "save_knowledge_draft",
                &json!({"title": "大企业沟通留痕", "content_md": "和大型企业的人沟通重要事项必须留痕。", "kind": "principle", "tags": ["沟通"]}).to_string(),
            )
            .unwrap();
        let pa = store.pending_actions_for_conversation(&conv).unwrap();
        let summary = execute_pending_action(&store, &pa[0]).unwrap();
        assert!(summary.contains("已保存"), "{summary}");
        let pages = store.list_wiki_pages(None, None).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].kind, "principle");
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn propose_people_relations_is_confirm_gated_then_saves() {
        let (store, path) = temp_db();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        let registry = ToolRegistry::default();

        // 1) 调用工具：只登记待确认动作，断言尚未写任何页面
        let call = ToolCall::new(
            "propose_people_relations",
            json!({
                "people": [
                    {"name": "张玮", "role_note": "双链路付款项目产研负责人"},
                    {"name": "和太极", "role_note": "外部合作方"}
                ],
                "relations": [
                    {"person": "张玮", "target": "双链路付款", "relation": "负责"},
                    {"person": "和太极", "target": "双链路付款", "relation": "合作"}
                ]
            }),
        );
        let result = dispatch(&call, &registry, &store, &conv);
        assert!(result.content.contains("待确认"), "{}", result.content);
        assert!(
            store.list_wiki_pages(None, None).unwrap().is_empty(),
            "确认前不应建档"
        );
        let pendings = store.pending_actions_for_conversation(&conv).unwrap();
        assert_eq!(pendings.len(), 1);

        // 2) 确认后执行：人物页 + 目标页 + 关系落地
        let summary = execute_pending_action(&store, &pendings[0]).unwrap();
        assert!(summary.contains("张玮"), "{summary}");
        assert!(summary.contains("和太极"), "{summary}");
        assert!(summary.contains("负责"), "{summary}");
        assert_eq!(
            store.list_wiki_pages(Some("person"), None).unwrap().len(),
            2
        );
        assert_eq!(
            store.list_relations_for_page("person/张玮").unwrap().len(),
            1
        );
        assert_eq!(
            store
                .list_relations_for_page("topic/双链路付款")
                .unwrap()
                .len(),
            2,
            "目标页自动建档，且两边关系都能查到"
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn list_events_by_date_returns_days_events() {
        let (store, path) = temp_db();
        let registry = ToolRegistry::default();
        let today = chrono::Local::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);
        store
            .insert_event(crate::event::NewEvent {
                raw_text: "今天的事件",
                occurred_at: chrono::Utc::now(),
                recorded_at: chrono::Utc::now(),
                source: "test",
            })
            .unwrap();
        store
            .insert_event(crate::event::NewEvent {
                raw_text: "昨天的事件",
                occurred_at: chrono::Utc::now() - chrono::Duration::days(1),
                recorded_at: chrono::Utc::now() - chrono::Duration::days(1),
                source: "test",
            })
            .unwrap();

        let call = ToolCall::new(
            "list_events_by_date",
            json!({"date": today.format("%Y-%m-%d").to_string()}),
        );
        let result = dispatch(&call, &registry, &store, "conv-1");
        assert!(result.content.contains("今天的事件"), "{}", result.content);
        assert!(!result.content.contains("昨天的事件"), "{}", result.content);
        assert!(result.content.contains("条事件"), "{}", result.content);

        let call = ToolCall::new(
            "list_events_by_date",
            json!({"date": yesterday.format("%Y-%m-%d").to_string()}),
        );
        let result = dispatch(&call, &registry, &store, "conv-1");
        assert!(result.content.contains("昨天的事件"), "{}", result.content);

        // 坏格式与空结果都友好返回
        let call = ToolCall::new("list_events_by_date", json!({"date": "2026/06/20"}));
        let result = dispatch(&call, &registry, &store, "conv-1");
        assert!(result.content.contains("YYYY-MM-DD"), "{}", result.content);
        let call = ToolCall::new("list_events_by_date", json!({"date": "2030-01-01"}));
        let result = dispatch(&call, &registry, &store, "conv-1");
        assert!(
            result.content.contains("没有事件记录"),
            "{}",
            result.content
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn archive_conversations_by_title_is_confirm_gated_then_archives() {
        let (store, path) = temp_db();
        let registry = ToolRegistry::default();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        // 主对话：空标题（显示为「新对话」）、Hello 两个、其他
        store.create_conversation(None, None).unwrap(); // 显示「新对话」
        store
            .create_conversation(Some("Hello World"), None)
            .unwrap();
        store
            .create_conversation(Some("Phase Hello 2"), None)
            .unwrap();
        store.create_conversation(Some("其他"), None).unwrap();
        // 知识页内聊天：不应被匹配
        store
            .create_wiki_chat_conversation("person/x", "页内对话")
            .unwrap();

        // 1) 包含匹配：草拟两个 Hello，未确认前不归档
        let call = ToolCall::new(
            "archive_conversations_by_title",
            json!({"contains": "hello"}),
        );
        let result = dispatch(&call, &registry, &store, &conv);
        assert!(result.content.contains("2 个匹配"), "{}", result.content);
        assert!(result.content.contains("Hello World"), "{}", result.content);
        let pendings = store.pending_actions_for_conversation(&conv).unwrap();
        assert_eq!(pendings.len(), 1);
        assert_eq!(store.list_conversations().unwrap().len(), 5, "确认前不归档");

        // 2) 确认执行：归档 2 个（执行不删 pending，手动删以模拟确认流）
        let summary = execute_pending_action(&store, &pendings[0]).unwrap();
        assert!(summary.contains("已归档 2 个对话"), "{summary}");
        store.delete_pending_action(&pendings[0].id).unwrap();
        let remaining_titles: Vec<String> = store
            .list_conversations()
            .unwrap()
            .iter()
            .map(|c| c.title.as_deref().unwrap_or("新对话").to_string())
            .collect();
        assert_eq!(remaining_titles.len(), 3);
        for t in ["t", "新对话", "其他"] {
            assert!(
                remaining_titles.iter().any(|x| x == t),
                "缺 {t}: {remaining_titles:?}"
            );
        }

        // 3) 精确匹配「新对话」= 空标题会话
        let call = ToolCall::new("archive_conversations_by_title", json!({"title": "新对话"}));
        let result = dispatch(&call, &registry, &store, &conv);
        assert!(result.content.contains("1 个匹配"), "{}", result.content);
        let pendings = store.pending_actions_for_conversation(&conv).unwrap();
        assert_eq!(pendings.len(), 1);
        execute_pending_action(&store, &pendings[0]).unwrap();
        store.delete_pending_action(&pendings[0].id).unwrap();
        let remaining_titles: Vec<String> = store
            .list_conversations()
            .unwrap()
            .iter()
            .map(|c| c.title.as_deref().unwrap_or("新对话").to_string())
            .collect();
        assert_eq!(remaining_titles.len(), 2);
        assert!(
            !remaining_titles.iter().any(|x| x == "新对话"),
            "{remaining_titles:?}"
        );

        // 4) 无匹配：友好错误，不登记
        let call = ToolCall::new(
            "archive_conversations_by_title",
            json!({"contains": "不存在的"}),
        );
        let result = dispatch(&call, &registry, &store, &conv);
        assert!(result.content.contains("没有找到"), "{}", result.content);
        assert!(store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty());

        // 5) 知识页聊天始终未被动过
        assert!(
            store
                .get_conversation(
                    &store
                        .find_wiki_chat_conversation("person/x")
                        .unwrap()
                        .unwrap()
                )
                .unwrap()
                .unwrap()
                .archived
                == false
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rename_wiki_page_is_confirm_gated_then_renames() {
        let (store, path) = temp_db();
        let registry = ToolRegistry::default();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        store
            .upsert_wiki_page(&crate::storage::WikiPageDraft {
                slug: "topic/付款流程".to_string(),
                kind: "topic".to_string(),
                title: "付款流程".to_string(),
                summary: "付款流程（自动建档）".to_string(),
                content_md: "# 付款流程".to_string(),
                tags: vec![],
                source_event_ids: vec![],
                status: "active".to_string(),
                reason: "test".to_string(),
                source_url: None,
            })
            .unwrap();
        store
            .upsert_relation(&crate::storage::RelationDraft {
                from_slug: "person/谭俊".to_string(),
                from_kind: "person".to_string(),
                to_slug: "topic/付款流程".to_string(),
                to_kind: "topic".to_string(),
                relation: "跟进".to_string(),
                note: None,
                confidence: 3,
                source_conversation_id: Some(conv.clone()),
            })
            .unwrap();

        // 1) 草拟：确认前不动任何数据
        let call = ToolCall::new(
            "rename_wiki_page",
            json!({"slug": "topic/付款流程", "new_title": "fpso111 尾款", "reason": "项目真名更正"}),
        );
        let result = dispatch(&call, &registry, &store, &conv);
        assert!(result.content.contains("重命名为"), "{}", result.content);
        assert!(
            result.content.contains("1 条人物关系"),
            "{}",
            result.content
        );
        let pendings = store.pending_actions_for_conversation(&conv).unwrap();
        assert_eq!(pendings.len(), 1);
        assert!(
            store.get_wiki_page("topic/付款流程").unwrap().is_some(),
            "确认前不应改名"
        );

        // 2) 确认执行：改名 + 关系引用迁移
        let summary = execute_pending_action(&store, &pendings[0]).unwrap();
        assert!(summary.contains("fpso111 尾款"), "{summary}");
        assert!(summary.contains("迁移了 1 条关系引用"), "{summary}");
        assert!(store.get_wiki_page("topic/付款流程").unwrap().is_none());
        assert_eq!(
            store
                .get_wiki_page("topic/fpso111-尾款")
                .unwrap()
                .unwrap()
                .title,
            "fpso111 尾款"
        );
        assert_eq!(
            store.list_relations().unwrap()[0].to_slug,
            "topic/fpso111-尾款"
        );

        // 3) 不存在 / 标题没变：友好错误，不登记 pending
        let call = ToolCall::new(
            "rename_wiki_page",
            json!({"slug": "topic/不存在", "new_title": "x"}),
        );
        let result = dispatch(&call, &registry, &store, &conv);
        assert!(
            result.content.contains("没有 slug=topic/不存在 的页面"),
            "{}",
            result.content
        );
        let call = ToolCall::new(
            "rename_wiki_page",
            json!({"slug": "topic/fpso111-尾款", "new_title": "fpso111 尾款"}),
        );
        let result = dispatch(&call, &registry, &store, &conv);
        assert!(
            result.content.contains("本来就是这个标题"),
            "{}",
            result.content
        );
        assert_eq!(
            store.pending_actions_for_conversation(&conv).unwrap().len(),
            1
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn save_wiki_revision_rejects_missing_slug_and_never_creates_ghost_page() {
        let (store, path) = temp_db();
        let registry = ToolRegistry::default();
        let conv = store.create_conversation(Some("t"), None).unwrap();
        // 页不存在（如改名后的旧 slug）：草拟阶段直接报错，不登记 pending
        let call = ToolCall::new(
            "save_wiki_revision",
            json!({
                "slug": "topic/fpso111整船项目",
                "title": "FPSO111整船项目",
                "content_md": "# x",
                "change_note": "补充内容"
            }),
        );
        let result = dispatch(&call, &registry, &store, &conv);
        assert!(
            result
                .content
                .contains("没有 slug=topic/fpso111整船项目 的页面"),
            "{}",
            result.content
        );
        assert!(store
            .pending_actions_for_conversation(&conv)
            .unwrap()
            .is_empty());
        assert!(
            store
                .get_wiki_page("topic/fpso111整船项目")
                .unwrap()
                .is_none(),
            "不能新建幽灵页"
        );

        // 绕过草拟直接登记 pending + 执行：页面不存在时必须报错而非 upsert 新建
        store
            .create_pending_action(
                &conv,
                "save_wiki_revision",
                &json!({
                    "slug": "topic/fpso111整船项目",
                    "title": "FPSO111整船项目",
                    "content_md": "# x",
                    "change_note": "补充内容"
                })
                .to_string(),
            )
            .unwrap();
        let pa = store.pending_actions_for_conversation(&conv).unwrap();
        let err = execute_pending_action(&store, &pa[0]).unwrap_err();
        assert!(err.to_string().contains("知识页不存在"), "{err}");
        assert!(store
            .get_wiki_page("topic/fpso111整船项目")
            .unwrap()
            .is_none());
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn fetch_page_strips_html() {
        let text = html_to_text("<html><head><style>.x{}</style></head><body><h1>标题</h1><p>正文内容</p></body></html>");
        assert!(text.contains("标题"), "{text}");
        assert!(text.contains("正文内容"), "{text}");
        assert!(!text.contains("<"), "{text}");
    }
}
