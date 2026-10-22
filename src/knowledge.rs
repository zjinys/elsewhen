//! One bounded retrieval contract shared by dialogue, analysis and review.
pub(crate) mod authoring;
pub(crate) mod dependencies;
pub mod maintenance;
pub mod organization;
pub mod queue;
pub(crate) mod reading;
pub mod review;
pub mod workflows;
use crate::ai::{
    memory::ContextMessage,
    provider::{AiProvider, AiReply},
};
use crate::storage::knowledge::content_hash;
use crate::storage::{Store, WikiPage, WikiPageDraft};
use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 后台结构化任务的回复完整性闸门。
///
/// 知识整理/阅读只接受完整、非空的 JSON 文本；把「空正文 / 只有推理 /
/// 输出被 max_tokens 截断」与真正的 JSON 语法错误区分开，避免把
/// `EOF while parsing a value at line 1 column 0` 当成模型格式问题。
pub(crate) fn structured_text<'a>(reply: &'a AiReply, task: &str) -> Result<&'a str> {
    anyhow::ensure!(
        reply.tool_calls.is_empty(),
        "{task}响应包含未执行的工具调用，不能作为结构化结果"
    );
    if reply.finish_reason.as_deref() == Some("length") {
        anyhow::bail!("{task}响应达到输出上限，内容不完整");
    }
    anyhow::ensure!(
        !matches!(
            reply.finish_reason.as_deref(),
            Some("tool_calls" | "function_call")
        ),
        "{task}响应未完整结束（finish_reason={:?}）",
        reply.finish_reason
    );
    let text = reply.content.trim();
    anyhow::ensure!(
        !text.is_empty(),
        "{task}响应为空（finish_reason={:?}{}）",
        reply.finish_reason,
        reply
            .reasoning_content
            .as_ref()
            .filter(|reasoning| !reasoning.trim().is_empty())
            .map(|reasoning| format!("，仅返回 {} 字推理内容", reasoning.chars().count()))
            .unwrap_or_default()
    );
    Ok(text)
}

mod bind;
pub mod health;
mod propose;
pub mod select;

pub use bind::{
    bound_model_context, candidates_in_context, latest_citations, record_usage, tool_context,
    validate_answer_citations,
};
pub use health::CITATION_INSTRUCTIONS;
pub use health::{KnowledgeHealthReport, health_report};
pub(crate) use propose::compile_requested_knowledge;
pub use propose::propose_knowledge;
pub(crate) use propose::{build_refresh_proposal, build_source_proposal, prepare_source_repair};
pub use select::{
    KnowledgeCitation, SourceReference, citation_for_page, citation_for_page_explicit,
    context_text, current_candidates, participating, select_knowledge, summary_mount,
};
pub(crate) use select::{mount_summary, query_terms, shorten};
