use super::*;
use crate::storage::{ContentPolicy, WikiPageDraft};
use crate::wiki::digest::finalize_event_reading;

struct TreeModel {
    meter: Metered,
    final_mode: &'static str,
    calls: std::cell::RefCell<Vec<&'static str>>,
}
impl TreeModel {
    fn new(mode: &'static str) -> Self {
        Self {
            meter: meter(6500, 1024, "ok", Arc::new(Mutex::new(vec![]))),
            final_mode: mode,
            calls: Default::default(),
        }
    }
}
impl AiProvider for TreeModel {
    fn fits_request(&self, m: &[ContextMessage]) -> Result<bool> {
        self.meter.fits_request(m)
    }
    fn visit_request_budgets(
        &self,
        v: &mut dyn FnMut(&RequestFit<'_>) -> Result<()>,
    ) -> Result<()> {
        self.meter.visit_request_budgets(v)
    }
    fn generate_reply_with_tools(
        &self,
        m: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        assert!(self.fits_request(&m)?);
        let user = &m[1].content;
        let mut reply = if user.starts_with("New events")
            || user.starts_with("待消化")
            || user.contains("[1] ")
        {
            self.calls.borrow_mut().push("final");
            assert!(user.contains("Generated summary after full-event reading"));
            assert!(user.contains("verified_original_quotes"));
            assert!(user.contains("covered_end"));
            let content:String=match self.final_mode {
                "json"=>"[{ malformed".into(),
                "source"=>r#"[{"kind":"topic","slug":"topic/long-event","title":"Long event","content":"bad","source_event_ids":["2"]}]"#.into(),
                "unknown"=>r#"[{"kind":"topic","slug":"topic/long-event","title":"Long event","content":"bad","source_event_ids":["1"],"sources":["fake"]}]"#.into(),
                "invalid_second"=>r#"[{"kind":"topic","slug":"topic/long-event","title":"Long event","content":"valid first","source_event_ids":["1"]},{"kind":"not-a-kind","slug":"evil/path","title":"bad","content":"bad","source_event_ids":["1"]}]"#.into(),
                _=>r#"[{"op":"update","kind":"topic","slug":"topic/long-event","title":"Long event","content":"START 17 units; MIDDLE not approved; TAIL 2026-12-01","summary":"Generated facts","source_event_ids":["1","1"]}]"#.into(),
            };
            AiReply::text(content)
        } else {
            let input: serde_json::Value = serde_json::from_str(user.split_once('\n').unwrap().1)?;
            let output = if let Some(text) = input["text"].as_str() {
                self.calls.borrow_mut().push("leaf");
                let start = input["start"].as_u64().unwrap() as usize;
                let text_quote: String = text.chars().take(30).collect();
                let facts = ["START", "MIDDLE", "TAIL"]
                    .iter()
                    .filter(|word| text.contains(**word))
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" ");
                ReadingOutput {
                    summary: if facts.is_empty() {
                        "Background context".into()
                    } else {
                        facts
                    },
                    quotes: vec![ReadingQuote {
                        start,
                        end: start + text_quote.chars().count(),
                        text: text_quote,
                    }],
                }
            } else {
                self.calls.borrow_mut().push("merge");
                let children = input["children"].as_array().unwrap();
                let readings = children
                    .iter()
                    .map(|child| {
                        serde_json::from_value::<ReadingOutput>(child["reading"].clone()).unwrap()
                    })
                    .collect::<Vec<_>>();
                ReadingOutput {
                    summary: readings
                        .iter()
                        .map(|r| r.summary.as_str())
                        .collect::<Vec<_>>()
                        .join("; "),
                    quotes: readings
                        .into_iter()
                        .flat_map(|r| r.quotes)
                        .take(8)
                        .collect(),
                }
            };
            AiReply::text(serde_json::to_string(&output)?)
        };
        if self.final_mode == "exact_uuid" && self.calls.borrow().last() == Some(&"final") {
            let user = &m[1].content;
            let marker = "\"event_id\":\"";
            let start = user.find(marker).unwrap() + marker.len();
            let id = user[start..].split('"').next().unwrap();
            reply.content = reply
                .content
                .replace("[\"1\",\"1\"]", &format!("[\"{id}\"]"));
        }
        reply.finish_reason = Some(
            if self.final_mode == "length" && self.calls.borrow().last() == Some(&"final") {
                "length"
            } else {
                "stop"
            }
            .into(),
        );
        Ok(reply)
    }
}

