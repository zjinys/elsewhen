use super::*;
use crate::wiki::{DigestTick, process_digest_queue};

fn fresh_queue() -> Fixture {
    let f = long_fixture();
    f.store
        .connection
        .execute("DELETE FROM event_reading_tasks WHERE id=?1", [&f.task.id])
        .unwrap();
    f.store.park_digest_for_reading(&f.batch).unwrap();
    f
}
fn task_id(f: &Fixture) -> String {
    f.store
        .connection
        .query_row(
            "SELECT id FROM event_reading_tasks WHERE event_id=?1",
            [&f.task.event_id],
            |r| r.get(0),
        )
        .unwrap()
}

struct WorkerModel {
    tree: TreeModel,
    short_calls: std::cell::Cell<usize>,
    bad_leaf: std::cell::Cell<bool>,
}
impl WorkerModel {
    fn new() -> Self {
        Self {
            tree: TreeModel::new("ok"),
            short_calls: std::cell::Cell::new(0),
            bad_leaf: std::cell::Cell::new(false),
        }
    }
}
impl AiProvider for WorkerModel {
    fn fits_request(&self, m: &[ContextMessage]) -> Result<bool> {
        self.tree.fits_request(m)
    }
    fn visit_request_budgets(
        &self,
        v: &mut dyn FnMut(&RequestFit<'_>) -> Result<()>,
    ) -> Result<()> {
        self.tree.visit_request_budgets(v)
    }
    fn generate_reply_with_tools(
        &self,
        m: Vec<ContextMessage>,
        t: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        if m[1].content.contains("SHORT") {
            self.short_calls.set(self.short_calls.get() + 1);
            let mut reply = AiReply::text(
                r#"[{"kind":"topic","slug":"topic/short","title":"Short","content":"SHORT","source_event_ids":["1"]}]"#,
            );
            reply.finish_reason = Some("stop".into());
            return Ok(reply);
        }
        if self.bad_leaf.replace(false) {
            let mut reply = AiReply::text("{ bad json");
            reply.finish_reason = Some("stop".into());
            return Ok(reply);
        }
        self.tree.generate_reply_with_tools(m, t)
    }
}

#[test]
fn background_ticks_discover_read_merge_and_commit_without_charging_preparation() {
    let f = fresh_queue();
    let model = WorkerModel::new();
    let first = process_digest_queue(&f.store, &model, None).unwrap();
    assert!(matches!(first,DigestTick::Reading{ref state,..} if state=="preparing"));
    assert!(model.tree.calls.borrow().is_empty());
    assert_eq!(f.store.digest_job_stats().unwrap().pending, 1);
    let attempts: i64 = f
        .store
        .connection
        .query_row("SELECT attempts FROM knowledge_digest_jobs", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(attempts, 0);
    let id = task_id(&f);
    for _ in 0..120 {
        let before = model.tree.calls.borrow().len();
        let store = Store::open_background(&f.path).unwrap().unwrap();
        store.recover_interrupted_digest_jobs().unwrap();
        let tick = process_digest_queue(&store, &model, None).unwrap();
        assert!(model.tree.calls.borrow().len() - before <= 1);
        if let DigestTick::Processed { events, .. } = tick {
            assert_eq!(events, 1);
            break;
        }
        assert!(
            matches!(tick,DigestTick::Reading{ref state,..} if state=="advanced"),
            "{tick:?}"
        );
        assert_eq!(store.digest_job_stats().unwrap().succeeded, 0);
        assert!(store.get_wiki_page("topic/long-event").unwrap().is_none());
    }
    assert_eq!(f.store.digest_job_stats().unwrap().succeeded, 1);
    let progress = f
        .store
        .event_reading_progress(&f.task.event_id)
        .unwrap()
        .unwrap();
    assert_eq!(progress.phase, "succeeded");
    assert_eq!(progress.read_chars, progress.total_chars);
    assert!(progress.completed_merges > 0);
    assert_eq!(f.store.event_reading_task(&id).unwrap().status, "succeeded");
    assert_eq!(
        f.store
            .get_wiki_page("topic/long-event")
            .unwrap()
            .unwrap()
            .source_event_ids,
        vec![f.task.event_id.clone()]
    );
}

#[test]
fn long_reading_yields_to_short_event_and_node_failure_uses_its_own_retry() {
    let f = fresh_queue();
    let model = WorkerModel::new();
    process_digest_queue(&f.store, &model, None).unwrap();
    let id = task_id(&f);
    let short = f
        .store
        .insert_event(NewEvent::now("SHORT 17 units"))
        .unwrap();
    f.store
        .set_event_recordability(&short, true, "synthetic")
        .unwrap();
    f.store
        .connection
        .execute(
            "UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
    model.bad_leaf.set(true);
    assert!(
        matches!(process_digest_queue(&f.store,&model,None).unwrap(),DigestTick::Reading{ref state,..} if state=="failed")
    );
    let attempts: i64 = f
        .store
        .connection
        .query_row(
            "SELECT attempts FROM knowledge_digest_jobs WHERE event_id=?1",
            [&f.task.event_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(attempts, 0);
    let p = f
        .store
        .event_reading_progress(&f.task.event_id)
        .unwrap()
        .unwrap();
    assert!(p.error.is_some());
    assert_eq!(p.read_chars, 0);
    assert!(matches!(
        process_digest_queue(&f.store, &model, None).unwrap(),
        DigestTick::Processed { events: 1, .. }
    ));
    assert_eq!(model.short_calls.get(), 1);
    assert!(matches!(
        process_digest_queue(&f.store, &model, None).unwrap(),
        DigestTick::Idle
    ));
    f.store
        .connection
        .execute("UPDATE event_reading_tasks SET available_at=0", [])
        .unwrap();
    f.retry_now();
    assert!(
        matches!(process_digest_queue(&f.store,&model,None).unwrap(),DigestTick::Reading{ref state,..} if state=="advanced")
    );
    assert!(f.store.event_reading_task(&id).unwrap().status == "active");
    assert!(
        f.store
            .event_reading_progress(&f.task.event_id)
            .unwrap()
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn healthy_final_lease_survives_api_recovery_and_stale_one_recovers_root() {
    let mut f = fresh_queue();
    let model = WorkerModel::new();
    process_digest_queue(&f.store, &model, None).unwrap();
    let id = task_id(&f);
    f.task = f.store.event_reading_task(&id).unwrap();
    read_all(&f, &model.tree);
    let task = f.store.event_reading_task(&id).unwrap();
    let batch = f
        .store
        .claim_event_reading_batch(&task, None)
        .unwrap()
        .unwrap();
    let now = chrono::Utc::now().timestamp();
    let lease = f
        .store
        .claim_event_reading_final(&id, &batch, now)
        .unwrap()
        .unwrap();
    assert_eq!(f.store.recover_interrupted_digest_jobs().unwrap(), 0);
    assert_eq!(f.store.digest_job_stats().unwrap().running, 1);
    f.store
        .connection
        .execute("UPDATE event_reading_tasks SET lease_until=?1", [now - 1])
        .unwrap();
    assert_eq!(f.store.recover_interrupted_digest_jobs().unwrap(), 1);
    assert!(
        f.store
            .apply_digest_batch_with_reading(&batch, &[], Some(&lease))
            .is_err()
    );
    assert!(matches!(
        process_digest_queue(&f.store, &model, None).unwrap(),
        DigestTick::Processed { events: 1, .. }
    ));
}

#[test]
fn cancelled_reading_is_skipped_and_reclassification_resumes_cache() {
    let f = fresh_queue();
    let model = WorkerModel::new();
    process_digest_queue(&f.store, &model, None).unwrap();
    process_digest_queue(&f.store, &model, None).unwrap();
    let id = task_id(&f);
    let nodes = f.store.event_reading_nodes(&id).unwrap();
    assert!(nodes[0].output.is_some());
    f.store
        .set_event_recordability(&f.task.event_id, false, "discussion")
        .unwrap();
    assert!(matches!(
        process_digest_queue(&f.store, &model, None).unwrap(),
        DigestTick::Idle
    ));
    assert_eq!(f.store.digest_job_stats().unwrap().skipped, 1);
    f.store
        .set_event_recordability(&f.task.event_id, true, "event")
        .unwrap();
    assert!(
        matches!(process_digest_queue(&f.store,&model,None).unwrap(),DigestTick::Reading{ref state,..} if state=="preparing")
    );
    assert_eq!(task_id(&f), id);
    assert_eq!(
        f.store.event_reading_nodes(&id).unwrap()[0].output,
        nodes[0].output
    );
}

#[test]
fn multiple_long_events_rotate_and_configuration_failure_leaves_short_work_eligible() {
    let f = fresh_queue();
    let model = WorkerModel::new();
    process_digest_queue(&f.store, &model, None).unwrap();
    let first = task_id(&f);
    let other = f
        .store
        .insert_event(NewEvent::now(&f.batch.events[0].raw_text))
        .unwrap();
    f.store
        .set_event_recordability(&other, true, "synthetic second long event")
        .unwrap();
    f.store
        .connection
        .execute(
            "UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
    process_digest_queue(&f.store, &model, None).unwrap(); // first leaf
    process_digest_queue(&f.store, &model, None).unwrap(); // second task preparation
    let second: String = f
        .store
        .connection
        .query_row(
            "SELECT id FROM event_reading_tasks WHERE event_id=?1",
            [&other],
            |r| r.get(0),
        )
        .unwrap();
    process_digest_queue(&f.store, &model, None).unwrap(); // second leaf
    assert!(
        f.store.event_reading_nodes(&first).unwrap()[0]
            .output
            .is_some()
    );
    assert!(
        f.store.event_reading_nodes(&second).unwrap()[0]
            .output
            .is_some()
    );
    let seen = Arc::new(Mutex::new(vec![]));
    assert!(
        matches!(process_digest_queue(&f.store,&meter(1800,1024,"ok",seen.clone()),None).unwrap(),DigestTick::Reading{ref state,..} if state=="failed")
    );
    assert!(seen.lock().unwrap().is_empty());
    let short = f
        .store
        .insert_event(NewEvent::now("SHORT after configuration failure"))
        .unwrap();
    f.store
        .set_event_recordability(&short, true, "synthetic")
        .unwrap();
    f.store
        .connection
        .execute(
            "UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
    assert!(matches!(
        process_digest_queue(&f.store, &model, None).unwrap(),
        DigestTick::Processed { events: 1, .. }
    ));
    assert_eq!(model.short_calls.get(), 1);
}

#[test]
fn final_retry_preserves_root_and_success_clears_wait_reason() {
    let mut f = fresh_queue();
    let model = WorkerModel::new();
    process_digest_queue(&f.store, &model, None).unwrap();
    let id = task_id(&f);
    f.task = f.store.event_reading_task(&id).unwrap();
    read_all(&f, &model.tree);
    let invalid = TreeModel::new("source");
    assert!(
        matches!(process_digest_queue(&f.store,&invalid,None).unwrap(),DigestTick::Reading{ref state,..} if state=="failed")
    );
    let p = f
        .store
        .event_reading_progress(&f.task.event_id)
        .unwrap()
        .unwrap();
    assert_eq!(p.phase, "ready");
    assert!(p.error.is_some());
    let before = model.tree.calls.borrow().len();
    f.store
        .connection
        .execute("UPDATE event_reading_tasks SET available_at=0", [])
        .unwrap();
    f.store
        .connection
        .execute(
            "UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
    assert!(matches!(
        process_digest_queue(&f.store, &model, None).unwrap(),
        DigestTick::Processed { events: 1, .. }
    ));
    assert_eq!(model.tree.calls.borrow().len(), before + 1);
    let p = f
        .store
        .event_reading_progress(&f.task.event_id)
        .unwrap()
        .unwrap();
    assert_eq!(p.phase, "succeeded");
    assert!(p.error.is_none());
}

#[test]
fn material_limit_fails_first_event_without_leaving_running_or_charging_tail() {
    let path = std::env::temp_dir().join(format!("elsewhen-limit-{}.db", uuid::Uuid::new_v4()));
    let store = Store::open(&path).unwrap();
    let long = store
        .insert_event(NewEvent::now(&"x".repeat(2_000_001)))
        .unwrap();
    store
        .set_event_recordability(&long, true, "synthetic limit")
        .unwrap();
    let short = store.insert_event(NewEvent::now("SHORT tail")).unwrap();
    store
        .set_event_recordability(&short, true, "synthetic")
        .unwrap();
    store
        .connection
        .execute(
            "UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
    let tick = process_digest_queue(&store, &WorkerModel::new(), None).unwrap();
    assert!(
        matches!(tick,DigestTick::Failed{events:1,ref error} if error.contains("material_limit"))
    );
    assert_eq!(store.digest_job_stats().unwrap().running, 0);
    assert_eq!(store.digest_job_stats().unwrap().retry, 1);
    let attempts: i64 = store
        .connection
        .query_row(
            "SELECT attempts FROM knowledge_digest_jobs WHERE event_id=?1",
            [short],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(attempts, 0);
    assert_eq!(store.list_events().unwrap().len(), 2);
    drop(store);
    let _ = std::fs::remove_file(path);
}
