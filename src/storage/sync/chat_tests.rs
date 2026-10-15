use super::*;
use crate::sync::crypto::{ContentKey, DeviceIdentity};

struct Database {
    store: Store,
    path: std::path::PathBuf,
}
impl Database {
    fn new(device: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("elsewhen-chat-sync-{}", uuid::Uuid::new_v4()));
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
        for peer in ["a", "b", "c"] {
            if peer != device {
                store
                    .authorize_sync_member(peer, &format!("stream-{peer}"))
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
fn deliver(source: &Database, target: &Database) {
    let from = source.store.sync_identity().unwrap().unwrap().device_id;
    let to = target.store.sync_identity().unwrap().unwrap().device_id;
    for op in source.store.pending_sync_operations(&to, 100).unwrap() {
        target
            .store
            .receive_sync_operation(&from, &serde_json::to_vec(&op).unwrap())
            .unwrap();
    }
}
fn input(db: &Database, conversation: &str, text: &str) -> String {
    db.store
        .submit_conversation_input(conversation, text, None)
        .unwrap()
        .message_id
        .unwrap()
}

#[test]
fn legacy_message_encoding_stays_hash_compatible_without_a_response_link() {
    let bytes = br#"{"id":"reply","conversation_id":"conversation","parent_message_id":null,"role":"assistant","content":"legacy reply","created_at":"t"}"#;
    let message: crate::sync::protocol::MessageRecord = serde_json::from_slice(bytes).unwrap();
    assert!(message.input_message_id.is_none());
    assert_eq!(serde_json::to_vec(&message).unwrap(), bytes);
}

#[test]
fn migration_backfills_only_explicit_completed_run_bases() {
    let mut connection = rusqlite::Connection::open_in_memory().unwrap();
    crate::storage::new_migrations::initialize_test_version(&mut connection, 20).unwrap();
    connection.execute_batch("INSERT INTO conversations(id,title,created_at,updated_at) VALUES('conversation','legacy','t','t');
        INSERT INTO messages(id,conversation_id,role,content,created_at) VALUES
          ('input','conversation','user','original','t'),
          ('reply','conversation','assistant','known answer','t'),
          ('legacy','conversation','assistant','unassociated answer','t');
        INSERT INTO chat_runs(id,request_key,conversation_id,input_message_id,input_hash,status,phase,executor_id,lease_until,created_at,updated_at,final_message_id)
          VALUES('run','request','conversation','input','hash','completed','completed','executor','t','t','t','reply');").unwrap();
    crate::storage::new_migrations::initialize(&mut connection).unwrap();
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM message_response_links", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
    let input: String = connection
        .query_row(
            "SELECT input_message_id FROM message_response_links WHERE message_id='reply'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(input, "input");
}

#[test]
fn concurrent_device_replies_keep_original_turns_without_copying_runs_or_dispatch() {
    let a = Database::new("a");
    let b = Database::new("b");
    let c = Database::new("c");
    let conversation = a.store.create_conversation(Some("shared"), None).unwrap();
    let input_a = input(&a, &conversation, "input a");
    let run_a = a
        .store
        .create_chat_run(&conversation, &input_a, "request-a", "executor-a")
        .unwrap();
    deliver(&a, &b);
    let input_b = input(&b, &conversation, "input b");
    let run_b = b
        .store
        .create_chat_run(&conversation, &input_b, "request-b", "executor-b")
        .unwrap();
    deliver(&b, &a);
    a.store
        .finish_chat_run(&run_a.id, "executor-a", "completed", Some("answer a"), None)
        .unwrap();
    b.store
        .finish_chat_run(&run_b.id, "executor-b", "completed", Some("answer b"), None)
        .unwrap();
    deliver(&a, &b);
    deliver(&b, &a);
    deliver(&a, &c);
    deliver(&b, &c);
    for db in [&a, &b, &c] {
        assert_eq!(db.count("message_response_links"), 2);
        let messages = db.store.list_messages(&conversation).unwrap();
        for (answer, owning_input) in [("answer a", &input_a), ("answer b", &input_b)] {
            let message = messages.iter().find(|m| m.content == answer).unwrap();
            let stored: String = db
                .store
                .connection
                .query_row(
                    "SELECT input_message_id FROM message_response_links WHERE message_id=?1",
                    [&message.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(&stored, owning_input);
            assert!(
                message.parent_message_id.is_none(),
                "input association must not invent a conversation branch"
            );
        }
    }
    assert_eq!(c.count("chat_runs"), 0);
    assert_eq!(c.count("chat_input_queue"), 0);
    assert_eq!(c.count("chat_proposal_decisions"), 0);
    let next = input(&c, &conversation, "followup");
    let context = c.store.context_for_input(&conversation, &next).unwrap();
    let texts: Vec<_> = context.iter().map(|m| m.content.as_str()).collect();
    assert_eq!(
        texts,
        vec!["input a", "answer a", "input b", "answer b", "followup"]
    );
}

#[test]
fn completed_reply_waits_for_its_remote_original_input() {
    let a = Database::new("a");
    let b = Database::new("b");
    let c = Database::new("c");
    let conversation = a.store.create_conversation(Some("shared"), None).unwrap();
    deliver(&a, &b);
    let original = input(&b, &conversation, "remote original");
    deliver(&b, &a);
    let run = a
        .store
        .create_chat_run(&conversation, &original, "request", "executor")
        .unwrap();
    a.store
        .finish_chat_run(&run.id, "executor", "completed", Some("bound answer"), None)
        .unwrap();
    deliver(&a, &c);
    assert_eq!(c.count("messages"), 0);
    assert_eq!(c.store.retry_waiting_sync_operations().unwrap().waiting, 1);
    deliver(&b, &c);
    assert_eq!(c.count("messages"), 2);
    assert_eq!(c.count("message_response_links"), 1);
    assert_eq!(c.count("chat_runs"), 0);
}

#[test]
fn snapshot_keeps_reply_basis_but_not_local_run_authority() {
    let a = Database::new("a");
    let b = Database::new("b");
    let conversation = a
        .store
        .create_conversation(Some("snapshot conversation"), None)
        .unwrap();
    let original = input(&a, &conversation, "input before snapshot");
    let run = a
        .store
        .create_chat_run(&conversation, &original, "request", "executor")
        .unwrap();
    let later = input(&a, &conversation, "input during generation");
    a.store
        .finish_chat_run(
            &run.id,
            "executor",
            "completed",
            Some("answer to original"),
            None,
        )
        .unwrap();
    let key = ContentKey::generate().unwrap();
    let signer = DeviceIdentity::generate().unwrap();
    let mut export = snapshot::SnapshotExporter::new(&a.store, "b", "response-basis", 1).unwrap();
    b.store
        .begin_snapshot_import("response-basis", "a", 1)
        .unwrap();
    let mut index = 0;
    while let Some(bytes) = export.next_encrypted_chunk(&key, &signer).unwrap() {
        b.store
            .stage_snapshot_chunk(
                "response-basis",
                index,
                "a",
                1,
                &bytes,
                &key,
                &signer.public_key(),
            )
            .unwrap();
        index += 1;
    }
    let manifest = export.finish(&key, &signer).unwrap();
    assert!(
        b.store
            .publish_snapshot(
                "response-basis",
                "a",
                1,
                &manifest,
                &key,
                &signer.public_key()
            )
            .unwrap()
    );
    let messages = b.store.context_for_input(&conversation, &later).unwrap();
    assert_eq!(
        messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>(),
        vec![
            "input before snapshot",
            "answer to original",
            "input during generation"
        ]
    );
    assert_eq!(b.count("chat_runs"), 0);
    assert_eq!(b.count("message_response_links"), 1);
}

#[test]
fn cancelled_drafts_and_invalid_cross_conversation_links_never_publish() {
    let a = Database::new("a");
    let conversation = a.store.create_conversation(Some("one"), None).unwrap();
    let original = input(&a, &conversation, "kept input");
    let run = a
        .store
        .create_chat_run(&conversation, &original, "request", "executor")
        .unwrap();
    a.store
        .finish_chat_run(
            &run.id,
            "executor",
            "cancelled",
            Some("private unfinished draft"),
            None,
        )
        .unwrap();
    assert_eq!(a.count("messages"), 1);
    assert_eq!(a.count("message_response_links"), 0);
    assert!(
        !serde_json::to_string(&a.store.pending_sync_operations("b", 100).unwrap())
            .unwrap()
            .contains("private unfinished draft")
    );
    let other = a.store.create_conversation(Some("other"), None).unwrap();
    assert!(
        a.store
            .send_message_with_input(&other, "assistant", "wrong basis", None, Some(&original))
            .is_err()
    );
    assert_eq!(a.count("messages"), 1);
    assert_eq!(a.count("message_response_links"), 0);
}
