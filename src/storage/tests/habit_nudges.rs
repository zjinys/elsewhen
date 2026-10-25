use super::*;
use crate::sync::membership::RosterAnchor;
use crate::sync::pairing::PairingDevice;
use chrono::{Duration, Local, TimeZone};

fn habit_store() -> (std::path::PathBuf, Store) {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    entity_page(&store, "habit/walk", "habit", "午饭后散步");
    (path, store)
}

/// 直接装一份最小同步权威：本机身份 + owner 锚点，足够判定「主设备」。
fn configure_sync_owner(store: &Store, local_device: &str, owner_device: &str) {
    let local = PairingDevice {
        device_id: local_device.to_string(),
        stream_id: format!("stream-{local_device}"),
        name: "Local".to_string(),
        platform: "linux".to_string(),
        signing_key: [1; 32],
    };
    let owner = PairingDevice {
        device_id: owner_device.to_string(),
        stream_id: format!("stream-{owner_device}"),
        name: "Owner".to_string(),
        platform: "linux".to_string(),
        signing_key: [2; 32],
    };
    let anchor = RosterAnchor {
        dataset_id: "dataset-1".to_string(),
        owner,
        revision: 1,
        digest: "0".repeat(64),
    };
    store
        .connection
        .execute(
            "INSERT INTO sync_replica(singleton,dataset_id,device_id,stream_id,processor_device_id)
             VALUES (1,'dataset-1',?1,?2,?3)",
            rusqlite::params![local.device_id, local.stream_id, local_device,],
        )
        .unwrap();
    store
        .connection
        .execute(
            "INSERT INTO sync_authority(singleton,anchor,local_device) VALUES (1,?1,?2)",
            rusqlite::params![
                serde_json::to_string(&anchor).unwrap(),
                serde_json::to_string(&local).unwrap(),
            ],
        )
        .unwrap();
}

fn at(hour: u32, minute: u32) -> chrono::DateTime<Local> {
    let date = Local::now().date_naive();
    Local
        .from_local_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
        .single()
        .unwrap()
}

fn save_walk_nudge(store: &Store) {
    store
        .save_habit_nudge(
            "habit/walk",
            "今天中午午饭后散步了吗？",
            "走了",
            "今天午饭后散步了",
            "没走",
            "今天还没走",
            127,
            "12:30",
            true,
        )
        .unwrap();
}

#[test]
fn only_sync_owner_device_inserts_nudge() {
    let (path, store) = habit_store();
    store.create_conversation(Some("主对话流"), None).unwrap();
    save_walk_nudge(&store);

    // 加入设备：能看到计划与同步来的消息，但不能自己写入问候。
    configure_sync_owner(&store, "device-joiner", "device-owner");
    assert!(!store.is_sync_owner_device().unwrap());
    assert!(
        store.tick_habit_nudges_at(at(12, 45)).unwrap().is_empty(),
        "非主设备不能插入问候"
    );
    let count: i64 = store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE kind='habit_nudge'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);

    // 本机切到 owner 后，同一到期计划由主设备插入一次。
    store
        .connection
        .execute(
            "UPDATE sync_replica
             SET device_id='device-owner',stream_id='stream-device-owner',processor_device_id='device-owner'",
            [],
        )
        .unwrap();
    assert!(store.is_sync_owner_device().unwrap());
    assert_eq!(store.tick_habit_nudges_at(at(12, 46)).unwrap().len(), 1);
    let _ = std::fs::remove_file(path);
}

