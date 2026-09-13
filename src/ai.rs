use crate::storage::Store;
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const PROMPT_VERSION: &str = "event-analysis-v1";

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub event_type: String,
    #[serde(default)]
    pub subtype: Option<String>,
    pub confidence: f64,
    #[serde(default)]
    pub facts: Value,
    #[serde(default)]
    pub entities: Vec<EntityCandidate>,
    #[serde(default)]
    pub state_changes: Value,
    #[serde(default)]
    pub clarifications: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EntityCandidate {
    Structured {
        entity_type: String,
        name: String,
        confidence: f64,
    },
    Text(String),
}

struct OpenAiCompatibleProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAiCompatibleProvider {
    fn from_config(store: &Store) -> Result<Self> {
        if store.active_ai_provider_config()?.is_none() {
            let base_url = std::env::var("ELSEWHEN_AI_BASE_URL")
                .context("no database provider and ELSEWHEN_AI_BASE_URL is missing")?;
            let model = std::env::var("ELSEWHEN_AI_MODEL")
                .context("no database provider and ELSEWHEN_AI_MODEL is missing")?;
            let api_key = std::env::var("ELSEWHEN_AI_API_KEY")
                .context("no database provider and ELSEWHEN_AI_API_KEY is missing")?;
            store.upsert_ai_provider_config(&base_url, &model, &api_key)?;
        }
        let config = store
            .active_ai_provider_config()?
            .context("no active AI provider configuration")?;
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
            base_url: config.base_url,
            api_key: config.api_key,
            model: config.model,
        })
    }

    fn analyze(&self, raw_text: &str) -> Result<AnalysisResult> {
        let response: Value = self.client.post(format!("{}/chat/completions", self.base_url.trim_end_matches('/')))
            .bearer_auth(&self.api_key)
            .json(&json!({
                "model": self.model,
                "response_format": {"type": "json_object"},
                "messages": [
                    {"role": "system", "content": "你是个人事件结构化分析器。只输出 JSON，字段为 event_type, subtype, confidence, facts, entities, state_changes, clarifications。entities 使用 entity_type/name/confidence。信息不足时放入 clarifications，不要臆造。"},
                    {"role": "user", "content": raw_text}
                ]
            })).send()?.error_for_status()?.json()?;
        let content = response
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .context("provider response missing message content")?;
        parse_analysis(content)
    }
}

fn parse_analysis(content: &str) -> Result<AnalysisResult> {
    let trimmed = content.trim();
    if let Ok(result) = serde_json::from_str(trimmed) {
        return Ok(result);
    }
    let candidate = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .or_else(|| {
            let start = trimmed.find('{')?;
            let end = trimmed.rfind('}')?;
            Some(&trimmed[start..=end])
        })
        .context("provider response contains no JSON object")?;
    serde_json::from_str(candidate).context("provider returned invalid analysis JSON")
}

pub fn run_once(store: &Store) -> Result<()> {
    if !process_one(store)? {
        println!("no pending analysis job");
    }
    Ok(())
}

pub fn run_worker(store: &Store) -> Result<()> {
    println!("Elsewhen AI worker started");
    loop {
        match process_one(store) {
            Ok(true) => {}
            Ok(false) => std::thread::sleep(std::time::Duration::from_secs(2)),
            Err(error) => {
                eprintln!("analysis failed: {error:#}");
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }
    }
}

fn process_one(store: &Store) -> Result<bool> {
    let Some(job) = store.claim_analysis_job()? else {
        return Ok(false);
    };
    let provider = OpenAiCompatibleProvider::from_config(store);
    let result = provider.and_then(|provider| provider.analyze(&job.raw_text));
    match result {
        Ok(analysis) => {
            store.complete_analysis(&job, PROMPT_VERSION, &serde_json::to_string(&analysis)?)?;
            println!("analyzed event {}", job.event_id);
            Ok(true)
        }
        Err(error) => {
            store.fail_analysis(&job, &format!("{error:#}"))?;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_markdown_wrapped_json() {
        let value = "```json\n{\"event_type\":\"work\",\"confidence\":0.9}\n```";
        assert_eq!(parse_analysis(value).unwrap().event_type, "work");
    }
}
