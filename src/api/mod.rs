pub mod chat_proposals;
pub mod chat_runs;
pub mod conversations;
pub mod db_maintenance;
pub mod entities;
pub mod event_stats;
pub mod fonts;
pub mod goals;
pub mod import;
pub mod knowledge;
pub mod knowledge_digest;
pub mod knowledge_workflows;
pub mod mcp;
pub mod page_marks;
pub mod provider_config;
pub mod provider_reachability;
pub mod relations;
pub mod review_report;
pub mod rules;
pub mod sync;
pub mod theme;
pub mod todos;
pub mod tweet;
pub mod user_skills;
pub mod web_search;
pub mod wiki;
pub mod wiki_chat;
use crate::ai::memory::ContextMessage;
use crate::ai::provider::{AiProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider};
use crate::event::NewEvent;
use crate::storage::Store;
use crate::storage::rule_triggers::PendingChatHit;
use anyhow::{Context, Result};
pub use chat_proposals::*;
pub use chat_runs::*;
pub use conversations::*;
pub use entities::*;
pub use fonts::*;
pub use import::*;
pub use knowledge_digest::*;
pub use mcp::*;
pub use provider_config::*;
pub use relations::*;
pub use rules::*;
use serde::{Deserialize, Serialize};
pub use theme::*;
pub use todos::*;
pub use tweet::*;
pub use user_skills::*;
pub use web_search::*;
pub use wiki::*;
pub use wiki_chat::*;

/// 现行分析结果契约标识（D1 起升 v3：新增 rule_confirmations；
/// guidance 段已在 D4 启用（prompt 可选要求 + wash 护栏）；emotion 分片待 D5）。
/// v1/v2 为历史读兼容标识；「event-analysis」（无后缀）为 v3 之前的现行写法。
mod analysis_queue;
pub mod analysis_schema;
pub mod companion_flow;
pub mod period_stats;
pub mod records;
pub mod review_flow;

pub(crate) use analysis_queue::*;
pub use analysis_schema::*;
pub use companion_flow::*;
pub use period_stats::*;
pub use records::*;
pub use review_flow::*;

#[cfg(test)]
#[path = "user_model_tests.rs"]
mod user_model_tests;

#[cfg(test)]
#[path = "analysis_tests.rs"]
mod analysis_tests;

#[cfg(test)]
#[path = "daily_review_tests.rs"]
mod daily_review_tests;

#[cfg(test)]
#[path = "guidance_tests.rs"]
mod guidance_tests;

#[cfg(test)]
#[path = "emotion_wash_tests.rs"]
mod emotion_wash_tests;

#[cfg(test)]
#[path = "record_event_tests.rs"]
mod record_event_tests;

#[cfg(test)]
#[path = "period_stats_tests.rs"]
mod period_stats_tests;
