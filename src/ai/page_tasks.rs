//! Task-scoped page material; full-source readings retain their existing cache.
use super::{
    generation,
    memory::{ContextMessage, OmissionPolicy, SegmentOrigin, SegmentPurpose},
    provider::AiProvider,
};
use crate::storage::{
    Store, WikiPage,
    chat_page_tasks::{PageTask, task_identity},
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::json;

const CHUNK: usize = 3000;
const MAX_PARTS: usize = 32;

/// 页面修订意图的显式触发词。宁可漏判不惊扰：误判的代价只是多出一份
/// 可拒绝的修订草稿，漏判的代价才是「说了修改却被拒」的挫败。
const PAGE_REVISION_TRIGGERS: &[&str] = &[
    "修改",
    "改正",
    "更正",
    "改动",
    "改成",
    "改写",
    "改一下",
    "改改",
    "改回",
    "修订",
    "订正",
    "更新",
    "补上",
    "补充",
    "加进去",
    "写进",
    "替换",
    "删掉",
    "删去",
    "去掉",
    "改掉",
    "编辑",
    "edit this",
    "update this",
    "revise this",
];

/// 判断页内对话输入是否明确要求修改当前知识页（自动升级编辑任务用）。
/// 命中触发词但紧邻否定/探询（不用、别、是否、要不要、吗……）不算。
/// 这是保守启发式：判定结果仍然只产生待审草稿，不跳过审阅。
pub(crate) fn explicit_page_revision_intent(query: &str) -> bool {
    if super::conversation::english::page_revision_request(query) {
        return true;
    }
    let chars: Vec<char> = query.trim().to_lowercase().chars().collect();
    for trigger in PAGE_REVISION_TRIGGERS {
        // English requests use whole clauses and word boundaries above. A short
        // character window would miss "do not" or a question farther away.
        if trigger.is_ascii() {
            continue;
        }
        let t: Vec<char> = trigger.chars().collect();
        let mut from = 0;
        while let Some(pos) = find_sub(&chars, &t, from) {
            let before: String = chars[pos.saturating_sub(6)..pos].iter().collect();
            let after: String = chars[(pos + t.len())..(pos + t.len() + 4).min(chars.len())]
                .iter()
                .collect();
            let negated = before.chars().any(|c| "不别没".contains(c))
                || before.contains("是否")
                || before.contains("要不要")
                || before.contains("怎么")
                || after.contains('吗')
                || after.contains('？')
                || after.contains('?')
                || after.contains("记录");
            if !negated {
                return true;
            }
            from = pos + 1;
        }
    }
    false
}

fn find_sub(haystack: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (from..=haystack.len() - needle.len()).find(|&i| haystack[i..i + needle.len()] == *needle)
}

/// 用户明确提出修改时把非编辑任务升级为编辑任务：问答任务已带选区的
/// 转局部编辑（沿用选区），否则转全页修订。材料版本保持原任务的 basis
/// 不变， revise_full/工具侧仍按同一 baseline 校验，接受前必过审阅。
/// 返回 None 表示不升级：已是编辑任务、无页面材料、或多页对照任务
///（对照第一页自动修订会改错对象，仍走工具侧的明确指引）。
pub(crate) fn escalate_page_task(task: &PageTask) -> Option<PageTask> {
    if task.pages.is_empty()
        || matches!(task.kind.as_str(), "full_revision" | "local_edit")
        || !matches!(task.kind.as_str(), "local_question" | "full_summary")
    {
        return None;
    }
    if task.kind == "local_question" && task.start_char.is_some() && task.end_char.is_some() {
        return Some(PageTask {
            kind: "local_edit".to_string(),
            pages: task.pages.clone(),
            start_char: task.start_char,
            end_char: task.end_char,
        });
    }
    Some(PageTask {
        kind: "full_revision".to_string(),
        pages: task.pages.clone(),
        start_char: None,
        end_char: None,
    })
}

pub(crate) fn task_for_run(store: &Store, conversation: &str) -> Result<Option<PageTask>> {
    if let Some(task) = store.current_page_task(conversation)? {
        return Ok(Some(task));
    }
    // Legacy inputs have no displayed-version contract. Read-only local questions
    // remain usable, but never claim that a newly fetched version was displayed.
    let Some(slug) = store
        .get_conversation(conversation)?
        .and_then(|c| c.wiki_page_slug)
    else {
        return Ok(None);
    };
    let (_, basis) = store.page_task_basis(&slug)?;
    Ok(Some(PageTask {
        kind: "local_question".into(),
        pages: vec![basis],
        start_char: None,
        end_char: None,
    }))
}

fn affirmative_phrase(query: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|trigger| {
        let filtered = if trigger.is_ascii() {
            crate::language::runtime::prose(query)
        } else {
            query.to_owned()
        };
        let chars: Vec<_> = filtered.chars().collect();
        let needle: Vec<_> = trigger.chars().collect();
        let mut from = 0;
        while let Some(position) = find_sub(&chars, &needle, from) {
            let before: String = chars[if trigger.is_ascii() {
                0
            } else {
                position.saturating_sub(6)
            }..position]
                .iter()
                .collect();
            let clause = before
                .rsplit(['，', ',', '。', ';', '；', '\n'])
                .next()
                .unwrap_or("");
            if ![
                "不", "别", "无需", "避免", "don't", "not ", "no ", "never ", "without ",
                "whether ", "how to ", "if ", "example:",
            ]
            .iter()
            .any(|term| clause.contains(term))
            {
                return true;
            }
            from = position + 1;
        }
        false
    })
}

