use super::advance_leaves as advance;
use super::*;
use crate::ai::{
    provider::{
        AiReply, FailoverEntry, FailoverProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider,
    },
    tool::ToolSpec,
};
use crate::{
    event::NewEvent,
    storage::{DigestBatch, event_readings::ReadingQuote},
};
use std::sync::{Arc, Mutex};

struct Fixture {
    store: Store,
    batch: DigestBatch,
    task: ReadingTask,
    path: std::path::PathBuf,
}
impl Fixture {
    fn new(text: &str, english: bool) -> Self {
        let path =
            std::env::temp_dir().join(format!("elsewhen-event-reader-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path).unwrap();
        if english {
            store
                .save_prompt_language_preferences(Some("en"), "auto", None, 0)
                .unwrap();
        }
        let id = store.insert_event(NewEvent::now(text)).unwrap();
        store
            .set_event_recordability(&id, true, "synthetic")
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00Z'",
                [],
            )
            .unwrap();
        let batch = store.claim_digest_batch(1, 10000, None).unwrap().unwrap();
        let task = store.ensure_event_reading(&batch).unwrap();
        Self {
            store,
            batch,
            task,
            path,
        }
    }
    fn retry_now(&self) {
        self.store
            .connection
            .execute(
                "UPDATE event_reading_nodes SET available_at=0 WHERE status='pending'",
                [],
            )
            .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

struct Metered {
    meter: OpenAiCompatibleProvider,
    mode: &'static str,
    seen: Arc<Mutex<Vec<Vec<ContextMessage>>>>,
}
fn meter(
    window: u32,
    reserve: u32,
    mode: &'static str,
    seen: Arc<Mutex<Vec<Vec<ContextMessage>>>>,
) -> Metered {
    Metered {
        meter: OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
            model: "unknown-byte-budget-model".into(),
            context_window: Some(window),
            max_tokens: Some(reserve),
            base_url: "http://127.0.0.1:1/v1".into(),
            api_key: "synthetic".into(),
            ..Default::default()
        })
        .unwrap(),
        mode,
        seen,
    }
}
impl AiProvider for Metered {
    fn fits_request(&self, messages: &[ContextMessage]) -> Result<bool> {
        self.meter.fits_request(messages)
    }
    fn visit_request_budgets(
        &self,
        visitor: &mut dyn FnMut(&RequestFit<'_>) -> Result<()>,
    ) -> Result<()> {
        visitor(&|messages| self.fits_request(messages))
    }
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        assert!(self.fits_request(&messages)?);
        let input: serde_json::Value =
            serde_json::from_str(messages[1].content.split_once('\n').unwrap().1)?;
        self.seen.lock().unwrap().push(messages);
        if self.mode == "network" {
            anyhow::bail!("503 synthetic");
        }
        let start = input["start"].as_u64().unwrap() as usize;
        let text = input["text"].as_str().unwrap();
        let quote: String = text.chars().take(30).collect();
        let mut output = ReadingOutput {
            summary: format!("generated: {}", text.chars().take(1400).collect::<String>()),
            quotes: vec![ReadingQuote {
                start,
                end: start + quote.chars().count(),
                text: quote,
            }],
        };
        if self.mode == "forged" {
            output.quotes[0].text = "not the original".into();
        }
        if self.mode == "oversize" {
            output.summary = "x".repeat(1700);
        }
        let content = if self.mode == "json" {
            "{} trailing".into()
        } else if self.mode == "unknown_fields" {
            r#"{"summary":"s","quotes":[],"source_event_ids":["fake"]}"#.into()
        } else {
            serde_json::to_string(&output)?
        };
        let mut reply = AiReply::text(content);
        reply.finish_reason = Some(
            if self.mode == "length" {
                "length"
            } else if self.mode == "unknown_finish" {
                "content_filter"
            } else {
                "stop"
            }
            .into(),
        );
        if self.mode == "missing_finish" {
            reply.finish_reason = None;
        }
        if self.mode == "tool" {
            reply
                .tool_calls
                .push(crate::ai::tool::ToolCall::new("fake", json!({})));
        }
        Ok(reply)
    }
}

#[test]
fn mixed_unicode_leaves_cover_every_scalar_once_across_ticks_and_restart() {
    let original = format!(
        "START 2026-11-09 count=17\n{}\nMIDDLE: not approved, 3 trials\n{}\nTAIL: deadline 2026-12-01, zero purchases",
        "中文🙂 English `code\\n` \"quote\" <<input>>\n".repeat(180),
        "否定 不执行 42 units\n".repeat(180)
    );
    let f = Fixture::new(&original, false);
    let seen = Arc::new(Mutex::new(vec![]));
    let provider = meter(6200, 1200, "ok", seen.clone());
    let mut ticks = 0;
    loop {
        let before = seen.lock().unwrap().len();
        let reopened = Store::open_background(&f.path).unwrap().unwrap();
        let tick = advance(&reopened, &provider, &f.task.id).unwrap();
        assert!(seen.lock().unwrap().len() - before <= 1);
        ticks += 1;
        match tick {
            ReadingTick::Advanced { .. } => {}
            ReadingTick::LeavesComplete => break,
            other => panic!("{other:?}"),
        }
        assert!(ticks < 100);
    }
    assert!(ticks > 3);
    let nodes = f.store.event_reading_nodes(&f.task.id).unwrap();
    let mut cursor = 0;
    let mut reconstructed = String::new();
    for node in &nodes {
        assert_eq!(node.start, cursor);
        assert!(node.output.is_some());
        cursor = node.end;
    }
    for messages in seen.lock().unwrap().iter() {
        let value: serde_json::Value =
            serde_json::from_str(messages[1].content.split_once('\n').unwrap().1).unwrap();
        reconstructed.push_str(value["text"].as_str().unwrap());
    }
    assert_eq!(cursor, original.chars().count());
    assert_eq!(reconstructed, original);
    assert!(reconstructed.contains("START"));
    assert!(reconstructed.contains("MIDDLE"));
    assert!(reconstructed.contains("TAIL"));
    let before = seen.lock().unwrap().len();
    assert_eq!(
        advance(&f.store, &provider, &f.task.id).unwrap(),
        ReadingTick::LeavesComplete
    );
    assert_eq!(seen.lock().unwrap().len(), before);
    assert_eq!(f.store.digest_job_stats().unwrap().running, 1);
    assert_eq!(f.store.list_events().unwrap()[0].raw_text, original);
}

#[test]
fn failover_uses_range_that_fits_smaller_candidate_and_keeps_pinned_bundle() {
    let f = Fixture::new(&"首中尾🙂否定 date=2026-11-09\n".repeat(600), true);
    let large_seen = Arc::new(Mutex::new(vec![]));
    let small_seen = Arc::new(Mutex::new(vec![]));
    let unique = uuid::Uuid::new_v4().to_string();
    let chain = FailoverProvider::new(vec![
        FailoverEntry::new(
            format!("large-{unique}"),
            "large",
            Box::new(meter(18000, 1024, "network", large_seen.clone())),
        ),
        FailoverEntry::new(
            format!("small-{unique}"),
            "small",
            Box::new(meter(5200, 1500, "ok", small_seen.clone())),
        ),
    ]);
    assert!(matches!(
        advance(&f.store, &chain, &f.task.id).unwrap(),
        ReadingTick::Advanced { .. }
    ));
    assert_eq!(large_seen.lock().unwrap().len(), 1);
    assert_eq!(small_seen.lock().unwrap().len(), 1);
    assert_eq!(
        large_seen.lock().unwrap()[0][1].content,
        small_seen.lock().unwrap()[0][1].content
    );
    let original_bundle = f.task.bundle_id.clone();
    let preferences = f.store.prompt_language_preferences().unwrap();
    f.store
        .save_prompt_language_preferences(Some("zh-Hans"), "auto", None, preferences.generation)
        .unwrap();
    assert!(matches!(
        advance(&f.store, &chain, &f.task.id).unwrap(),
        ReadingTick::Advanced { .. }
    ));
    assert_eq!(
        f.store.event_reading_task(&f.task.id).unwrap().bundle_id,
        original_bundle
    );
    for messages in small_seen.lock().unwrap().iter() {
        assert!(
            messages[0]
                .content
                .contains("Read only the supplied event range")
        );
    }
}

#[test]
fn fixed_instructions_and_unknown_budgets_do_not_send_or_plan_empty_nodes() {
    let f = Fixture::new(&"中文🙂".repeat(1000), false);
    let seen = Arc::new(Mutex::new(vec![]));
    let provider = meter(2000, 1024, "ok", seen.clone());
    let error = advance(&f.store, &provider, &f.task.id).unwrap_err();
    assert!(format!("{error:#}").contains("fixed_instructions_exceed_budget"));
    assert!(f.store.event_reading_nodes(&f.task.id).unwrap().is_empty());
    assert!(seen.lock().unwrap().is_empty());
    struct Unknown;
    impl AiProvider for Unknown {
        fn generate_reply_with_tools(
            &self,
            _: Vec<ContextMessage>,
            _: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            panic!("unknown budget must not send")
        }
    }
    assert!(
        advance(&f.store, &Unknown, &f.task.id)
            .unwrap_err()
            .to_string()
            .contains("provider_budget_unavailable")
    );
    let empty = FailoverProvider::new(vec![]);
    assert!(
        advance(&f.store, &empty, &f.task.id)
            .unwrap_err()
            .to_string()
            .contains("provider_budget_unavailable")
    );
    let chain = FailoverProvider::new(vec![
        FailoverEntry::new(
            format!("invalid-{}", uuid::Uuid::new_v4()),
            "invalid",
            Box::new(meter(1000, 1500, "ok", seen.clone())),
        ),
        FailoverEntry::new(
            format!("unknown-{}", uuid::Uuid::new_v4()),
            "unknown",
            Box::new(Unknown),
        ),
        FailoverEntry::new(
            format!("known-{}", uuid::Uuid::new_v4()),
            "known",
            Box::new(meter(8000, 1024, "ok", seen.clone())),
        ),
    ]);
    assert!(matches!(
        advance(&f.store, &chain, &f.task.id).unwrap(),
        ReadingTick::Advanced { .. }
    ));
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[test]
fn malformed_truncated_forged_and_oversized_outputs_never_enter_completed_cache() {
    for mode in [
        "json",
        "length",
        "forged",
        "oversize",
        "unknown_fields",
        "unknown_finish",
        "missing_finish",
        "tool",
    ] {
        let f = Fixture::new(&"长引文🙂否定42\"code\"\n".repeat(300), false);
        let seen = Arc::new(Mutex::new(vec![]));
        assert!(matches!(
            advance(&f.store, &meter(8000, 1024, mode, seen.clone()), &f.task.id).unwrap(),
            ReadingTick::Failed { .. }
        ));
        assert_eq!(seen.lock().unwrap().len(), 1);
        let nodes = f.store.event_reading_nodes(&f.task.id).unwrap();
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].output.is_none());
        assert_eq!(
            advance(&f.store, &meter(8000, 1024, "ok", seen.clone()), &f.task.id).unwrap(),
            ReadingTick::Waiting
        );
        assert_eq!(seen.lock().unwrap().len(), 1);
        f.retry_now();
        assert!(matches!(
            advance(&f.store, &meter(8000, 1024, "ok", seen.clone()), &f.task.id).unwrap(),
            ReadingTick::Advanced { .. }
        ));
        assert_eq!(
            f.store.event_reading_nodes(&f.task.id).unwrap()[0].id,
            nodes[0].id
        );
        assert_eq!(f.store.digest_job_stats().unwrap().running, 1);
    }
}

