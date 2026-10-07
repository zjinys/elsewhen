use super::*;
use crate::sync::crypto::ContentKey;

pub(super) struct Fixture {
    pub(super) config: AppConfig,
    pub(super) store: Store,
}
impl Fixture {
    pub(super) fn new() -> Self {
        let path = std::env::temp_dir().join(format!("elsewhen-runtime-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        let config = AppConfig {
            database_path: path.join("elsewhen.db"),
            data_dir: path,
        };
        let store = Store::open(&config.database_path).unwrap();
        Self { config, store }
    }
    pub(super) fn initialize(&self) -> ([u8; 32], KeyCommit) {
        let request = prepare_key(&self.config, Some("owner"), Some("test")).unwrap();
        assert!(request.creating);
        assert_eq!(
            request.revision, 1,
            "native storage reserves initial revision before DB creation"
        );
        let key = ContentKey::generate().unwrap().to_bytes();
        let commit = install_key(&self.config, &request.token, key.to_vec(), 1).unwrap();
        (key, commit)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        lock(&self.config);
        let _ = std::fs::remove_dir_all(&self.config.data_dir);
    }
}

#[test]
fn initialization_requires_protected_revision_and_confirmation_before_background_use() {
    let f = Fixture::new();
    f.store
        .submit_input_as_event("preserved local input", "capture", None)
        .unwrap();
    assert!(background_guard(&f.config, &f.store).unwrap().is_some());
    let request = prepare_key(&f.config, Some("owner"), Some("test")).unwrap();
    let key = ContentKey::generate().unwrap().to_bytes();
    assert!(install_key(&f.config, &request.token, vec![0; 31], 1).is_err());
    assert!(install_key(&f.config, &request.token, key.to_vec(), 0).is_err());
    assert!(f.store.sync_identity().unwrap().is_none());
    let commit = install_key(&f.config, &request.token, key.to_vec(), 1).unwrap();
    assert!(!unlocked(&f.config, 1));
    assert!(background_guard(&f.config, &f.store).unwrap().is_none());
    assert!(confirm_key(&f.config, &commit.token, 0).is_err());
    confirm_key(&f.config, &commit.token, 1).unwrap();
    assert!(unlocked(&f.config, 1));
    assert!(background_guard(&f.config, &f.store).unwrap().is_some());
    assert_eq!(f.store.list_events().unwrap().len(), 1);
    let job = f.store.claim_analysis_job().unwrap().unwrap();
    confirm_key(&f.config, &commit.token, 1).unwrap();
    assert_eq!(
        f.store.analysis_job_stats().unwrap().running,
        1,
        "duplicate confirmation must not recover a live job"
    );
    f.store
        .complete_analysis(&job, "test", r#"{"summary":"done"}"#)
        .unwrap();
}

#[test]
fn locking_does_not_release_the_file_lease_of_a_live_background_call() {
    let f = Fixture::new();
    let (key, commit) = f.initialize();
    confirm_key(&f.config, &commit.token, 1).unwrap();
    let guard = background_guard(&f.config, &f.store).unwrap().unwrap();
    assert!(lease(&f.config.data_dir).is_err());
    lock(&f.config);
    assert!(!unlocked(&f.config, 1));
    assert!(prepare_key(&f.config, None, None).is_err());
    drop(guard);
    let request = prepare_key(&f.config, None, None).unwrap();
    assert!(!request.creating);
    let commit = install_key(&f.config, &request.token, key.to_vec(), 1).unwrap();
    confirm_key(&f.config, &commit.token, 1).unwrap();
    assert!(unlocked(&f.config, 1));
}

#[test]
fn missing_revision_wrong_key_and_changed_scope_fail_closed() {
    let f = Fixture::new();
    let (key, commit) = f.initialize();
    confirm_key(&f.config, &commit.token, 1).unwrap();
    lock(&f.config);
    let request = prepare_key(&f.config, None, None).unwrap();
    assert!(install_key(&f.config, &request.token, key.to_vec(), 0).is_err());
    assert!(install_key(&f.config, &request.token, vec![0; 32], 1).is_err());
    assert!(!unlocked(&f.config, 1));
    let wrong_scope = AppConfig {
        data_dir: f.config.data_dir.clone(),
        database_path: f.config.data_dir.join("other.db"),
    };
    assert!(install_key(&wrong_scope, &request.token, key.to_vec(), 1).is_err());
    assert!(!wrong_scope.database_path.exists());
    let commit = install_key(&f.config, &request.token, key.to_vec(), 1).unwrap();
    assert!(confirm_key(&f.config, &request.token, 1).is_err());
    confirm_key(&f.config, &commit.token, 1).unwrap();
}

#[test]
fn standalone_background_calls_block_setup_and_recovery_until_they_finish() {
    let f = Fixture::new();
    let guard = background_guard(&f.config, &f.store).unwrap().unwrap();
    assert!(background_guard(&f.config, &f.store).unwrap().is_none());
    assert!(initialization_guard(&f.config).unwrap().is_none());
    assert!(prepare_key(&f.config, Some("owner"), Some("test")).is_err());
    f.store
        .submit_input_as_event("capture remains available", "capture", None)
        .unwrap();
    assert_eq!(f.store.list_events().unwrap().len(), 1);
    drop(guard);
    let recovery = initialization_guard(&f.config).unwrap().unwrap();
    assert!(background_guard(&f.config, &f.store).unwrap().is_none());
    drop(recovery);
    assert!(prepare_key(&f.config, Some("owner"), Some("test")).is_ok());
}

#[test]
fn stale_request_release_cannot_close_a_newer_unlocked_session() {
    let f = Fixture::new();
    let first = prepare_key(&f.config, Some("owner"), Some("test")).unwrap();
    let key = ContentKey::generate().unwrap().to_bytes();
    let commit = install_key(&f.config, &first.token, key.to_vec(), 1).unwrap();
    confirm_key(&f.config, &commit.token, 1).unwrap();
    assert!(release_request(&f.config, &first.token));
    let second = prepare_key(&f.config, None, None).unwrap();
    let commit = install_key(&f.config, &second.token, key.to_vec(), 1).unwrap();
    confirm_key(&f.config, &commit.token, 1).unwrap();
    assert!(!release_request(&f.config, &first.token));
    assert!(unlocked(&f.config, 1));
    assert!(release_request(&f.config, &second.token));
    assert!(!unlocked(&f.config, 1));
}

#[test]
fn expired_key_request_never_creates_an_identity() {
    let f = Fixture::new();
    let request = prepare_key(&f.config, Some("owner"), Some("test")).unwrap();
    sessions()
        .lock()
        .unwrap()
        .get_mut(&f.config.data_dir)
        .unwrap()
        .expires_at = 0;
    assert!(install_key(&f.config, &request.token, vec![1; 32], 1).is_err());
    assert!(f.store.sync_identity().unwrap().is_none());
    let replacement = prepare_key(&f.config, Some("owner"), Some("test")).unwrap();
    assert_ne!(replacement.token, request.token);
    assert_ne!(replacement.installation_id, request.installation_id);
}

#[test]
fn initial_setup_waits_for_inflight_work_and_bridge_reinit_does_not_steal_sync_jobs() {
    let _env = crate::config::test_env_guard();
    crate::config::unpin();
    let f = Fixture::new();
    f.store
        .submit_input_as_event("in flight", "capture", None)
        .unwrap();
    let job = f.store.claim_analysis_job().unwrap().unwrap();
    assert!(prepare_key(&f.config, Some("owner"), Some("test")).is_err());
    f.store.fail_analysis(&job, "synthetic failure").unwrap();
    let (_, commit) = f.initialize();
    confirm_key(&f.config, &commit.token, 1).unwrap();
    f.store
        .connection
        .execute(
            "UPDATE analysis_jobs SET available_at='2000-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
    let _running = f.store.claim_analysis_job().unwrap().unwrap();
    let guard = background_guard(&f.config, &f.store).unwrap().unwrap();
    lock(&f.config);
    crate::api::init_bridge(Some(f.config.data_dir.to_string_lossy().into())).unwrap();
    assert_eq!(f.store.analysis_job_stats().unwrap().running, 1);
    assert!(matches!(
        crate::api::trigger_analysis().unwrap(),
        crate::api::AnalysisTriggerResult::Processed { count: 0 }
    ));
    assert_eq!(f.store.analysis_job_stats().unwrap().running, 1);
    drop(guard);
    crate::config::unpin();
}
