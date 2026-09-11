use chrono::{DateTime, Utc};

pub struct NewEvent<'a> {
    pub raw_text: &'a str,
    pub occurred_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub source: &'static str,
}

impl<'a> NewEvent<'a> {
    pub fn now(raw_text: &'a str) -> Self {
        let now = Utc::now();
        Self {
            raw_text,
            occurred_at: now,
            recorded_at: now,
            source: "capture",
        }
    }
}

pub struct EventSummary {
    pub recorded_at: String,
    pub raw_text: String,
}

