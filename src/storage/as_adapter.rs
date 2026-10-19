//! StorageAdapter for Store： migrating/returning 连接参数与迁移入口。

use super::*;

impl StorageAdapter for Store {
    fn insert_event(&self, event: NewEvent) -> Result<String> {
        Store::insert_event(self, event)
    }

    fn list_events(&self) -> Result<Vec<EventSummary>> {
        Store::list_events(self)
    }

    fn list_analyses(&self) -> Result<Vec<AnalysisSummary>> {
        Store::list_analyses(self)
    }

    fn claim_analysis_job(&self) -> Result<Option<AnalysisJob>> {
        Store::claim_analysis_job(self)
    }

    fn complete_analysis(
        &self,
        job: &AnalysisJob,
        prompt_version: &str,
        result_json: &str,
    ) -> Result<()> {
        Store::complete_analysis(self, job, prompt_version, result_json)
    }

    fn fail_analysis(&self, job: &AnalysisJob, error: &str) -> Result<()> {
        Store::fail_analysis(self, job, error)
    }

    fn active_ai_provider_config(&self) -> Result<Option<AiProviderConfig>> {
        Store::active_ai_provider_config(self)
    }

    fn upsert_ai_provider_config(&self, base_url: &str, model: &str, api_key: &str) -> Result<()> {
        Store::upsert_ai_provider_config(self, base_url, model, api_key)
    }

    fn list_ai_provider_configs(&self) -> Result<Vec<AiProviderConfigRow>> {
        Store::list_ai_provider_configs(self)
    }

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
    ) -> Result<String> {
        Store::save_ai_provider_config(
            self,
            id,
            name,
            provider_type,
            base_url,
            model,
            api_key,
            temperature,
            max_tokens,
        )
    }

    fn set_active_ai_provider_config(&self, id: &str) -> Result<()> {
        Store::set_active_ai_provider_config(self, id)
    }

    fn delete_ai_provider_config(&self, id: &str) -> Result<()> {
        Store::delete_ai_provider_config(self, id)
    }
}
