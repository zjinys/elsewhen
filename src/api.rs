use crate::event::NewEvent;
use crate::storage::Store;
use anyhow::Result;
use chrono::{DateTime, Utc};

/// Event data transfer object for Flutter
#[derive(Clone, Debug)]
pub struct EventDto {
    pub id: String,
    pub raw_text: String,
    pub recorded_at: String,
    pub occurred_at: String,
    pub source: String,
    pub status: String,
}

/// Analysis result DTO
#[derive(Clone, Debug)]
pub struct AnalysisDto {
    pub event_type: String,
    pub confidence: f64,
    pub summary: String,
    pub clarifications: Vec<String>,
}

/// Initialize the bridge with database path
pub fn init_bridge(database_path: Option<String>) -> Result<String> {
    let config = crate::config::AppConfig::load()?;
    let db_path = database_path
        .map(|p| std::path::PathBuf::from(p))
        .unwrap_or(config.database_path);

    Ok(db_path.display().to_string())
}

/// Record a new event
pub fn record_event(raw_text: String) -> Result<EventDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let new_event = NewEvent::now(&raw_text);
    let id = store.insert_event(new_event)?;

    // Query back the created event
    let events = store.list_events()?;
    let event = events.into_iter()
        .find(|e| e.raw_text == raw_text)
        .ok_or_else(|| anyhow::anyhow!("Event not found after insert"))?;

    Ok(EventDto {
        id,
        raw_text: event.raw_text,
        recorded_at: event.recorded_at,
        occurred_at: event.recorded_at.clone(),
        source: "flutter_gui".to_string(),
        status: "pending".to_string(),
    })
}

/// List all events
pub fn list_events() -> Result<Vec<EventDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let events = store.list_events()?;

    Ok(events.into_iter().map(|e| EventDto {
        id: uuid::Uuid::new_v4().to_string(), // TODO: Store should return ID
        raw_text: e.raw_text,
        recorded_at: e.recorded_at.clone(),
        occurred_at: e.recorded_at,
        source: "unknown".to_string(),
        status: "completed".to_string(),
    }).collect())
}

/// List completed analyses
pub fn list_analyses() -> Result<Vec<AnalysisDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    let analyses = store.list_analyses()?;

    Ok(analyses.into_iter().map(|a| AnalysisDto {
        event_type: a.event_type,
        confidence: a.confidence,
        summary: a.raw_text,
        clarifications: serde_json::from_str(&a.clarifications).unwrap_or_default(),
    }).collect())
}

/// Get active AI provider info
pub fn get_ai_provider() -> Result<Option<String>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    if let Some(provider) = store.active_ai_provider_config()? {
        Ok(Some(format!("{} - {}", provider.model, provider.base_url)))
    } else {
        Ok(None)
    }
}

/// Trigger AI analysis for pending events
pub fn trigger_analysis() -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    // Spawn background analysis
    std::thread::spawn(move || {
        let _ = crate::ai::run_once(&store);
    });

    Ok(true)
}