#[test]
fn a_previously_pinned_range_is_never_relabelled_for_a_smaller_window() {
    let f = Fixture::new(&"事实🙂 date 17\n".repeat(500), true);
    let failed = Arc::new(Mutex::new(vec![]));
    assert!(matches!(
        advance(&f.store, &meter(18000, 1024, "network", failed), &f.task.id).unwrap(),
        ReadingTick::Failed { .. }
    ));
    let before = f.store.event_reading_nodes(&f.task.id).unwrap()[0].clone();
    f.retry_now();
    let seen = Arc::new(Mutex::new(vec![]));
    assert!(matches!(
        advance(&f.store, &meter(4500, 1024, "ok", seen.clone()), &f.task.id).unwrap(),
        ReadingTick::Failed { .. }
    ));
    assert!(seen.lock().unwrap().is_empty());
    let after = f.store.event_reading_nodes(&f.task.id).unwrap()[0].clone();
    assert_eq!(before.id, after.id);
    assert_eq!(before.end, after.end);
    assert!(after.output.is_none());
}

#[test]
fn in_flight_reclassification_writes_without_network_lock_and_fences_result() {
    let f = Fixture::new(&"否定🙂important\n".repeat(200), true);
    struct Reclassifies<'a> {
        inner: Metered,
        fixture: &'a Fixture,
    }
    impl AiProvider for Reclassifies<'_> {
        fn visit_request_budgets(
            &self,
            visitor: &mut dyn FnMut(&RequestFit<'_>) -> Result<()>,
        ) -> Result<()> {
            self.inner.visit_request_budgets(visitor)
        }
        fn fits_request(&self, messages: &[ContextMessage]) -> Result<bool> {
            self.inner.fits_request(messages)
        }
        fn generate_reply_with_tools(
            &self,
            messages: Vec<ContextMessage>,
            tools: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            assert!(self.fixture.store.connection.is_autocommit());
            let another = Store::open_background(&self.fixture.path)?.unwrap();
            another.set_event_recordability(
                &self.fixture.task.event_id,
                false,
                "revoked during synthetic network call",
            )?;
            another.set_event_recordability(&self.fixture.task.event_id, true, "reclassified")?;
            self.inner.generate_reply_with_tools(messages, tools)
        }
    }
    let provider = Reclassifies {
        inner: meter(8000, 1024, "ok", Arc::new(Mutex::new(vec![]))),
        fixture: &f,
    };
    assert!(advance(&f.store, &provider, &f.task.id).is_err());
    assert_eq!(
        f.store.event_reading_task(&f.task.id).unwrap().status,
        "cancelled"
    );
    assert!(
        f.store.event_reading_nodes(&f.task.id).unwrap()[0]
            .output
            .is_none()
    );
    assert_eq!(f.store.digest_job_stats().unwrap().running, 1);
}

