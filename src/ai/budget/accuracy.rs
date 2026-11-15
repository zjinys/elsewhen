//! 估算口径的偏差测量：发送前的估算 vs 上游返回的真实 prompt_tokens。
//!
//! 每次经运行通道发出的请求都留了两个数：`request_budget` 事件里的
//! `input_tokens_after`（发送前对整份请求的估算），和 `chat_run_requests`
//! 里上游报告的 `prompt_tokens`。按请求配对即可得到「真实 ÷ 估算」的分布。
//! 预算事件同时记了当时的估算口径（`counting`），不同口径分开统计——
//! 把换口径前后的请求混在一起会得到没有意义的双峰分布。
//!
//! 只读、只取数值、模型名和口径标签，不读消息或事件正文里的任何文本。
//! 测量本身不改变估算口径；要不要校准由人看过分布后决定（估算宁高不低的
//! 红线不在这里动）。

use anyhow::Result;
use rusqlite::Connection;

/// 一个（估算口径，模型）组合的偏差分布。比值 = 真实 prompt_tokens ÷ 发送前
/// 估算：小于 1 表示估算偏高（浪费可用窗口），大于 1 表示估算偏低（有超限
/// 风险）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ModelAccuracy {
    /// 预算事件记录的估算口径标签。
    pub counting: String,
    pub model: String,
    pub requests: usize,
    pub p05: f64,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
    /// 估算偏低的请求数（比值大于 1）。
    pub underestimated: usize,
    /// 这些请求的估算值中位数：比值与请求规模有关时便于判读。
    pub median_estimate: usize,
}

