use crate::event::{EventSummary, NewEvent};
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;
use uuid::Uuid;

pub struct Store {
    connection: Connection,
}

impl Clone for Store {
    fn clone(&self) -> Self {
        Self { connection: Connection::open(self.connection.path().expect("database path")).expect("reopen database") }
    }
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)
            .with_context(|| format!("open SQLite database {}", path.display()))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS schema_migrations (
               version INTEGER PRIMARY KEY,
               applied_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS events (
               id TEXT PRIMARY KEY,
               occurred_at TEXT NOT NULL,
               recorded_at TEXT NOT NULL,
               processed_at TEXT,
               raw_text TEXT NOT NULL CHECK (length(trim(raw_text)) > 0),
               source TEXT NOT NULL,
               status TEXT NOT NULL DEFAULT 'pending',
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_events_recorded_at ON events(recorded_at);
             CREATE INDEX IF NOT EXISTS idx_events_status ON events(status);",
        )?;
        Ok(Self { connection })
    }

    pub fn insert_event(&self, event: NewEvent<'_>) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO events
             (id, occurred_at, recorded_at, raw_text, source, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?6)",
            params![
                id,
                event.occurred_at.to_rfc3339(),
                event.recorded_at.to_rfc3339(),
                event.raw_text,
                event.source,
                now,
            ],
        )?;
        Ok(id)
    }

    pub fn list_events(&self) -> Result<Vec<EventSummary>> {
        let mut statement = self
            .connection
            .prepare("SELECT recorded_at, raw_text FROM events ORDER BY recorded_at DESC")?;
        let rows = statement.query_map([], |row| {
            Ok(EventSummary {
                recorded_at: row.get(0)?,
                raw_text: row.get(1)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }
}
