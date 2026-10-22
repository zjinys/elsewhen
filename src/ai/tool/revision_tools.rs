//! revision_tools.rs：原 tool/mod.rs 同名分段外移，行为零变化。

use super::Tool;
use super::dispatch::{arg_str, arg_str_opt};
use super::write_tools::{insert_event_from_args, store_create_pending};
use super::{ToolContext, ToolPolicy};
use crate::storage::{ContentPolicy, PendingAction, Store};
use anyhow::{Context, Result};
use serde_json::{Value, json};

// ── 知识页修订工具（页内 AI 处理会话用） ─────────────────────────

/// 保存一篇知识页修订（页内 AI 聊天用；确认后写入 wiki_pages + revisions）
pub(super) struct SaveWikiRevisionTool;
impl Tool for SaveWikiRevisionTool {
    fn name(&self) -> &'static str {
        "save_wiki_revision"
    }
    fn description(&self) -> &'static str {
        "把处理当前知识页得出的新版本草拟保存。生成类加工默认保存为派生产物，不改原文；明确修改本页才用 save_as=revision。确认修订个人规则也表示确认修订后的规则及适用条件，保留原强度；调用后等待用户确认。"
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
                "content_type":{"type":"string","description":"产物类型标签（save_as=derivative 时必填，如：总结/提炼观点/抖音文案/翻译/学习笔记）"},
                "applicable_when":{"type":"string","description":"修订后的适用条件；省略则保留已有条件"}
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
        let current = ctx.store.get_wiki_page(&slug)?;
        if current.is_none() {
            anyhow::bail!(
                "知识库没有 slug={slug} 的页面。如果这个页面刚改过名，请用新名字操作；不确定 slug 时先在对话里列一下知识库页面（slug 形如 person/xx、topic/xx）。"
            );
        }
        if save_as != "derivative"
            && current
                .as_ref()
                .is_some_and(|p| matches!(p.area.as_str(), "imported" | "derivative"))
        {
            anyhow::bail!(
                "当前页是受保护原料或派生产物，不能覆盖原文；请用 save_as=derivative 草拟新的派生产物，等待用户确认"
            );
        }
        let current = current.context("知识页不存在")?;
        let task = ctx.store.current_page_task(ctx.conversation_id)?;
        let mut content_md = content_md;
        let mut title = title;
        if let Some(task) = &task {
            task.validate(ctx.store)?;
            anyhow::ensure!(
                task.pages.first().is_some_and(|p| p.slug == slug),
                "当前任务没有选择这个修订目标"
            );
            anyhow::ensure!(
                save_as == "derivative"
                    || matches!(task.kind.as_str(), "local_edit" | "full_revision"),
                "问答、总结和对照任务只能草拟派生产物，不能直接修订本页。要修改正文：\
                 在对话上方『材料范围』中切换到『局部编辑』并框选要改的文字，\
                 或选择『全页修订』后重新发送；也可以直接明确要求修改这一页，\
                 系统会自动切换到编辑任务生成待审草稿"
            );
            if task.kind == "local_edit" {
                let chars: Vec<_> = current.content_md.chars().collect();
                let (start, end) = (
                    task.start_char.context("缺少选区")?,
                    task.end_char.context("缺少选区")?,
                );
                content_md = format!(
                    "{}{}{}",
                    chars[..start].iter().collect::<String>(),
                    content_md,
                    chars[end..].iter().collect::<String>()
                );
                title = current.title.clone();
            }
        }
        let basis = crate::knowledge::authoring::revision_basis(ctx.store, &current)?;
        let applicable = arg_str_opt(args, "applicable_when")
            .unwrap_or_else(|| basis["applicable_when"].as_str().unwrap_or("").to_owned());
        anyhow::ensure!(applicable.chars().count() <= 600, "适用条件最多 600 字");
        anyhow::ensure!(
            basis["strength"] != "rule" || !applicable.trim().is_empty(),
            "规则必须保留适用条件"
        );
        let action_args = json!({
            "slug": slug,
            "title": title.clone(),
            "content_md": content_md,
            "change_note": change_note,
            "save_as": save_as,
            "content_type": content_type.clone(),
            "base_hash":crate::storage::knowledge::content_hash(&current.content_md),
            "revision_basis":basis,
            "page_task":task,
            "applicable_when":applicable,
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
            format!(
                "直接修订当前页；适用条件：{}。若为个人规则，确认即确认修订后的规则与条件，保留原强度",
                applicable
            )
        };
        Ok(format!(
            "已草拟（{mode}）：{change_note}\n—— 在审阅卡点击接受即保存。"
        ))
    }
}

