//! Every page affected by a source update owns a durable completion record.
use crate::{ai::provider::AiProvider, storage::Store};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Transaction, TransactionBehavior};

pub(crate) fn run_knowledge_refresh(store: &Store, provider: &dyn AiProvider) -> Result<i64> {
    let now = chrono::Utc::now();
    // Queue only pages whose direct parents are ready. Descendants follow after
    // their parents are confirmed, avoiding repeated model calls on stale input.
    store.connection.execute("INSERT INTO knowledge_refresh_jobs(page_id,requested_at,available_at,review_only,dependency_epoch)
        SELECT p.id,?1,?1,1,(SELECT SUM(u.knowledge_epoch+1) FROM knowledge_dependencies d JOIN wiki_pages u ON u.id=d.upstream_id WHERE d.page_id=p.id)
        FROM knowledge_stale_dependencies st JOIN wiki_pages p ON p.id=st.page_id
        WHERE p.status<>'archived' AND COALESCE(p.opinion,'')<>'reject'
        AND NOT EXISTS(SELECT 1 FROM knowledge_dependencies d JOIN knowledge_stale_dependencies parent ON parent.page_id=d.upstream_id WHERE d.page_id=p.id)
        ON CONFLICT(page_id) DO UPDATE SET review_only=1,dependency_epoch=excluded.dependency_epoch,
        status='pending',generation=knowledge_refresh_jobs.generation+1,attempts=0,available_at=excluded.available_at
        WHERE knowledge_refresh_jobs.dependency_epoch IS NOT excluded.dependency_epoch",[now.to_rfc3339()])?;
    // Re-evaluate skipped jobs after endorsement/restoration, including changes
    // made while the app was stopped. Accepted/rejected proposal history is kept.
    store.connection.execute("UPDATE knowledge_refresh_jobs AS j SET status='pending',generation=generation+1,attempts=0,available_at=?1,detail='来源重新可用，已恢复更新检查'
        WHERE j.status='skipped'
        AND EXISTS(SELECT 1 FROM wiki_pages p WHERE p.id=j.page_id AND p.status<>'archived' AND COALESCE(p.opinion,'')<>'reject')
        AND EXISTS(SELECT 1 FROM knowledge_page_sources k JOIN knowledge_snapshots s ON s.id=k.snapshot_id WHERE k.page_id=j.page_id AND s.version<(SELECT MAX(version) FROM knowledge_snapshots WHERE source_id=s.source_id))
        AND NOT EXISTS(SELECT 1 FROM knowledge_page_sources k JOIN knowledge_snapshots s ON s.id=k.snapshot_id JOIN knowledge_sources o ON o.id=s.source_id
            WHERE k.page_id=j.page_id AND (o.opinion='reject' OR NOT EXISTS(SELECT 1 FROM knowledge_source_pages m JOIN wiki_pages p ON p.id=m.page_id WHERE m.source_id=o.id AND p.status<>'archived')))", [now.to_rfc3339()])?;
    let candidates=store.connection.prepare("SELECT page_id,generation FROM knowledge_refresh_jobs WHERE status IN ('pending','running','retry') AND julianday(available_at)<=julianday(?1) ORDER BY requested_at,page_id LIMIT 64")?
        .query_map([now.to_rfc3339()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, generation) in candidates {
        let slug: String =
            store
                .connection
                .query_row("SELECT slug FROM wiki_pages WHERE id=?1", [&id], |r| {
                    r.get(0)
                })?;
        let page = store.get_wiki_page(&slug)?.context("待更新页面不存在")?;
        if page.status == "archived" || page.opinion.as_deref() == Some("reject") {
            store.connection.execute("UPDATE knowledge_refresh_jobs SET status='skipped',detail='页面已归档或被拒绝，保留人工决定' WHERE page_id=?1 AND generation=?2",params![id,generation])?;
            continue;
        }
        let blocked:bool=store.connection.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_dependencies d JOIN wiki_pages u ON u.id=d.upstream_id WHERE d.page_id=?1 AND (u.status='archived' OR u.opinion='reject' OR u.id IN (SELECT page_id FROM knowledge_stale_dependencies)))",[&id],|r|r.get(0))?;
        if blocked {
            store.connection.execute("UPDATE knowledge_refresh_jobs SET available_at=?2,detail='等待上游知识修复与确认' WHERE page_id=?1",params![id,(now+chrono::Duration::minutes(1)).to_rfc3339()])?;
            continue;
        }
        let mut ready = true;
        for source in store.page_source_snapshots(&slug)? {
            let latest:String=store.connection.query_row("SELECT id FROM knowledge_snapshots WHERE source_id=?1 ORDER BY version DESC LIMIT 1",[source.source_id],|r|r.get(0))?;
            let snapshot = store.source_snapshot(&latest)?.context("来源不存在")?;
            let origin = snapshot
                .page_slug
                .as_deref()
                .map(|s| store.get_wiki_page(s))
                .transpose()?
                .flatten();
            if snapshot.opinion.as_deref() == Some("reject")
                || origin.is_none_or(|p| p.status == "archived")
            {
                store.connection.execute("UPDATE knowledge_refresh_jobs SET status='skipped',detail='来源被拒绝或归档，保留当前知识与核对提示' WHERE page_id=?1 AND generation=?2",params![id,generation])?;
                ready = false;
                break;
            }
            if !crate::knowledge::reading::ready(store, &snapshot)? {
                ready = false;
                break;
            }
        }
        if !ready {
            store.connection.execute("UPDATE knowledge_refresh_jobs SET available_at=?2 WHERE page_id=?1 AND status<>'skipped'",params![id,(now+chrono::Duration::minutes(1)).to_rfc3339()])?;
            continue;
        }
        let tx = Transaction::new_unchecked(&store.connection, TransactionBehavior::Immediate)?;
        let changed=store.connection.execute("UPDATE knowledge_refresh_jobs SET status='running',attempts=attempts+1,available_at=?3 WHERE page_id=?1 AND generation=?2 AND julianday(available_at)<=julianday(?4)",params![id,generation,(now+chrono::Duration::minutes(10)).to_rfc3339(),now.to_rfc3339()])?;
        if changed == 0 {
            continue;
        }
        store.connection.execute("UPDATE knowledge_background_runs SET status='failed',finished_at=?2,detail='上次更新中断，任务已恢复' WHERE task='knowledge-refresh' AND input_key LIKE ?1 AND status='running'",params![format!("{id}:%"),now.to_rfc3339()])?;
        let run = uuid::Uuid::new_v4().to_string();
        store.connection.execute("INSERT INTO knowledge_background_runs(id,task,input_key,status,started_at,source_slug,source_title,detail,strategy_version) VALUES(?1,'knowledge-refresh',?2,'running',?3,?4,?5,'来源有新版本，正在检查依赖知识','fulltext-maintenance-v2')",params![run,format!("{id}:{generation}"),now.to_rfc3339(),slug,page.title])?;
        tx.commit()?;
        let result = (|| -> Result<i64> {
            let proposal = crate::knowledge::build_refresh_proposal(store, &slug, provider)?;
            let tx = Transaction::new_unchecked(&store.connection, TransactionBehavior::Immediate)?;
            let active:bool=store.connection.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_refresh_jobs WHERE page_id=?1 AND generation=?2 AND status='running' AND EXISTS(SELECT 1 FROM knowledge_background_runs WHERE id=?3 AND status='running'))",params![id,generation,run],|r|r.get(0))?;
            ensure!(active, "更新期间来源再次变化，等待新一轮处理");
            let review_only: bool = store.connection.query_row(
                "SELECT review_only FROM knowledge_refresh_jobs WHERE page_id=?1",
                [&id],
                |r| r.get(0),
            )?;
            if review_only {
                store.connection.execute("UPDATE knowledge_proposals SET origin='manual' WHERE id=?1 AND status='pending'",[&proposal])?;
            }
            let published = !review_only && store.publish_reference_in_tx(&proposal)?.is_some();
            let status: String = store.connection.query_row(
                "SELECT status FROM knowledge_proposals WHERE id=?1",
                [proposal],
                |r| r.get(0),
            )?;
            let detail = if published {
                "已依据新来源更新参考知识"
            } else if status == "rejected" {
                "保留已有拒绝决定"
            } else {
                "修订已准备，等待确认；当前正文保留"
            };
            store.connection.execute("UPDATE knowledge_refresh_jobs SET status='succeeded',detail=?3 WHERE page_id=?1 AND generation=?2",params![id,generation,detail])?;
            store.connection.execute("UPDATE knowledge_background_runs SET status='succeeded',finished_at=?2,result_count=1,detail=?3 WHERE id=?1",params![run,chrono::Utc::now().to_rfc3339(),detail])?;
            tx.commit()?;
            Ok(1)
        })();
        match result {
            Ok(n) => return Ok(n),
            Err(error) => {
                if std::env::var("ELSEWHEN_DEBUG").is_ok() {
                    eprintln!("[knowledge-refresh] {error:#}");
                }
                let attempts: i64 = store.connection.query_row(
                    "SELECT attempts FROM knowledge_refresh_jobs WHERE page_id=?1",
                    [&id],
                    |r| r.get(0),
                )?;
                let seconds = if attempts >= 5 {
                    21600
                } else {
                    60 * (1i64 << attempts)
                };
                let retry = (chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339();
                store.connection.execute("UPDATE knowledge_refresh_jobs SET status='retry',available_at=?3,detail='本次更新未完成，原知识保留，稍后重试' WHERE page_id=?1 AND generation=?2 AND EXISTS(SELECT 1 FROM knowledge_background_runs WHERE id=?4 AND status='running')",params![id,generation,retry,run])?;
                store.connection.execute("UPDATE knowledge_background_runs SET status='failed',finished_at=?2,retry_at=?3,error='未完成来源更新，原知识保留' WHERE id=?1",params![run,chrono::Utc::now().to_rfc3339(),retry])?;
                return Ok(0);
            }
        }
    }
    Ok(0)
}
