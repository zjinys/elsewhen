//! query_tools.rs：原 tool/mod.rs 同名分段外移，行为零变化。

use super::Tool;
use super::dispatch::{arg_str, arg_str_opt};
use super::write_tools::store_create_pending;
use super::{ToolContext, ToolPolicy};
use anyhow::{Context, Result};
use serde_json::{Value, json};

// ── 查询 / 会话管理工具 ────────────────────────────────────────

/// 查看某一天记录的事件（只读）
pub(super) struct ListEventsByDateTool;
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
pub(super) struct ArchiveConversationsByTitleTool;
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
    fn deferred(&self) -> bool {
        true
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
            "找到 {} 个匹配的对话，将归档：\n{}\n—— 在审阅卡点击接受即归档。",
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
pub(super) struct RenameWikiPageTool;
impl Tool for RenameWikiPageTool {
    fn name(&self) -> &'static str {
        "rename_wiki_page"
    }
    fn description(&self) -> &'static str {
        "重命名一个知识页（改标题；person/项目 等带前缀的页面会把唯一标识 slug 一起换成新名字，相关的联系人关系引用与页内聊天会话自动迁移）。用户说「把 X 改名为 Y」「这个项目不叫 X，实际叫 Y」时使用。草拟确认制：调用后用户确认才真正改名。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "slug":{"type":"string","description":"当前页面的 slug（唯一标识，如 person/张三、topic/网站改版），必填"},
                "new_title":{"type":"string","description":"新的页面标题/名字，必填"},
                "reason":{"type":"string","description":"改名原因说明，可选，会写进修订历史"}
            },
            "required":["slug","new_title"],
            "additionalProperties":false
        })
    }
    fn deferred(&self) -> bool {
        true
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
            lines.push(format!("· 同步迁移 {relations_moved} 条联系人关系引用"));
        }
        if has_chat {
            lines.push("· 页内聊天会话一并迁移到新名字下".to_string());
        }
        if !reason.is_empty() {
            lines.push(format!("· 原因：{reason}"));
        }
        lines.push("—— 在审阅卡点击接受即生效。".to_string());
        Ok(lines.join("\n"))
    }
}

// ── 任意 URL 导入工具 ─────────────────────────────────────────

/// 把任意网址的内容导入知识库（推文走 fxtwitter，其他走网页文本提取）
pub(super) struct ImportUrlToWikiTool;
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
    fn deferred(&self) -> bool {
        true
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
        let existing = ctx
            .store
            .matching_source_page(Some(&c.source_url), &c.content_md)?;
        let snapshot = existing
            .as_ref()
            .map(|p| ctx.store.source_history(&p.slug))
            .transpose()?
            .and_then(|h| h.into_iter().next());
        let change = snapshot
            .as_ref()
            .map(|s| crate::storage::knowledge::source_change_preview(&s.content_md, &c.content_md))
            .unwrap_or_default();
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
            "expected_snapshot_id":snapshot.map(|s|s.id),
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
            "已抓取「{title}」并草拟保存（待确认）：\n{preview}{}\n{change}\n—— 在审阅卡点击接受保存该版本，在审阅卡点击拒绝放弃；旧原文保留。",
            if c.content_md.chars().count() > 160 {
                "…"
            } else {
                ""
            }
        ))
    }
}
