//! 知识消化（FR-PES-004 阶段 1）FRB 门面：后台 worker tick + 只读队列/运行日志。
//!
//! 没有手动触发入口：GUI 的后台 worker 在分析队列之后周期调用
//! [`trigger_knowledge_digest`]；失败项冷却期满后由队列自动回队。

use super::*;

/// 一次后台消化 tick 的结果。
#[derive(Clone, Debug)]
pub enum KnowledgeDigestTickResult {
    /// 尚未配置 AI provider：事件照常保存，任务等待
    NoProvider,
    /// 没有到期可消化的事件
    Idle,
    /// 一批事件消化成功（字段名避开 C 保留字：FFI 结构体里 `protected` 会被改名）
    Processed {
        events: i64,
        created_slugs: Vec<String>,
        updated_slugs: Vec<String>,
        protected_slugs: Vec<String>,
    },
    /// 本批失败，已退避回队（原因见运行日志）
    Failed { events: i64, error: String },
}

#[derive(Clone, Debug)]
pub struct KnowledgeDigestStatsDto {
    pub pending: i64,
    pub running: i64,
    pub retry: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub skipped: i64,
    pub last_success_at: Option<String>,
    pub last_error: Option<String>,
    pub last_error_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct KnowledgeDigestJobDto {
    pub job_id: String,
    pub event_id: String,
    pub event_excerpt: String,
    pub recorded_at: String,
    pub status: String,
    pub attempts: i64,
    pub failed_rounds: i64,
    pub available_at: String,
    pub last_error: Option<String>,
    pub skip_reason: Option<String>,
    pub batch_id: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
pub struct KnowledgeDigestRunDto {
    pub id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub event_count: i64,
    pub model: Option<String>,
    pub duration_ms: Option<i64>,
    pub created_slugs: Vec<String>,
    pub updated_slugs: Vec<String>,
    pub protected_slugs: Vec<String>,
    pub error: Option<String>,
}

/// 后台 worker 调用：处理一批到期的知识消化任务（最多一批，调用方循环排空）。
pub fn trigger_knowledge_digest() -> Result<KnowledgeDigestTickResult> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let Some((provider, model)) = crate::wiki::digest_provider(&store)? else {
        return Ok(KnowledgeDigestTickResult::NoProvider);
    };
    // 同分析队列：每次 tick 先回收上次中断遗留的 running（同一时刻只有一个 worker）。
    store.recover_interrupted_digest_jobs()?;
    Ok(
        match crate::wiki::process_digest_queue(&store, &provider, Some(model.as_str()))? {
            crate::wiki::DigestTick::Idle => KnowledgeDigestTickResult::Idle,
            crate::wiki::DigestTick::Processed { events, outcome } => {
                KnowledgeDigestTickResult::Processed {
                    events: events as i64,
                    created_slugs: outcome.created,
                    updated_slugs: outcome.updated,
                    protected_slugs: outcome.protected,
                }
            }
            crate::wiki::DigestTick::Failed { events, error } => {
                KnowledgeDigestTickResult::Failed {
                    events: events as i64,
                    error,
                }
            }
        },
    )
}

pub fn get_knowledge_digest_stats() -> Result<KnowledgeDigestStatsDto> {
    let config = crate::config::AppConfig::load()?;
    let stats = Store::open(&config.database_path)?.digest_job_stats()?;
    Ok(KnowledgeDigestStatsDto {
        pending: stats.pending,
        running: stats.running,
        retry: stats.retry,
        succeeded: stats.succeeded,
        failed: stats.failed,
        skipped: stats.skipped,
        last_success_at: stats.last_success_at,
        last_error: stats.last_error,
        last_error_at: stats.last_error_at,
    })
}

/// 队列明细（只读）。`status` 为 None 时返回全部状态，按最近更新倒序。
pub fn list_knowledge_digest_jobs(
    status: Option<String>,
    limit: u32,
) -> Result<Vec<KnowledgeDigestJobDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let status = status.as_deref().map(str::trim).filter(|s| !s.is_empty());
    Ok(store
        .list_digest_jobs(status, limit.clamp(1, 500) as usize)?
        .into_iter()
        .map(|row| KnowledgeDigestJobDto {
            job_id: row.job_id,
            event_id: row.event_id,
            event_excerpt: row.event_excerpt,
            recorded_at: row.recorded_at,
            status: row.status,
            attempts: row.attempts,
            failed_rounds: row.failed_rounds,
            available_at: row.available_at,
            last_error: row.last_error,
            skip_reason: row.skip_reason,
            batch_id: row.batch_id,
            updated_at: row.updated_at,
        })
        .collect())
}

/// 最近的运行日志（只读），按开始时间倒序。
pub fn list_knowledge_digest_runs(limit: u32) -> Result<Vec<KnowledgeDigestRunDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    Ok(store
        .list_digest_runs(limit.clamp(1, 200) as usize)?
        .into_iter()
        .map(|row| KnowledgeDigestRunDto {
            id: row.id,
            started_at: row.started_at,
            finished_at: row.finished_at,
            status: row.status,
            event_count: row.event_count,
            model: row.model,
            duration_ms: row.duration_ms,
            created_slugs: row.created_slugs,
            updated_slugs: row.updated_slugs,
            protected_slugs: row.protected_slugs,
            error: row.error,
        })
        .collect())
}
