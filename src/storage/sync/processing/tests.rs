use super::*;
use crate::{
    ai::{
        memory::ContextMessage,
        provider::{AiProvider, AiReply},
        tool::ToolSpec,
    },
    storage::sync::ReplicaIdentity,
};

struct Database {
    store: Store,
    path: std::path::PathBuf,
}
impl Database {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("elsewhen-processor-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self {
            store: Store::open(&path.join("test.db")).unwrap(),
            path,
        }
    }
    fn enable(&self, device: &str, processor: &str) -> Result<()> {
        self.store.initialize_sync_replica(&ReplicaIdentity {
            dataset_id: "dataset".into(),
            device_id: device.into(),
            stream_id: format!("stream-{device}"),
            processor_device_id: processor.into(),
        })
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
struct NoModel;
impl AiProvider for NoModel {
    fn generate_reply_with_tools(
        &self,
        _: Vec<ContextMessage>,
        _: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        panic!("a non-processor must never call the model")
    }
}

#[test]
fn non_processor_still_captures_offline_but_cannot_claim_existing_or_new_jobs() {
    let db = Database::new();
    db.store
        .submit_input_as_event("before enabling", "capture", None)
        .unwrap();
    db.enable("b", "a").unwrap();
    db.store
        .submit_input_as_event("after enabling", "capture", None)
        .unwrap();
    assert_eq!(db.store.list_events().unwrap().len(), 2);
    assert_eq!(db.store.analysis_job_stats().unwrap().pending, 2);
    assert!(!db.store.background_processing_allowed().unwrap());
    assert!(db.store.claim_analysis_job().unwrap().is_none());
    assert_eq!(db.store.recover_interrupted_analysis_jobs().unwrap(), 0);
    db.store.connection.execute_batch("UPDATE analysis_jobs SET status='failed'; UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00+00:00';").unwrap();
    assert!(
        db.store
            .claim_digest_batch(10, 10000, None)
            .unwrap()
            .is_none()
    );
    assert_eq!(db.store.settle_digest_queue().unwrap(), 0);
    assert_eq!(db.store.recover_interrupted_digest_jobs().unwrap(), 0);
    assert_eq!(db.store.digest_job_stats().unwrap().pending, 2);
    let reopened = Store::open(&db.store.path).unwrap();
    assert!(!reopened.background_processing_allowed().unwrap());
    assert!(reopened.claim_analysis_job().unwrap().is_none());
}

#[test]
fn sqlite_guards_every_background_claim_even_without_the_worker_checks() {
    let db = Database::new();
    db.enable("b", "a").unwrap();
    db.store
        .submit_input_as_event("fact", "capture", None)
        .unwrap();
    for sql in [
        "UPDATE analysis_jobs SET status='running'",
        "UPDATE knowledge_digest_jobs SET status='running'",
        "INSERT INTO knowledge_digest_runs(id,started_at,status,event_count) VALUES('digest','t','running',1)",
        "INSERT INTO knowledge_background_runs(id,task,input_key,status,started_at) VALUES('reading','source-reading','source','running','t')",
        "INSERT INTO knowledge_refresh_jobs(page_id,requested_at,available_at,status) VALUES('missing-page','t','t','running')",
    ] {
        let error = db.store.connection.execute_batch(sql).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("sync_background_processor_required"),
            "unexpected error: {error}"
        );
    }
    assert_eq!(db.store.analysis_job_stats().unwrap().pending, 1);
    assert_eq!(db.store.digest_job_stats().unwrap().pending, 1);
}

#[test]
fn knowledge_workers_do_not_normalize_queues_or_call_models_on_other_devices() {
    let db = Database::new();
    db.enable("b", "a").unwrap();
    db.store
        .submit_input_as_event("background input", "capture", None)
        .unwrap();
    let model = NoModel;
    assert_eq!(
        crate::knowledge::reading::advance(&db.store, &model).unwrap(),
        0
    );
    assert_eq!(
        crate::knowledge_background::run_knowledge_refresh(&db.store, &model).unwrap(),
        0
    );
    assert_eq!(
        crate::knowledge_background::run_automatic_source_compilation(&db.store, &model).unwrap(),
        0
    );
    assert_eq!(
        crate::knowledge_background::run_automatic_integration(&db.store, &model).unwrap(),
        0
    );
    assert_eq!(
        crate::knowledge_background::run_automatic_insights(&db.store, &model).unwrap(),
        0
    );
    let runs: i64 = db
        .store
        .connection
        .query_row("SELECT COUNT(*) FROM knowledge_background_runs", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(runs, 0);
}

#[test]
fn enabling_sync_refuses_running_work_instead_of_switching_its_owner() {
    let db = Database::new();
    db.store
        .submit_input_as_event("in flight", "capture", None)
        .unwrap();
    let job = db.store.claim_analysis_job().unwrap().unwrap();
    assert!(db.enable("b", "a").is_err());
    assert!(db.store.sync_identity().unwrap().is_none());
    assert_eq!(db.store.analysis_job_stats().unwrap().running, 1);
    db.store
        .complete_analysis(&job, "test", r#"{"recordable":true}"#)
        .unwrap();
    db.enable("b", "a").unwrap();
    assert!(!db.store.background_processing_allowed().unwrap());
}

#[test]
fn designated_processor_claims_but_membership_and_initial_staging_can_block_it() {
    let db = Database::new();
    assert!(db.store.background_processing_allowed().unwrap());
    db.enable("a", "a").unwrap();
    assert!(db.store.background_processing_allowed().unwrap());
    db.store
        .connection
        .execute("UPDATE sync_members SET active=0", [])
        .unwrap();
    assert!(!db.store.background_processing_allowed().unwrap());
    db.store
        .connection
        .execute("UPDATE sync_members SET active=1", [])
        .unwrap();
    db.store.authorize_sync_member("b", "stream-b").unwrap();
    db.store.begin_snapshot_import("pending", "b", 1).unwrap();
    assert!(!db.store.background_processing_allowed().unwrap());
    let active = Database::new();
    active.enable("a", "a").unwrap();
    active
        .store
        .submit_input_as_event("process here", "capture", None)
        .unwrap();
    assert!(active.store.claim_analysis_job().unwrap().is_some());
}

#[test]
fn late_analysis_completion_is_rejected_after_local_processing_permission_changes() {
    let db = Database::new();
    db.enable("a", "a").unwrap();
    db.store
        .submit_input_as_event("original", "capture", None)
        .unwrap();
    let job = db.store.claim_analysis_job().unwrap().unwrap();
    // Test the commit guard independently; this is not a supported handoff API.
    db.store
        .connection
        .execute("UPDATE sync_replica SET processor_device_id='b'", [])
        .unwrap();
    assert!(
        db.store
            .complete_analysis(&job, "test", r#"{"recordable":true}"#)
            .is_err()
    );
    let results: i64 = db
        .store
        .connection
        .query_row("SELECT COUNT(*) FROM event_analyses", [], |r| r.get(0))
        .unwrap();
    assert_eq!(results, 0);
    assert_eq!(db.store.analysis_job_stats().unwrap().running, 1);
    assert_eq!(db.store.list_events().unwrap().len(), 1);
}