#[test]
fn old_digest_bundle_uses_frozen_reading_contract_in_its_original_locale() {
    let f = Fixture::new(&"Original 17 units not approved🙂\n".repeat(100), true);
    // Model a pre-v10 immutable bundle: it has the original English resources
    // and policies, but its task-processing pack predates event reading.
    let body = f
        .store
        .prompt_from_bundle(&f.task.bundle_id, "task.processing.fixed_contract")
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&body).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .retain(|key, _| !key.starts_with("event_reading_"));
    let body = value.to_string();
    let revision=f.store.connection.query_row("SELECT revision_id FROM prompt_bundle_entries WHERE bundle_id=?1 AND prompt_key='task.processing.fixed_contract'",[&f.task.bundle_id],|r|r.get::<_,String>(0)).unwrap();
    f.store.connection.execute("INSERT INTO prompt_revisions SELECT 'old-processing',prompt_key,locale,origin,?1,?2,contract_version,variables_version,'synthetic-pre-v10',parent_revision_id,base_default_revision_id,translation_source_revision_id,change_note,created_at FROM prompt_revisions WHERE id=?3",rusqlite::params![body,crate::storage::knowledge::content_hash(&body),revision]).unwrap();
    f.store.connection.execute("INSERT INTO prompt_bundles SELECT 'old-bundle',owner_kind,'old-owner',ui_locale,requested_instruction_locale,response_policy_json,knowledge_output_locale,content_language_policy_version,'prompts-v9',locale_policy_version,renderer_version,bundle_hash,created_at FROM prompt_bundles WHERE id=?1",[&f.task.bundle_id]).unwrap();
    f.store.connection.execute("INSERT INTO prompt_bundle_entries SELECT 'old-bundle',ordinal,prompt_key,requested_locale,resolved_locale,CASE WHEN prompt_key='task.processing.fixed_contract' THEN 'old-processing' ELSE revision_id END,fallback_reason FROM prompt_bundle_entries WHERE bundle_id=?1",[&f.task.bundle_id]).unwrap();
    f.store
        .connection
        .execute(
            "UPDATE knowledge_digest_jobs SET prompt_bundle_id='old-bundle'",
            [],
        )
        .unwrap();
    f.store
        .connection
        .execute(
            "UPDATE knowledge_digest_runs SET prompt_bundle_id='old-bundle'",
            [],
        )
        .unwrap();
    let task = f.store.ensure_event_reading(&f.batch).unwrap();
    let preferences = f.store.prompt_language_preferences().unwrap();
    f.store
        .save_prompt_language_preferences(Some("zh-Hans"), "auto", None, preferences.generation)
        .unwrap();
    let seen = Arc::new(Mutex::new(vec![]));
    assert!(matches!(
        advance(&f.store, &meter(8000, 1024, "ok", seen.clone()), &task.id).unwrap(),
        ReadingTick::Advanced { .. }
    ));
    assert!(
        seen.lock().unwrap()[0][0]
            .content
            .contains("Read only the supplied event range")
    );
}