fn summary_task(task: &PageTask, query: &str) -> Option<PageTask> {
    if task.kind != "local_question" || task.pages.len() != 1 {
        return None;
    }
    let query = query.to_lowercase();
    let whole = affirmative_phrase(
        &query,
        &[
            "全文",
            "整篇",
            "整页",
            "整个页面",
            "全部内容",
            "完整文章",
            "whole article",
            "entire page",
            "full article",
            "entire article",
        ],
    );
    let local = [
        "选中",
        "选区",
        "这段",
        "这一段",
        "这几段",
        "片段",
        "局部",
        "第一段",
        "最后一段",
        "paragraph",
        "selection",
        "excerpt",
    ]
    .iter()
    .any(|term| query.contains(term));
    if !whole && (task.start_char.is_some() || task.end_char.is_some() || local) {
        return None;
    }
    let requested = affirmative_phrase(
        &query,
        &[
            "总结",
            "概括",
            "归纳",
            "提炼",
            "摘要",
            "梳理",
            "summarize",
            "summarise",
            "summary",
        ],
    );
    requested.then(|| PageTask {
        kind: "full_summary".into(),
        pages: task.pages.clone(),
        start_char: None,
        end_char: None,
    })
}

pub(crate) fn prepare(
    store: &Store,
    conversation: &str,
    task: &PageTask,
    query: &str,
    provider: &dyn AiProvider,
    name: &str,
) -> Result<ContextMessage> {
    let pages = task.validate(store)?;
    let expanded = summary_task(task, query);
    let task = expanded.as_ref().unwrap_or(task);
    let mut parts = 0;
    if matches!(task.kind.as_str(), "full_summary" | "compare") {
        for page in &pages {
            if page.content_md.chars().count() > CHUNK
                && cached_full_material(store, page)?.is_none()
            {
                parts += reading_ranges(&page.content_md.chars().collect::<Vec<_>>()).len();
            }
        }
    }
    ensure!(
        parts <= MAX_PARTS,
        "本轮全文材料超过32段，请减少对照材料或拆分任务；没有截断原文"
    );
    let mut material =
        vec![store.render_execution_instruction("page_task", &[("kind", task.kind.clone())])?];
    if task.kind == "none" {
        material.push(store.render_execution_instruction("page_none", &[])?);
    }
    for page in &pages {
        let body = match task.kind.as_str() {
            "local_question" | "local_edit" => local_material(store, page, task, query)?,
            "full_summary" | "compare" => full_material(store, page, provider, name)?,
            "full_revision" => store.render_execution_instruction("page_revision", &[])?,
            _ => String::new(),
        };
        material.push(store.render_execution_instruction(
            "page_material",
            &[
                ("title", page.title.clone()),
                ("slug", page.slug.clone()),
                (
                    "version",
                    crate::storage::knowledge::content_hash(&page.content_md),
                ),
                ("kind", page.kind.clone()),
                (
                    "discipline",
                    store.render_execution_instruction(
                        if matches!(page.area.as_str(), "imported" | "derivative") {
                            "page_protected"
                        } else {
                            "page_review"
                        },
                        &[],
                    )?,
                ),
                ("body", body),
            ],
        )?);
    }
    if task.kind == "local_edit" {
        material.push(store.render_execution_instruction("page_local_edit", &[])?);
    }
    generation::check(store)?;
    task.validate(store)?;
    generation::event(
        store,
        "page_task",
        json!({"kind":task.kind,"identity":task_identity(task)?,"pages":pages.len()}),
    )?;
    let _ = conversation;
    Ok(
        ContextMessage::new("system", material.join("\n\n")).with_segment(
            SegmentPurpose::CurrentPage,
            SegmentOrigin::KnowledgePage,
            Some(&task_identity(task)?),
            OmissionPolicy::Required,
        ),
    )
}

