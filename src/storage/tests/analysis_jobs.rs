use super::*;

#[test]
fn analysis_job_stats_cover_every_queue_status() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.analysis_job_stats().unwrap(),
        AnalysisJobStats::default()
    );

    for (index, status) in ["pending", "running", "retry", "succeeded", "failed"]
        .into_iter()
        .enumerate()
    {
        let event_id = store
            .insert_event(NewEvent::now(&format!("queue status {index}")))
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE analysis_jobs SET status = ?1 WHERE event_id = ?2",
                params![status, event_id],
            )
            .unwrap();
    }

    assert_eq!(
        store.analysis_job_stats().unwrap(),
        AnalysisJobStats {
            pending: 1,
            running: 1,
            retry: 1,
            succeeded: 1,
            failed: 1,
        }
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}
#[test]
fn submit_input_as_event_is_atomic_and_idempotent() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let first = store
        .submit_input_as_event("今天完成统一输入", "main_input", Some("submit-1"))
        .unwrap();
    let repeated = store
        .submit_input_as_event("不会产生第二条", "main_input", Some("submit-1"))
        .unwrap();
    assert_eq!(first.id, repeated.id);
    assert_eq!(first.route_status, "routed");
    assert!(first.event_id.is_some());
    assert_eq!(store.list_events().unwrap().len(), 1);
    assert_eq!(store.analysis_job_stats().unwrap().pending, 1);

    drop(store);
    let _ = std::fs::remove_file(path);
}
#[test]
fn event_creation_enqueues_one_analysis_job() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_id = store.insert_event(NewEvent::now("等待后台理解")).unwrap();
    let count: i64 = store
        .connection
        .query_row(
            "SELECT count(*) FROM analysis_jobs WHERE event_id = ?1 AND status = 'pending'",
            [event_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    let _ = std::fs::remove_file(path);
}
#[test]
fn failed_analysis_keeps_raw_event_and_schedules_retry() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store
        .insert_event(NewEvent::now("AI 失败也不能丢"))
        .unwrap();
    let job = store.claim_analysis_job().unwrap().unwrap();
    store.fail_analysis(&job, "provider unavailable").unwrap();
    let status: String = store
        .connection
        .query_row(
            "SELECT status FROM analysis_jobs WHERE id = ?1",
            [&job.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "retry");
    assert_eq!(store.list_events().unwrap()[0].raw_text, "AI 失败也不能丢");
    let _ = std::fs::remove_file(path);
}
#[test]
fn recover_interrupted_analysis_jobs_requeues_running_work() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store.insert_event(NewEvent::now("恢复中的任务")).unwrap();
    let job = store.claim_analysis_job().unwrap().unwrap();
    assert_eq!(store.analysis_job_stats().unwrap().running, 1);
    assert_eq!(store.recover_interrupted_analysis_jobs().unwrap(), 1);
    assert_eq!(store.analysis_job_stats().unwrap().retry, 1);
    assert!(store.claim_analysis_job().unwrap().is_some());
    let _ = std::fs::remove_file(path);
    drop(job);
}
#[test]
fn event_analysis_detail_exposes_queue_result_and_error() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_id = store.insert_event(NewEvent::now("查看分析详情")).unwrap();
    let pending = store.event_analysis_detail(&event_id).unwrap().unwrap();
    assert_eq!(pending.job_status, "pending");
    assert_eq!(pending.attempts, 0);
    assert!(pending.result_json.is_none());

    let job = store.claim_analysis_job().unwrap().unwrap();
    store.fail_analysis(&job, "invalid schema").unwrap();
    let retry = store.event_analysis_detail(&event_id).unwrap().unwrap();
    assert_eq!(retry.job_status, "retry");
    assert_eq!(retry.last_error.as_deref(), Some("invalid schema"));

    store
        .complete_analysis(
            &job,
            "event-analysis-v1",
            r#"{"schema_version":"event-analysis-v1"}"#,
        )
        .unwrap();
    let succeeded = store.event_analysis_detail(&event_id).unwrap().unwrap();
    assert_eq!(succeeded.job_status, "succeeded");
    assert_eq!(
        succeeded.prompt_version.as_deref(),
        Some("event-analysis-v1")
    );
    assert!(succeeded.result_json.is_some());
    drop(store);
    let _ = std::fs::remove_file(path);
}
#[test]
fn reanalysis_requeues_without_deleting_previous_analysis() {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let event_id = store.insert_event(NewEvent::now("存量重新分析")).unwrap();
    let job = store.claim_analysis_job().unwrap().unwrap();
    store.complete_analysis(&job, "event-analysis", r#"{"schema_version":"event-analysis","recordable":true,"kind":"event","event_type":"note","confidence":0.8,"summary":"旧结果","clarifications":[],"people":[],"projects":[],"activities":[],"follow_ups":[]}"#).unwrap();
    assert!(store.requeue_event_analysis(&event_id).unwrap());
    let detail = store.event_analysis_detail(&event_id).unwrap().unwrap();
    assert_eq!(detail.job_status, "pending");
    assert!(detail.result_json.unwrap().contains("旧结果"));
    assert_eq!(
        store
            .connection
            .query_row(
                "SELECT count(*) FROM event_analyses WHERE event_id=?1",
                [&event_id],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}