fn long_fixture() -> Fixture {
    Fixture::new(
        &format!(
            "START 17 units\n{}\nMIDDLE not approved\n{}\nTAIL 2026-12-01",
            "background filler words 1234\n".repeat(1500),
            "更多背景🙂 \"quoted code\"\n".repeat(1500)
        ),
        true,
    )
}
fn read_all(f: &Fixture, model: &TreeModel) {
    for _ in 0..100 {
        let before = model.calls.borrow().len();
        let store = Store::open_background(&f.path).unwrap().unwrap();
        let tick = crate::knowledge::event_reading::advance(&store, model, &f.task.id).unwrap();
        assert!(model.calls.borrow().len() - before <= 1);
        if tick == ReadingTick::Ready {
            return;
        }
        assert!(matches!(tick, ReadingTick::Advanced { .. }), "{tick:?}");
    }
    panic!("reading did not finish within bounded ticks");
}
fn page_draft(event: &str) -> WikiPageDraft {
    WikiPageDraft {
        slug: "topic/long-event".into(),
        kind: "topic".into(),
        title: "Long event".into(),
        summary: "".into(),
        content_md: "human original".into(),
        tags: vec![],
        source_event_ids: vec![event.into()],
        status: "active".into(),
        reason: "fixture".into(),
        source_url: None,
    }
}

#[test]
fn multilevel_merges_and_final_digest_keep_original_event_and_confirm_once() {
    let f = long_fixture();
    let model = TreeModel::new("ok");
    assert!(f.store.apply_digest_batch(&f.batch, &[]).is_err());
    read_all(&f, &model);
    let task = f.store.event_reading_task(&f.task.id).unwrap();
    assert_eq!(task.status, "ready");
    let root = f.store.event_reading_root(&task).unwrap();
    assert!(root.level >= 2);
    let output: ReadingOutput = serde_json::from_str(root.output.as_ref().unwrap()).unwrap();
    for word in ["START", "MIDDLE", "TAIL"] {
        assert!(output.summary.contains(word), "{}", output.summary);
    }
    assert!(model.calls.borrow().iter().all(|c| *c != "final"));
    assert!(f.store.get_wiki_page("topic/long-event").unwrap().is_none());
    let before = model.calls.borrow().len();
    assert!(
        finalize_event_reading(&f.store, &model, &f.batch, &task.id)
            .unwrap()
            .is_some()
    );
    assert_eq!(model.calls.borrow().len(), before + 1);
    assert_eq!(
        f.store.event_reading_task(&task.id).unwrap().status,
        "succeeded"
    );
    let page = f.store.get_wiki_page("topic/long-event").unwrap().unwrap();
    assert_eq!(page.source_event_ids, vec![f.task.event_id.clone()]);
    assert_eq!(page.evidence_count, 1);
    assert_eq!(f.store.digest_job_stats().unwrap().succeeded, 1);
    assert!(finalize_event_reading(&f.store, &model, &f.batch, &task.id).is_err());
    assert_eq!(
        f.store
            .get_wiki_page("topic/long-event")
            .unwrap()
            .unwrap()
            .evidence_count,
        1
    );
    assert_eq!(f.store.list_events().unwrap().len(), 1);
    assert_eq!(
        f.store.list_events().unwrap()[0].raw_text,
        f.batch.events[0].raw_text
    );
}