// ── 待确认动作的真正执行（用户确认后调用） ─────────────────────────

/// 执行一条待确认动作（写真源）。返回给用户的摘要。
pub fn execute_pending_action(store: &Store, pa: &PendingAction) -> Result<String> {
    let args: Value = serde_json::from_str(&pa.args_json)?;
    match pa.action.as_str() {
        // 用户技能脚本：路径与环境校验在确认时重做一遍，不信任 pending 时的状态
        "run_skill_script" => {
            let data_dir = crate::config::AppConfig::load()?.data_dir;
            super::skill_script::execute_confirmed(&data_dir, &args)
        }
        // MCP 工具调用：参数以确认时封存的 args 原样执行（dispatch 时已
        // 创建 pending，这里不再二次批准）；server 须仍处启用态，否则报错。
        "mcp_tool_call" => {
            let full_name = arg_str(&args, "full_name")?;
            let arguments = args
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            crate::ai::mcp::call_tool(store, &full_name, &arguments)
        }
        // 历史遗留的 record_event 待确认动作（改直接入库前的旧数据）仍可执行
        "record_event" => insert_event_from_args(&args, store),
        "save_knowledge_draft" => {
            let title = arg_str(&args, "title")?;
            let content_md = arg_str(&args, "content_md")?;
            // 确认执行也必须幂等：历史版本可能已经积累了多个同名待确认动作，
            // 或另一个请求刚刚先完成了创建。按标题复查，避免随机 slug 生成重复页面。
            if let Some(existing) = store.find_wiki_page_by_title(&title)? {
                return Ok(format!(
                    "知识页「{}」已经存在（slug={}），本次确认未重复创建。",
                    existing.title, existing.slug
                ));
            }
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
            let page = crate::knowledge::authoring::save_draft(store, &draft, &args)?;
            Ok(format!(
                "已保存知识页「{}」（{}，slug={}）",
                title,
                if args
                    .get("verified_evidence")
                    .and_then(Value::as_array)
                    .is_some_and(|v| !v.is_empty())
                {
                    "来源已保留"
                } else {
                    "笔记，尚无可核验来源"
                },
                page.slug
            ))
        }
        "create_todo" => {
            let title = arg_str(&args, "title")?;
            let due_at = arg_str_opt(&args, "due_at");
            let priority = arg_str_opt(&args, "priority").unwrap_or_else(|| "normal".to_string());
            let related_wiki_slug = arg_str_opt(&args, "related_wiki_slug");
            let note = arg_str_opt(&args, "note");
            let reminder = args
                .get("reminder")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            // 确认门纪律（评审 3.2）：若确认时 due_at 已过则原样拒绝，
            // 禁止静默改期——向用户如实报出，让其重新确认一个未来时间。
            if reminder {
                let due = due_at
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("提醒必须带有到期时间"))?;
                let parsed = chrono::DateTime::parse_from_rfc3339(due)
                    .map_err(|_| anyhow::anyhow!("提醒时间不是有效时间格式：{due}"))?;
                if parsed <= chrono::Utc::now() {
                    anyhow::bail!("该提醒的时间已过期（{due}），请确认一个新的未来时间");
                }
            }
            let t = store.create_todo(
                &title,
                &priority,
                due_at.as_deref(),
                None,
                related_wiki_slug.as_deref(),
                note.as_deref(),
                reminder,
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
            let tx = store.begin_write_transaction()?;
            store.check_source_preview(
                &source_url,
                &content_md,
                args.get("expected_snapshot_id").and_then(Value::as_str),
            )?;
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
            let outcome = store.upsert_wiki_page_in_tx(&draft, ContentPolicy::Always)?;
            if let Some(tx) = tx {
                tx.commit()?;
            }
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
        "import_directory_as_project" => {
            let directory = arg_str(&args, "directory")?;
            let project_name = arg_str(&args, "project_name")?;
            let content_md = arg_str(&args, "content_md")?;
            let files = args.get("files").and_then(Value::as_u64).unwrap_or(0) as usize;
            let skipped = args.get("skipped").and_then(Value::as_u64).unwrap_or(0) as usize;
            let scan_truncated = args
                .get("scan_truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let summary = crate::local_sources::ProjectSummary {
                project_name,
                files,
                skipped,
                scan_truncated,
                content_md,
            };
            let report = crate::local_sources::ingest_project_summary(
                store,
                std::path::Path::new(&directory),
                summary,
            )?;
            Ok(format!(
                "已将目录作为项目导入知识库：{}（收集 {} 个有界候选文件）",
                report.pages.join("、"),
                report.files
            ))
        }
        "import_directory_files" => {
            let preview: crate::local_sources::FileImportPreview = serde_json::from_value(
                args.get("preview")
                    .cloned()
                    .context("旧导入没有原文预览，请重新请求导入后确认")?,
            )?;
            let report = crate::local_sources::confirm_file_import(store, &preview)?;
            Ok(format!(
                "已将目录文件导入知识库：创建 {} 张知识页，扫描 {} 个文件",
                report.pages.len(),
                report.files
            ))
        }
        "save_wiki_revision" => {
            let tx = store.begin_write_transaction()?;
            let materials = crate::storage::chat_page_tasks::validate_proposal_task(store, &args)?;
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
            let result = if save_as == "derivative" {
                crate::knowledge::authoring::validate_revision_basis(
                    store,
                    &existing,
                    args.get("revision_basis"),
                )?;
                let ct = content_type
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "AI 加工".to_string());
                let page = store.create_derivative_with_generation(
                    &slug,
                    &ct,
                    &title,
                    &content_md,
                    &format!("AI 加工派生：{change_note}"),
                    Some(&pa.conversation_id),
                )?;
                crate::storage::chat_page_tasks::bind_proposal_materials(store, &page, &materials)?;
                format!(
                    "已保存「{}」的派生产物（{ct}）：{change_note}。原文未改动。",
                    page.title
                )
            } else {
                let current = store.get_wiki_page(&slug)?.context("知识页不存在")?;
                crate::knowledge::authoring::validate_revision_basis(
                    store,
                    &current,
                    args.get("revision_basis"),
                )?;
                let expected = args
                    .get("base_hash")
                    .and_then(Value::as_str)
                    .context("旧修订缺少版本依据，请重新生成建议")?;
                if crate::storage::knowledge::content_hash(&current.content_md) != expected {
                    anyhow::bail!("页面已被修改，请重新审阅，不能覆盖新的编辑");
                }
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
                let outcome = store.upsert_wiki_page_in_tx(&draft, ContentPolicy::Always)?;
                store.connection.execute(
                    "UPDATE wiki_pages SET human_edited_at=?1 WHERE id=?2",
                    rusqlite::params![chrono::Utc::now().to_rfc3339(), outcome.page.id],
                )?;
                let metadata = store.knowledge_metadata(&outcome.page.slug)?;
                let applicable = args["applicable_when"]
                    .as_str()
                    .unwrap_or(&metadata.applicable_when);
                store.confirm_authored_metadata_in_tx(
                    &outcome.page.id,
                    applicable,
                    &metadata.strength,
                )?;
                format!("知识页「{}」修订已保存：{change_note}", outcome.page.title)
            };
            if let Some(tx) = tx {
                tx.commit()?;
            }
            Ok(result)
        }
        other => anyhow::bail!("未知待执行动作：{other}"),
    }
}

/// 用户从草稿预览中单独确认一篇知识页，其他待确认动作不受影响。
pub fn confirm_knowledge_draft(
    store: &Store,
    conversation_id: &str,
    action_id: &str,
) -> Result<String> {
    let pa = store
        .pending_action_by_id(conversation_id, action_id)?
        .context("待确认草稿不存在或已经处理")?;
    if pa.action != "save_knowledge_draft" {
        anyhow::bail!("只能从此入口保存知识页草稿");
    }
    let result = execute_pending_action(store, &pa)?;
    store.delete_pending_action(&pa.id)?;
    Ok(result)
}

/// 拒绝指定知识草稿，保留拒绝状态供审计，不影响其他待确认动作或知识页。
pub fn decline_knowledge_draft(
    store: &Store,
    conversation_id: &str,
    action_id: &str,
) -> Result<()> {
    let pa = store
        .pending_action_by_id(conversation_id, action_id)?
        .context("待确认草稿不存在或已经处理")?;
    if pa.action != "save_knowledge_draft" {
        anyhow::bail!("只能从此入口删除知识页草稿");
    }
    if !store.decline_pending_action(action_id)? {
        anyhow::bail!("待确认草稿已经处理");
    }
    Ok(())
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
