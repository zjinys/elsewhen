//! Read-only inventory of work, including inputs without a run-log entry yet.
use crate::storage::Store;
use anyhow::{ensure, Result};
use rusqlite::params;

#[derive(Debug, Clone)]
pub struct KnowledgeQueueItem {
    pub id: String,
    pub task: String,
    pub page_slug: String,
    pub title: String,
    pub status: String,
    pub detail: String,
    pub available_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KnowledgeQueuePage {
    pub pending: i64,
    pub running: i64,
    pub waiting: i64,
    pub retry: i64,
    pub skipped: i64,
    pub completed: i64,
    pub total: i64,
    pub items: Vec<KnowledgeQueueItem>,
    pub has_more: bool,
}

pub(crate) fn usable(store: &Store, source: &crate::storage::SourceSnapshot) -> Result<bool> {
    Ok(store.connection.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_current_sources WHERE id=?1 AND usable)",[&source.id],|r|r.get(0))?)
}

// The query transports metadata only. Pagination and counts stay in SQLite.
const QUEUE: &str = r#"
WITH sources AS MATERIALIZED (
 SELECT c.*, (c.chars<=1400 OR EXISTS(SELECT 1 FROM knowledge_source_readings r WHERE r.snapshot_id=c.id AND r.strategy_version=?1 AND r.is_root=1)) AS ready
 FROM knowledge_current_sources c
), tasks(task) AS (VALUES('source-reading'),('source-compilation'),('wiki-integration')),
raw AS MATERIALIZED (
 SELECT t.task||':'||s.id AS id,t.task,s.slug AS page_slug,s.title||' · v'||s.version AS title,
 CASE WHEN NOT s.usable THEN 'skipped' WHEN t.task='source-reading' AND s.ready THEN 'succeeded'
 WHEN t.task<>'source-reading' AND NOT s.ready THEN 'waiting'
 WHEN r.status='running' THEN 'running' WHEN r.status='failed' THEN 'retry'
 WHEN t.task='source-reading' THEN 'pending'
 WHEN t.task='wiki-integration' AND julianday(COALESCE(r.finished_at,r.started_at))<=julianday('now')-30 THEN 'pending'
 WHEN r.status='succeeded' AND r.result_count=0 THEN 'skipped' WHEN r.status='succeeded' THEN 'succeeded' ELSE 'pending' END AS status,
 CASE WHEN NOT s.usable THEN '来源不认可或已归档，保留原文'
 WHEN t.task='source-reading' AND s.ready THEN '全文阅读已完成'
 WHEN NOT s.ready AND t.task<>'source-reading' THEN '等待原料全文阅读完成'
 ELSE COALESCE(r.detail,r.error,'等待后台处理') END AS detail,r.retry_at AS available_at
 FROM sources s CROSS JOIN tasks t LEFT JOIN knowledge_background_runs r ON r.rowid=(
 SELECT b.rowid FROM knowledge_background_runs b WHERE b.task=t.task AND b.strategy_version=CASE WHEN t.task='source-reading' THEN ?1 ELSE 'fulltext-maintenance-v2' END AND b.input_snapshot=s.id ORDER BY b.started_at DESC,b.rowid DESC LIMIT 1)
 WHERE t.task<>'source-reading' OR s.chars>1400
 UNION ALL
 SELECT 'refresh:'||p.id,'knowledge-refresh',p.slug,p.title,
 CASE WHEN j.status IN ('pending','retry') AND EXISTS(SELECT 1 FROM knowledge_page_sources k JOIN knowledge_snapshots old ON old.id=k.snapshot_id JOIN sources c ON c.source_id=old.source_id WHERE k.page_id=p.id AND NOT c.usable) THEN 'skipped'
 WHEN j.status IN ('pending','retry') AND (EXISTS(SELECT 1 FROM knowledge_page_sources k JOIN knowledge_snapshots old ON old.id=k.snapshot_id JOIN sources c ON c.source_id=old.source_id WHERE k.page_id=p.id AND NOT c.ready) OR EXISTS(SELECT 1 FROM knowledge_dependencies d JOIN knowledge_stale_dependencies st ON st.page_id=d.upstream_id WHERE d.page_id=p.id)) THEN 'waiting'
 ELSE j.status END,COALESCE(j.detail,'等待后台复核'),j.available_at
 FROM knowledge_refresh_jobs j JOIN wiki_pages p ON p.id=j.page_id
 UNION ALL
 SELECT 'dependency:'||p.id,'knowledge-review',p.slug,p.title,'waiting','上游知识已变化，等待修订与人工审阅',NULL
 FROM knowledge_stale_dependencies s JOIN wiki_pages p ON p.id=s.page_id WHERE p.status<>'archived'
)
"#;

pub(crate) fn list(store: &Store, offset: i64, status: Option<&str>) -> Result<KnowledgeQueuePage> {
    ensure!(offset >= 0, "队列位置无效");
    ensure!(
        status.is_none_or(|s| matches!(
            s,
            "pending" | "running" | "waiting" | "retry" | "skipped" | "succeeded"
        )),
        "队列筛选无效"
    );
    let mut out = KnowledgeQueuePage {
        pending: 0,
        running: 0,
        waiting: 0,
        retry: 0,
        skipped: 0,
        completed: 0,
        total: 0,
        items: vec![],
        has_more: false,
    };
    let rows=store.connection.prepare(&format!("{QUEUE}
        SELECT (SELECT json_group_object(status,n) FROM (SELECT status,COUNT(*) n FROM raw GROUP BY status)),p.id,p.task,p.page_slug,p.title,p.status,p.detail,p.available_at
        FROM (SELECT 1) LEFT JOIN (SELECT * FROM raw WHERE (?2 IS NULL OR status=?2) ORDER BY CASE status WHEN 'running' THEN 0 WHEN 'retry' THEN 1 WHEN 'waiting' THEN 2 WHEN 'pending' THEN 3 WHEN 'skipped' THEN 4 ELSE 5 END,id LIMIT 51 OFFSET ?3) p ON 1=1"))?
        .query_map(params![super::reading::VERSION,status,offset],|r| {
            let item=if let Some(id)=r.get::<_,Option<String>>(1)? {Some(KnowledgeQueueItem{id,task:r.get(2)?,page_slug:r.get(3)?,title:r.get(4)?,status:r.get(5)?,detail:r.get(6)?,available_at:r.get(7)?})}else{None};
            Ok((r.get::<_,String>(0)?,item))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
    if let Some((counts, _)) = rows.first() {
        let counts: std::collections::HashMap<String, i64> = serde_json::from_str(counts)?;
        out.total = counts.values().sum();
        for (s, n) in counts {
            match s.as_str() {
                "pending" => out.pending = n,
                "running" => out.running = n,
                "waiting" => out.waiting = n,
                "retry" => out.retry = n,
                "skipped" => out.skipped = n,
                "succeeded" => out.completed = n,
                _ => {}
            }
        }
    }
    out.items = rows.into_iter().filter_map(|(_, item)| item).collect();
    out.has_more = out.items.len() > 50;
    out.items.truncate(50);
    Ok(out)
}