#[test]
fn due_nudge_is_inserted_once_and_answer_creates_event() {
    let (path, store) = habit_store();
    store.create_conversation(Some("主对话流"), None).unwrap();
    save_walk_nudge(&store);

    let first = store.tick_habit_nudges_at(at(12, 45)).unwrap();
    assert_eq!(first.len(), 1);
    let messages = store
        .list_messages_for_display(&{
            let mut stmt = store
                .connection
                .prepare("SELECT id FROM conversations LIMIT 1")
                .unwrap();
            stmt.query_row([], |row| row.get::<_, String>(0)).unwrap()
        })
        .unwrap();
    let nudge = messages.iter().find(|m| m.kind == "habit_nudge").unwrap();
    assert_eq!(nudge.content, "今天中午午饭后散步了吗？");
    assert!(nudge.message_ref.as_deref().unwrap().contains(':'));
    assert_eq!(store.pending_habit_nudge_count().unwrap(), 1);

    // 同一 tick 不重复插入。
    assert!(store.tick_habit_nudges_at(at(12, 50)).unwrap().is_empty());

    let before = store.list_events().unwrap().len();
    let (checkin, created) = store.answer_habit_nudge(&nudge.id, "done").unwrap();
    assert!(created);
    assert_eq!(store.list_events().unwrap().len(), before + 1);
    assert_eq!(store.list_events().unwrap()[0].raw_text, "今天午饭后散步了");
    assert_eq!(store.pending_habit_nudge_count().unwrap(), 0);

    // 幂等：重复点击不追加消息、不重复建事件。
    let (again, created_again) = store.answer_habit_nudge(&nudge.id, "done").unwrap();
    assert_eq!(again, checkin);
    assert!(!created_again);
    assert_eq!(store.list_events().unwrap().len(), before + 1);

    let checkin_message = store
        .list_messages_for_display(&nudge.conversation_id)
        .unwrap()
        .into_iter()
        .find(|m| m.id == checkin)
        .unwrap();
    assert_eq!(checkin_message.kind, "habit_checkin");
    assert_eq!(
        checkin_message.parent_message_id.as_deref(),
        Some(nudge.id.as_str())
    );
    assert_eq!(checkin_message.content, "今天午饭后散步了");

    let _ = std::fs::remove_file(path);
}

#[test]
fn missed_answer_writes_chat_but_no_event() {
    let (path, store) = habit_store();
    store.create_conversation(Some("主对话流"), None).unwrap();
    save_walk_nudge(&store);
    let nudge_id = store.tick_habit_nudges_at(at(12, 45)).unwrap()[0].clone();
    let before = store.list_events().unwrap().len();
    let (checkin, created) = store.answer_habit_nudge(&nudge_id, "missed").unwrap();
    assert!(!created);
    assert_eq!(store.list_events().unwrap().len(), before);
    let checkin = store
        .connection
        .query_row(
            "SELECT content FROM messages WHERE id=?1",
            [checkin],
            |row| row.get::<_, String>(0),
        )
        .unwrap();
    assert_eq!(checkin, "今天还没走");
    let _ = std::fs::remove_file(path);
}

