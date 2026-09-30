use crate::event::{EventSummary, NewEvent};
use anyhow::Result;

/// Analysis job from the queue
#[derive(Clone, Debug)]
pub struct AnalysisJob {
    pub id: String,
    pub event_id: String,
    pub raw_text: String,
    pub attempts: i64,
}

/// Analysis result summary
#[derive(Clone, Debug)]
pub struct AnalysisSummary {
    pub raw_text: String,
    pub event_type: String,
    pub confidence: f64,
    pub clarifications: String,
}

/// AI provider configuration
#[derive(Clone, Debug)]
pub struct AiProviderConfig {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub model: String,
    pub api_key_source: String,
    pub api_key: String,
    pub is_active: bool,
    pub temperature: f64,
    pub max_tokens: Option<i64>,
    pub context_window: Option<i64>,
}

/// AI provider 配置行（管理列表用：多配置 + 单激活）
#[derive(Clone, Debug)]
pub struct AiProviderConfigRow {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub model: String,
    pub api_key_source: String,
    pub is_active: bool,
    pub temperature: f64,
    pub max_tokens: Option<i64>,
    pub context_window: Option<i64>,
}

/// Storage adapter trait - allows switching storage implementations
///
/// Note: Implementations don't need to be Send + Sync themselves.
/// The trait is object-safe to allow dynamic dispatch.
pub trait StorageAdapter {
    // Event operations
    fn insert_event(&self, event: NewEvent) -> Result<String>;
    fn list_events(&self) -> Result<Vec<EventSummary>>;

    // Analysis operations
    fn list_analyses(&self) -> Result<Vec<AnalysisSummary>>;
    fn claim_analysis_job(&self) -> Result<Option<AnalysisJob>>;
    fn complete_analysis(
        &self,
        job: &AnalysisJob,
        prompt_version: &str,
        result_json: &str,
    ) -> Result<()>;
    fn fail_analysis(&self, job: &AnalysisJob, error: &str) -> Result<()>;

    // AI provider operations
    fn active_ai_provider_config(&self) -> Result<Option<AiProviderConfig>>;
    fn upsert_ai_provider_config(&self, base_url: &str, model: &str, api_key: &str) -> Result<()>;
    fn list_ai_provider_configs(&self) -> Result<Vec<AiProviderConfigRow>>;
    fn save_ai_provider_config(
        &self,
        id: Option<&str>,
        name: &str,
        provider_type: &str,
        base_url: &str,
        model: &str,
        api_key: &str,
        temperature: f64,
        max_tokens: Option<i64>,
    ) -> Result<String>;
    fn set_active_ai_provider_config(&self, id: &str) -> Result<()>;
    fn delete_ai_provider_config(&self, id: &str) -> Result<()>;
}
