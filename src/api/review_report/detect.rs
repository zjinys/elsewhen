//! 偏差检测：校验/去重、detect_deviation、run 记账、退避与周期 tick。

use super::*;

/// 对单份报告跑一次偏差检测。返回去重后的偏差数。
/// AI/解析失败原样上抛（tick 记账后退避重试），报告本体不受影响。
pub(crate) fn detect_deviation(
    store: &Store,
    report_id: &str,
    provider: &dyn crate::ai::provider::AiProvider,
) -> Result<usize> {
    let Some((period_type, report_json, existing)) = store.deviation_report_row(report_id)? else {
        anyhow::bail!("报告不存在：{report_id}");
    };
    if existing.is_some() {
        return Ok(0); // 已检测过（并发/重复调用），不重复打 provider。
    }
    let report: PeriodReport =
        serde_json::from_str(&report_json).context("report_json 解析失败")?;
    let seen = store.reported_deviation_keys(&period_type)?;
    let prompt = build_deviation_prompt(&report, &seen);
    let reply = provider.generate_reply_with_tools(
        vec![crate::ai::memory::ContextMessage::new("user", prompt)],
        None,
    )?;
    if !reply.tool_calls.is_empty() {
        anyhow::bail!("偏差检测不应触发工具调用");
    }
    let candidates = validate_deviation_candidates(&reply.content, &report.goals, &seen)?;
    let json = serde_json::to_string(&candidates)?;
    store.record_detected_deviations(report_id, &json, Utc::now())?;
    Ok(candidates.len())
}

// ── 运行账本（knowledge_background_runs 同款，task=period-report-*）──

pub(crate) fn start_run(store: &Store, task: &str, input_key: &str) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    store.connection.execute(
        "INSERT INTO knowledge_background_runs(id,task,input_key,status,started_at)
         VALUES (?1,?2,?3,'running',?4)",
        rusqlite::params![id, task, input_key, Utc::now().to_rfc3339()],
    )?;
    Ok(id)
}

pub(crate) fn finish_run(
    store: &Store,
    run_id: &str,
    status: &str,
    error: Option<&str>,
    result_count: Option<i64>,
) -> Result<()> {
    store.connection.execute(
        "UPDATE knowledge_background_runs
         SET status=?2, finished_at=?3, error=?4, result_count=?5
         WHERE id=?1",
        rusqlite::params![
            run_id,
            status,
            Utc::now().to_rfc3339(),
            error,
            result_count.unwrap_or(0)
        ],
    )?;
    Ok(())
}

// ── FRB 门面（tick；Dart worker 在知识消化之后调用）──────────────────

/// 一次后台 tick 的结果（Dart worker 只记日志/刷新，不做业务分支）。
#[derive(Clone, Debug)]
pub enum PeriodReportTickResult {
    /// 未配置 AI provider：报告仍会生成，检测等待（静默重试）
    NoProvider,
    /// 无事可做（报告齐、无待检测，或检测退避未到期）
    Idle,
    /// 新建了报告（检测由同 tick 或后续 tick 接续）
    Generated { period: String, period_type: String },
    /// 完成一次检测：deviations 为最终（去重后）条数
    Detected {
        report_id: String,
        deviations: usize,
    },
    /// 检测失败（已记账，退避后重试）
    DetectFailed { report_id: String, error: String },
}

/// 检测退避：进程内状态（不落库——重启后从头重试一次没有伤害）。
struct DetectBackoff {
    next_at_ms: std::sync::atomic::AtomicI64,
    failures: std::sync::atomic::AtomicI64,
}

fn detect_backoff() -> &'static DetectBackoff {
    use std::sync::OnceLock;
    use std::sync::atomic::AtomicI64;
    static BACKOFF: OnceLock<DetectBackoff> = OnceLock::new();
    BACKOFF.get_or_init(|| DetectBackoff {
        next_at_ms: AtomicI64::new(0),
        failures: AtomicI64::new(0),
    })
}