#[test]
fn bilingual_event_resources_validate_placeholders_and_frozen_v1_compatibility() {
    for (current, frozen) in [
        (
            include_str!("../../../resources/prompts/task-processing.en.json"),
            include_str!("../../../resources/prompts/event-reading.v1.en.json"),
        ),
        (
            include_str!("../../../resources/prompts/task-processing.zh-Hans.json"),
            include_str!("../../../resources/prompts/event-reading.v1.zh-Hans.json"),
        ),
    ] {
        TextResources::parse(current).unwrap().validate().unwrap();
        TextResources::parse(frozen).unwrap().validate().unwrap();
        let current: serde_json::Value = serde_json::from_str(current).unwrap();
        let frozen: serde_json::Value = serde_json::from_str(frozen).unwrap();
        for (key, value) in frozen.as_object().unwrap() {
            assert_eq!(&current[key], value);
        }
    }
}

#[path = "final_tests.rs"]
mod final_tests;

#[test]
fn leaf_quote_coordinates_use_exact_text_in_its_range_and_canonical_repeated_locations() {
    let chars: Vec<char> = "prefix🙂 exact quotation tail duplicate duplicate"
        .chars()
        .collect();
    let node = ReadingNode {
        id: "node".into(),
        task_id: "task".into(),
        kind: "leaf".into(),
        level: 0,
        start: 7,
        end: 27,
        input_hash: "hash".into(),
        output: None,
    };
    let mut output = ReadingOutput {
        summary: "Generated".into(),
        quotes: vec![ReadingQuote {
            start: 0,
            end: 3,
            text: "exact quotation".into(),
        }],
    };
    locate_leaf_quotes(&mut output, &node, &chars).unwrap();
    assert_eq!(output.quotes[0].start, 8);
    assert_eq!(output.quotes[0].end, 23);
    output.quotes[0] = ReadingQuote {
        start: 0,
        end: 6,
        text: "prefix".into(),
    };
    assert!(locate_leaf_quotes(&mut output, &node, &chars).is_err());
    output.quotes[0] = ReadingQuote {
        start: 0,
        end: 9,
        text: "duplicate".into(),
    };
    let node = ReadingNode {
        start: 0,
        end: chars.len(),
        ..node
    };
    locate_leaf_quotes(&mut output, &node, &chars).unwrap();
    assert_eq!(output.quotes[0].start, 29);
    assert_eq!(output.quotes[0].end, 38);
    output.quotes[0] = ReadingQuote {
        start: 29,
        end: 38,
        text: "duplicate".into(),
    };
    let start = chars.iter().collect::<String>().find("duplicate").unwrap();
    let canonical = chars.iter().collect::<String>()[..start].chars().count();
    output.quotes[0].start = canonical;
    output.quotes[0].end = canonical + 9;
    locate_leaf_quotes(&mut output, &node, &chars).unwrap();
    assert_eq!(output.quotes[0].start, canonical);
}
