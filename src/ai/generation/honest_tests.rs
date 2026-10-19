//! 诚实降级策略测试。

use super::*;
use crate::storage::Store;

fn fixture() -> (Store, ChatRun, String) {
    let dir = std::env::temp_dir().join(format!("elsewhen-honest-prefix-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("test.db")).unwrap();
    crate::storage::Store::mark_initialized(&dir.join("test.db")).unwrap();
    let conversation = store.create_conversation(None, None).unwrap();
    let input = store
        .send_message(&conversation, "user", "第一问", None)
        .unwrap();
    let run = store
        .create_chat_run(
            &conversation,
            &input,
            &uuid::Uuid::new_v4().to_string(),
            "owner",
        )
        .unwrap();
    (store, run, conversation)
}

#[test]
fn completed_reply_carries_honest_prefix_when_summary_unavailable() {
    // 10-19 档案 P0-c：completed run 里用户必须直接看到「背景摘要
    // 不可用」，不再依赖模型转述 system 免责声明。
    let (store, run, _conversation) = fixture();
    let plain = honest_summary_degradation(&store, &run.id, "正常回复".into());
    assert_eq!(plain, "正常回复", "无事件时不加前缀");
    store
        .append_chat_run_event(
            &run.id,
            "owner",
            "summary_unavailable",
            &serde_json::json!({"reason":"incomplete_or_invalid_basis","detail":"没有可摘要的历史"}),
        )
        .unwrap();
    let degraded = honest_summary_degradation(&store, &run.id, "模型回答".into());
    assert!(degraded.contains("背景摘要不可用"));
    assert!(degraded.ends_with("模型回答"), "前缀在回复之首");
    // 不走 failure_leave——P1-A 指纹不因此污染
    assert!(!degraded.contains("失败通知"));
}
