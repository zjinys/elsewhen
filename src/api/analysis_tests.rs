use super::*;

use super::*;
use crate::ai::{provider::AiReply, tool::ToolSpec};
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_database() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "elsewhen-api-test-{}.db",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

struct StubProvider(Option<&'static str>);

impl AiProvider for StubProvider {
    fn generate_reply_with_tools(
        &self,
        _: Vec<ContextMessage>,
        tools: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        assert!(tools.is_none());
        match self.0 {
            Some(content) => Ok(AiReply::text(content)),
            None => anyhow::bail!("simulated timeout"),
        }
    }
}

#[test]
fn queue_persists_multiple_results_without_reprocessing() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store.insert_event(NewEvent::now("first")).unwrap();
    store.insert_event(NewEvent::now("second")).unwrap();
    let provider = StubProvider(Some(
        r#"{"schema_version":"event-analysis-v1","event_type":"note","confidence":0.8,"summary":"test","clarifications":[],"people":[],"projects":[],"follow_ups":[]}"#,
    ));
    assert_eq!(process_analysis_queue(&store, &provider).unwrap(), 2);
    assert_eq!(store.analysis_job_stats().unwrap().succeeded, 2);
    assert_eq!(store.list_analyses().unwrap().len(), 2);
    assert_eq!(process_analysis_queue(&store, &provider).unwrap(), 0);
    assert_eq!(store.list_analyses().unwrap().len(), 2);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn invalid_json_and_provider_failure_preserve_records_for_retry() {
    for reply in [
        Some("invalid"),
        Some("[]"),
        Some(
            r#"{"schema_version":"event-analysis-v1","event_type":"note","confidence":2,"summary":"bad","clarifications":[],"people":[],"projects":[],"follow_ups":[]}"#,
        ),
        Some(
            r#"{"schema_version":"event-analysis-v1","event_type":"note","confidence":0.5,"summary":"extra","clarifications":[],"people":[],"projects":[],"follow_ups":[],"unexpected":true}"#,
        ),
        None,
    ] {
        let path = temporary_database();
        let store = Store::open(&path).unwrap();
        store.insert_event(NewEvent::now("original")).unwrap();
        assert_eq!(
            process_analysis_queue(&store, &StubProvider(reply)).unwrap(),
            0
        );
        let stats = store.analysis_job_stats().unwrap();
        assert_eq!(stats.retry, 1);
        assert_eq!(stats.running, 0);
        assert!(store.list_analyses().unwrap().is_empty());
        assert_eq!(store.list_events().unwrap()[0].raw_text, "original");
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn analysis_schema_normalizes_repeated_and_blank_strings() {
    let parsed = EventAnalysisV1::parse(
        r#"{"schema_version":"event-analysis-v1","event_type":" note ","confidence":0.5,"summary":" summary ","clarifications":[" ask ","","ask"],"people":[" Ada ","Ada"],"projects":[],"follow_ups":[]}"#,
    )
    .unwrap();
    assert_eq!(parsed.event_type, "note");
    assert_eq!(parsed.summary, "summary");
    assert_eq!(parsed.clarifications, ["ask"]);
    assert_eq!(parsed.people, ["Ada"]);
    // v1 输入归一到无后缀历史标识（prompt_version 列保真），不是现行 v3。
    assert_eq!(parsed.schema_version, PRE_V3_EVENT_ANALYSIS);
}
