use super::*;
use crate::ai::provider::{AiProvider, AiReply};
use std::time::{SystemTime, UNIX_EPOCH};

// 公共测试辅助（原 mod tests 内嵌段平移，pub(super) 供分组子文件复用）。
struct ScriptedProvider(Vec<Result<String>>);
impl AiProvider for ScriptedProvider {
    fn generate_reply_with_tools(
        &self,
        _messages: Vec<crate::ai::memory::ContextMessage>,
        _tools: Option<&[crate::ai::tool::ToolSpec]>,
    ) -> Result<AiReply> {
        self.0
            .iter()
            .find_map(|r| r.as_ref().ok().cloned())
            .map(AiReply::text)
            .with_context(|| "脚本耗尽".to_string())
    }
}

struct DataDirGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl DataDirGuard {
    fn set(dir: &std::path::Path) -> Self {
        let lock = crate::config::test_env_guard();
        crate::config::unpin();
        crate::config::AppConfig::pin(Some(dir.to_str().unwrap())).unwrap();
        Self { _lock: lock }
    }
}

impl Drop for DataDirGuard {
    fn drop(&mut self) {
        crate::config::unpin();
    }
}

pub(super) fn open_pinned_store() -> (DataDirGuard, Store) {
    let dir = std::env::temp_dir().join(format!(
        "elsewhen-review-report-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let guard = DataDirGuard::set(&dir);
    let config = crate::config::AppConfig::load().unwrap();
    (guard, Store::open(&config.database_path).unwrap())
}

/// 本地某日某时（归属唯一、避开午夜 DST 转换窗）。
fn local_at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
    let naive = NaiveDate::from_ymd_opt(y, m, d)
        .unwrap()
        .and_hms_opt(h, min, 0)
        .unwrap();
    Local
        .from_local_datetime(&naive)
        .single()
        .expect("测试时刻无 DST 歧义")
}

pub(super) fn analysis_json(activities: &[&str], recordable: bool) -> String {
    let kind = if recordable { "event" } else { "chitchat" };
    format!(
        r#"{{"schema_version":"{}","event_type":"运动",
         "confidence":0.9,"summary":"s","clarifications":[],
         "people":[],"projects":[],"activities":{activities},
         "follow_ups":[],"recordable":{recordable},"kind":"{kind}"}}"#,
        super::super::EVENT_ANALYSIS_VERSION,
        activities = serde_json::to_string(activities).unwrap(),
    )
}

pub(super) fn seed_event(
    store: &Store,
    text: &str,
    recorded_at: chrono::DateTime<chrono::Utc>,
    result_json: &str,
) -> String {
    let id = store
        .insert_event(crate::event::NewEvent {
            raw_text: text,
            occurred_at: recorded_at,
            recorded_at,
            source: "capture",
        })
        .unwrap();
    store
        .connection
        .execute(
            "INSERT INTO event_analyses (id, event_id, prompt_version, result_json, created_at)
             VALUES (?1, ?2, 'p', ?3, ?4)",
            rusqlite::params![
                format!("analysis-{id}"),
                id,
                result_json,
                recorded_at.to_rfc3339()
            ],
        )
        .unwrap();
    id
}

pub(super) fn noon(date: NaiveDate) -> chrono::DateTime<chrono::Utc> {
    use chrono::TimeZone;
    let naive = date.and_hms_opt(12, 0, 0).unwrap();
    Local
        .from_local_datetime(&naive)
        .single()
        .expect("本地正午无 DST 歧义")
        .with_timezone(&chrono::Utc)
}

pub(super) fn seed_goal(store: &Store, content: &str) -> String {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    store
        .connection
        .execute(
            "INSERT INTO goals (id, content, phase, status, created_at, updated_at)
             VALUES (?1, ?2, 'near', 'active', ?3, ?3)",
            rusqlite::params![id, content, now],
        )
        .unwrap();
    id
}

mod review_report_part1;
mod review_report_part2;
mod review_report_part3;
mod review_report_part4;
