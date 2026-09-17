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
            Box::new(ImportUrlToWikiTool),
            Box::new(SaveWikiRevisionTool),
        ];
        Self { tools }
    }
}

impl ToolRegistry {
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.iter().find(|t| t.name() == name).map(|b| b.as_ref())
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
    args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn arg_i64(args: &Value, key: &str, default: i64) -> i64 {
    args.get(key)
        .and_then(|v| v.as_i64())
        .unwrap_or(default)
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
            Some(k) => ctx.store.list_wiki_pages(Some(&k))?,
            None => ctx.store.list_wiki_pages(None)?,
        };
        if pages.is_empty() {
            return Ok("知识库暂无页面".to_string());
        }
        let mut out = format!("知识库共 {} 页：\n", pages.len());
        for p in pages {
            let status = if p.status == "active" { "" } else { "（非 active）" };
            out.push_str(&format!("- {}（{}）{status}：{}\n", p.title, p.kind, p.summary));
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
                    let name: String = tag_end.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
                    Some(after.find(&format!("</{name}>")).map(|c| c + name.len() + 3))
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
        "把一段经历/事件直接记录为个人事件，立即保存到事件记录（无需确认）。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "raw_text":{"type":"string","description":"事件内容（第一人称，具体），必填"},
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
        store_create_pending(ctx.store, ctx.conversation_id, "save_knowledge_draft", &action_args)?;
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
                if note.is_empty() { String::new() } else { format!("（{note}）") }
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

// ── 任意 URL 导入工具 ─────────────────────────────────────────

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
                if note.is_empty() { String::new() } else { format!("（{note}）") }
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
        store_create_pending(ctx.store, ctx.conversation_id, "import_url_to_wiki", &action_args)?;
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
            if c.content_md.chars().count() > 160 { "…" } else { "" }
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
        "把处理当前知识页得出的新版本（总结/补充/改写）写回知识库。调用后进入待确认状态，确认后才保存。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "slug":{"type":"string","description":"页面 slug（保持当前页面不变），必填"},
                "title":{"type":"string","description":"页面标题，必填"},
                "content_md":{"type":"string","description":"页面新正文（Markdown，在旧内容基础上修订，不丢失旧事实），必填"},
                "change_note":{"type":"string","description":"本次修改说明，必填"}
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
        let action_args = json!({
            "slug": slug,
            "title": title.clone(),
            "content_md": content_md,
            "change_note": change_note,
        });
        store_create_pending(ctx.store, ctx.conversation_id, "save_wiki_revision", &action_args)?;
        Ok(format!(
            "已草拟知识页「{title}」的新版本（待确认）：\n修改说明：{change_note}\n—— 回复「好」即保存。"
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
                if outcome.created { "新创建" } else { "已更新" },
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
            all_tags.push(if source_kind == "tweet" { "tweet" } else { "web" }.to_string());
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
                if outcome.created { "新页面" } else { "已更新" },
                outcome.page.slug
            ))
        }
        "save_wiki_revision" => {
            let slug = arg_str(&args, "slug")?;
            let title = arg_str(&args, "title")?;
            let content_md = arg_str(&args, "content_md")?;
            let change_note = arg_str(&args, "change_note")?;
            let summary: String = content_md
                .chars()
                .take(80)
                .collect::<String>()
                .trim_end()
                .to_string();
            // 保留原页面的 kind、source_url 与 tags
            let existing = store.get_wiki_page(&slug)?;
            let kind = existing
                .as_ref()
                .map(|p| p.kind.clone())
                .unwrap_or_else(|| "topic".to_string());
            let source_url = existing.as_ref().and_then(|p| p.source_url.clone());
            let tags = existing.as_ref().map(|p| p.tags.clone()).unwrap_or_default();
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
    let base = if base.is_empty() { "knowledge".to_string() } else { base };
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
            .add_rule("和大型企业的人沟通重要事项必须留痕", crate::storage::RuleStatus::Active, None)
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
        assert!(store.pending_actions_for_conversation(&conv).unwrap().is_empty());
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
        let pages = store.list_wiki_pages(None).unwrap();
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
        assert!(store.list_wiki_pages(None).unwrap().is_empty(), "确认前不应建档");
        let pendings = store.pending_actions_for_conversation(&conv).unwrap();
        assert_eq!(pendings.len(), 1);

        // 2) 确认后执行：人物页 + 目标页 + 关系落地
        let summary = execute_pending_action(&store, &pendings[0]).unwrap();
        assert!(summary.contains("张玮"), "{summary}");
        assert!(summary.contains("和太极"), "{summary}");
        assert!(summary.contains("负责"), "{summary}");
        assert_eq!(store.list_wiki_pages(Some("person")).unwrap().len(), 2);
        assert_eq!(
            store.list_relations_for_page("person/张玮").unwrap().len(),
            1
        );
        assert_eq!(
            store.list_relations_for_page("topic/双链路付款").unwrap().len(),
            2,
            "目标页自动建档，且两边关系都能查到"
        );
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