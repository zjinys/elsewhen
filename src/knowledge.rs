//! One bounded retrieval contract shared by dialogue, analysis and review.
pub(crate) mod authoring;
pub(crate) mod dependencies;
pub mod maintenance;
pub mod organization;
pub mod queue;
pub(crate) mod reading;
pub mod review;
pub mod workflows;
use crate::ai::{memory::ContextMessage, provider::AiProvider};
use crate::storage::knowledge::content_hash;
use crate::storage::{Store, WikiPage, WikiPageDraft};
use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