fn local_material(store: &Store, page: &WikiPage, task: &PageTask, query: &str) -> Result<String> {
    let chars: Vec<_> = page.content_md.chars().collect();
    if let (Some(start), Some(end)) = (task.start_char, task.end_char) {
        return store.render_execution_instruction(
            "page_selection",
            &[
                ("start", start.to_string()),
                ("end", end.to_string()),
                ("total", chars.len().to_string()),
                ("body", chars[start..end].iter().collect()),
            ],
        );
    }

    let terms: Vec<String> = query
        .split_whitespace()
        .filter(|s| s.chars().count() > 1)
        .map(str::to_owned)
        .chain(
            query
                .chars()
                .collect::<Vec<_>>()
                .windows(2)
                .map(|w| w.iter().collect()),
        )
        .collect();
    let mut ranked: Vec<_> = chars
        .chunks(700)
        .enumerate()
        .map(|(i, c)| {
            let text: String = c.iter().collect();
            let score = terms.iter().filter(|t| text.contains(t.as_str())).count();
            (score, i, text)
        })
        .collect();
    ranked.sort_by_key(|(score, i, _)| (std::cmp::Reverse(*score), *i));
    ranked.truncate(2);
    ranked.sort_by_key(|(_, i, _)| *i);
    let selected = ranked
        .into_iter()
        .map(|(_, i, text)| {
            store.render_execution_instruction(
                "page_range",
                &[
                    ("start", (i * 700).to_string()),
                    ("end", (i * 700 + 700).min(chars.len()).to_string()),
                    ("total", chars.len().to_string()),
                    ("body", text),
                ],
            )
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n");
    store.render_execution_instruction("page_local", &[("body", selected)])
}

// Prefer sentence/paragraph boundaries so a negation does not become a positive
// assertion in the next chunk. Hard splits retain bounded read-only neighbours.
fn reading_ranges(chars: &[char]) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let limit = (start + CHUNK).min(chars.len());
        let end = if limit == chars.len() {
            limit
        } else {
            ((start + CHUNK - 700)..limit)
                .rev()
                .find(|&index| matches!(chars[index], '\n' | '。' | '！' | '？'))
                .map_or(limit, |index| index + 1)
        };
        ranges.push(start..end);
        start = end;
    }
    ranges
}

fn cached_full_material(store: &Store, page: &WikiPage) -> Result<Option<String>> {
    if page.area == "imported" {
        for source in store.page_source_snapshots(&page.slug)? {
            if source.content_md == page.content_md
                && crate::knowledge::reading::ready(store, &source)?
            {
                return crate::knowledge::reading::context(store, &source)
                    .map(|(text, _)| Some(text));
            }
        }
    }
    Ok(None)
}

