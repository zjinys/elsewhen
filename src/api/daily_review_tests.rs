use super::*;

use super::*;
use crate::ai::provider::AiReply;
use crate::ai::tool::ToolSpec;
use crate::event::NewEvent;
use crate::storage::{ContentPolicy, RuleStatus, Store};
use std::time::{SystemTime, UNIX_EPOCH};

struct StubProvider(&'static str);
impl AiProvider for StubProvider {
    fn generate_reply_with_tools(
        &self,
        _: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        Ok(AiReply::text(self.0))
    }
}

fn temporary_database() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "elsewhen-daily-review-api-{}.db",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn daily_review_requires_per_item_sources_from_declared_set() {
    let valid = r#"{
      "schema_version":"daily-review-v1",
      "date":"2026-09-18",
      "accomplishments":[{"text":"完成分析闭环","source_event_ids":["event-1"]}],
      "ideas_decisions":[],
      "people_projects":[],
      "follow_ups":[{"text":"继续每日聚合","source_event_ids":["event-1"]}]
    }"#;
    let parsed = DailyReviewV1::parse(valid, "2026-09-18", &["event-1".to_string()]).unwrap();
    assert_eq!(parsed.accomplishments[0].text, "完成分析闭环");

    let missing_source = valid.replace(
        r#""source_event_ids":["event-1"]"#,
        r#""source_event_ids":[]"#,
    );
    assert!(
        DailyReviewV1::parse(&missing_source, "2026-09-18", &["event-1".to_string()],).is_err()
    );

    let unknown_source = valid.replace("event-1", "event-other");
    assert!(
        DailyReviewV1::parse(&unknown_source, "2026-09-18", &["event-1".to_string()],).is_err()
    );
}

#[test]
fn generate_daily_review_with_stub_provider_appends_valid_version() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_id = store
        .insert_event(NewEvent::now("完成 Phase 3 数据契约"))
        .unwrap();
    let reply = format!(
        r#"{{"schema_version":"daily-review-v1","date":"{}","accomplishments":[{{"text":"完成数据契约","source_event_ids":["{}"]}}],"ideas_decisions":[],"people_projects":[],"follow_ups":[]}}"#,
        chrono::Local::now().date_naive(),
        event_id
    );
    let result = generate_daily_review_with_provider(
        &store,
        chrono::Local::now().date_naive(),
        &StubProvider(Box::leak(reply.into_boxed_str())),
    )
    .unwrap();
    assert!(
        matches!(result, DailyReviewResult::Created { .. }),
        "应返回 Created，实际：{result:?}"
    );
    assert!(
        store
            .latest_daily_review(chrono::Local::now().date_naive())
            .unwrap()
            .is_some()
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn daily_review_keeps_only_actual_knowledge_citations() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event = store
        .insert_event(NewEvent::now("实践 SQLite 事务"))
        .unwrap();
    let page = crate::wiki::save_text_page(
        "SQLite 事务需要验证提交边界",
        Some("SQLite 事务"),
        &[],
        &store,
    )
    .unwrap();
    let reply=serde_json::json!({"schema_version":"daily-review-v1","date":chrono::Local::now().date_naive().to_string(),
        "accomplishments":[{"text":format!("完成 SQLite 实践 [[kb:{}]] [[kb:missing]]",page.slug),"source_event_ids":[event]}],
        "ideas_decisions":[],"people_projects":[],"follow_ups":[]}).to_string();
    let result = generate_daily_review_with_provider(
        &store,
        chrono::Local::now().date_naive(),
        &StubProvider(Box::leak(reply.into_boxed_str())),
    )
    .unwrap();
    let DailyReviewResult::Created { id } = result else {
        panic!("review not created")
    };
    let citations = crate::knowledge::latest_citations(&store, "daily_review", &id).unwrap();
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].page_slug, page.slug);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn event_analysis_v2_classifies_non_recordable_discussion() {
    let raw = r#"{"schema_version":"event-analysis-v2","recordable":false,"kind":"meta","event_type":"conversation","confidence":0.9,"summary":"用户在评价助手回复","clarifications":[],"people":[],"projects":[],"follow_ups":[]}"#;
    let parsed = EventAnalysisV1::parse(raw).unwrap();
    assert!(!parsed.recordable);
    assert_eq!(parsed.kind, "meta");
    assert_eq!(parsed.schema_version, PRE_V3_EVENT_ANALYSIS);
}

#[test]
fn event_analysis_v1_defaults_to_recordable_event() {
    let raw = r#"{"schema_version":"event-analysis-v1","event_type":"note","confidence":0.8,"summary":"旧结果","clarifications":[],"people":[],"projects":[],"follow_ups":[]}"#;
    let parsed = EventAnalysisV1::parse(raw).unwrap();
    assert!(parsed.recordable);
    assert_eq!(parsed.kind, "event");
    assert_eq!(parsed.schema_version, PRE_V3_EVENT_ANALYSIS);
}

#[test]
fn decision_support_context_contains_only_confirmed_reference_material() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .add_rule("先确认付款方再推进", RuleStatus::Active, None)
        .unwrap();
    let draft = crate::storage::WikiPageDraft {
        slug: "topic/payment-check".to_string(),
        kind: "topic".to_string(),
        title: "付款检查".to_string(),
        summary: "付款前确认责任人".to_string(),
        content_md: "付款前确认责任人".to_string(),
        tags: vec!["decision".to_string()],
        source_event_ids: vec![],
        status: "active".to_string(),
        reason: "test".to_string(),
        source_url: None,
    };
    store
        .upsert_wiki_page(&draft, ContentPolicy::Always)
        .unwrap();
    let (context, _) = decision_support_material(&store, "付款检查").unwrap();
    assert!(context.contains("先确认付款方再推进"));
    assert!(
        !context.contains("付款检查"),
        "无来源的页面不能当作分析证据"
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}
