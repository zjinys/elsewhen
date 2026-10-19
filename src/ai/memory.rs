use super::tool::ToolCall;
use crate::storage::Store;
use anyhow::Result;
use serde_json::json;

mod segments;
pub use segments::{
    ContextSegment, OmissionPolicy, SegmentOrigin, SegmentPurpose, SelectionOmission,
};

/// Message for AI context
mod message;
mod prompts;
mod providers;
mod system_context;

pub use message::ContextMessage;
#[cfg(test)]
pub(crate) use prompts::{SYSTEM_PROMPT_BASE, TOOL_GUIDES};
pub use providers::{
    MemoryProvider, SimpleMemory, SlidingWindowMemory, compress_context, estimate_tokens,
};
pub(crate) use providers::{compress_context_for_model, prepare_uncompressed_context};
pub use system_context::build_content_system_prompt;
pub(crate) use system_context::{build_system_context, recent_background, today_memory_background};

#[path = "memory_tests/mod.rs"]
mod tests;
