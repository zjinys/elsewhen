use super::*;
struct Db {
    store: Store,
    dir: std::path::PathBuf,
}
impl Db {
    fn new() -> Self {
        let dir =
            std::env::temp_dir().join(format!("elsewhen-language-policy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        Self { store, dir }
    }
}
impl Drop for Db {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn local_policy_is_cas_durable_and_preserves_events_and_ui_preferences() {
    let db = Db::new();
    let s = &db.store;
    let event = s
        .insert_event(crate::event::NewEvent::now("原始事实😀"))
        .unwrap();
    assert_eq!(s.knowledge_language_policy().unwrap().generation, 0);
    let saved = s.set_knowledge_language("en", 0).unwrap();
    assert_eq!(saved.output_locale, "en");
    assert_eq!(saved.generation, 1);
    assert!(s.set_knowledge_language("zh-Hans", 0).is_err());
    assert!(s.set_knowledge_language("fr", 1).is_err());
    assert_eq!(
        Store::open(&s.path)
            .unwrap()
            .knowledge_language_policy()
            .unwrap(),
        saved
    );
    assert_eq!(
        s.list_events()
            .unwrap()
            .into_iter()
            .find(|e| e.id == event)
            .unwrap()
            .raw_text,
        "原始事实😀"
    );
    assert_eq!(s.prompt_language_preferences().unwrap().generation, 0);
    assert!(s.background_processing_allowed().unwrap());
}

#[test]
fn unsupported_strategy_and_legacy_connections_fail_closed_at_database_claim_boundary() {
    let db = Db::new();
    let s = &db.store;
    s.insert_event(crate::event::NewEvent::now("source record"))
        .unwrap();
    s.connection
        .execute(
            "UPDATE dataset_content_language SET output_locale='en',policy_version=999",
            [],
        )
        .unwrap();
    assert!(!s.background_processing_allowed().unwrap());
    assert!(s.claim_analysis_job().unwrap().is_none());
    assert!(
        s.connection
            .execute("UPDATE analysis_jobs SET status='running'", [])
            .unwrap_err()
            .to_string()
            .contains("sync_background_processor_required")
    );
    s.connection
        .execute("UPDATE dataset_content_language SET policy_version=1", [])
        .unwrap();
    let legacy = rusqlite::Connection::open(&s.path).unwrap();
    let error = legacy
        .execute("UPDATE analysis_jobs SET status='running'", [])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("elsewhen_language_policy_supported"),
        "{error}"
    );
    assert!(s.background_processing_allowed().unwrap());
    assert_eq!(s.list_events().unwrap().len(), 1);
}

#[test]
fn claimed_analysis_and_digest_keep_policy_across_retries_and_restart() {
    let db = Db::new();
    let s = &db.store;
    let first = s
        .insert_event(crate::event::NewEvent::now("original first"))
        .unwrap();
    let job = s.claim_analysis_job().unwrap().unwrap();
    let bundle = s.analysis_prompt_bundle(&job.id).unwrap();
    s.set_knowledge_language("en", 0).unwrap();
    let reopened = Store::open(&s.path).unwrap();
    reopened.recover_interrupted_analysis_jobs().unwrap();
    let retry = reopened.claim_analysis_job().unwrap().unwrap();
    assert_eq!(retry.id, job.id);
    assert_eq!(reopened.analysis_prompt_bundle(&retry.id).unwrap(), bundle);
    reopened
        .complete_analysis(&retry, "v1", r#"{"recordable":true}"#)
        .unwrap();
    let metadata: (String,i64) = reopened.connection.query_row("SELECT output_locale,policy_generation FROM knowledge_language_artifacts WHERE artifact_kind='analysis'",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(metadata, ("zh-Hans".into(), 0));
    reopened
        .connection
        .execute(
            "UPDATE knowledge_digest_jobs SET available_at='2000-01-01'",
            [],
        )
        .unwrap();
    let batch = reopened
        .claim_digest_batch(10, 10000, None)
        .unwrap()
        .unwrap();
    assert_eq!(batch.events[0].id, first);
    let digest_bundle = reopened.digest_prompt_bundle(&batch.id).unwrap();
    reopened.fail_digest_batch(&batch, "interrupted").unwrap();
    reopened.set_knowledge_language("zh-Hans", 1).unwrap();
    reopened
        .connection
        .execute(
            "UPDATE knowledge_digest_jobs SET available_at='2000-01-01'",
            [],
        )
        .unwrap();
    let retry = reopened
        .claim_digest_batch(10, 10000, None)
        .unwrap()
        .unwrap();
    assert_eq!(
        reopened.digest_prompt_bundle(&retry.id).unwrap(),
        digest_bundle
    );
    reopened.apply_digest_batch(&retry, &[]).unwrap();
    let scope = reopened.use_prompt_bundle(&digest_bundle);
    assert_eq!(reopened.run_language().unwrap().knowledge_locale, "en");
    assert_eq!(
        reopened.run_language().unwrap().knowledge_policy_generation,
        1
    );
    drop(scope);
    let second = reopened
        .insert_event(crate::event::NewEvent::now("original second"))
        .unwrap();
    let new_job = reopened.claim_analysis_job().unwrap().unwrap();
    assert_eq!(new_job.event_id, second);
    let new_bundle = reopened.analysis_prompt_bundle(&new_job.id).unwrap();
    let _scope = reopened.use_prompt_bundle(&new_bundle);
    assert_eq!(
        reopened.run_language().unwrap().knowledge_policy_generation,
        2
    );
    assert_ne!(bundle, new_bundle);
}

#[test]
fn reading_task_resumes_old_policy_and_explicit_new_work_has_separate_strategy() {
    let db = Db::new();
    let s = &db.store;
    let old = s.reading_prompt_task("snapshot-a", "reading-v1").unwrap();
    let before = s.prompt_dependency(&["reading.policy"]).unwrap();
    s.set_knowledge_language("en", 0).unwrap();
    let reopened = Store::open(&s.path).unwrap();
    assert_eq!(
        reopened
            .reading_prompt_task("snapshot-a", "reading-v1")
            .unwrap(),
        old
    );
    let old_scope = reopened.use_prompt_bundle(&old.0);
    assert_eq!(
        reopened.run_language().unwrap().knowledge_policy_generation,
        0
    );
    assert_eq!(
        reopened.prompt_dependency(&["reading.policy"]).unwrap(),
        before
    );
    drop(old_scope);
    let scope = reopened
        .prompt_scope("background_task", "explicit-new-reading")
        .unwrap();
    let new = reopened
        .reading_prompt_task("snapshot-a", "reading-v1")
        .unwrap();
    assert_ne!(old, new);
    assert_eq!(
        reopened
            .reading_prompt_strategy("snapshot-a", "reading-v1")
            .unwrap(),
        new.1
    );
    assert_eq!(
        reopened.run_language().unwrap().knowledge_policy_generation,
        1
    );
    drop(scope);
    let _scope = reopened.use_prompt_bundle(&old.0);
    assert_eq!(
        reopened
            .reading_prompt_strategy("snapshot-a", "reading-v1")
            .unwrap(),
        old.1
    );
}
