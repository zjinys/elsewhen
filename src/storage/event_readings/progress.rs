use super::*;

pub(crate) struct ReadingProgress {
    pub phase: String,
    pub read_chars: i64,
    pub total_chars: i64,
    pub completed_leaves: i64,
    pub completed_merges: i64,
    pub error: Option<String>,
    pub retry_at: String,
}
impl Store {
    pub(crate) fn event_reading_progress(&self, event: &str) -> Result<Option<ReadingProgress>> {
        let row:Option<(String,i64,i64,i64,i64,Option<String>,i64)>=self.connection.query_row("SELECT t.status,t.total_chars,COALESCE(SUM(CASE WHEN n.kind='leaf' AND n.status='succeeded' THEN n.end_char-n.start_char ELSE 0 END),0),COALESCE(SUM(n.kind='leaf' AND n.status='succeeded'),0),COALESCE(SUM(n.kind='merge' AND n.status='succeeded'),0),t.last_error,t.available_at FROM event_reading_tasks t JOIN knowledge_digest_jobs j ON j.event_id=t.event_id AND j.digest_version=t.digest_version AND j.prompt_bundle_id=t.bundle_id LEFT JOIN event_reading_nodes n ON n.task_id=t.id WHERE t.event_id=?1 GROUP BY t.id ORDER BY t.created_at DESC LIMIT 1",[event],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional()?;
        Ok(row.map(
            |(
                status,
                total_chars,
                read_chars,
                completed_leaves,
                completed_merges,
                error,
                retry,
            )| ReadingProgress {
                phase: if status == "active" {
                    if read_chars < total_chars {
                        "reading"
                    } else {
                        "merging"
                    }
                    .into()
                } else {
                    status
                },
                read_chars,
                total_chars,
                completed_leaves,
                completed_merges,
                error,
                retry_at: chrono::DateTime::from_timestamp(retry, 0)
                    .unwrap_or_default()
                    .to_rfc3339(),
            },
        ))
    }
}
