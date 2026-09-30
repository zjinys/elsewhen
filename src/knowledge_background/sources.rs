//! One source version per tick, using the existing durable background ledger.
use crate::{ai::provider::AiProvider, storage::Store};
use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

const TASK: &str = "source-compilation";

pub(crate) fn run_automatic_source_compilation(
    store: &Store,
    provider: &dyn AiProvider,
) -> Result<i64> {
    run_source_task(store, provider, TASK, 0, compile)
}

pub(super) fn run_source_task(
    store: &Store,
    provider: &dyn AiProvider,
    task: &str,
    refresh_days: i64,
    process: fn(&Store, &dyn AiProvider, &str, &str, &str) -> Result<i64>,
) -> Result<i64> {
    let now = chrono::Utc::now();
    let strategy = "fulltext-maintenance-v2";
    let tx = Transaction::new_unchecked(&store.connection, TransactionBehavior::Immediate)?;
    store.connection.execute(
        "UPDATE knowledge_background_runs SET status='failed',finished_at=?1,error='上次整理中断，稍后自动重试'
         WHERE task=?2 AND status='running' AND started_at<?3",
        params![now.to_rfc3339(), task, (now-chrono::Duration::minutes(10)).to_rfc3339()],
    )?;
    let active: bool = store.connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM knowledge_background_runs WHERE task=?1 AND status='running')",
        [task],
        |r| r.get(0),
    )?;
    if active {
        tx.commit()?;
        return Ok(0);
    }
    // Current originals only, including imported pages that predate this worker.
    // Successful versions are filtered in SQL so old history cannot starve new input.
    let selected: Option<(String, String, String, i64)> = store
        .connection
        .query_row(
            "WITH failures AS (
            SELECT input_key,COUNT(*) AS count,MAX(finished_at) AS last
            FROM knowledge_background_runs WHERE task=?1 AND strategy_version=?4 AND status='failed' GROUP BY input_key
         ) SELECT p.slug,s.id,p.title,s.version FROM knowledge_source_pages m
         JOIN wiki_pages p ON p.id=m.page_id JOIN knowledge_sources origin ON origin.id=m.source_id
         JOIN knowledge_snapshots s ON s.source_id=m.source_id
         LEFT JOIN failures f ON f.input_key=s.id
         WHERE p.kind IN ('source','note') AND p.status<>'archived' AND COALESCE(origin.opinion,'')<>'reject'
         AND s.version=(SELECT MAX(version) FROM knowledge_snapshots WHERE source_id=m.source_id)
         AND (length(s.content_md)<=1400 OR EXISTS(SELECT 1 FROM knowledge_source_readings reading WHERE reading.snapshot_id=s.id AND reading.strategy_version=?5 AND reading.is_root=1))
         AND NOT EXISTS(SELECT 1 FROM knowledge_background_runs r
             WHERE r.task=?1 AND r.input_key=s.id AND r.status='succeeded' AND r.strategy_version=?4
             AND (?3=0 OR julianday(r.finished_at)>julianday(?2)-?3))
         AND (f.last IS NULL OR julianday(?2)>=julianday(f.last)+
             (CASE WHEN f.count>=5 THEN 21600 ELSE 60*(1 << MIN(f.count,4)) END)/86400.0)
         ORDER BY s.captured_at,p.slug LIMIT 1",
            params![task, now.to_rfc3339(),refresh_days,strategy,crate::knowledge::reading::VERSION],
            |r| Ok((r.get(0)?, r.get(1)?,r.get(2)?,r.get(3)?)),
        )
        .optional()?;
    let Some((slug, snapshot_id, title, version)) = selected else {
        tx.commit()?;
        return Ok(0);
    };
    let run = uuid::Uuid::new_v4().to_string();
    store.connection.execute(
        "INSERT INTO knowledge_background_runs(id,task,input_key,status,started_at,source_slug,source_title,source_version,strategy_version) VALUES(?1,?2,?3,'running',?4,?5,?6,?7,?8)",
        params![run,task,snapshot_id,now.to_rfc3339(),slug,title,version,strategy],
    )?;
    tx.commit()?;
    let result = process(store, provider, &slug, &snapshot_id, &run);
    match result {
        Ok(count) => {
            store.connection.execute(
                "UPDATE knowledge_background_runs SET status='succeeded',finished_at=?2,result_count=?3,detail=COALESCE(detail,?4) WHERE id=?1 AND status='running'",
                params![run,chrono::Utc::now().to_rfc3339(),count,if count==0 {"已检查，没有新增产出"}else{"整理结果已保存"}],
            )?;
            Ok(count)
        }
        Err(error) => {
            // No response bodies or private source text in the durable error field.
            if std::env::var("ELSEWHEN_DEBUG").is_ok() {
                eprintln!("[{task}] run={run} failed: {error:#}");
            }
            let attempts:i64=store.connection.query_row("SELECT COUNT(*) FROM knowledge_background_runs WHERE task=?1 AND input_key=?2 AND strategy_version=?3 AND status='failed'",params![task,snapshot_id,strategy],|r|r.get(0))?;
            let delay = if attempts >= 4 {
                21600
            } else {
                60 * (1i64 << (attempts + 1))
            };
            store.connection.execute(
                "UPDATE knowledge_background_runs SET status='failed',finished_at=?2,retry_at=?3,error='本次整理未完成，稍后自动重试；原文已保留' WHERE id=?1 AND status='running'",
                params![run,chrono::Utc::now().to_rfc3339(),(chrono::Utc::now()+chrono::Duration::seconds(delay)).to_rfc3339()],
            )?;
            Ok(0)
        }
    }
}

fn compile(
    store: &Store,
    provider: &dyn AiProvider,
    slug: &str,
    snapshot_id: &str,
    run: &str,
) -> Result<i64> {
    let current = store
        .source_history(slug)?
        .into_iter()
        .max_by_key(|s| s.version)
        .context("原料不存在")?;
    anyhow::ensure!(current.id == snapshot_id, "原料版本已更新，下次处理新版本");
    // Existing human choices win, even if a background tick was already scheduled.
    let manual_decision: bool = store.connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM knowledge_proposals k JOIN json_each(k.snapshot_ids) j
         JOIN knowledge_snapshots s ON s.id=j.value
         WHERE s.source_id=?1 AND k.kind IN ('method','case','principle')
         AND (k.status='rejected' OR (k.status='pending' AND k.origin='manual')))",
        [&current.source_id],
        |r| r.get(0),
    )?;
    if manual_decision {
        store.connection.execute(
            "UPDATE knowledge_background_runs SET detail='保留已有人工待审或拒绝决定' WHERE id=?1",
            [run],
        )?;
        return Ok(0);
    }
    for output in store.knowledge_output_pages(slug)? {
        if !matches!(output.kind.as_str(), "method" | "case" | "principle") {
            continue;
        }
        let metadata = store.knowledge_metadata(&output.slug)?;
        if output.human_edited_at.is_some()
            || metadata.confirmed_at.is_some()
            || metadata.strength != "reference"
        {
            store.connection.execute("UPDATE knowledge_background_runs SET detail='已有人工定稿，保留正文；变更由审阅处理' WHERE id=?1",[run])?;
            return Ok(0);
        }
    }
    let Some(proposal) = crate::knowledge::build_source_proposal(store, slug, provider, run)?
    else {
        return Ok(0);
    };
    // This call checks proposal ownership, current source versions, rejection,
    // optimistic page hash and human edits inside its write transaction.
    Ok(i64::from(
        store.save_automatic_reference(&proposal)?.is_some(),
    ))
}