/// 按估算口径和模型统计。只统计上游确实报了 usage、且两个数都为正的已完成
/// 请求；一次请求有多次预算装配（超限后缩窗重试）时取最后一次，即实际发出
/// 的那份。
pub(crate) fn accuracy_by_model(connection: &Connection) -> Result<Vec<ModelAccuracy>> {
    let mut statement = connection.prepare(
        "SELECT COALESCE(r.model,''), r.prompt_tokens,
                json_extract(b.payload_json,'$.report.input_tokens_after'),
                COALESCE(json_extract(b.payload_json,'$.report.counting'),'')
           FROM chat_run_requests r
           JOIN chat_run_events b ON b.run_id=r.run_id AND b.sequence=(
                SELECT MAX(e.sequence) FROM chat_run_events e
                 WHERE e.run_id=r.run_id AND e.kind='request_budget'
                   AND json_extract(e.payload_json,'$.request_id')=r.id)
          WHERE r.status='completed' AND r.usage_source='provider' AND r.prompt_tokens>0",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut groups = std::collections::BTreeMap::<(String, String), Vec<(f64, usize)>>::new();
    for (model, real, estimate, counting) in rows {
        let Some(estimate) = estimate.filter(|estimate| *estimate > 0) else {
            continue;
        };
        groups
            .entry((counting, model))
            .or_default()
            .push((real as f64 / estimate as f64, estimate as usize));
    }
    Ok(groups
        .into_iter()
        .map(|((counting, model), mut pairs)| {
            pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
            let ratios: Vec<f64> = pairs.iter().map(|pair| pair.0).collect();
            let mut estimates: Vec<usize> = pairs.iter().map(|pair| pair.1).collect();
            estimates.sort_unstable();
            ModelAccuracy {
                counting,
                model,
                requests: ratios.len(),
                p05: percentile(&ratios, 0.05),
                p50: percentile(&ratios, 0.50),
                p95: percentile(&ratios, 0.95),
                max: ratios[ratios.len() - 1],
                underestimated: ratios.iter().filter(|ratio| **ratio > 1.0).count(),
                median_estimate: estimates[estimates.len() / 2],
            }
        })
        .collect())
}

/// 已排序序列的分位数（最近秩）。
fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    let rank = (quantile * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Store;

    fn request(store: &Store, id: &str, model: Option<&str>, real: i64, source: &str) {
        store
            .connection
            .execute(
                "INSERT INTO chat_run_requests(id,run_id,provider_name,model,status,prompt_tokens,usage_source,started_at)
                 VALUES(?1,'run','p',?2,'completed',?3,?4,'t')",
                rusqlite::params![id, model, real, source],
            )
            .unwrap();
    }

    fn budget(store: &Store, sequence: i64, id: &str, estimate: i64, counting: &str) {
        store
            .connection
            .execute(
                "INSERT INTO chat_run_events(run_id,sequence,kind,payload_json,created_at)
                 VALUES('run',?1,'request_budget',?2,'t')",
                rusqlite::params![
                    sequence,
                    serde_json::json!({"request_id":id,
                        "report":{"input_tokens_after":estimate,"counting":counting}})
                    .to_string()
                ],
            )
            .unwrap();
    }

    #[test]
    fn accuracy_pairs_each_request_with_its_final_estimate_per_estimator_and_model() {
        let path = std::env::temp_dir().join(format!("accuracy-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path).unwrap();
        let conversation = store.create_conversation(Some("t"), None).unwrap();
        let message = store
            .send_message(&conversation, "user", "x", None)
            .unwrap();
        store
            .connection
            .execute(
                "INSERT INTO chat_runs(id,request_key,conversation_id,input_message_id,input_hash,status,executor_id,lease_until,created_at,updated_at)
                 VALUES('run','k',?1,?2,'h','completed','e','t','t','t')",
                rusqlite::params![conversation, message],
            )
            .unwrap();
        // 模型甲、现行口径：四次请求，估算都是 1000，真实 400/500/600/1200。
        for (index, real) in [400, 500, 600, 1200].into_iter().enumerate() {
            let id = format!("a{index}");
            request(&store, &id, Some("model-a"), real, "provider");
            budget(&store, index as i64 + 1, &id, 1000, "new");
        }
        // 同一模型、旧口径的请求单独成组，不与现行口径混算。
        request(&store, "a-old", Some("model-a"), 250, "provider");
        budget(&store, 5, "a-old", 1000, "old");
        // 缩窗重试：同一请求两次装配，取后一次（实际发出的那份）。
        request(&store, "b0", Some("model-b"), 900, "provider");
        budget(&store, 10, "b0", 4000, "new");
        budget(&store, 11, "b0", 1800, "new");
        // 不计入：没有预算事件、上游没报 usage、估算为 0。
        request(&store, "c0", Some("model-b"), 700, "provider");
        request(&store, "c1", Some("model-b"), 700, "unknown");
        budget(&store, 12, "c1", 1000, "new");
        request(&store, "c2", Some("model-b"), 700, "provider");
        budget(&store, 13, "c2", 0, "new");

        let row =
            |counting: &str, model: &str, ratios: [f64; 4], n, under, estimate| ModelAccuracy {
                counting: counting.into(),
                model: model.into(),
                requests: n,
                p05: ratios[0],
                p50: ratios[1],
                p95: ratios[2],
                max: ratios[3],
                underestimated: under,
                median_estimate: estimate,
            };
        assert_eq!(
            accuracy_by_model(&store.connection).unwrap(),
            vec![
                row("new", "model-a", [0.4, 0.5, 1.2, 1.2], 4, 1, 1000),
                row("new", "model-b", [0.5; 4], 1, 0, 1800),
                row("old", "model-a", [0.25; 4], 1, 0, 1000),
            ]
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    /// 在一份数据库上量估算偏差。只读打开，只输出模型名、口径标签和数值统计。
    /// 不设 ELSEWHEN_ACCURACY_DB 时用应用配置解析出的当前数据库。
    ///   cargo test --lib estimate_accuracy_report -- --ignored --nocapture
    #[test]
    #[ignore = "诊断：只读打开当前（或 ELSEWHEN_ACCURACY_DB 指定的）数据库，只输出数值统计"]
    fn estimate_accuracy_report() {
        let path = match std::env::var_os("ELSEWHEN_ACCURACY_DB") {
            Some(path) => std::path::PathBuf::from(path),
            None => crate::config::AppConfig::load().unwrap().database_path,
        };
        let connection =
            Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let (requests, earliest, latest): (i64, Option<String>, Option<String>) = connection
            .query_row(
                "SELECT COUNT(*), MIN(started_at), MAX(started_at) FROM chat_run_requests
                 WHERE status='completed' AND usage_source='provider' AND prompt_tokens>0",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        println!("上游报了 usage 的已完成请求：{requests} 条，{earliest:?} 至 {latest:?}");
        println!("真实 prompt_tokens ÷ 发送前估算（<1 估算偏高，>1 估算偏低）");
        let report = accuracy_by_model(&connection).unwrap();
        let mut counting = None;
        for row in &report {
            if counting != Some(&row.counting) {
                counting = Some(&row.counting);
                println!("\n估算口径：{}", row.counting);
                println!(
                    "{:<34} {:>5} {:>6} {:>6} {:>6} {:>6} {:>6} {:>9}",
                    "model", "n", "p05", "p50", "p95", "max", "低估", "估算中位"
                );
            }
            println!(
                "{:<34} {:>5} {:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6} {:>9}",
                row.model,
                row.requests,
                row.p05,
                row.p50,
                row.p95,
                row.max,
                row.underestimated,
                row.median_estimate
            );
        }
        if report.is_empty() {
            println!("没有可配对的请求（需要 request_budget 事件与上游 usage 同时存在）");
        }
    }
}
