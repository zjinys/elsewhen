use super::*;
use crate::{event::NewEvent, storage::sync::ReplicaIdentity};

struct TempDirectory(std::path::PathBuf);
impl TempDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("elsewhen-reading-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Fixture {
    store: Store,
    directory: TempDirectory,
    batch: DigestBatch,
    task: ReadingTask,
}

impl Fixture {
    fn new(sync: bool) -> Self {
        let directory = TempDirectory::new();
        let store = Store::open(&directory.path().join("test.db")).unwrap();
        if sync {
            store
                .initialize_sync_replica(&ReplicaIdentity {
                    dataset_id: "dataset".into(),
                    device_id: "a".into(),
                    stream_id: "stream-a".into(),
                    processor_device_id: "a".into(),
                })
                .unwrap();
        }
        let id = store
            .insert_event(NewEvent::now("首🙂middle否定结尾"))
            .unwrap();
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
            directory,
            batch,
            task,
        }
    }

    fn prepare(&self, start: usize, end: usize) -> ReadingNode {
        self.store
            .prepare_event_reading_node(&self.task.id, start, end, &[])
            .unwrap()
    }

    fn lease(&self, now: i64) -> ReadingLease {
        self.store
            .claim_event_reading_node(&self.task.id, now)
            .unwrap()
            .unwrap()
    }

    fn output(&self, start: usize, end: usize) -> ReadingOutput {
        ReadingOutput {
            summary: "生成摘要，保留否定与顺序".into(),
            quotes: vec![ReadingQuote {
                start,
                end,
                text: self.batch.events[0]
                    .raw_text
                    .chars()
                    .skip(start)
                    .take(end - start)
                    .collect(),
            }],
        }
    }
}

