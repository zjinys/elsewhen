use crate::event::{EventSummary, NewEvent};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use std::path::Path;
use uuid::Uuid;

pub struct AnalysisJob {
    pub id: String,
    pub event_id: String,
    pub raw_text: String,
    pub attempts: i64,
}

pub struct AiProviderConfig {
    pub provider_type: String,
    pub base_url: String,
    pub model: String,
    pub api_key_source: String,
    pub api_key: String,
}

pub struct AnalysisSummary {
    pub raw_text: String,
    pub event_type: String,
    pub confidence: f64,
    pub clarifications: String,
}

pub struct Store {
    connection: Connection,
    path: std::path::PathBuf,
}

impl Clone for Store {
    fn clone(&self) -> Self {
        Self::open(&self.path).expect("reopen database")
    }
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)
            .with_context(|| format!("open SQLite database {}", path.display()))?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
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
             CREATE INDEX IF NOT EXISTS idx_events_status ON events(status);
             CREATE TRIGGER IF NOT EXISTS prevent_raw_event_mutation
             BEFORE UPDATE OF raw_text, recorded_at, source ON events
             BEGIN
               SELECT RAISE(ABORT, 'raw_event_is_immutable');
             END;
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
             CREATE TABLE IF NOT EXISTS analysis_jobs (
               id TEXT PRIMARY KEY, event_id TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending',
               attempts INTEGER NOT NULL DEFAULT 0, last_error TEXT, available_at TEXT NOT NULL,
               created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
               FOREIGN KEY(event_id) REFERENCES events(id), UNIQUE(event_id)
             );
             CREATE INDEX IF NOT EXISTS idx_analysis_jobs_ready ON analysis_jobs(status, available_at);
             CREATE TABLE IF NOT EXISTS event_analyses (
               id TEXT PRIMARY KEY, event_id TEXT NOT NULL, prompt_version TEXT NOT NULL,
               result_json TEXT NOT NULL, created_at TEXT NOT NULL,
               FOREIGN KEY(event_id) REFERENCES events(id)
             );
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
             CREATE TABLE IF NOT EXISTS ai_provider_configs (
               id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE,
               provider_type TEXT NOT NULL, base_url TEXT NOT NULL, model TEXT NOT NULL,
               api_key_source TEXT NOT NULL, api_key TEXT,
               enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
               created_at TEXT NOT NULL, updated_at TEXT NOT NULL
             );
             INSERT OR IGNORE INTO schema_migrations(version, applied_at)
             VALUES (3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));",
        )?;
        let has_api_key = {
            let mut statement = connection.prepare("PRAGMA table_info(ai_provider_configs)")?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            columns.iter().any(|name| name == "api_key")
        };
        if !has_api_key {
            connection.execute(
                "ALTER TABLE ai_provider_configs ADD COLUMN api_key TEXT",
                [],
            )?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(Self {
            connection,
            path: path.to_path_buf(),
        })
    }

    pub fn insert_event(&self, event: NewEvent<'_>) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
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
        transaction.execute(
            "INSERT INTO analysis_jobs (id, event_id, status, attempts, available_at, created_at, updated_at)
             VALUES (?1, ?2, 'pending', 0, ?3, ?3, ?3)",
            params![Uuid::new_v4().to_string(), id, now],
        )?;
        transaction.commit()?;
        Ok(id)
    }

    pub fn claim_analysis_job(&self) -> Result<Option<AnalysisJob>> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let job = transaction.query_row(
            "SELECT j.id, j.event_id, e.raw_text, j.attempts FROM analysis_jobs j JOIN events e ON e.id=j.event_id
             WHERE j.status IN ('pending','retry') AND j.available_at <= ?1 ORDER BY j.created_at LIMIT 1",
            [chrono::Utc::now().to_rfc3339()],
            |row| Ok(AnalysisJob { id: row.get(0)?, event_id: row.get(1)?, raw_text: row.get(2)?, attempts: row.get(3)? }),
        ).optional()?;
        if let Some(ref job) = job {
            transaction.execute("UPDATE analysis_jobs SET status='running', attempts=attempts+1, updated_at=?2 WHERE id=?1", params![job.id, chrono::Utc::now().to_rfc3339()])?;
        }
        transaction.commit()?;
        Ok(job)
    }

    pub fn complete_analysis(
        &self,
        job: &AnalysisJob,
        prompt_version: &str,
        result_json: &str,
    ) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        let now = chrono::Utc::now().to_rfc3339();
        transaction.execute("INSERT INTO event_analyses (id,event_id,prompt_version,result_json,created_at) VALUES (?1,?2,?3,?4,?5)", params![Uuid::new_v4().to_string(), job.event_id, prompt_version, result_json, now])?;
        transaction.execute(
            "UPDATE analysis_jobs SET status='succeeded', updated_at=?2 WHERE id=?1",
            params![job.id, now],
        )?;
        transaction.execute(
            "UPDATE events SET status='processed', processed_at=?2, updated_at=?2 WHERE id=?1",
            params![job.event_id, now],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn fail_analysis(&self, job: &AnalysisJob, error: &str) -> Result<()> {
        let delay = 2_i64.pow((job.attempts as u32).min(8));
        let available = chrono::Utc::now() + chrono::Duration::seconds(delay);
        self.connection.execute("UPDATE analysis_jobs SET status=CASE WHEN attempts >= 5 THEN 'failed' ELSE 'retry' END, last_error=?2, available_at=?3, updated_at=?4 WHERE id=?1", params![job.id, error, available.to_rfc3339(), chrono::Utc::now().to_rfc3339()])?;
        Ok(())
    }

    pub fn upsert_ai_provider_config(
        &self,
        base_url: &str,
        model: &str,
        api_key: &str,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO ai_provider_configs
             (id,name,provider_type,base_url,model,api_key_source,api_key,enabled,created_at,updated_at)
             VALUES (?1,'default','openai-compatible',?2,?3,'database',?4,1,?5,?5)
             ON CONFLICT(name) DO UPDATE SET base_url=excluded.base_url, model=excluded.model,
             provider_type=excluded.provider_type, api_key_source=excluded.api_key_source, api_key=excluded.api_key,
             enabled=1, updated_at=excluded.updated_at",
            params![Uuid::new_v4().to_string(), base_url, model, api_key, now],
        )?;
        Ok(())
    }

    pub fn active_ai_provider_config(&self) -> Result<Option<AiProviderConfig>> {
        self.connection
            .query_row(
            "SELECT provider_type,base_url,model,api_key_source,api_key FROM ai_provider_configs
             WHERE enabled=1 AND api_key IS NOT NULL ORDER BY updated_at DESC LIMIT 1",
                [],
                |row| {
                    Ok(AiProviderConfig {
                        provider_type: row.get(0)?,
                        base_url: row.get(1)?,
                        model: row.get(2)?,
                    api_key_source: row.get(3)?,
                    api_key: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
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
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn list_analyses(&self) -> Result<Vec<AnalysisSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT e.raw_text,
                    coalesce(json_extract(a.result_json,'$.event_type'),'unknown'),
                    coalesce(json_extract(a.result_json,'$.confidence'),0),
                    coalesce(json_extract(a.result_json,'$.clarifications'),'[]')
             FROM event_analyses a JOIN events e ON e.id=a.event_id
             ORDER BY a.created_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(AnalysisSummary {
                raw_text: row.get(0)?,
                event_type: row.get(1)?,
                confidence: row.get(2)?,
                clarifications: row.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_database() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "elsewhen-storage-test-{}.db",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn persists_and_lists_raw_events() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store.insert_event(NewEvent::now("完成最小 MVP")).unwrap();
        let events = store.list_events().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].raw_text, "完成最小 MVP");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn raw_text_cannot_be_changed() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let id = store.insert_event(NewEvent::now("原始事实")).unwrap();
        let result = store
            .connection
            .execute("UPDATE events SET raw_text = '被覆盖' WHERE id = ?1", [id]);
        assert!(result.is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn event_creation_enqueues_one_analysis_job() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let event_id = store.insert_event(NewEvent::now("等待后台理解")).unwrap();
        let count: i64 = store
            .connection
            .query_row(
                "SELECT count(*) FROM analysis_jobs WHERE event_id = ?1 AND status = 'pending'",
                [event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failed_analysis_keeps_raw_event_and_schedules_retry() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store
            .insert_event(NewEvent::now("AI 失败也不能丢"))
            .unwrap();
        let job = store.claim_analysis_job().unwrap().unwrap();
        store.fail_analysis(&job, "provider unavailable").unwrap();
        let status: String = store
            .connection
            .query_row(
                "SELECT status FROM analysis_jobs WHERE id = ?1",
                [&job.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "retry");
        assert_eq!(store.list_events().unwrap()[0].raw_text, "AI 失败也不能丢");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn provider_config_is_fully_persisted() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store
            .upsert_ai_provider_config("https://example.test/v1", "test-model", "secret-value")
            .unwrap();
        let provider = store.active_ai_provider_config().unwrap().unwrap();
        assert_eq!(provider.base_url, "https://example.test/v1");
        assert_eq!(provider.model, "test-model");
        assert_eq!(provider.api_key_source, "database");
        assert_eq!(provider.api_key, "secret-value");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn concurrent_connections_can_insert_events() {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        let handles = (0..8)
            .map(|index| {
                let connection = store.clone();
                std::thread::spawn(move || {
                    connection
                        .insert_event(NewEvent::now(&format!("并发事件 {index}")))
                        .unwrap();
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(store.list_events().unwrap().len(), 8);
        let _ = std::fs::remove_file(path);
    }
}
