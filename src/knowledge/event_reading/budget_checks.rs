use super::*;

#[test]
fn final_failover_rechecks_candidate_window_and_preserves_verified_material() {
    let f = long_fixture();
    let model = TreeModel::new("ok");
    read_all(&f, &model);
    struct Unavailable {
        model: TreeModel,
        mode: &'static str,
        calls: Arc<Mutex<usize>>,
    }
    impl AiProvider for Unavailable {
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
            _: Option<&[ToolSpec]>,
        ) -> Result<AiReply> {
            assert!(self.fits_request(&m)?);
            *self.calls.lock().unwrap() += 1;
            if self.mode == "network" {
                anyhow::bail!("synthetic 503");
            }
            let mut reply = self.model.generate_reply_with_tools(m, None)?;
            if self.mode == "length" {
                reply.finish_reason = Some("length".into());
            }
            Ok(reply)
        }
    }
    let tiny = Arc::new(Mutex::new(vec![]));
    let failed = Arc::new(Mutex::new(0));
    let truncated = Arc::new(Mutex::new(0));
    let good = Arc::new(Mutex::new(0));
    let unique = uuid::Uuid::new_v4().to_string();
    let chain = FailoverProvider::new(vec![
        FailoverEntry::new(
            format!("tiny-{unique}"),
            "tiny",
            Box::new(meter(1700, 1024, "network", tiny.clone())),
        ),
        FailoverEntry::new(
            format!("network-{unique}"),
            "network",
            Box::new(Unavailable {
                model: TreeModel::new("ok"),
                mode: "network",
                calls: failed.clone(),
            }),
        ),
        FailoverEntry::new(
            format!("truncated-{unique}"),
            "truncated",
            Box::new(Unavailable {
                model: TreeModel::new("ok"),
                mode: "length",
                calls: truncated.clone(),
            }),
        ),
        FailoverEntry::new(
            format!("good-{unique}"),
            "good",
            Box::new(Unavailable {
                model: TreeModel::new("ok"),
                mode: "ok",
                calls: good.clone(),
            }),
        ),
    ]);
    assert!(
        finalize_event_reading(&f.store, &chain, &f.batch, &f.task.id)
            .unwrap()
            .is_some()
    );
    assert!(tiny.lock().unwrap().is_empty());
    assert_eq!(*failed.lock().unwrap(), 1);
    assert_eq!(*truncated.lock().unwrap(), 1);
    assert_eq!(*good.lock().unwrap(), 1);
    assert_eq!(f.store.digest_job_stats().unwrap().succeeded, 1);
}

#[test]
fn an_unmergeable_pair_waits_for_budget_without_claiming_partial_root() {
    let f = Fixture::new(&format!("{}🙂TAIL", "unique words 123 ".repeat(300)), true);
    let now = chrono::Utc::now().timestamp();
    let middle = f.task.total_chars / 2;
    let long = ReadingOutput {
        summary: "s".repeat(1500),
        quotes: vec![],
    };
    f.store
        .prepare_event_reading_node(&f.task.id, 0, middle, &[])
        .unwrap();
    f.store
        .prepare_event_reading_node(&f.task.id, middle, f.task.total_chars, &[])
        .unwrap();
    for _ in 0..2 {
        let lease = f
            .store
            .claim_event_reading_node(&f.task.id, now)
            .unwrap()
            .unwrap();
        f.store
            .complete_event_reading_node(&lease, &long, now)
            .unwrap();
    }
    let seen = Arc::new(Mutex::new(vec![]));
    let provider = meter(3500, 1024, "ok", seen.clone());
    assert!(
        crate::knowledge::event_reading::advance(&f.store, &provider, &f.task.id)
            .unwrap_err()
            .to_string()
            .contains("merge_input_exceeds_budget")
    );
    assert!(seen.lock().unwrap().is_empty());
    assert_eq!(f.store.event_reading_nodes(&f.task.id).unwrap().len(), 2);
    assert_eq!(
        f.store.event_reading_task(&f.task.id).unwrap().status,
        "active"
    );
    assert_eq!(f.store.digest_job_stats().unwrap().running, 1);
}
