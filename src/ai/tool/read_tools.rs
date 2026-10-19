//! read_tools.rs：原 tool/mod.rs 同名分段外移，行为零变化。

use super::Tool;
use super::ToolContext;
use super::dispatch::{arg_i64, arg_str, arg_str_opt};
use anyhow::{Context, Result};
use serde_json::{Value, json};

// ── 只读工具 ────────────────────────────────────────────────────

/// 列出个人规则库中的全部规则
pub(super) struct ListRulesTool;
impl Tool for ListRulesTool {
    fn name(&self) -> &'static str {
        "list_rules"
    }
    fn description(&self) -> &'static str {
        "列出个人规则库中已生效的全部经验规则。在用户分享踩坑/教训/心得类经历、或回复需要对照既有经验规范时调用；规则库原文不随对话注入，需要时再查。"
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
pub(super) struct ListWikiPagesTool;
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
                "- {}（{}，slug: {}）{status}：{}\n",
                p.title, p.kind, p.slug, p.summary
            ));
        }
        // 预防：模型常拿本列表的 slug 直接当 save_knowledge_draft 的来源，
        // 被证据校验拒绝后又圆谎说已保存。在此提前声明限制。
        out.push_str(
            "（注意：以上 slug 仅供查看页面用；要作为 save_knowledge_draft 的 source_slugs，\n\
             必须先 search_knowledge_base 或 get_wiki_page 把该页提供给当前对话。）",
        );
        Ok(out)
    }
}

/// 获取单个知识库页面全文
pub(super) struct GetWikiPageTool;
impl Tool for GetWikiPageTool {
    fn name(&self) -> &'static str {
        "get_wiki_page"
    }
    fn description(&self) -> &'static str {
        "按 slug 读取知识页（长文返回限长片段），带来源状态。先用 list_wiki_pages 拿到 slug。"
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
        let citation = crate::knowledge::citation_for_page_explicit(
            ctx.store,
            &page,
            "用户明确打开此页".into(),
            2400,
        )?;
        match citation {
            Some(citation) => crate::knowledge::tool_context(&[citation]),
            None => Ok(json!({"knowledge_candidates":[],"unverified_page":{
                "slug":page.slug,"title":page.title,
                "content_md":page.content_md.chars().take(2400).collect::<String>(),
                "notice":"仅供读取与编辑；缺少有效来源、来源待复核或已被拒绝，不可作为正面证据。"
            }})
            .to_string()),
        }
    }
}

/// 每日 token 用量统计
pub(super) struct DailyTokenUsageTool;
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
pub(super) struct SearchKnowledgeBaseTool;
impl Tool for SearchKnowledgeBaseTool {
    fn name(&self) -> &'static str {
        "search_knowledge_base"
    }
    fn description(&self) -> &'static str {
        "在知识库页面与个人事件记录中按关键词搜索，返回相关片段（带引用 slug）。\
         涉及用户过往细节或约定时先检索，闲聊不必检索。"
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
        let mut candidates = crate::knowledge::select_knowledge(ctx.store, &query, "search", 4000)?;
        candidates.truncate(limit);
        let mut events = Vec::new();
        let mut stmt = ctx.store.connection.prepare(
            "SELECT id,substr(raw_text,1,350) FROM events
            WHERE instr(lower(raw_text),lower(?1))>0 ORDER BY recorded_at DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![query, limit.min(8) as i64], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, text) in rows {
            if ctx.store.recordable_event(&id)? {
                events.push(json!({"event_id":id,"excerpt":text,"category":"用户记录"}));
            }
        }
        Ok(json!({"knowledge_candidates":candidates,"events":events}).to_string())
    }
}

/// 抓取推文长文（只解析，不入库）
pub(super) struct FetchTweetTool;
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
    fn deferred(&self) -> bool {
        true
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
/// 外网搜索：只在用户显式要求（时效词/搜一下/网上查）时由模型调用。
/// 未启用/未配置/无结果都返回诚实说明，不编造。
pub(super) struct WebSearchTool;
impl Tool for WebSearchTool {
    fn name(&self) -> &'static str {
        "web_search"
    }
    fn description(&self) -> &'static str {
        "搜索外网获取最新/时效信息（新闻、热点）。只在用户明确要求或查时效内容时调用；拿不准先反问是否联网搜。只读，不写入。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{"query":{"type":"string","description":"搜索关键词"}},
            "required":["query"],
            "additionalProperties":false
        })
    }
    fn deferred(&self) -> bool {
        true
    }
    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let query = arg_str(args, "query")?;
        let cfg = ctx.store.web_search_config()?;
        if !cfg.enabled {
            return Ok(
                "外网搜索未启用。请在「设置 → 外网搜索」开启并配置搜索引擎后再试。".to_string(),
            );
        }
        // SearxNG 自托管无需 key；Tavily/Brave 需要。需 key 而未填时给出明确引导。
        if crate::web_search::engine_needs_api_key(&cfg.engine) && cfg.api_key.trim().is_empty() {
            return Ok(format!(
                "外网搜索已启用但引擎「{}」尚未填写 API key。请在「设置 → 外网搜索」填入，或改用无需 key 的 SearxNG。",
                cfg.engine
            ));
        }
        let network = ctx.store.network_settings()?;
        let max = arg_i64(args, "max_results", cfg.max_results).clamp(1, 10) as u32;
        let engine = crate::web_search::build_engine(
            &cfg.engine,
            &cfg.api_key,
            &cfg.base_url,
            &network,
            std::time::Duration::from_secs(20),
        )?;
        let hits = engine.search(&query, max)?;
        if hits.is_empty() {
            return Ok(format!("关于「{query}」未检索到结果。"));
        }
        // 总量封顶，防止超长上下文。
        let mut out = format!("关于「{query}」的外网检索结果（{} 条）：\n", hits.len());
        let mut budget = 3000usize;
        for (i, h) in hits.iter().enumerate() {
            let entry = format!("{}. {}\n   {}\n   {}\n", i + 1, h.title, h.url, h.snippet);
            if entry.len() > budget {
                out.push_str("（结果过多，其余略）\n");
                break;
            }
            budget -= entry.len();
            out.push_str(&entry);
        }
        Ok(out)
    }
}

pub(super) struct FetchPageTool;
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
    fn deferred(&self) -> bool {
        true
    }
    fn run(&self, args: &Value, _ctx: &ToolContext) -> Result<String> {
        let url = arg_str(args, "url")?;
        fetch_page_plain_text(&url)
    }
}

pub(super) fn fetch_page_plain_text(url: &str) -> Result<String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        anyhow::bail!("仅支持 http/https 链接");
    }
    let client = crate::ai::provider::shared_blocking_client(20)?;
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
        match find_case_insensitive(rest, open) {
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

/// 在 haystack 中定位 needle 首次出现（忽略 ASCII 大小写），返回的字节偏移
/// 落在 UTF-8 字符边界上，可直接对原串切片。
/// 不能用 `haystack.to_lowercase().find(needle)`：to_lowercase 会改变编码长度
/// （如 İ→i̇ 从 2 字节变 3 字节），换算回的索引套到原串上会越界或切中字符
/// 中间，导致 panic。
fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let needle_chars: Vec<char> = needle.chars().map(|c| c.to_ascii_lowercase()).collect();
    if needle_chars.is_empty() {
        return Some(0);
    }
    let n = needle_chars.len();
    for (idx, _) in haystack.char_indices() {
        let fragment: Vec<char> = haystack[idx..]
            .chars()
            .take(n)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        if fragment.len() == n && fragment == needle_chars {
            return Some(idx);
        }
    }
    None
}
