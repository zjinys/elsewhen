use super::*;

struct TestDirectory(std::path::PathBuf);
impl TestDirectory {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> (TestDirectory, Store, String, Vec<String>) {
    let dir = TestDirectory(
        std::env::temp_dir().join(format!("elsewhen-queue-{}", uuid::Uuid::new_v4())),
    );
    std::fs::create_dir_all(dir.path()).unwrap();
    let store = Store::open(&dir.path().join("queue.db")).unwrap();
    let conversation = store.create_conversation(Some("queue"), None).unwrap();
    let inputs = (0..3)
        .map(|i| {
            store
                .send_message(&conversation, "user", &format!("input {i}"), None)
                .unwrap()
        })
        .collect();
    (dir, store, conversation, inputs)
}

#[test]
fn fifo_and_stable_input_identity_survive_reopening() {
    let (dir, store, conversation, inputs) = fixture();
    let first = store.enqueue_chat_input(&conversation, &inputs[0]).unwrap();
    assert_eq!(
        first,
        store.enqueue_chat_input(&conversation, &inputs[0]).unwrap()
    );
    store.enqueue_chat_input(&conversation, &inputs[1]).unwrap();
    // enqueue 只登记资格、不取得派发权：从未 resume 过的队列按显式暂停语义
    // 对待（dispatch 行由 enqueue 创建为 paused=1），仍需显式继续。
    assert!(
        store
            .claim_next_chat_input(&conversation, "process", "worker")
            .unwrap()
            .is_none()
    );
    store.resume_chat_queue(&conversation, "process").unwrap();
    let run = store
        .claim_next_chat_input(&conversation, "process", "worker")
        .unwrap()
        .unwrap();
    assert_eq!(run.input_message_id, inputs[0]);
    assert!(
        store
            .claim_next_chat_input(&conversation, "process", "worker2")
            .unwrap()
            .is_none()
    );
    store
        .finish_chat_run(&run.id, "worker", "completed", Some("done"), None)
        .unwrap();
    drop(store);
    // 重开（模拟重启换进程身份）：租约归属旧进程，但队列未显式暂停，
    // 新进程的第一次领取应直接接管并继续，不要求人工「继续队列」。
    let store = Store::open(&dir.path().join("queue.db")).unwrap();
    let next = store
        .claim_next_chat_input(&conversation, "new-process", "worker2")
        .unwrap()
        .unwrap();
    assert_eq!(next.input_message_id, inputs[1]);
    let snapshot = store.snapshot_for_input(&conversation, &inputs[1]).unwrap();
    snapshot
        .validate_chat_run_context(&next.id, &conversation)
        .unwrap();
    let context = snapshot.list_messages(&conversation).unwrap();
    assert_eq!(
        context
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>(),
        vec!["input 0", "done", "input 1"]
    );
    let queue = store.list_chat_input_queue(&conversation, 0, 100).unwrap();
    assert_eq!(queue.len(), 2);
    assert_eq!(queue[0].run_id.as_deref(), Some(run.id.as_str()));
    assert_eq!(
        store
            .list_messages(&conversation)
            .unwrap()
            .iter()
            .filter(|m| m.role == "user")
            .count(),
        3
    );
}

#[test]
fn three_queued_inputs_restore_answers_to_their_own_turns() {
    let (_dir, store, conversation, inputs) = fixture();
    for input in &inputs {
        store.enqueue_chat_input(&conversation, input).unwrap();
    }
    store.resume_chat_queue(&conversation, "process").unwrap();
    for answer in ["answer 0", "answer 1"] {
        let run = store
            .claim_next_chat_input(&conversation, "process", "worker")
            .unwrap()
            .unwrap();
        store
            .finish_chat_run(&run.id, "worker", "completed", Some(answer), None)
            .unwrap();
    }
    store
        .send_message(&conversation, "user", "future input", None)
        .unwrap();
    store
        .send_message(&conversation, "assistant", "unrelated late answer", None)
        .unwrap();
    let originals = store.list_messages(&conversation).unwrap();
    let third = store
        .claim_next_chat_input(&conversation, "process", "worker")
        .unwrap()
        .unwrap();
    let snapshot = store
        .snapshot_for_input(&conversation, &third.input_message_id)
        .unwrap();
    snapshot
        .validate_chat_run_context(&third.id, &conversation)
        .unwrap();
    let context = snapshot.list_messages(&conversation).unwrap();
    assert_eq!(
        context
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>(),
        ["input 0", "answer 0", "input 1", "answer 1", "input 2"]
    );
    crate::storage::chat_summaries::validate_history_path(&context).unwrap();
    assert_eq!(
        store
            .list_messages(&conversation)
            .unwrap()
            .iter()
            .map(crate::storage::chat_summaries::message_basis)
            .collect::<Vec<_>>(),
        originals
            .iter()
            .map(crate::storage::chat_summaries::message_basis)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stop_freezes_dispatch_and_withdrawal_does_not_delete_input() {
    let (_dir, store, conversation, inputs) = fixture();
    store.enqueue_chat_input(&conversation, &inputs[0]).unwrap();
    let second = store.enqueue_chat_input(&conversation, &inputs[1]).unwrap();
    store.resume_chat_queue(&conversation, "process").unwrap();
    let run = store
        .claim_next_chat_input(&conversation, "process", "worker")
        .unwrap()
        .unwrap();
    store.cancel_chat_run(&conversation, &run.id).unwrap();
    store
        .finish_chat_run(&run.id, "worker", "cancelled", None, None)
        .unwrap();
    assert!(store.chat_queue_paused(&conversation, "process").unwrap());
    assert!(
        store
            .claim_next_chat_input(&conversation, "process", "worker2")
            .unwrap()
            .is_none()
    );
    assert!(store.withdraw_chat_input(&conversation, second).unwrap());
    assert!(!store.withdraw_chat_input(&conversation, second).unwrap());
    store.resume_chat_queue(&conversation, "process").unwrap();
    assert!(
        store
            .claim_next_chat_input(&conversation, "process", "worker2")
            .unwrap()
            .is_none()
    );
    assert_eq!(store.list_messages(&conversation).unwrap().len(), 3);
    assert_eq!(
        store.enqueue_chat_input(&conversation, &inputs[1]).unwrap(),
        second
    );
    assert_eq!(
        store.list_chat_input_queue(&conversation, 0, 100).unwrap()[1].status,
        "withdrawn"
    );
}

#[test]
fn invalid_scope_and_failed_claim_leave_pending_eligibility_unchanged() {
    let (_dir, store, conversation, inputs) = fixture();
    let other = store.create_conversation(Some("other"), None).unwrap();
    assert!(store.enqueue_chat_input(&other, &inputs[0]).is_err());
    let id = store.enqueue_chat_input(&conversation, &inputs[0]).unwrap();
    assert!(!store.withdraw_chat_input(&other, id).unwrap());
    store.resume_chat_queue(&conversation, "process").unwrap();
    store.connection.execute_batch("CREATE TRIGGER reject_run BEFORE INSERT ON chat_runs BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
    assert!(
        store
            .claim_next_chat_input(&conversation, "process", "worker")
            .is_err()
    );
    let queue = store.list_chat_input_queue(&conversation, 0, 100).unwrap();
    assert_eq!(queue[0].status, "pending");
    assert!(queue[0].run_id.is_none());
    store
        .connection
        .execute_batch("DROP TRIGGER reject_run;")
        .unwrap();
    let run = store
        .claim_next_chat_input(&conversation, "process", "worker")
        .unwrap()
        .unwrap();
    assert_eq!(run.input_message_id, inputs[0]);
}

#[test]
fn new_inputs_cannot_jump_an_existing_paused_or_ready_queue() {
    let (_dir, store, conversation, inputs) = fixture();
    let first = store
        .schedule_chat_input(&conversation, &inputs[0], "process", "worker")
        .unwrap()
        .unwrap();
    assert!(
        store
            .schedule_chat_input(&conversation, &inputs[1], "process", "worker2")
            .unwrap()
            .is_none()
    );
    store
        .finish_chat_run(&first.id, "worker", "completed", Some("first done"), None)
        .unwrap();
    // The old run has finished, but the dispatch timer has not claimed B yet.
    assert!(
        store
            .schedule_chat_input(&conversation, &inputs[2], "process", "worker3")
            .unwrap()
            .is_none()
    );
    store.resume_chat_queue(&conversation, "process").unwrap();
    let next = store
        .claim_next_chat_input(&conversation, "process", "worker2")
        .unwrap()
        .unwrap();
    assert_eq!(next.input_message_id, inputs[1]);
    store
        .finish_chat_run(&next.id, "worker2", "completed", Some("second done"), None)
        .unwrap();
    let last = store
        .claim_next_chat_input(&conversation, "process", "worker3")
        .unwrap()
        .unwrap();
    assert_eq!(last.input_message_id, inputs[2]);
    assert_eq!(
        store
            .schedule_chat_input(&conversation, &inputs[0], "process", "duplicate")
            .unwrap()
            .unwrap()
            .id,
        first.id
    );
    assert_eq!(
        store.list_chat_runs(&conversation, 0, 100).unwrap().len(),
        3
    );
}

#[test]
fn stop_during_input_save_cannot_be_undone_by_late_scheduling() {
    let (_dir, store, conversation, inputs) = fixture();
    let first = store
        .schedule_chat_input(&conversation, &inputs[0], "process", "worker")
        .unwrap()
        .unwrap();
    store.cancel_chat_run(&conversation, &first.id).unwrap();
    store
        .finish_chat_run(&first.id, "worker", "cancelled", None, None)
        .unwrap();
    assert!(
        store
            .schedule_chat_input(&conversation, &inputs[1], "process", "late")
            .unwrap()
            .is_none()
    );
    assert!(store.chat_queue_paused(&conversation, "process").unwrap());
    store.resume_chat_queue(&conversation, "process").unwrap();
    let next = store
        .claim_next_chat_input(&conversation, "process", "next")
        .unwrap()
        .unwrap();
    assert_eq!(next.input_message_id, inputs[1]);
}

#[test]
fn concurrent_dispatch_claims_only_one_saved_input() {
    let (_dir, store, conversation, inputs) = fixture();
    store.enqueue_chat_input(&conversation, &inputs[0]).unwrap();
    store.resume_chat_queue(&conversation, "process").unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = (0..2)
        .map(|i| {
            let path = store.path.clone();
            let cid = conversation.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let store = Store::open(&path).unwrap();
                barrier.wait();
                store
                    .claim_next_chat_input(&cid, "process", &format!("worker-{i}"))
                    .unwrap()
                    .is_some()
            })
        })
        .collect();
    assert_eq!(
        workers
            .into_iter()
            .map(|worker| usize::from(worker.join().unwrap()))
            .sum::<usize>(),
        1
    );
}

#[test]
fn expired_dispatch_lease_is_taken_over_and_delete_cascades() {
    let (_dir, store, conversation, inputs) = fixture();
    store.enqueue_chat_input(&conversation, &inputs[0]).unwrap();
    store.resume_chat_queue(&conversation, "process").unwrap();
    store
        .connection
        .execute(
            "UPDATE chat_queue_dispatch SET lease_until='2000-01-01' WHERE conversation_id=?1",
            [&conversation],
        )
        .unwrap();
    // 租约过期不再是暂停：当前进程直接接管并领取待生成输入。
    let run = store
        .claim_next_chat_input(&conversation, "process", "worker")
        .unwrap()
        .unwrap();
    assert_eq!(run.input_message_id, inputs[0]);
    store
        .connection
        .execute("DELETE FROM conversations WHERE id=?1", [&conversation])
        .unwrap();
    assert!(
        store
            .list_chat_input_queue(&conversation, 0, 100)
            .unwrap()
            .is_empty()
    );
    assert!(store.resume_chat_queue(&conversation, "process").is_err());
}

#[test]
fn saved_input_recovers_after_registration_failure_and_an_older_answer() {
    let (_dir, store, conversation, inputs) = fixture();
    let first = store
        .schedule_chat_input(&conversation, &inputs[0], "process", "worker")
        .unwrap()
        .unwrap();
    store.connection.execute_batch("CREATE TRIGGER reject_queue BEFORE INSERT ON chat_input_queue BEGIN SELECT RAISE(ABORT,'registration failed'); END;").unwrap();
    assert!(
        store
            .schedule_chat_input(&conversation, &inputs[1], "process", "next")
            .is_err()
    );
    store
        .connection
        .execute_batch("DROP TRIGGER reject_queue;")
        .unwrap();
    store
        .finish_chat_run(&first.id, "worker", "completed", Some("older answer"), None)
        .unwrap();
    let reopened = Store::open(&store.path).unwrap();
    let saved = reopened
        .unscheduled_chat_inputs(&conversation, 0, 100)
        .unwrap();
    assert_eq!(saved.len(), 2);
    assert!(
        saved
            .iter()
            .any(|input| input.input_message_id == inputs[1])
    );
    assert!(
        !saved
            .iter()
            .any(|input| input.input_message_id == inputs[0])
    );
    // Another input was already queued before the user explicitly recovered B.
    reopened
        .enqueue_chat_input(&conversation, &inputs[2])
        .unwrap();
    reopened.pause_chat_queue(&conversation).unwrap();
    assert!(
        reopened
            .schedule_chat_input(&conversation, &inputs[1], "process", "next")
            .unwrap()
            .is_none()
    );
    assert!(
        reopened
            .schedule_chat_input(&conversation, &inputs[1], "process", "duplicate")
            .unwrap()
            .is_none()
    );
    assert!(
        reopened
            .chat_queue_paused(&conversation, "process")
            .unwrap()
    );
    assert!(
        reopened
            .unscheduled_chat_inputs(&conversation, 0, 100)
            .unwrap()
            .is_empty()
    );
    let queued = reopened
        .pending_chat_input_queue(&conversation, 0, 100)
        .unwrap();
    assert_eq!(
        queued
            .iter()
            .map(|q| q.input_message_id.as_str())
            .collect::<Vec<_>>(),
        [inputs[2].as_str(), inputs[1].as_str()]
    );
    assert_eq!(reopened.list_messages(&conversation).unwrap().len(), 4);
}

#[test]
fn unscheduled_inventory_excludes_every_durable_association_and_other_conversations() {
    let (_dir, store, conversation, inputs) = fixture();
    let run = store
        .create_chat_run(&conversation, &inputs[0], "original", "worker")
        .unwrap();
    store
        .enqueue_chat_supplement(&conversation, &inputs[1], &run.id, "{}")
        .unwrap();
    let supplement = store
        .next_chat_supplement(&conversation, &run.id)
        .unwrap()
        .unwrap();
    store
        .consume_chat_supplement(&run.id, "worker", &supplement)
        .unwrap();
    let withdrawn = store.enqueue_chat_input(&conversation, &inputs[2]).unwrap();
    store.withdraw_chat_input(&conversation, withdrawn).unwrap();
    let pending = store
        .send_message(&conversation, "user", "pending", None)
        .unwrap();
    store.enqueue_chat_input(&conversation, &pending).unwrap();
    let other = store.create_conversation(Some("other"), None).unwrap();
    store
        .send_message(&other, "user", "not this conversation", None)
        .unwrap();
    let older = store
        .send_message(&conversation, "user", "saved only", None)
        .unwrap();
    let latest = store
        .send_message(&conversation, "user", &"长".repeat(200), None)
        .unwrap();
    let page = store.unscheduled_chat_inputs(&conversation, 0, 1).unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].input_message_id, latest);
    assert_eq!(page[0].preview.chars().count(), 160);
    let page = store.unscheduled_chat_inputs(&conversation, 1, 1).unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].input_message_id, older);
    assert!(
        store
            .unscheduled_chat_inputs(&conversation, 2, 1)
            .unwrap()
            .is_empty()
    );
}
