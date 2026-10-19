//! 页级再汇总后台化（提案 2026-10-19 §6，P2）。
//!
//! 与手动「重新整理此页」共用 `wiki::reconsolidate_page` worker（同一提示词、
//! 同一 preservation 铁律、同一账本 task='page-reconsolidation'），只是把
//! **排程**搬进后台：证据增量 / 时间阈值由派生读算出（见
//! `Store::reconsolidation_enqueue_candidates`），每 tick 至多处理一页、串行、
//! 失败指数退避，不做「一次事件触发全库重算」。

use crate::ai::provider::AiProvider;
use crate::storage::Store;
use anyhow::Result;

pub(crate) fn run_knowledge_reconsolidation(
    store: &Store,
    provider: &dyn AiProvider,
) -> Result<i64> {
    if !store.background_processing_allowed()? {
        return Ok(0);
    }
    store.reconsolidation_enqueue_candidates(&chrono::Utc::now().to_rfc3339())?;
    // 死进程遗留：心跳（available_at）过期仍在 running = 上次中断，退回重试。
    store.connection.execute(
        "UPDATE knowledge_reconsolidation_jobs SET status = 'retry',
           detail = '上次处理中断，稍后重试'
         WHERE status = 'running' AND julianday(available_at) < julianday(?1)",
        [chrono::Utc::now().to_rfc3339()],
    )?;
    let Some((page_id, generation)) =
        store.claim_reconsolidation_job(&chrono::Utc::now().to_rfc3339())?
    else {
        return Ok(0);
    };
    let slug: String = store.connection.query_row(
        "SELECT slug FROM wiki_pages WHERE id = ?1",
        [&page_id],
        |row| row.get(0),
    )?;
    let page = store.get_wiki_page(&slug)?.context("待再汇总页面不存在")?;
    // 人工编辑保护（提案立场）：不调模型，任务终止并说明。
    if page.human_edited_at.is_some() {
        store.finish_reconsolidation_job(
            &page_id,
            "skipped",
            "人工编辑保护，正文不动；改写请走审阅提案通道",
        )?;
        return Ok(0);
    }
    let _ = generation; // 单进程 tick 串行，generation 仅作乐观锁留痕
    let result = crate::wiki::reconsolidate_page(store, provider, &slug);
    match result {
        Ok(message) => {
            store.finish_reconsolidation_job(&page_id, "succeeded", &message)?;
            Ok(1)
        }
        Err(error) => {
            let attempts: i64 = store.connection.query_row(
                "SELECT attempts FROM knowledge_reconsolidation_jobs WHERE page_id = ?1",
                [&page_id],
                |row| row.get(0),
            )?;
            let seconds = if attempts >= 5 {
                21600
            } else {
                60 * (1i64 << attempts)
            };
            let retry_at = (chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339();
            store.connection.execute(
                "UPDATE knowledge_reconsolidation_jobs SET status = 'retry',
                   available_at = ?2,
                   detail = ?3
                 WHERE page_id = ?1",
                rusqlite::params![
                    page_id,
                    retry_at,
                    format!(
                        "{}——原知识保留，稍后重试",
                        error.to_string().chars().take(200).collect::<String>()
                    )
                ],
            )?;
            if std::env::var("ELSEWHEN_DEBUG").is_ok() {
                eprintln!("[page-reconsolidation] {slug}: {error:#}");
            }
            Ok(0)
        }
    }
}

use anyhow::Context;
