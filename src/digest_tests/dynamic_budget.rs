use super::*;
use crate::ai::provider::{
    FailoverEntry, FailoverProvider, OpenAiCompatibleConfig, OpenAiCompatibleProvider,
};
use std::sync::{Arc, Mutex};

/// The concrete transport's pure estimator admits the real envelope. Only the
/// network reply is scripted; production selection and queue commit run intact.
struct MeteredProvider {
    meter: OpenAiCompatibleProvider,
    fail: bool,
    seen: Arc<Mutex<Vec<Vec<ContextMessage>>>>,
}
impl AiProvider for MeteredProvider {
    fn fits_request(&self, messages: &[ContextMessage]) -> Result<bool> {
        self.meter.fits_request(messages)
    }
    fn generate_reply_with_tools(
        &self,
        messages: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        assert!(self.meter.fits_request(&messages)?);
        let user = &messages.last().unwrap().content;
        let reply = habit_page_for_all(user);
        self.seen.lock().unwrap().push(messages);
        if self.fail {
            anyhow::bail!("503 fixture")
        }
        Ok(AiReply::text(reply))
    }
}
fn provider(
    window: u32,
    fail: bool,
    seen: Arc<Mutex<Vec<Vec<ContextMessage>>>>,
) -> MeteredProvider {
    MeteredProvider {
        meter: OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
            model: "unknown-byte-budget-model".into(),
            context_window: Some(window),
            max_tokens: Some(1024),
            base_url: "http://127.0.0.1:1/v1".into(),
            api_key: "synthetic".into(),
            ..Default::default()
        })
        .unwrap(),
        fail,
        seen,
    }
}

#[test]
fn digest_candidate_budget_splits_without_truncation_or_charging_unsubmitted_jobs() {
    let t = TempStore::new();
    let originals = (0..3)
        .map(|i| format!("event-{i}-{}-TAIL-{i}", "中文记录".repeat(300)))
        .collect::<Vec<_>>();
    for text in &originals {
        record(&t.store, text);
    }
    make_ready(&t.store);
    let seen = Arc::new(Mutex::new(Vec::new()));
    // One full ~3.6KB event fits; two do not.
    let metered = provider(7000, false, seen.clone());
    let tick = process_digest_queue(&t.store, &metered, None).unwrap();
    assert!(
        matches!(tick, DigestTick::Processed { events: 1, .. }),
        "{tick:?}"
    );
    let first_bundle: String = t
        .store
        .connection
        .query_row(
            "SELECT prompt_bundle_id FROM knowledge_digest_runs LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let pending: Vec<(i64, String)> = t
        .store
        .connection
        .prepare(
            "SELECT attempts,prompt_bundle_id FROM knowledge_digest_jobs WHERE status='pending'",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        pending,
        vec![(0, first_bundle.clone()), (0, first_bundle.clone())]
    );
    for _ in 0..2 {
        assert!(matches!(
            process_digest_queue(&t.store, &metered, None).unwrap(),
            DigestTick::Processed { events: 1, .. }
        ));
    }
    let captured = seen.lock().unwrap();
    assert_eq!(captured.len(), 3);
    for original in originals {
        assert_eq!(
            captured
                .iter()
                .filter(|messages| messages.last().unwrap().content.contains(&original))
                .count(),
            1
        );
    }
    let stats = t.store.digest_job_stats().unwrap();
    assert_eq!(stats.succeeded, 3);
    assert_eq!(stats.pending, 0);
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.retry, 0);
    let different_bundle: i64 = t
        .store
        .connection
        .query_row(
            "SELECT count(*) FROM knowledge_digest_runs WHERE prompt_bundle_id != ?1",
            [&first_bundle],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(different_bundle, 0);
}

#[test]
fn digest_failover_rebuilds_for_smaller_window_and_retains_unprocessed_sources() {
    let t = TempStore::new();
    for i in 0..3 {
        record(
            &t.store,
            &format!("event-{i} {} TAIL-{i}", "中文记录".repeat(300)),
        );
    }
    make_ready(&t.store);
    let big = Arc::new(Mutex::new(Vec::new()));
    let small = Arc::new(Mutex::new(Vec::new()));
    let chain = FailoverProvider::new(vec![
        FailoverEntry::new(
            "dynamic-budget-big",
            "big",
            Box::new(provider(32000, true, big.clone())),
        ),
        FailoverEntry::new(
            "dynamic-budget-small",
            "small",
            Box::new(provider(7000, false, small.clone())),
        ),
    ]);
    let tick = process_digest_queue(&t.store, &chain, None).unwrap();
    assert!(
        matches!(tick, DigestTick::Processed { events: 1, .. }),
        "{tick:?}"
    );
    assert_eq!(
        batch_size(&big.lock().unwrap()[0].last().unwrap().content),
        3
    );
    assert_eq!(
        batch_size(&small.lock().unwrap()[0].last().unwrap().content),
        1
    );
    assert_eq!(t.store.digest_job_stats().unwrap().pending, 2);
    assert_eq!(
        t.store
            .get_wiki_page("habit/morning-run")
            .unwrap()
            .unwrap()
            .evidence_count,
        1
    );
}

#[test]
fn digest_oversized_single_event_never_calls_network_or_confirms_partial_input() {
    let t = TempStore::new();
    let original = format!("BEGIN{}END", "完整超长事件".repeat(3000));
    let id = record(&t.store, &original);
    make_ready(&t.store);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let metered = provider(7000, false, seen.clone());
    let tick = process_digest_queue(&t.store, &metered, None).unwrap();
    assert!(
        matches!(&tick,DigestTick::Reading{state,..} if state=="preparing"),
        "{tick:?}"
    );
    assert!(seen.lock().unwrap().is_empty());
    assert_eq!(t.store.digest_job_stats().unwrap().succeeded, 0);
    assert_eq!(t.store.digest_job_stats().unwrap().pending, 1);
    let retained: String = t
        .store
        .connection
        .query_row("SELECT raw_text FROM events WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(retained, original);
    assert!(t.store.list_wiki_pages(None, None).unwrap().is_empty());
}

#[test]
fn digest_failed_prepared_prefix_refunds_unsubmitted_tail() {
    let t = TempStore::new();
    for i in 0..3 {
        record(
            &t.store,
            &format!("event-{i} {} TAIL-{i}", "中文记录".repeat(300)),
        );
    }
    make_ready(&t.store);
    let seen = Arc::new(Mutex::new(vec![]));
    assert!(matches!(
        process_digest_queue(&t.store, &provider(7000, true, seen.clone()), None).unwrap(),
        DigestTick::Failed { events: 1, .. }
    ));
    assert_eq!(seen.lock().unwrap().len(), 1);
    let attempts: Vec<(String, i64)> = t
        .store
        .connection
        .prepare("SELECT status,attempts FROM knowledge_digest_jobs ORDER BY status")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        attempts,
        vec![
            ("pending".into(), 0),
            ("pending".into(), 0),
            ("retry".into(), 1)
        ]
    );
}
