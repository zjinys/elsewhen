use super::*;
use crate::{
    event::NewEvent,
    storage::{GoalPhase, sync::ReplicaIdentity},
};

struct Fixture {
    store: Store,
    dir: std::path::PathBuf,
    goal: Goal,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("compass-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        let goal = store
            .create_goal_with_facets(
                "访谈客户",
                GoalPhase::Near,
                None,
                Some("本周完成三次用户访谈"),
                None,
                Some("获得三份访谈记录"),
            )
            .unwrap();
        let at = (Utc::now() - Duration::days(6)).to_rfc3339();
        store
            .connection
            .execute(
                "UPDATE goals SET updated_at=?2 WHERE id=?1",
                params![goal.id, at],
            )
            .unwrap();
        let goal = store.get_goal(&goal.id).unwrap().unwrap();
        Self { store, dir, goal }
    }
    fn event(&self, text: &str, days: i64) -> String {
        let at = Utc::now() - Duration::days(days);
        self.store
            .insert_event(NewEvent {
                raw_text: text,
                occurred_at: at,
                recorded_at: at,
                source: "capture",
            })
            .unwrap()
    }
    fn ready(&self) -> CompassLease {
        self.event("今天又调整了网页配色，没有联系任何客户", 2);
        self.event("今天重新做了图标，访谈还没有开始", 1);
        self.store
            .claim_compass_assessment(Utc::now())
            .unwrap()
            .unwrap()
    }
    fn verdict(&self, lease: &CompassLease) -> CompassVerdict {
        CompassVerdict {
            classification: "drifting".into(),
            confidence: 0.9,
            reason: "连续两天记录了其他工作，访谈尚未开始".into(),
            question: "当前计划还合适吗？".into(),
            citations: lease
                .input
                .evidence
                .iter()
                .take(2)
                .map(|e| CompassCitation {
                    source_id: e.source_id.clone(),
                    quote: e.excerpt.clone(),
                })
                .collect(),
            recommendations: vec!["一起检查这周的优先级".into()],
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn same_input_dedupes_clock_changes_and_survives_restart() {
    let f = Fixture::new();
    let lease = f.ready();
    assert!(
        f.store
            .claim_compass_assessment(Utc::now())
            .unwrap()
            .is_none()
    );
    let mut verdict = f.verdict(&lease);
    verdict.classification = "aligned".into();
    f.store
        .complete_compass_assessment(&lease, &verdict, None, None, Utc::now().timestamp())
        .unwrap();
    let store = Store::open(&f.dir.join("test.db")).unwrap();
    assert!(
        store
            .claim_compass_assessment(Utc::now() + Duration::hours(7))
            .unwrap()
            .is_none()
    );
    let count: i64 = store
        .connection
        .query_row("SELECT count(*) FROM goal_compass_assessments", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn progress_feedback_is_durable_and_prevents_same_basis_drift() {
    let f = Fixture::new();
    let lease = f.ready();
    let verdict = f.verdict(&lease);
    f.store
        .complete_compass_assessment(&lease, &verdict, None, None, Utc::now().timestamp())
        .unwrap();
    let nudge = f.store.get_active_compass_nudge().unwrap().unwrap();
    f.store
        .compass_feedback(&nudge.id, "still_progressing", "访谈在线下进行")
        .unwrap();
    assert!(f.store.get_active_compass_nudge().unwrap().is_none());
    let reopened = Store::open(&f.dir.join("test.db")).unwrap();
    let input = reopened.build_compass_input(&f.goal, Utc::now()).unwrap();
    assert_eq!(input.feedback.len(), 1);
    assert_eq!(input.feedback[0].note, "访谈在线下进行");
    assert!(verdict.validate(&input).is_err());
    assert!(
        f.store
            .compass_feedback(&nudge.id, "still_progressing", "")
            .is_err()
    );
    assert_eq!(
        f.store.get_goal(&f.goal.id).unwrap().unwrap().current_focus,
        f.goal.current_focus
    );
}

#[test]
fn goal_change_and_aba_ownership_fence_old_network_response() {
    let f = Fixture::new();
    let lease = f.ready();
    let verdict = f.verdict(&lease);
    f.store
        .update_goal_focus(&f.goal.id, Some("先完成一场访谈"), None)
        .unwrap();
    assert!(
        f.store
            .complete_compass_assessment(&lease, &verdict, None, None, Utc::now().timestamp())
            .is_err()
    );
    assert!(f.store.get_active_compass_nudge().unwrap().is_none());
    let f = Fixture::new();
    let lease = f.ready();
    // A sync enable must first wait for this independent background lease.
    assert!(
        f.store
            .initialize_sync_replica(&ReplicaIdentity {
                dataset_id: "dataset".into(),
                device_id: "a".into(),
                stream_id: "a-stream".into(),
                processor_device_id: "a".into()
            })
            .is_err()
    );
}

#[test]
fn clipped_and_single_day_evidence_cannot_make_drift() {
    let f = Fixture::new();
    let lease = f.ready();
    let mut verdict = f.verdict(&lease);
    verdict.citations[1].quote = "模型编造的内容".into();
    assert!(verdict.validate(&lease.input).is_err());
    let verdict = f.verdict(&lease);
    let mut input = lease.input.clone();
    input.evidence[1].day = input.evidence[0].day.clone();
    assert!(verdict.validate(&input).is_err());
    input = lease.input.clone();
    input.has_more_evidence = true;
    assert!(verdict.validate(&input).is_err());
    let f = Fixture::new();
    f.event(&"很长的记录".repeat(1000), 2);
    f.event("今天没有联系客户", 1);
    let input = f.store.build_compass_input(&f.goal, Utc::now()).unwrap();
    assert!(input.has_more_evidence);
    assert!(input.evidence.iter().any(|e| e.total_chars > e.excerpt_end));
}

#[test]
fn stale_failure_cannot_release_reclaimed_lease_and_failed_tasks_back_off() {
    let f = Fixture::new();
    let lease = f.ready();
    let now = Utc::now();
    f.store
        .fail_compass_assessment(&lease, "synthetic", now.timestamp())
        .unwrap();
    assert!(f.store.claim_compass_assessment(now).unwrap().is_none());
    let retry = f
        .store
        .claim_compass_assessment(now + Duration::hours(7))
        .unwrap()
        .unwrap();
    assert_eq!(retry.id, lease.id);
    assert_ne!(retry.token, lease.token);
    f.store
        .fail_compass_assessment(&lease, "old-response", now.timestamp())
        .unwrap();
    let status: String = f
        .store
        .connection
        .query_row(
            "SELECT status FROM goal_compass_assessments WHERE id=?1",
            [retry.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "running");
}

#[test]
fn discussion_does_not_dismiss_or_realign_and_result_is_immutable() {
    let f = Fixture::new();
    let lease = f.ready();
    let verdict = f.verdict(&lease);
    f.store
        .complete_compass_assessment(&lease, &verdict, None, None, Utc::now().timestamp())
        .unwrap();
    let nudge = f.store.get_active_compass_nudge().unwrap().unwrap();
    f.store.compass_feedback(&nudge.id, "discuss", "").unwrap();
    assert_eq!(
        f.store.get_active_compass_nudge().unwrap().unwrap().id,
        nudge.id
    );
    assert!(
        f.store
            .connection
            .execute(
                "UPDATE goal_compass_assessments SET output_json='{}' WHERE id=?1",
                [lease.id]
            )
            .is_err()
    );
}

#[test]
fn eligible_old_retry_does_not_charge_new_input_before_six_hours() {
    let f = Fixture::new();
    let lease = f.ready();
    f.store
        .fail_compass_assessment(
            &lease,
            "synthetic",
            (Utc::now() - Duration::hours(7)).timestamp(),
        )
        .unwrap();
    f.event("新增记录：正在整理配色", 0);
    assert!(
        f.store
            .claim_compass_assessment(Utc::now())
            .unwrap()
            .is_none()
    );
    let next = f
        .store
        .claim_compass_assessment(Utc::now() + Duration::hours(7))
        .unwrap()
        .unwrap();
    assert_ne!(next.id, lease.id);
}

#[test]
fn completion_rolls_back_the_result_when_card_insert_fails() {
    let f = Fixture::new();
    let lease = f.ready();
    let verdict = f.verdict(&lease);
    f.store.connection.execute_batch("CREATE TRIGGER synthetic_card_failure BEFORE INSERT ON goal_compass_nudges BEGIN SELECT RAISE(ABORT,'synthetic'); END;").unwrap();
    assert!(
        f.store
            .complete_compass_assessment(&lease, &verdict, None, None, Utc::now().timestamp())
            .is_err()
    );
    let status: String = f
        .store
        .connection
        .query_row(
            "SELECT status FROM goal_compass_assessments WHERE id=?1",
            [&lease.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "running");
    assert!(f.store.get_active_compass_nudge().unwrap().is_none());
    f.store
        .connection
        .execute_batch("DROP TRIGGER synthetic_card_failure;")
        .unwrap();
    f.store
        .complete_compass_assessment(&lease, &verdict, None, None, Utc::now().timestamp())
        .unwrap();
    assert!(f.store.get_active_compass_nudge().unwrap().is_some());
    f.event("今天已经完成两场访谈，正在整理结果", 0);
    assert!(f.store.get_active_compass_nudge().unwrap().is_none());
}

#[test]
fn expired_lease_and_ownership_aba_reject_delayed_results() {
    let f = Fixture::new();
    f.store
        .initialize_sync_replica(&ReplicaIdentity {
            dataset_id: "test".into(),
            device_id: "a".into(),
            stream_id: "a-stream".into(),
            processor_device_id: "a".into(),
        })
        .unwrap();
    let lease = f.ready();
    let verdict = f.verdict(&lease);
    f.store
        .connection
        .execute("UPDATE sync_replica SET processor_device_id='b'", [])
        .unwrap();
    f.store
        .connection
        .execute("UPDATE sync_replica SET processor_device_id='a'", [])
        .unwrap();
    assert!(
        f.store
            .complete_compass_assessment(&lease, &verdict, None, None, Utc::now().timestamp())
            .is_err()
    );
    let f = Fixture::new();
    let lease = f.ready();
    let retry = f
        .store
        .claim_compass_assessment(Utc::now() + Duration::seconds(LEASE_SECONDS + 1))
        .unwrap()
        .unwrap();
    assert_eq!(retry.id, lease.id);
    assert_ne!(retry.token, lease.token);
    assert!(
        f.store
            .complete_compass_assessment(
                &lease,
                &f.verdict(&lease),
                None,
                None,
                Utc::now().timestamp()
            )
            .is_err()
    );
}

#[test]
fn linked_capture_has_one_authority_and_excludes_ai_and_notebook() {
    let f = Fixture::new();
    let conversation = f.store.create_conversation(Some("test"), None).unwrap();
    let event = f.event("普通聊天，不能当成偏离事实", 1);
    let message = f
        .store
        .send_message(&conversation, "user", "普通聊天，不能当成偏离事实", None)
        .unwrap();
    let now = Utc::now().to_rfc3339();
    f.store.connection.execute("INSERT INTO input_records(id,raw_text,source,event_id,message_id,created_at,updated_at) VALUES('linked','普通聊天','capture',?1,?2,?3,?3)",params![event,message,now]).unwrap();
    f.store
        .send_message(&conversation, "assistant", "AI编造的访谈进展", None)
        .unwrap();
    let memo = f
        .store
        .send_message(&conversation, "user", "笔记中的案例不是亲身进展", None)
        .unwrap();
    f.store.connection.execute("INSERT INTO input_records(id,raw_text,source,message_id,created_at,updated_at) VALUES('memo','笔记','notebook',?1,?2,?2)",params![memo,now]).unwrap();
    let input = f.store.build_compass_input(&f.goal, Utc::now()).unwrap();
    assert_eq!(input.evidence.len(), 1);
    assert_eq!(input.evidence[0].source_id, event);
    f.store.connection.execute("INSERT INTO event_recordability_decisions(id,event_id,recordable,kind,reason,created_at) VALUES('decision',?1,0,'chitchat','test',?2)",params![event,now]).unwrap();
    assert!(
        f.store
            .build_compass_input(&f.goal, Utc::now())
            .unwrap()
            .evidence
            .is_empty()
    );
}

#[test]
fn reset_clears_derived_assessments_feedback_and_plan_context_is_captured() {
    let f = Fixture::new();
    let plan = f
        .store
        .create_plan("验证需求", "找到第一个客户", r#"["客户愿意付费"]"#)
        .unwrap();
    f.store
        .attach_goal_to_plan(&f.goal.id, &plan.id, None)
        .unwrap();
    f.store
        .connection
        .execute(
            "UPDATE goals SET updated_at=?2 WHERE id=?1",
            params![f.goal.id, (Utc::now() - Duration::days(6)).to_rfc3339()],
        )
        .unwrap();
    let lease = f.ready();
    assert!(
        lease
            .input
            .goal
            .plan_context
            .as_ref()
            .unwrap()
            .contains("客户愿意付费")
    );
    f.store
        .complete_compass_assessment(
            &lease,
            &f.verdict(&lease),
            None,
            None,
            Utc::now().timestamp(),
        )
        .unwrap();
    let nudge = f.store.get_active_compass_nudge().unwrap().unwrap();
    f.store.compass_feedback(&nudge.id, "snooze", "").unwrap();
    f.store.clear_goal_workspace_data().unwrap();
    for table in [
        "goal_compass_nudges",
        "goal_compass_feedback",
        "goal_compass_assessments",
    ] {
        let count: i64 = f
            .store
            .connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
}

#[test]
fn late_captured_events_preserve_occurrence_time_and_cannot_fabricate_two_days() {
    let f = Fixture::new();
    let occurred_at = Utc::now() - Duration::days(2);
    for (i, text) in ["昨天调了配色，尚未开始访谈", "昨天又做了图标，仍未联系客户"]
        .iter()
        .enumerate()
    {
        f.store
            .insert_event(NewEvent {
                raw_text: text,
                occurred_at,
                recorded_at: Utc::now() - Duration::days(i as i64),
                source: "capture",
            })
            .unwrap();
    }
    let lease = f
        .store
        .claim_compass_assessment(Utc::now())
        .unwrap()
        .unwrap();
    assert_eq!(
        lease.input.evidence[0].occurred_at,
        occurred_at.to_rfc3339()
    );
    assert_eq!(lease.input.evidence[0].day, lease.input.evidence[1].day);
    assert!(f.verdict(&lease).validate(&lease.input).is_err());
}
