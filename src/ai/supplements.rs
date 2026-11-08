//! Boundary-only task steering. Queue ownership remains the durable arbiter.
use super::{
    memory::{ContextMessage, OmissionPolicy, SegmentOrigin, SegmentPurpose},
    provider::AiProvider,
    tool::ToolSpec,
};
use crate::storage::{Store, chat_supplements::ChatSupplement};
use anyhow::{Result, ensure};
use serde_json::json;

fn valid_basis(input: &ChatSupplement) -> bool {
    crate::storage::knowledge::content_hash(
        &json!([input.input_id, input.content, input.created_at]).to_string(),
    ) == input.basis
}

/// 解析消息开头的知识引用行：每行形如 `@wiki(slug)`，直到首个非引用行。
/// 返回（引用 slug 列表，剩余原文）；没有引用行时返回 None（纯文本补充不动）。
fn parse_page_references(content: &str) -> Option<(Vec<String>, &str)> {
    let mut slugs = Vec::new();
    let mut rest = content;
    loop {
        let line_end = rest.find('\n').unwrap_or(rest.len());
        let line = rest[..line_end].trim();
        let slug = line
            .strip_prefix("@wiki(")
            .and_then(|body| body.strip_suffix(')'))
            .map(str::trim)
            .filter(|slug| {
                !slug.is_empty()
                    && slug
                        .chars()
                        .all(|c| !c.is_whitespace() && c != '(' && c != ')')
            });
        match slug {
            Some(slug) => {
                slugs.push(slug.to_string());
                rest = &rest[line_end..];
                rest = rest.strip_prefix('\n').unwrap_or(rest);
            }
            None => break,
        }
    }
    (!slugs.is_empty()).then(|| (slugs, rest))
}

/// 知识引用展开为挂载视图：每页只带 AI 摘要 + 指向全文的取回路径
/// （get_wiki_page 工具），这正是知识库「按需检索」的本意；引用不存在的
/// 页面就地如实声明。纯文本补充不受影响。
fn expand_page_references(store: &Store, content: &str) -> Result<String> {
    let Some((slugs, instruction)) = parse_page_references(content) else {
        return Ok(content.to_string());
    };
    let mut blocks = vec![store.render_supplement_instruction("references", &[])?];
    for slug in &slugs {
        match store.get_wiki_page(slug) {
            Ok(Some(page)) => {
                let summary = if page.summary.trim().is_empty() {
                    &store.render_supplement_instruction("no_summary", &[])?
                } else {
                    page.summary.trim()
                };
                blocks.push(store.render_supplement_instruction(
                    "page",
                    &[
                        ("title", page.title.clone()),
                        ("kind", page.kind.clone()),
                        ("slug", page.slug.clone()),
                        ("summary", summary.into()),
                    ],
                )?);
            }
            Ok(None) | Err(_) => blocks
                .push(store.render_supplement_instruction("missing", &[("slug", slug.clone())])?),
        }
    }
    let instruction = instruction.trim();
    if !instruction.is_empty() {
        blocks.push(
            store.render_supplement_instruction("instruction", &[("body", instruction.into())])?,
        );
    }
    Ok(blocks.join("\n"))
}

/// 挂载预检上限（2026-10-18 事故「预算前移」）：补充材料由客户端提交，
/// 曾被整包挂载（手机 v48 一次挂 27 页全文）直接把请求顶爆。这里给单条
/// 补充一个宽松上限，超限截断并在材料内如实声明；完整原文仍在输入队列
/// 记录里，未丢失。预算内的最终把关仍在各 Provider 的 fit。
const SUPPLEMENT_MOUNT_CHAR_LIMIT: usize = 24000;
fn clipped(store: &Store, content: &str) -> Result<String> {
    if content.chars().count() <= SUPPLEMENT_MOUNT_CHAR_LIMIT {
        return Ok(content.to_string());
    }
    let head: String = content.chars().take(SUPPLEMENT_MOUNT_CHAR_LIMIT).collect();
    Ok(format!(
        "{head}{}",
        store.render_supplement_instruction("clipped", &[])?
    ))
}

pub(crate) fn context(store: &Store, input: &ChatSupplement) -> Result<ContextMessage> {
    // 先展开知识引用（若有），再应用挂载上限：两道预检都写在材料内。
    Ok(ContextMessage::new(
        "system",
        store.render_supplement_instruction(
            "context",
            &[
                ("time", input.created_at.clone()),
                (
                    "body",
                    clipped(store, &expand_page_references(store, &input.content)?)?,
                ),
            ],
        )?,
    )
    .with_segment(
        SegmentPurpose::TaskSupplement,
        SegmentOrigin::StoredMessage,
        Some(&input.input_id),
        OmissionPolicy::Required,
    ))
}

pub(crate) fn restore(
    store: &Store,
    conversation: &str,
    messages: &mut Vec<ContextMessage>,
) -> Result<()> {
    let Some(run) = &store.chat_run_identity else {
        return Ok(());
    };
    for input in store.inherited_chat_supplements(conversation, &run.id)? {
        super::generation::check(store)?;
        ensure!(
            valid_basis(&input) && store.supplement_is_plain(&input)?,
            "原任务的补充材料已经变化，不能静默沿用，请重新提交任务"
        );
        messages.push(context(store, &input)?);
        super::generation::event(
            store,
            "input_restored",
            json!({
                "queue_id":input.queue_id,"input_message_id":input.input_id,"input_basis":input.basis,
                "meaning":"included_for_next_request_not_execution",
            }),
        )?;
    }
    Ok(())
}

pub(crate) fn inject_next(
    store: &Store,
    conversation: &str,
    provider: &dyn AiProvider,
    request_context: &[ContextMessage],
    tools: Option<&[ToolSpec]>,
) -> Result<Option<ContextMessage>> {
    let Some(run) = &store.chat_run_identity else {
        return Ok(None);
    };
    super::generation::check(store)?;
    let Some(input) = store.next_chat_supplement(conversation, &run.id)? else {
        return Ok(None);
    };
    if let Some(reason) = store.supplement_defer_reason(&input)? {
        store.defer_chat_supplement(input.queue_id, &run.id, reason)?;
        return Ok(None);
    }
    if !valid_basis(&input) {
        store.defer_chat_supplement(input.queue_id, &run.id, "input_changed")?;
        return Ok(None);
    }
    let context = context(store, &input)?;
    let mut candidate = request_context.to_vec();
    candidate.push(context.clone());
    if !provider.accepts_supplement(&candidate, tools)? {
        store.defer_chat_supplement(input.queue_id, &run.id, "budget_or_protocol")?;
        return Ok(None);
    }
    if store.consume_chat_supplement(&run.id, &run.owner, &input)? {
        Ok(Some(context))
    } else {
        Ok(None)
    }
}