#[test]
fn invalid_final_results_leave_complete_root_and_retry_without_rereading() {
    for mode in ["json", "source", "unknown", "invalid_second", "length"] {
        let f = long_fixture();
        let model = TreeModel::new(mode);
        read_all(&f, &model);
        let ids = f
            .store
            .event_reading_nodes(&f.task.id)
            .unwrap()
            .iter()
            .map(|n| n.id.clone())
            .collect::<Vec<_>>();
        assert!(finalize_event_reading(&f.store, &model, &f.batch, &f.task.id).is_err());
        assert!(f.store.get_wiki_page("topic/long-event").unwrap().is_none());
        assert_eq!(
            f.store.event_reading_task(&f.task.id).unwrap().status,
            "ready"
        );
        assert_eq!(f.store.digest_job_stats().unwrap().running, 1);
        f.store
            .fail_digest_batch(&f.batch, "synthetic invalid proposal")
            .unwrap();
        f.retry_now();
        f.store
            .connection
            .execute(
                "UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00Z'",
                [],
            )
            .unwrap();
        let batch = f
            .store
            .claim_event_reading_batch(&f.store.event_reading_task(&f.task.id).unwrap(), None)
            .unwrap()
            .unwrap();
        assert_eq!(f.store.ensure_event_reading(&batch).unwrap().id, f.task.id);
        let good = TreeModel::new("ok");
        assert!(
            finalize_event_reading(&f.store, &good, &batch, &f.task.id)
                .unwrap()
                .is_some()
        );
        assert_eq!(*good.calls.borrow(), vec!["final"]);
        assert_eq!(
            ids,
            f.store
                .event_reading_nodes(&f.task.id)
                .unwrap()
                .iter()
                .map(|n| n.id.clone())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn final_transaction_failure_rolls_back_pages_log_and_both_acknowledgements() {
    let f = long_fixture();
    let model = TreeModel::new("ok");
    read_all(&f, &model);
    let logs: i64 = f
        .store
        .connection
        .query_row("SELECT COUNT(*) FROM wiki_log", [], |r| r.get(0))
        .unwrap();
    f.store.connection.execute_batch("CREATE TEMP TRIGGER fail_final_reading BEFORE UPDATE OF status ON event_reading_tasks WHEN NEW.status='succeeded' BEGIN SELECT RAISE(ABORT,'injected_final_disk_failure'); END;").unwrap();
    assert!(finalize_event_reading(&f.store, &model, &f.batch, &f.task.id).is_err());
    assert!(f.store.get_wiki_page("topic/long-event").unwrap().is_none());
    assert_eq!(f.store.digest_job_stats().unwrap().running, 1);
    assert_eq!(
        f.store.event_reading_task(&f.task.id).unwrap().status,
        "ready"
    );
    assert_eq!(
        logs,
        f.store
            .connection
            .query_row::<i64, _, _>("SELECT COUNT(*) FROM wiki_log", [], |r| r.get(0))
            .unwrap()
    );
    f.store
        .connection
        .execute_batch("DROP TRIGGER fail_final_reading;")
        .unwrap();
    finalize_event_reading(&f.store, &model, &f.batch, &f.task.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        f.store
            .get_wiki_page("topic/long-event")
            .unwrap()
            .unwrap()
            .evidence_count,
        1
    );
}

#[test]
fn final_digest_respects_human_body_and_missing_coverage_cannot_be_ready() {
    let f = long_fixture();
    let model = TreeModel::new("ok");
    let node = f
        .store
        .prepare_event_reading_node(&f.task.id, 0, 30, &[])
        .unwrap();
    assert!(f.store.mark_event_reading_ready(&f.task.id).is_err());
    assert!(finalize_event_reading(&f.store, &model, &f.batch, &f.task.id).is_err());
    assert!(model.calls.borrow().is_empty());
    read_all(&f, &model);
    assert!(
        f.store
            .event_reading_nodes(&f.task.id)
            .unwrap()
            .iter()
            .any(|n| n.id == node.id)
    );
    f.store
        .upsert_wiki_page(&page_draft(&f.task.event_id), ContentPolicy::Always)
        .unwrap();
    f.store
        .save_wiki_page_content("topic/long-event", "manual protected body", "manual", None)
        .unwrap();
    let outcome = finalize_event_reading(&f.store, &model, &f.batch, &f.task.id)
        .unwrap()
        .unwrap();
    assert_eq!(outcome.protected, vec!["topic/long-event"]);
    let page = f.store.get_wiki_page("topic/long-event").unwrap().unwrap();
    assert_eq!(page.content_md, "manual protected body");
    assert_eq!(page.evidence_count, 1);
}

#[test]
fn final_lease_timeout_and_dependency_tampering_reject_publication() {
    let f = long_fixture();
    let model = TreeModel::new("ok");
    read_all(&f, &model);
    let now = chrono::Utc::now().timestamp();
    let lease = f
        .store
        .claim_event_reading_final(&f.task.id, &f.batch, now)
        .unwrap()
        .unwrap();
    assert!(
        f.store
            .claim_event_reading_final(&f.task.id, &f.batch, now + 1)
            .unwrap()
            .is_none()
    );
    f.store
        .connection
        .execute("UPDATE event_reading_tasks SET lease_until=?1", [now - 1])
        .unwrap();
    assert!(
        f.store
            .apply_digest_batch_with_reading(
                &f.batch,
                &[page_draft(&f.task.event_id)],
                Some(&lease)
            )
            .is_err()
    );
    let second = f
        .store
        .claim_event_reading_final(&f.task.id, &f.batch, now)
        .unwrap()
        .unwrap();
    assert!(second.generation > lease.generation);
    assert!(
        f.store
            .apply_digest_batch_with_reading(&f.batch, &[], Some(&lease))
            .is_err()
    );
    // Simulate damaged on-disk dependency metadata without modifying any
    // immutable node result. Root proof must fail before publishing.
    f.store.connection.execute("DELETE FROM event_reading_dependencies WHERE rowid=(SELECT rowid FROM event_reading_dependencies LIMIT 1)",[]).unwrap();
    assert!(
        f.store
            .apply_digest_batch_with_reading(
                &f.batch,
                &[page_draft(&f.task.event_id)],
                Some(&second)
            )
            .is_err()
    );
    assert!(f.store.get_wiki_page("topic/long-event").unwrap().is_none());
    assert_eq!(f.store.digest_job_stats().unwrap().running, 1);
}

#[test]
fn merge_cannot_add_a_quote_that_was_never_supplied_by_its_children() {
    let f = Fixture::new("START🙂 MIDDLE TAIL", true);
    let now = chrono::Utc::now().timestamp();
    let first = f
        .store
        .prepare_event_reading_node(&f.task.id, 0, 5, &[])
        .unwrap();
    let last = f
        .store
        .prepare_event_reading_node(&f.task.id, 5, f.task.total_chars, &[])
        .unwrap();
    let output = ReadingOutput {
        summary: "Generated summary".into(),
        quotes: vec![],
    };
    for _ in 0..2 {
        let lease = f
            .store
            .claim_event_reading_node(&f.task.id, now)
            .unwrap()
            .unwrap();
        f.store
            .complete_event_reading_node(&lease, &output, now)
            .unwrap();
    }
    let parent = f
        .store
        .prepare_event_reading_node(&f.task.id, 0, f.task.total_chars, &[first.id, last.id])
        .unwrap();
    let lease = f
        .store
        .claim_event_reading_node(&f.task.id, now)
        .unwrap()
        .unwrap();
    let forged = ReadingOutput {
        summary: "Generated summary".into(),
        quotes: vec![ReadingQuote {
            start: 0,
            end: 5,
            text: "START".into(),
        }],
    };
    assert!(
        f.store
            .complete_event_reading_node(&lease, &forged, now)
            .is_err()
    );
    assert!(
        f.store
            .event_reading_nodes(&f.task.id)
            .unwrap()
            .iter()
            .find(|n| n.id == parent.id)
            .unwrap()
            .output
            .is_none()
    );
    f.store
        .complete_event_reading_node(&lease, &output, now)
        .unwrap();
    f.store.mark_event_reading_ready(&f.task.id).unwrap();
}

#[test]
fn final_network_call_holds_no_write_lock_and_reclassification_rejects_late_proposal() {
    let f = long_fixture();
    let model = TreeModel::new("ok");
    read_all(&f, &model);
    struct Reclassify<'a> {
        fixture: &'a Fixture,
        model: &'a TreeModel,
    }
    impl AiProvider for Reclassify<'_> {
        fn fits_request(&self, m: &[ContextMessage]) -> Result<bool> {
            self.model.fits_request(m)
        }
        fn visit_request_budgets(
            &self,
            v: &mut dyn FnMut(&RequestFit<'_>) -> Result<()>,
        ) -> Result<()> {
            self.model.visit_request_budgets(v)
        }
        fn generate_reply_with_tools(
            &self,
            m: Vec<ContextMessage>,
            t: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            assert!(self.fixture.store.connection.is_autocommit());
            let writer = Store::open_background(&self.fixture.path)?.unwrap();
            writer.set_event_recordability(
                &self.fixture.task.event_id,
                false,
                "revoked during final request",
            )?;
            writer.set_event_recordability(
                &self.fixture.task.event_id,
                true,
                "restored after revocation",
            )?;
            self.model.generate_reply_with_tools(m, t)
        }
    }
    assert!(
        finalize_event_reading(
            &f.store,
            &Reclassify {
                fixture: &f,
                model: &model
            },
            &f.batch,
            &f.task.id
        )
        .is_err()
    );
    assert!(f.store.get_wiki_page("topic/long-event").unwrap().is_none());
    assert_eq!(
        f.store.event_reading_task(&f.task.id).unwrap().status,
        "cancelled"
    );
    assert_eq!(f.store.digest_job_stats().unwrap().running, 1);
    assert!(f.store.apply_digest_batch(&f.batch, &[]).is_err());
}

#[path = "budget_checks.rs"]
mod budget_checks;

#[path = "worker_tests.rs"]
mod worker_tests;

#[path = "http_tests.rs"]
mod http_tests;

#[test]
fn final_source_may_name_only_the_exact_in_batch_original_uuid() {
    let f = long_fixture();
    let model = TreeModel::new("exact_uuid");
    read_all(&f, &model);
    assert!(
        finalize_event_reading(&f.store, &model, &f.batch, &f.task.id)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        f.store
            .get_wiki_page("topic/long-event")
            .unwrap()
            .unwrap()
            .source_event_ids,
        vec![f.task.event_id.clone()]
    );
}
