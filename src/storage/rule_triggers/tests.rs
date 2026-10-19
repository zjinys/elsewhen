//! 规则触发引擎测试（事件链 + 对话影子链，从单体文件原样平移）。
use rusqlite::params;

use super::event_chain::{apply_rule_confirmations_on, pending_hits_for_event};
use super::*;
use crate::storage::RuleStatus;
use crate::storage::Store;

fn memory_store() -> Store {
    let path = std::env::temp_dir().join(format!(
        "elsewhen-rule-triggers-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    Store::open(&path).unwrap()
}

fn add_rule_with_keywords(store: &Store, content: &str, keywords: &[&str]) -> String {
    let id = store.add_rule(content, RuleStatus::Active, None).unwrap();
    let keywords_json =
        serde_json::to_string(&keywords.iter().map(|k| k.to_string()).collect::<Vec<_>>()).unwrap();
    store
        .connection
        .execute(
            "UPDATE rules SET trigger_keywords=?2 WHERE id=?1",
            params![id, keywords_json],
        )
        .unwrap();
    id
}

fn hit_status(store: &Store, rule_id: &str, event_id: &str) -> Option<String> {
    store
        .connection
        .query_row(
            "SELECT status FROM rule_hits WHERE rule_id=?1 AND event_id=?2",
            params![rule_id, event_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .unwrap()
}

#[test]
fn keyword_match_is_case_insensitive_deterministic_and_bad_json_tolerant() {
    let store = memory_store();
    let good = add_rule_with_keywords(&store, "X 项目相关要提醒", &["X项目", "deadline"]);
    // 坏 keywords（数组元素非字符串）：该规则被跳过，预筛整体不崩。
    store
        .connection
        .execute(
            "UPDATE rules SET trigger_keywords='[1,2]' WHERE id=?1",
            params![good],
        )
        .unwrap();
    assert!(
        match_rules_for_text(&store.connection, "X项目")
            .unwrap()
            .is_empty()
    );
    // 恢复正常关键词后命中；大小写折叠；同输入同结果。
    store
        .connection
        .execute(
            "UPDATE rules SET trigger_keywords=?2 WHERE id=?1",
            params![good, r#"["X项目","Deadline"]"#],
        )
        .unwrap();
    let first = match_rules_for_text(&store.connection, "推进 X项目 联调").unwrap();
    assert_eq!(first, vec![good.clone()]);
    assert_eq!(
        first,
        match_rules_for_text(&store.connection, "推进 x项目 联调").unwrap()
    );
}

#[test]
fn screen_is_idempotent_and_respects_off_mode() {
    let store = memory_store();
    let rule = add_rule_with_keywords(&store, "X 项目", &["X项目"]);
    let event_id = store
        .insert_event(crate::event::NewEvent::now("X项目 有了新进展"))
        .unwrap();
    screen_event_on(&store.connection, &event_id, "X项目 有了新进展");
    assert_eq!(
        hit_status(&store, &rule, &event_id).as_deref(),
        Some("pending")
    );
    // 重复入库/重放：仍是同一条 pending，不重复计数。
    screen_event_on(&store.connection, &event_id, "X项目 有了新进展");
    let count: i64 = store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM rule_hits WHERE rule_id=?1 AND event_id=?2",
            params![rule, event_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);

    // off 模式：预筛零成本直接放行（不应删已有记录，只是不新增）。
    set_rule_trigger_mode(&store.connection, RULE_TRIGGER_MODE_OFF).unwrap();
    let event2 = store
        .insert_event(crate::event::NewEvent::now("X项目 第二条"))
        .unwrap();
    screen_event_on(&store.connection, &event2, "X项目 第二条");
    assert_eq!(hit_status(&store, &rule, &event2), None);
    set_rule_trigger_mode(&store.connection, RULE_TRIGGER_MODE_INSTANT).unwrap();
}

#[test]
fn confirmations_follow_threshold_and_review_only_mode() {
    let store = memory_store();
    let rule = add_rule_with_keywords(&store, "X 项目", &["X项目"]);
    let e1 = store
        .insert_event(crate::event::NewEvent::now("X项目 A"))
        .unwrap();
    let e2 = store
        .insert_event(crate::event::NewEvent::now("X项目 B"))
        .unwrap();
    let e3 = store
        .insert_event(crate::event::NewEvent::now("X项目 C"))
        .unwrap();
    screen_event_on(&store.connection, &e1, "X项目 A");
    screen_event_on(&store.connection, &e2, "X项目 B");
    screen_event_on(&store.connection, &e3, "X项目 C");

    let conf = |applicable: bool, confidence: f64| RuleConfirmation {
        rule_id: rule.clone(),
        applicable,
        confidence,
    };
    // 高置信 → notified；低置信 → aggregated；不适用 → declined。
    let confirmations = [conf(true, 0.9), conf(true, 0.5), conf(false, 0.9)];
    apply_rule_confirmations_on(&store.connection, &e1, &confirmations[0..1]).unwrap();
    apply_rule_confirmations_on(&store.connection, &e2, &confirmations[1..2]).unwrap();
    apply_rule_confirmations_on(&store.connection, &e3, &confirmations[2..3]).unwrap();
    assert_eq!(hit_status(&store, &rule, &e1).as_deref(), Some("notified"));
    assert_eq!(
        hit_status(&store, &rule, &e2).as_deref(),
        Some("aggregated")
    );
    assert_eq!(hit_status(&store, &rule, &e3).as_deref(), Some("declined"));

    // review_only：applicable 高置信也聚合。
    set_rule_trigger_mode(&store.connection, RULE_TRIGGER_MODE_REVIEW_ONLY).unwrap();
    let e4 = store
        .insert_event(crate::event::NewEvent::now("X项目 D"))
        .unwrap();
    screen_event_on(&store.connection, &e4, "X项目 D");
    apply_rule_confirmations_on(&store.connection, &e4, &confirmations[0..1]).unwrap();
    assert_eq!(
        hit_status(&store, &rule, &e4).as_deref(),
        Some("aggregated")
    );

    // 只处理 pending：已 notified 的记录不被重放改写。
    set_rule_trigger_mode(&store.connection, RULE_TRIGGER_MODE_INSTANT).unwrap();
    apply_rule_confirmations_on(&store.connection, &e1, &confirmations[2..3]).unwrap();
    assert_eq!(hit_status(&store, &rule, &e1).as_deref(), Some("notified"));
}

#[test]
fn chat_shadow_screen_pends_resolves_and_stays_idempotent() {
    let store = memory_store();
    let rule = add_rule_with_keywords(&store, "X 项目联调要留痕", &["X项目"]);
    let conversation = store.create_conversation(None, None).unwrap();
    let message_id = store
        .send_message(&conversation, "user", "今天推进 X项目 联调时踩了个坑", None)
        .unwrap();

    // 预筛命中 → pending，幂等（重复预筛不重复落行）。
    store.screen_chat_message(&conversation, &message_id, "今天推进 X项目 联调时踩了个坑");
    store.screen_chat_message(&conversation, &message_id, "今天推进 X项目 联调时踩了个坑");
    let pending = store.pending_chat_hits(10).unwrap();
    assert_eq!(pending.len(), 1, "幂等：同输入同消息只落一条 pending");
    assert_eq!(pending[0].rule_id, rule);
    assert!(pending[0].message_text.contains("X项目"));

    // 判定回填：只动 pending 行，重放不改写已定状态。
    store
        .resolve_chat_hit(&pending[0].id, true, Some(0.9))
        .unwrap();
    let hits = store.recent_chat_rule_hits(10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].status, "applicable");
    assert_eq!(hits[0].confidence, Some(0.9));
    store.resolve_chat_hit(&pending[0].id, false, None).unwrap();
    assert_eq!(
        store.recent_chat_rule_hits(10).unwrap()[0].status,
        "applicable"
    );
}

#[test]
fn chat_shadow_respects_mode_off_and_offline_messages() {
    let store = memory_store();
    add_rule_with_keywords(&store, "X 项目相关要提醒", &["X项目"]);
    let conversation = store.create_conversation(None, None).unwrap();
    let message_id = store
        .send_message(&conversation, "user", "X项目 推进-normal", None)
        .unwrap();
    set_rule_trigger_mode(&store.connection, RULE_TRIGGER_MODE_OFF).unwrap();
    store.screen_chat_message(&conversation, &message_id, "X项目 推进-normal");
    assert!(
        store.pending_chat_hits(10).unwrap().is_empty(),
        "mode=off 不预筛"
    );
    // 不参与预筛的规则不建记录（match_rules_for_text 只认 trigger_keywords）。
    set_rule_trigger_mode(&store.connection, RULE_TRIGGER_MODE_INSTANT).unwrap();
    store.screen_chat_message(&conversation, &message_id, "今天聊点别的");
    assert!(store.pending_chat_hits(10).unwrap().is_empty());
}

#[test]
fn cascade_delete_and_event_screen_notification_surface() {
    let store = memory_store();
    let rule = add_rule_with_keywords(&store, "X 项目", &["X项目"]);
    let event_id = store
        .insert_event(crate::event::NewEvent::now("X项目 E"))
        .unwrap();
    screen_event_on(&store.connection, &event_id, "X项目 E");
    assert!(
        pending_hits_for_event(&store.connection, &event_id)
            .unwrap()
            .len()
            == 1
    );
    // 删除规则 → 命中级联消失（外键 ON DELETE CASCADE）。
    store.delete_rule(&rule).unwrap();
    assert!(
        pending_hits_for_event(&store.connection, &event_id)
            .unwrap()
            .is_empty()
    );
}
