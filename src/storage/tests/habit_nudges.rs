use super::*;
use chrono::{Duration, Local, TimeZone};

fn habit_store() -> (std::path::PathBuf, Store) {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    entity_page(&store, "habit/walk", "habit", "午饭后散步");
    (path, store)
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
