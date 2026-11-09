use super::*;

#[test]
fn final_receipt_cannot_cross_processor_or_membership_generation() {
    for sql in [
        "UPDATE sync_replica SET processor_device_id='b';UPDATE sync_replica SET processor_device_id='a';",
        "UPDATE sync_members SET active=0;UPDATE sync_members SET active=1;",
    ] {
        let f = Fixture::new(true);
        f.prepare(0, f.task.total_chars);
        let now = chrono::Utc::now().timestamp();
        let node = f.lease(now);
        f.store
            .complete_event_reading_node(&node, &f.output(0, f.task.total_chars), now)
            .unwrap();
        f.store.mark_event_reading_ready(&f.task.id).unwrap();
        let lease = f
            .store
            .claim_event_reading_final(&f.task.id, &f.batch, now)
            .unwrap()
            .unwrap();
        f.store.connection.execute_batch(sql).unwrap();
        assert!(f.store.background_processing_allowed().unwrap());
        assert!(
            f.store
                .renew_event_reading_final(&lease, &f.batch, now)
                .is_err()
        );
        assert!(
            f.store
                .apply_digest_batch_with_reading(&f.batch, &[], Some(&lease))
                .is_err()
        );
        assert_eq!(f.store.digest_job_stats().unwrap().running, 1);
        assert_eq!(
            f.store.event_reading_task(&f.task.id).unwrap().status,
            "ready"
        );
    }
}

#[test]
fn final_recovery_preserves_cached_root_and_rejects_stale_generation_after_restart() {
    let f = Fixture::new(false);
    f.prepare(0, f.task.total_chars);
    let now = chrono::Utc::now().timestamp();
    let node = f.lease(now);
    f.store
        .complete_event_reading_node(&node, &f.output(0, f.task.total_chars), now)
        .unwrap();
    f.store.mark_event_reading_ready(&f.task.id).unwrap();
    let old = f
        .store
        .claim_event_reading_final(&f.task.id, &f.batch, now - 601)
        .unwrap()
        .unwrap();
    let store = Store::open_background(&f.directory.path().join("test.db"))
        .unwrap()
        .unwrap();
    let fresh = store
        .claim_event_reading_final(&f.task.id, &f.batch, now)
        .unwrap()
        .unwrap();
    assert_eq!(fresh.root_id, old.root_id);
    assert_eq!(fresh.root_hash, old.root_hash);
    assert!(
        store
            .apply_digest_batch_with_reading(&f.batch, &[], Some(&old))
            .is_err()
    );
    store
        .apply_digest_batch_with_reading(&f.batch, &[], Some(&fresh))
        .unwrap();
    assert_eq!(store.digest_job_stats().unwrap().succeeded, 1);
    assert_eq!(
        store.event_reading_task(&f.task.id).unwrap().status,
        "succeeded"
    );
    assert!(
        store
            .apply_digest_batch_with_reading(&f.batch, &[], Some(&fresh))
            .is_err()
    );
}

#[test]
fn v78_to_v79_preserves_event_bundle_identity_and_confirmed_node_bytes() {
    let dir = TempDirectory::new();
    let path = dir.path().join("v78.db");
    let mut conn = rusqlite::Connection::open(&path).unwrap();
    super::super::super::new_migrations::initialize_test_version(&mut conn, 78).unwrap();
    conn.execute_batch("INSERT INTO events(id,occurred_at,recorded_at,raw_text,source,status,created_at,updated_at) VALUES('e','t','t','原文🙂','test','pending','t','t');
        INSERT INTO prompt_bundles VALUES('bundle','background_task','digest','en','en','{}','en',1,'old','old','old','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','t');
        INSERT INTO knowledge_digest_jobs(id,event_id,digest_version,status,available_at,created_at,updated_at,prompt_bundle_id) VALUES('j','e','wiki-digest-v2','pending','t','t','t','bundle');
        INSERT INTO event_reading_tasks(id,event_id,digest_version,content_hash,total_chars,bundle_id,strategy,dependency_hash,created_at,updated_at) VALUES('task','e','wiki-digest-v2','hash',3,'bundle','event-reading-v1','dependency',1,2);
        INSERT INTO event_reading_nodes(id,task_id,kind,level,start_char,end_char,input_hash,status,output_json,output_hash,receipt_token,receipt_generation) VALUES('node','task','leaf',0,0,3,'input','succeeded','{\"summary\":\"summary\",\"quotes\":[]}','output','token',1);").unwrap();
    super::super::super::new_migrations::initialize(&mut conn).unwrap();
    let row:(String,String,String,String,i64)=conn.query_row("SELECT e.raw_text,t.bundle_id,t.dependency_hash,n.output_json,t.available_at FROM event_reading_tasks t JOIN events e ON e.id=t.event_id JOIN event_reading_nodes n ON n.task_id=t.id",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
    assert_eq!(
        row,
        (
            "原文🙂".into(),
            "bundle".into(),
            "dependency".into(),
            "{\"summary\":\"summary\",\"quotes\":[]}".into(),
            0
        )
    );
    assert_eq!(
        conn.query_row::<String, _, _>("SELECT receipt_token FROM event_reading_nodes", [], |r| r
            .get(0))
            .unwrap(),
        "token"
    );
}