#[test]
fn snooze_reasks_once_with_linked_second_message() {
    let (path, store) = habit_store();
    store.create_conversation(Some("主对话流"), None).unwrap();
    save_walk_nudge(&store);
    let first = store.tick_habit_nudges_at(at(12, 45)).unwrap()[0].clone();
    store.snooze_habit_nudge_at(&first, at(12, 45)).unwrap();
    assert!(
        store.tick_habit_nudges_at(at(13, 0)).unwrap().is_empty(),
        "snooze 未到期不重复问"
    );
    let second = store.tick_habit_nudges_at(at(14, 0)).unwrap();
    assert_eq!(second.len(), 1);
    assert_ne!(second[0], first);
    let second_ref = store
        .connection
        .query_row(
            "SELECT ref,parent_message_id FROM messages WHERE id=?1",
            [&second[0]],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .unwrap();
    assert!(second_ref.0.ends_with(":2"));
    assert_eq!(second_ref.1.as_deref(), Some(first.as_str()));
    assert!(
        store.tick_habit_nudges_at(at(15, 0)).unwrap().is_empty(),
        "一天最多补问一次"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn after_22_does_not_catch_up_and_page_delete_cleans_plan() {
    let (path, store) = habit_store();
    store.create_conversation(Some("主对话流"), None).unwrap();
    save_walk_nudge(&store);
    assert!(store.tick_habit_nudges_at(at(22, 30)).unwrap().is_empty());
    store.delete_wiki_page("habit/walk").unwrap();
    assert!(
        store
            .get_habit_nudge_for_page("habit/walk")
            .unwrap()
            .is_none()
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn context_keeps_nudge_and_checkin_as_their_own_turn() {
    let (path, store) = habit_store();
    let conversation = store.create_conversation(Some("主对话流"), None).unwrap();
    let first = store
        .send_message(&conversation, "user", "第一句", None)
        .unwrap();
    save_walk_nudge(&store);
    let nudge = store.tick_habit_nudges_at(at(12, 45)).unwrap()[0].clone();
    let (checkin, _) = store.answer_habit_nudge(&nudge, "done").unwrap();
    let next = store
        .send_message(&conversation, "user", "第二句", None)
        .unwrap();

    let context = store.context_for_input(&conversation, &next).unwrap();
    let ids: Vec<&str> = context.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            first.as_str(),
            nudge.as_str(),
            checkin.as_str(),
            next.as_str()
        ]
    );
    let kinds: Vec<&str> = context.iter().map(|m| m.kind.as_str()).collect();
    assert_eq!(kinds, vec!["chat", "habit_nudge", "habit_checkin", "chat"]);
    let _ = std::fs::remove_file(path);
}

#[test]
fn yesterday_unanswered_nudge_does_not_stay_unread() {
    let (path, store) = habit_store();
    store.create_conversation(Some("主对话流"), None).unwrap();
    save_walk_nudge(&store);
    let yesterday = (Local::now() - Duration::days(1)).date_naive();
    let at_yesterday = Local
        .from_local_datetime(&yesterday.and_hms_opt(12, 45, 0).unwrap())
        .single()
        .unwrap();
    let old = store.tick_habit_nudges_at(at_yesterday).unwrap();
    assert_eq!(old.len(), 1, "昨天到点仍可插入历史问候");
    assert_eq!(
        store.pending_habit_nudge_count().unwrap(),
        0,
        "过期未回答的问候不能一直挂未读点"
    );
    let _ = std::fs::remove_file(path);
}

fn at_day(days_ago: i64, hour: u32, minute: u32) -> chrono::DateTime<Local> {
    let date = (Local::now() - Duration::days(days_ago)).date_naive();
    Local
        .from_local_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
        .single()
        .unwrap()
}

#[test]
fn global_daily_limit_allows_only_two_first_nudges() {
    let (path, store) = habit_store();
    store.create_conversation(Some("主对话流"), None).unwrap();
    entity_page(&store, "habit/run", "habit", "跑步");
    entity_page(&store, "habit/read", "habit", "阅读");
    save_walk_nudge(&store);
    store
        .save_habit_nudge(
            "habit/run",
            "今天跑步了吗？",
            "跑了",
            "今天跑步了",
            "没跑",
            "今天还没跑",
            127,
            "12:30",
            true,
        )
        .unwrap();
    store
        .save_habit_nudge(
            "habit/read",
            "今天阅读了吗？",
            "读了",
            "今天阅读了",
            "没读",
            "今天还没读",
            127,
            "12:30",
            true,
        )
        .unwrap();

    assert_eq!(store.tick_habit_nudges_at(at(12, 45)).unwrap().len(), 1);
    assert_eq!(store.tick_habit_nudges_at(at(12, 46)).unwrap().len(), 1);
    assert!(
        store.tick_habit_nudges_at(at(12, 47)).unwrap().is_empty(),
        "当天第三条首次问候应被全局限流拦下"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn three_consecutive_dismissed_days_yield_suggestion_card() {
    let (path, store) = habit_store();
    store.create_conversation(Some("主对话流"), None).unwrap();
    save_walk_nudge(&store);
    for days_ago in [3i64, 2, 1] {
        let fired = store
            .tick_habit_nudges_at(at_day(days_ago, 12, 45))
            .unwrap();
        assert_eq!(fired.len(), 1, "第 {days_ago} 天前应正常触发问候");
        store.dismiss_habit_nudge(&fired[0]).unwrap();
    }

    let today = store.tick_habit_nudges_at(at(12, 45)).unwrap();
    assert_eq!(today.len(), 1);
    let (kind, content): (String, String) = store
        .connection
        .query_row(
            "SELECT kind,content FROM messages WHERE id=?1",
            rusqlite::params![&today[0]],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(kind, "habit_nudge_suggest");
    assert!(
        content.contains("调个时间"),
        "建议卡应是确定性文案：{content}"
    );

    let nudge = store
        .get_habit_nudge_for_page("habit/walk")
        .unwrap()
        .unwrap();
    let date = Local::now().format("%Y-%m-%d").to_string();
    let fire = store
        .get_habit_nudge_fire(&nudge.id, &date)
        .unwrap()
        .unwrap();
    assert_eq!(fire.response.as_deref(), Some("suggested"));
    assert!(
        store.tick_habit_nudges_at(at(13, 0)).unwrap().is_empty(),
        "建议卡当天不应再重复触发"
    );
    let _ = std::fs::remove_file(path);
}