#[test]
fn unicode_ranges_are_contiguous_idempotent_and_do_not_acknowledge_event() {
    let f = Fixture::new(false);
    let a = f.prepare(0, 2);
    assert_eq!(a.id, f.prepare(0, 2).id);
    assert!(
        f.store
            .prepare_event_reading_node(&f.task.id, 3, 4, &[])
            .is_err()
    );
    assert!(
        f.store
            .prepare_event_reading_node(&f.task.id, 1, 4, &[])
            .is_err()
    );
    assert!(
        f.store
            .prepare_event_reading_node(&f.task.id, 2, 99, &[])
            .is_err()
    );
    let b = f.prepare(2, f.task.total_chars);
    assert_eq!(
        f.store.ensure_event_reading(&f.batch).unwrap().id,
        f.task.id
    );
    let lease = f.lease(1000);
    assert_eq!(lease.node.id, a.id);
    let output = f.output(0, 2);
    f.store
        .complete_event_reading_node(&lease, &output, 1001)
        .unwrap();
    f.store
        .complete_event_reading_node(&lease, &output, 1002)
        .unwrap();
    let mut conflict = output.clone();
    conflict.summary = "different".into();
    assert!(
        f.store
            .complete_event_reading_node(&lease, &conflict, 1003)
            .is_err()
    );
    assert!(
        f.store
            .connection
            .execute(
                "UPDATE event_reading_nodes SET output_json='{}' WHERE id=?1",
                [&a.id]
            )
            .is_err()
    );
    assert_eq!(f.store.event_reading_nodes(&f.task.id).unwrap().len(), 2);
    assert_eq!(f.lease(1004).node.id, b.id);
    assert_eq!(f.store.digest_job_stats().unwrap().running, 1);
    assert_eq!(f.store.list_events().unwrap().len(), 1);
    assert_eq!(
        f.store.list_events().unwrap()[0].raw_text,
        f.batch.events[0].raw_text
    );
    let pages: i64 = f
        .store
        .connection
        .query_row("SELECT COUNT(*) FROM wiki_pages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(pages, 0);
}

#[test]
fn restart_keeps_healthy_lease_then_recovers_expired_without_repeating_completed_nodes() {
    let f = Fixture::new(false);
    let first = f.prepare(0, 2);
    f.prepare(2, f.task.total_chars);
    let lease = f.lease(1000);
    let reopened = Store::open_background(&f.directory.path().join("test.db"))
        .unwrap()
        .unwrap();
    assert!(
        reopened
            .claim_event_reading_node(&f.task.id, 1001)
            .unwrap()
            .is_none()
    );
    reopened.renew_event_reading_lease(&lease, 1500).unwrap();
    assert!(
        f.store
            .claim_event_reading_node(&f.task.id, 1601)
            .unwrap()
            .is_none()
    );
    let recovered = f.lease(2100);
    assert_eq!(recovered.node.id, first.id);
    assert!(recovered.generation > lease.generation);
    assert!(
        f.store
            .complete_event_reading_node(&lease, &f.output(0, 2), 2101)
            .is_err()
    );
    assert!(f.store.renew_event_reading_lease(&lease, 2101).is_err());
    reopened
        .complete_event_reading_node(&recovered, &f.output(0, 2), 2101)
        .unwrap();
    let next = f.lease(2102);
    assert_ne!(next.node.id, first.id);
    assert!(f.store.connection.is_autocommit());
    assert!(reopened.connection.is_autocommit());
}

#[test]
fn node_failures_back_off_independently_and_invalid_output_is_not_cached() {
    let f = Fixture::new(false);
    f.prepare(0, 2);
    let lease = f.lease(1000);
    let mut forged = f.output(0, 2);
    forged.quotes[0].text = "伪造".into();
    assert!(
        f.store
            .complete_event_reading_node(&lease, &forged, 1001)
            .is_err()
    );
    assert!(
        f.store.event_reading_nodes(&f.task.id).unwrap()[0]
            .output
            .is_none()
    );
    f.store
        .fail_event_reading_node(&lease, "invalid quote", 1002)
        .unwrap();
    assert!(
        f.store
            .claim_event_reading_node(&f.task.id, 1121)
            .unwrap()
            .is_none()
    );
    let next = f.lease(1122);
    assert!(next.generation > lease.generation);
    assert!(
        f.store
            .fail_event_reading_node(&lease, "late failure", 1123)
            .is_err()
    );
    f.store
        .complete_event_reading_node(&next, &f.output(0, 2), 1123)
        .unwrap();
    let attempts: i64 = f
        .store
        .connection
        .query_row("SELECT attempts FROM knowledge_digest_jobs", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(attempts, 1);
}

#[test]
fn reclassification_even_when_reversed_fences_old_result_and_reuses_immutable_cache() {
    let f = Fixture::new(false);
    f.prepare(0, 2);
    let lease = f.lease(1000);
    f.store
        .set_event_recordability(&f.task.event_id, false, "discussion")
        .unwrap();
    assert!(
        f.store
            .complete_event_reading_node(&lease, &f.output(0, 2), 1001)
            .is_err()
    );
    f.store
        .set_event_recordability(&f.task.event_id, true, "event")
        .unwrap();
    assert!(
        f.store
            .complete_event_reading_node(&lease, &f.output(0, 2), 1002)
            .is_err()
    );
    let resumed = f.store.ensure_event_reading(&f.batch).unwrap();
    assert_eq!(resumed.id, f.task.id);
    let current = f.lease(1003);
    assert!(
        f.store
            .complete_event_reading_node(&lease, &f.output(0, 2), 1004)
            .is_err()
    );
    f.store
        .complete_event_reading_node(&current, &f.output(0, 2), 1004)
        .unwrap();
}

#[test]
fn membership_revocation_and_processor_round_trip_reject_late_results() {
    for sql in [
        "UPDATE sync_members SET active=0; UPDATE sync_members SET active=1;",
        "UPDATE sync_replica SET processor_device_id='b'; UPDATE sync_replica SET processor_device_id='a';",
        "INSERT INTO sync_processor_state(singleton,state) VALUES(1,'relinquished'); UPDATE sync_processor_state SET state='active';",
    ] {
        let f = Fixture::new(true);
        f.prepare(0, 2);
        let lease = f.lease(1000);
        f.store.connection.execute_batch(sql).unwrap();
        assert!(f.store.background_processing_allowed().unwrap());
        assert!(
            f.store
                .complete_event_reading_node(&lease, &f.output(0, 2), 1001)
                .is_err()
        );
        assert!(f.store.renew_event_reading_lease(&lease, 1001).is_err());
        assert!(
            f.store.event_reading_nodes(&f.task.id).unwrap()[0]
                .output
                .is_none()
        );
    }
    let f = Fixture::new(true);
    f.prepare(0, 2);
    let lease = f.lease(1000);
    f.store
        .connection
        .execute("UPDATE sync_members SET active=0", [])
        .unwrap();
    assert!(
        f.store
            .complete_event_reading_node(&lease, &f.output(0, 2), 1001)
            .is_err()
    );
    assert!(f.store.claim_event_reading_node(&f.task.id, 2000).is_err());
}

#[test]
fn merge_dependencies_bind_complete_contiguous_results_and_rollback_partial_parent() {
    let f = Fixture::new(false);
    let a = f.prepare(0, 2);
    let b = f.prepare(2, 5);
    let c = f.prepare(5, f.task.total_chars);
    assert!(
        f.store
            .prepare_event_reading_node(&f.task.id, 0, 5, &[a.id.clone(), b.id.clone()])
            .is_err()
    );
    for (i, (start, end)) in [(0, 2), (2, 5), (5, f.task.total_chars)]
        .into_iter()
        .enumerate()
    {
        let lease = f.lease(1000 + i as i64);
        f.store
            .complete_event_reading_node(&lease, &f.output(start, end), 1000 + i as i64)
            .unwrap();
    }
    assert!(
        f.store
            .prepare_event_reading_node(
                &f.task.id,
                0,
                f.task.total_chars,
                &[a.id.clone(), c.id.clone()]
            )
            .is_err()
    );
    let parent = f
        .store
        .prepare_event_reading_node(&f.task.id, 0, 5, &[a.id.clone(), b.id.clone()])
        .unwrap();
    assert_eq!(
        parent.id,
        f.store
            .prepare_event_reading_node(&f.task.id, 0, 5, &[a.id.clone(), b.id.clone()])
            .unwrap()
            .id
    );
    // Insertion of the alternate parent succeeds first, then its first already
    // consumed dependency violates uniqueness. The parent must roll back too.
    assert!(
        f.store
            .prepare_event_reading_node(&f.task.id, 0, f.task.total_chars, &[a.id, b.id, c.id])
            .is_err()
    );
    assert_eq!(f.store.event_reading_nodes(&f.task.id).unwrap().len(), 4);
    let deps: i64 = f
        .store
        .connection
        .query_row("SELECT COUNT(*) FROM event_reading_dependencies", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(deps, 2);
}

#[test]
fn v77_upgrade_preserves_events_jobs_bundles_and_background_does_not_migrate() {
    let dir = TempDirectory::new();
    let path = dir.path().join("old.db");
    let mut conn = rusqlite::Connection::open(&path).unwrap();
    super::super::new_migrations::initialize_test_version(&mut conn, 77).unwrap();
    conn.execute("INSERT INTO events(id,raw_text,occurred_at,recorded_at,source,status,created_at,updated_at) VALUES('old','原文🙂','t','t','test','pending','t','t')",[]).unwrap();
    conn.execute("INSERT INTO prompt_bundles VALUES('old-bundle','background_task','old-digest','zh-Hans','zh-Hans','{}','zh-Hans',1,'old','old','old',?1,'t')",["a".repeat(64)]).unwrap();
    conn.execute("INSERT INTO knowledge_digest_jobs(id,event_id,digest_version,status,available_at,created_at,updated_at,prompt_bundle_id) VALUES('old-job','old',?1,'retry','t','t','t','old-bundle')",[super::super::DIGEST_VERSION]).unwrap();
    drop(conn);
    assert!(Store::open_background(&path).unwrap().is_none());
    let conn = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 77);
    drop(conn);
    let store = Store::open(&path).unwrap();
    let text: String = store
        .connection
        .query_row("SELECT raw_text FROM events WHERE id='old'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(text, "原文🙂");
    let job: (String, String) = store
        .connection
        .query_row(
            "SELECT status,prompt_bundle_id FROM knowledge_digest_jobs WHERE id='old-job'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(job, ("retry".into(), "old-bundle".into()));
    let hash: String = store
        .connection
        .query_row(
            "SELECT bundle_hash FROM prompt_bundles WHERE id='old-bundle'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hash, "a".repeat(64));
    let count: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM event_reading_tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn settings_changes_do_not_replace_task_bundle_or_completed_results() {
    let f = Fixture::new(false);
    f.prepare(0, 2);
    let lease = f.lease(1000);
    f.store
        .complete_event_reading_node(&lease, &f.output(0, 2), 1001)
        .unwrap();
    let before = f.store.event_reading_nodes(&f.task.id).unwrap();
    let original = f
        .store
        .prompt_from_bundle(&f.task.bundle_id, "reading.policy")
        .unwrap();
    let preferences = f.store.prompt_language_preferences().unwrap();
    f.store
        .save_prompt_language_preferences(Some("en"), "fixed", Some("en"), preferences.generation)
        .unwrap();
    let current = f.store.ensure_event_reading(&f.batch).unwrap();
    assert_eq!(current.id, f.task.id);
    assert_eq!(current.bundle_id, f.task.bundle_id);
    assert_eq!(
        original,
        f.store
            .prompt_from_bundle(&current.bundle_id, "reading.policy")
            .unwrap()
    );
    assert_eq!(
        before[0].output,
        f.store.event_reading_nodes(&current.id).unwrap()[0].output
    );
    assert!(
        f.store
            .connection
            .execute(
                "UPDATE event_reading_tasks SET bundle_id='missing' WHERE id=?1",
                [&current.id]
            )
            .is_err()
    );
}

#[test]
fn failed_commit_rolls_back_node_and_lease_release_together() {
    let f = Fixture::new(false);
    f.prepare(0, 2);
    let lease = f.lease(1000);
    f.store.connection.execute_batch("CREATE TEMP TRIGGER fail_reading_task_commit BEFORE UPDATE OF lease_token ON event_reading_tasks WHEN NEW.lease_token IS NULL BEGIN SELECT RAISE(ABORT,'injected_disk_error'); END;").unwrap();
    assert!(
        f.store
            .complete_event_reading_node(&lease, &f.output(0, 2), 1001)
            .is_err()
    );
    assert!(
        f.store.event_reading_nodes(&f.task.id).unwrap()[0]
            .output
            .is_none()
    );
    assert!(
        f.store
            .claim_event_reading_node(&f.task.id, 1002)
            .unwrap()
            .is_none()
    );
    f.store
        .connection
        .execute_batch("DROP TRIGGER fail_reading_task_commit;")
        .unwrap();
    f.store
        .complete_event_reading_node(&lease, &f.output(0, 2), 1003)
        .unwrap();
}

#[test]
fn lease_cannot_relabel_a_node_range_and_sync_enable_waits_for_reading_call() {
    let f = Fixture::new(false);
    f.prepare(0, 2);
    let lease = f.lease(1000);
    let mut forged = lease.clone();
    forged.node.end = 3;
    assert!(
        f.store
            .complete_event_reading_node(&forged, &f.output(0, 3), 1001)
            .is_err()
    );
    // Independently isolate the reading lease from the original digest batch.
    f.store
        .connection
        .execute("UPDATE knowledge_digest_jobs SET status='retry'", [])
        .unwrap();
    f.store
        .connection
        .execute("UPDATE knowledge_digest_runs SET status='failed'", [])
        .unwrap();
    assert!(
        f.store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "dataset".into(),
                device_id: "a".into(),
                stream_id: "stream-a".into(),
                processor_device_id: "a".into()
            })
            .is_err()
    );
    assert!(f.store.sync_identity().unwrap().is_none());
}

#[test]
fn normal_sync_clocks_do_not_revoke_a_reading_lease() {
    let f = Fixture::new(true);
    f.prepare(0, 2);
    let lease = f.lease(1000);
    f.store
        .connection
        .execute(
            "UPDATE sync_replica SET next_sequence=next_sequence+1,logical_clock=logical_clock+1",
            [],
        )
        .unwrap();
    f.store
        .connection
        .execute("UPDATE sync_members SET active=1", [])
        .unwrap();
    f.store
        .complete_event_reading_node(&lease, &f.output(0, 2), 1001)
        .unwrap();
}

#[path = "finalization_tests.rs"]
mod finalization_tests;