impl DetectBackoff {
    fn ready(&self, now_ms: i64) -> bool {
        now_ms >= self.next_at_ms.load(std::sync::atomic::Ordering::Relaxed)
    }
    fn reset(&self) {
        use std::sync::atomic::Ordering;
        self.failures.store(0, Ordering::Relaxed);
        self.next_at_ms.store(0, Ordering::Relaxed);
    }
    /// 指数退避：5min → 10 → 20 → 40 → … → 60min 封顶。
    fn bump(&self, now_ms: i64) {
        use std::sync::atomic::Ordering;
        let failures = self.failures.fetch_add(1, Ordering::Relaxed);
        let delay = (DETECT_BACKOFF_BASE_MS << failures.min(7)).min(DETECT_BACKOFF_MAX_MS);
        self.next_at_ms.store(now_ms + delay, Ordering::Relaxed);
    }
}

/// 后台 worker 调用。错误语义：
/// - 存储/guard 失败：上抛（与知识消化 tick 相同，调用方记日志）。
/// - AI/校验失败：不抛——返回 `DetectFailed`，报告仍可读，退避后重试。
pub fn tick_periodic_reports() -> Result<PeriodReportTickResult> {
    let config = crate::config::AppConfig::load()?;
    // 同分析 worker：后台路径不做启动写入，库待升级时本轮跳过。
    let Some(store) = crate::storage::Store::open_background(&config.database_path)? else {
        return Ok(PeriodReportTickResult::Idle);
    };
    let Some(_sync_guard) = crate::sync::runtime::background_guard(&config, &store)? else {
        return Ok(PeriodReportTickResult::Idle);
    };
    if !store.background_processing_allowed()? {
        return Ok(PeriodReportTickResult::Idle);
    }

    // 1) 生成：每个 tick 最多补一份（最新完整周期，幂等）。
    for (period_type, period) in due_reports(Local::now()) {
        let Some(report) = build_period_report(&store, period_type, period)? else {
            continue; // 已存在（幂等）或空窗跳过。
        };
        let json = serde_json::to_string(&report).context("报告序列化")?;
        let id = Uuid::new_v4().to_string();
        if store.insert_deviation_report(
            &id,
            NaiveDate::parse_from_str(&report.period, "%F")?,
            period_type,
            &json,
            Utc::now(),
        )? {
            return Ok(PeriodReportTickResult::Generated {
                period: report.period.clone(),
                period_type: period_type.to_string(),
            });
        }
        // insert 返回 false：并发 tick 抢先建好；下个 tick 检测接续。
    }

    // 2) 检测：最旧的一份「尚未检测」；退避未到期或无 provider 则等待。
    let now_ms = Utc::now().timestamp_millis();
    if !detect_backoff().ready(now_ms) {
        return Ok(PeriodReportTickResult::Idle);
    }
    let Some((report_id, _)) = store.pending_deviation_report_ids()?.into_iter().next() else {
        return Ok(PeriodReportTickResult::Idle);
    };
    let Some((provider, _model)) = crate::wiki::digest_provider(&store)? else {
        return Ok(PeriodReportTickResult::NoProvider);
    };
    let run_id = start_run(&store, "period-report-detect", &report_id)?;
    match detect_deviation(&store, &report_id, provider.as_ref()) {
        Ok(count) => {
            finish_run(&store, &run_id, "succeeded", None, Some(count as i64))?;
            detect_backoff().reset();
            Ok(PeriodReportTickResult::Detected {
                report_id,
                deviations: count,
            })
        }
        Err(error) => {
            let message = format!("{error:#}");
            finish_run(&store, &run_id, "failed", Some(message.as_str()), None)?;
            detect_backoff().bump(now_ms);
            Ok(PeriodReportTickResult::DetectFailed {
                report_id,
                error: message,
            })
        }
    }
}
