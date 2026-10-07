use super::*;
use crate::storage::sync::ReplicaIdentity;

struct Database {
    store: Store,
    path: std::path::PathBuf,
}
impl Database {
    fn new(device: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("elsewhen-sync-analysis-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        let store = Store::open(&path.join("test.db")).unwrap();
        store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "dataset".into(),
                device_id: device.into(),
                stream_id: format!("stream-{device}"),
                processor_device_id: "a".into(),
            })
            .unwrap();
        for other in ["a", "b", "c"] {
            if other != device {
                store
                    .authorize_sync_member(other, &format!("stream-{other}"))
                    .unwrap();
            }
        }
        Self { store, path }
    }
    fn count(&self, table: &str) -> i64 {
        self.store
            .connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
fn deliver(target: &Store, sender: &str, op: &SyncOperation) -> super::super::ReceiveReport {
    target
        .receive_sync_operation(sender, &serde_json::to_vec(op).unwrap())
        .unwrap()
}
const RESULT: &str = r#"{"recordable":true,"summary":"synthetic analysis"}"#;

#[test]
fn only_processor_schedules_imported_events_and_completed_result_returns_without_reexecution() {
    let a = Database::new("a");
    let b = Database::new("b");
    let input = b
        .store
        .submit_input_as_event("offline b fact", "capture", None)
        .unwrap();
    let capture = b.store.pending_sync_operations("a", 10).unwrap().remove(0);
    deliver(&a.store, "b", &capture);
    assert_eq!(a.store.analysis_job_stats().unwrap().pending, 1);
    assert_eq!(a.store.digest_job_stats().unwrap().pending, 1);
    assert!(b.store.claim_analysis_job().unwrap().is_none());
    let job = a.store.claim_analysis_job().unwrap().unwrap();
    a.store
        .complete_analysis(&job, "event-analysis", RESULT)
        .unwrap();
    let result = a.store.pending_sync_operations("b", 10).unwrap().remove(0);
    let SyncMutation::AnalysisCompleted { analysis } = &result.mutation else {
        panic!("expected analysis result")
    };
    assert_eq!(
        analysis.task_id, job.id,
        "imported job and result share the stable input/strategy identity"
    );
    deliver(&b.store, "a", &result);
    assert_eq!(b.store.analysis_job_stats().unwrap().succeeded, 1);
    assert_eq!(b.store.analysis_job_stats().unwrap().pending, 0);
    assert_eq!(b.count("event_analyses"), 1);
    assert_eq!(b.count("chat_runs"), 0);
    assert_eq!(b.count("chat_proposal_decisions"), 0);
    let detail = b
        .store
        .event_analysis_detail(input.event_id.as_deref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(detail.result_json.as_deref(), Some(RESULT));
    deliver(&a.store, "b", &capture);
    deliver(&b.store, "a", &result);
    assert!(a.store.claim_analysis_job().unwrap().is_none());
    assert!(b.store.claim_analysis_job().unwrap().is_none());
    assert_eq!(a.count("analysis_jobs"), 1);
    assert_eq!(b.count("event_analyses"), 1);
}

#[test]
fn result_before_original_waits_and_a_third_device_receives_terminal_state_without_pending_work() {
    let a = Database::new("a");
    let b = Database::new("b");
    let c = Database::new("c");
    b.store
        .submit_input_as_event("original before result", "capture", None)
        .unwrap();
    let capture = b.store.pending_sync_operations("a", 10).unwrap().remove(0);
    deliver(&a.store, "b", &capture);
    let job = a.store.claim_analysis_job().unwrap().unwrap();
    a.store
        .complete_analysis(&job, "event-analysis", RESULT)
        .unwrap();
    let result = a.store.pending_sync_operations("c", 10).unwrap().remove(0);
    let report = deliver(&c.store, "a", &result);
    assert_eq!(report.waiting, 1);
    assert_eq!(c.count("event_analyses"), 0);
    assert_eq!(c.store.pending_sync_receipts("a", 10).unwrap().len(), 0);
    deliver(&c.store, "b", &capture);
    assert_eq!(c.count("event_analyses"), 1);
    assert_eq!(c.store.analysis_job_stats().unwrap().pending, 0);
    assert_eq!(c.store.analysis_job_stats().unwrap().succeeded, 1);
    assert_eq!(c.count("knowledge_digest_jobs"), 0);
}

#[test]
fn wrong_processor_and_wrong_input_hash_cannot_publish_analysis() {
    let a = Database::new("a");
    let b = Database::new("b");
    let c = Database::new("c");
    b.store
        .submit_input_as_event("verified raw fact", "capture", None)
        .unwrap();
    let capture = b.store.pending_sync_operations("a", 10).unwrap().remove(0);
    deliver(&a.store, "b", &capture);
    deliver(&c.store, "b", &capture);
    let job = a.store.claim_analysis_job().unwrap().unwrap();
    a.store
        .complete_analysis(&job, "event-analysis", RESULT)
        .unwrap();
    let mut result = a.store.pending_sync_operations("c", 10).unwrap().remove(0);
    let mut forged = result.clone();
    forged.origin_device_id = "b".into();
    forged.origin_stream_id = "stream-b".into();
    forged.origin_sequence = 2;
    forged.op_id = "stream-b:2".into();
    assert_eq!(deliver(&c.store, "b", &forged).rejected, 1);
    let SyncMutation::AnalysisCompleted { analysis } = &mut result.mutation else {
        unreachable!()
    };
    analysis.input_hash = "0".repeat(64);
    assert_eq!(deliver(&c.store, "a", &result).rejected, 1);
    assert_eq!(c.count("event_analyses"), 0);
    assert_eq!(c.store.analysis_job_stats().unwrap().succeeded, 0);
    assert_eq!(c.store.pending_sync_receipts("a", 10).unwrap().len(), 0);
}

#[test]
fn analysis_and_outbox_commit_together_and_failed_incoming_receipt_rolls_back_completion() {
    let a = Database::new("a");
    let b = Database::new("b");
    b.store
        .submit_input_as_event("atomic result", "capture", None)
        .unwrap();
    let capture = b.store.pending_sync_operations("a", 10).unwrap().remove(0);
    deliver(&a.store, "b", &capture);
    let job = a.store.claim_analysis_job().unwrap().unwrap();
    a.store.connection.execute_batch("CREATE TRIGGER fail_analysis_op BEFORE INSERT ON sync_operations WHEN NEW.local=1 BEGIN SELECT RAISE(ABORT,'synthetic operation failure'); END;").unwrap();
    assert!(
        a.store
            .complete_analysis(&job, "event-analysis", RESULT)
            .is_err()
    );
    assert_eq!(a.count("event_analyses"), 0);
    assert_eq!(a.store.analysis_job_stats().unwrap().running, 1);
    a.store
        .connection
        .execute_batch("DROP TRIGGER fail_analysis_op")
        .unwrap();
    a.store
        .complete_analysis(&job, "event-analysis", RESULT)
        .unwrap();
    let result = a.store.pending_sync_operations("b", 10).unwrap().remove(0);
    b.store.connection.execute_batch("CREATE TRIGGER fail_analysis_receipt BEFORE INSERT ON sync_receipts BEGIN SELECT RAISE(ABORT,'synthetic receipt failure'); END;").unwrap();
    assert!(
        b.store
            .receive_sync_operation("a", &serde_json::to_vec(&result).unwrap())
            .is_err()
    );
    assert_eq!(b.count("event_analyses"), 0);
    assert_eq!(b.store.analysis_job_stats().unwrap().pending, 1);
    b.store
        .connection
        .execute_batch("DROP TRIGGER fail_analysis_receipt")
        .unwrap();
    deliver(&b.store, "a", &result);
    assert_eq!(b.store.analysis_job_stats().unwrap().succeeded, 1);
}
