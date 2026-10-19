//! conversations display_order 内嵌测试平移。

use super::*;

#[test]
fn queued_replies_display_interleaved_with_their_inputs() {
    let dir = std::env::temp_dir().join(format!("elsewhen-display-order-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("test.db")).unwrap();
    store
        .initialize_sync_replica(&crate::storage::sync::ReplicaIdentity {
            dataset_id: "dataset".into(),
            device_id: "device-a".into(),
            stream_id: "stream-a".into(),
            processor_device_id: "a".into(),
        })
        .unwrap();

    let conversation = store.create_conversation(None, None).unwrap();
    // 用户连发两条，回复随后逐条生成——提交时钟序为 A、B、答A、答B。
    let a = store
        .send_message(&conversation, "user", "问A", None)
        .unwrap();
    let b = store
        .send_message(&conversation, "user", "问B", None)
        .unwrap();
    let reply_a = store
        .send_message_with_input(&conversation, "assistant", "答A", None, Some(&a))
        .unwrap();
    let reply_b = store
        .send_message_with_input(&conversation, "assistant", "答B", None, Some(&b))
        .unwrap();

    let ids = |messages: &[MessageSummary]| -> Vec<String> {
        messages.iter().map(|m| m.id.clone()).collect()
    };

    // 因果序（AI 上下文）保持提交时钟序不变。
    assert_eq!(
        ids(&store.list_messages(&conversation).unwrap()),
        vec![a.clone(), b.clone(), reply_a.clone(), reply_b.clone()]
    );
    // 展示序：回复锚定其输入 → A、答A、B、答B 一问一答交错。
    assert_eq!(
        ids(&store.list_messages_for_display(&conversation).unwrap()),
        vec![a, reply_a, b, reply_b]
    );

    std::fs::remove_dir_all(&dir).ok();
}