fn full_material(
    store: &Store,
    page: &WikiPage,
    provider: &dyn AiProvider,
    name: &str,
) -> Result<String> {
    if page.content_md.chars().count() <= CHUNK {
        return store
            .render_execution_instruction("page_original", &[("body", page.content_md.clone())]);
    }
    if let Some(material) = cached_full_material(store, page)? {
        return Ok(material);
    }
    let chars: Vec<_> = page.content_md.chars().collect();
    let ranges = reading_ranges(&chars);
    ensure!(
        ranges.len() <= MAX_PARTS,
        "全文任务超过本轮 32 段上限，请拆分任务；没有截断原文"
    );
    let mut summaries = Vec::new();
    for range in ranges {
        generation::check(store)?;
        let mut messages = vec![ContextMessage::new(
            "system",
            format!(
                "{}\n{}",
                store.render_execution_instruction(
                    "page_summarize",
                    &[
                        ("start", range.start.to_string()),
                        ("end", range.end.to_string()),
                        ("total", chars.len().to_string()),
                    ],
                )?,
                store.output_language_directive(true)?
            ),
        )];
        if range.start > 0 || range.end < chars.len() {
            messages.push(ContextMessage::new(
                "system",
                store.render_execution_instruction(
                    "page_neighbors",
                    &[
                        (
                            "before",
                            chars[range.start.saturating_sub(120)..range.start]
                                .iter()
                                .collect(),
                        ),
                        (
                            "after",
                            chars[range.end..(range.end + 120).min(chars.len())]
                                .iter()
                                .collect(),
                        ),
                    ],
                )?,
            ));
        }
        messages.push(ContextMessage::new(
            "user",
            chars[range.clone()].iter().collect::<String>(),
        ));
        let reply = generation::auxiliary_request(store, provider, name, messages)?;
        ensure!(
            reply.tool_calls.is_empty() && reply.finish_reason.as_deref() != Some("length"),
            "材料处理响应被截断或包含未执行工具，未建立完整结果"
        );
        ensure!(
            !reply.content.trim().is_empty() && reply.content.chars().count() <= 900,
            "全文分块归纳为空或超长，未声称覆盖完成"
        );
        summaries.push(store.render_execution_instruction(
            "page_summary_range",
            &[
                ("start", range.start.to_string()),
                ("end", range.end.to_string()),
                ("body", reply.content),
            ],
        )?);
    }
    store.render_execution_instruction(
        "page_summary",
        &[
            ("total", chars.len().to_string()),
            ("body", summaries.join("\n")),
        ],
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisedPart {
    part: usize,
    content_md: String,
}

/// A full-page edit is a single reviewed proposal assembled from every range.
/// No partial result or model-authored coverage claim can commit a revision.
pub(crate) fn revise_full(
    store: &Store,
    conversation: &str,
    task: &PageTask,
    query: &str,
    provider: &dyn AiProvider,
    name: &str,
    context: &[ContextMessage],
) -> Result<String> {
    let pages = task.validate(store)?;
    let page = pages.first().context("全页修订缺少材料")?;
    let chars: Vec<_> = page.content_md.chars().collect();
    let count = chars.len().div_ceil(CHUNK);
    ensure!(
        count > 0 && count <= MAX_PARTS,
        "全页修订支持 1–32 个连续片段；请拆分超长任务，原文未截断"
    );
    let mut output = String::new();
    for (i, chunk) in chars.chunks(CHUNK).enumerate() {
        generation::check(store)?;
        task.validate(store)?;
        let mut messages = context.to_vec();
        messages.extend([
            ContextMessage::new(
                "system",
                store.render_execution_instruction("page_revise", &[])?,
            ),
            ContextMessage::new(
                "user",
                store.render_execution_instruction(
                    "page_revise_input",
                    &[
                        ("query", query.into()),
                        ("part", i.to_string()),
                        ("count", count.to_string()),
                        ("start", (i * CHUNK).to_string()),
                        ("end", (i * CHUNK + chunk.len()).to_string()),
                        ("total", chars.len().to_string()),
                        ("body", chunk.iter().collect()),
                    ],
                )?,
            ),
        ]);
        let reply = generation::auxiliary_request(store, provider, name, messages)?;
        ensure!(
            reply.tool_calls.is_empty() && reply.finish_reason.as_deref() != Some("length"),
            "材料处理响应被截断或包含未执行工具，未建立完整结果"
        );
        let raw = reply
            .content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let part: RevisedPart =
            serde_json::from_str(raw).context("修订片段格式不合法，未建立部分草稿")?;
        ensure!(
            part.part == i
                && !part.content_md.trim().is_empty()
                && part.content_md.chars().count() <= CHUNK * 4,
            "修订片段身份、完整性或长度不合法"
        );
        output.push_str(&part.content_md);
        generation::event(
            store,
            "page_task_progress",
            json!({"completed_parts":i+1,"total_parts":count}),
        )?;
    }
    let tx = store.begin_write_transaction()?;
    generation::check(store)?;
    task.validate(store)?;
    let basis = &task.pages[0].revision;
    let protected = matches!(page.area.as_str(), "imported" | "derivative");
    let args = json!({"slug":page.slug,"title":page.title,"content_md":output,"change_note":"全页连续分块修订（待审阅）",
        "save_as":if protected {"derivative"} else {"revision"},"content_type":"全页修订","base_hash":basis["hash"],
        "revision_basis":basis,"applicable_when":basis["applicable_when"],"page_task":task});
    let id = store.create_pending_action(conversation, "save_wiki_revision", &args.to_string())?;
    generation::event(
        store,
        "proposal_ready",
        json!({"kind":"action","proposal_id":id}),
    )?;
    if let Some(tx) = tx {
        tx.commit()?;
    }
    store.render_reply_instruction(
        "revision_ready",
        &[
            ("count", count.to_string()),
            ("total", chars.len().to_string()),
            (
                "discipline",
                store.render_reply_instruction(
                    if protected {
                        "revision_protected"
                    } else {
                        "revision_basis"
                    },
                    &[],
                )?,
            ),
        ],
    )
}

#[cfg(test)]
mod tests;
