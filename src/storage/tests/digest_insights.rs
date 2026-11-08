//! 知识库洞见浮现：候选登记、频控 tick、四重闭环动作（提案
//! 2026-11-05-surfacing-digest-insights §3–§5）。

use super::*;
use crate::sync::membership::RosterAnchor;
use crate::sync::pairing::PairingDevice;
use chrono::{Duration, Local, TimeZone};

fn insight_store() -> (std::path::PathBuf, Store) {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    store.create_conversation(Some("主对话流"), None).unwrap();
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
            rusqlite::params![local.device_id, local.stream_id, local_device],
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
    at_day(0, hour, minute)
}

fn at_day(offset_days: i64, hour: u32, minute: u32) -> chrono::DateTime<Local> {
    let date = (Local::now() + Duration::days(offset_days)).date_naive();
    Local
        .from_local_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
        .single()
        .unwrap()
}

fn main_conversation_id(store: &Store) -> String {
    store
        .connection
        .query_row("SELECT id FROM conversations LIMIT 1", [], |row| row.get(0))
        .unwrap()
}

fn register_walk_candidate(store: &Store) {
    entity_page(store, "habit/walk", "habit", "午饭后散步");
    assert!(
        store
            .record_digest_insight_candidate(
                "habit",
                "habit/walk",
                "午饭后散步已固化",
                "连续 12 天午饭后散步。",
                Some("散步后直接做 5 分钟拉伸"),
                None,
            )
            .unwrap()
    );
}

#[test]
fn candidate_registers_once_per_page_and_surfaces_one_card_per_day() {
    let (path, store) = insight_store();
    register_walk_candidate(&store);
    // 终身一次：同页重复登记直接跳过（换标题/来源也不行）。
    assert!(
        !store
            .record_digest_insight_candidate(
                "habit",
                "habit/walk",
                "重复标题",
                "重复观察",
                None,
                None,
            )
            .unwrap()
    );

    let ids = store.tick_digest_insights_at(at(9, 30)).unwrap();
    assert_eq!(ids.len(), 1);
    let messages = store
        .list_messages_for_display(&main_conversation_id(&store))
        .unwrap();
    let card = messages
        .iter()
        .find(|m| m.kind == "digest_insight")
        .expect("应有一张洞见卡片");
    assert_eq!(card.role, "assistant");
    assert!(card.content.starts_with("💡 认知洞察｜"));
    assert!(card.content.contains("建议行动：散步后直接做 5 分钟拉伸"));
    assert!(card.content.contains("来源：[[habit/walk]]"));

    let surfacings = store.list_digest_insight_surfacings().unwrap();
    assert_eq!(surfacings.len(), 1);
    let row = &surfacings[0];
    assert_eq!(row.status, "surfaced");
    assert_eq!(row.message_id.as_deref(), Some(ids[0].as_str()));
    assert_eq!(
        card.message_ref.as_deref(),
        Some(format!("surfacing:{}", row.id).as_str())
    );
    assert_eq!(
        row.surfaced_date.as_deref(),
        Some(at(9, 30).format("%Y-%m-%d").to_string().as_str())
    );
    assert_eq!(row.page_kind.as_deref(), Some("habit"));
    assert!(row.page_active);
    assert_eq!(store.pending_digest_insight_count().unwrap(), 1);

    // 卡片正文可完整解析回结构化字段（副设备重建依赖）。
    let parsed = crate::storage::digest_insights::parse_digest_insight_card_text(&card.content)
        .expect("卡片格式应可解析");
    assert_eq!(parsed.0, "午饭后散步已固化");
    assert_eq!(parsed.1, "连续 12 天午饭后散步。");
    assert_eq!(parsed.2.as_deref(), Some("散步后直接做 5 分钟拉伸"));
    assert_eq!(parsed.3, "habit/walk");

    // 当日额度已用：第二个候选排队但不再浮现。
    entity_page(&store, "insight/other", "insight", "其它洞察");
    assert!(
        store
            .record_digest_insight_candidate(
                "insight",
                "insight/other",
                "标题",
                "观察",
                Some("行动"),
                None
            )
            .unwrap()
    );
    assert!(
        store.tick_digest_insights_at(at(11, 0)).unwrap().is_empty(),
        "当日已浮现 1 条，不再插入"
    );
    let statuses: Vec<String> = store
        .list_digest_insight_surfacings()
        .unwrap()
        .into_iter()
        .map(|row| row.status)
        .collect();
    assert!(statuses.contains(&"pending".to_string()));

    let _ = std::fs::remove_file(path);
}

#[test]
fn quiet_hours_hold_candidates_and_quota_resets_next_day() {
    let (path, store) = insight_store();
    register_walk_candidate(&store);
    assert!(
        store.tick_digest_insights_at(at(6, 30)).unwrap().is_empty(),
        "07:00 前静默"
    );
    assert!(
        store
            .tick_digest_insights_at(at(22, 10))
            .unwrap()
            .is_empty(),
        "22:00 后静默"
    );
    assert_eq!(store.tick_digest_insights_at(at(10, 0)).unwrap().len(), 1);

    entity_page(&store, "insight/next", "insight", "次日洞察");
    assert!(
        store
            .record_digest_insight_candidate(
                "insight",
                "insight/next",
                "标题",
                "观察",
                Some("行动"),
                None
            )
            .unwrap()
    );
    assert!(
        store.tick_digest_insights_at(at(12, 0)).unwrap().is_empty(),
        "当日已有卡片，不再浮现"
    );
    assert_eq!(
        store
            .tick_digest_insights_at(at_day(1, 9, 0))
            .unwrap()
            .len(),
        1,
        "次日恢复浮现"
    );
    let rows = store.list_digest_insight_surfacings().unwrap();
    let second = rows
        .iter()
        .find(|row| row.page_slug == "insight/next")
        .unwrap();
    assert_eq!(
        second.surfaced_date.as_deref(),
        Some(at_day(1, 9, 0).format("%Y-%m-%d").to_string().as_str())
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn only_owner_device_writes_cards() {
    let (path, store) = insight_store();
    register_walk_candidate(&store);

    configure_sync_owner(&store, "device-joiner", "device-owner");
    assert!(!store.is_sync_owner_device().unwrap());
    assert!(
        store.tick_digest_insights_at(at(10, 0)).unwrap().is_empty(),
        "副设备不能插入卡片"
    );
    let count: i64 = store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE kind='digest_insight'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);

    store
        .connection
        .execute(
            "UPDATE sync_replica
             SET device_id='device-owner',stream_id='stream-device-owner',processor_device_id='device-owner'",
            [],
        )
        .unwrap();
    assert!(store.is_sync_owner_device().unwrap());
    assert_eq!(store.tick_digest_insights_at(at(10, 5)).unwrap().len(), 1);

    let _ = std::fs::remove_file(path);
}

#[test]
fn action_loops_rule_todo_dismiss_are_idempotent() {
    let (path, store) = insight_store();
    register_walk_candidate(&store);
    let card1 = store.tick_digest_insights_at(at(10, 0)).unwrap()[0].clone();

    let out = store.act_on_digest_insight(&card1, "rule").unwrap();
    assert_eq!(
        (out.status.as_str(), out.action_type.as_deref()),
        ("acted", Some("rule"))
    );
    let rules: i64 = store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM rules WHERE content='散步后直接做 5 分钟拉伸'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rules, 1);
    // 幂等：重复点击不产生第二条规则。
    store.act_on_digest_insight(&card1, "rule").unwrap();
    let rules: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM rules", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rules, 1);

    entity_page(&store, "insight/todo", "insight", "待办洞察");
    assert!(
        store
            .record_digest_insight_candidate(
                "insight",
                "insight/todo",
                "标题二",
                "观察二",
                Some("把拉伸写进晚间流程"),
                None
            )
            .unwrap()
    );
    let card2 = store.tick_digest_insights_at(at_day(1, 10, 0)).unwrap()[0].clone();
    let out = store.act_on_digest_insight(&card2, "todo").unwrap();
    assert_eq!(out.action_type.as_deref(), Some("todo"));
    let (title, slug): (String, Option<String>) = store
        .connection
        .query_row(
            "SELECT title,related_wiki_slug FROM todos ORDER BY created_at DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(title, "把拉伸写进晚间流程");
    assert_eq!(slug.as_deref(), Some("insight/todo"));

    entity_page(&store, "insight/dismiss", "insight", "忽略洞察");
    assert!(
        store
            .record_digest_insight_candidate(
                "insight",
                "insight/dismiss",
                "标题三",
                "观察三",
                None,
                None
            )
            .unwrap()
    );
    let card3 = store.tick_digest_insights_at(at_day(2, 10, 0)).unwrap()[0].clone();
    let out = store.act_on_digest_insight(&card3, "dismiss").unwrap();
    assert_eq!(
        (out.status.as_str(), out.action_type.as_deref()),
        ("dismissed", Some("dismiss"))
    );
    // 幂等：终态后换动作也不产生副作用。
    let out = store.act_on_digest_insight(&card3, "rule").unwrap();
    assert_eq!(out.status, "dismissed");
    assert_eq!(store.pending_digest_insight_count().unwrap(), 0);
    assert!(store.act_on_digest_insight(&card3, "explode").is_err());

    let _ = std::fs::remove_file(path);
}

#[test]
fn surfaced_cards_expire_after_72_hours_without_action() {
    let (path, store) = insight_store();
    register_walk_candidate(&store);
    let _card = store.tick_digest_insights_at(at_day(0, 10, 0)).unwrap()[0].clone();
    assert_eq!(store.pending_digest_insight_count().unwrap(), 1);

    // 48h 后仍有效（未读计数不丢）。
    store.tick_digest_insights_at(at_day(2, 9, 0)).unwrap();
    let rows = store.list_digest_insight_surfacings().unwrap();
    assert_eq!(rows[0].status, "surfaced");

    // 72h 后自动过期，未读计数归零。
    store.tick_digest_insights_at(at_day(3, 11, 0)).unwrap();
    let rows = store.list_digest_insight_surfacings().unwrap();
    assert_eq!(rows[0].status, "expired");
    assert_eq!(store.pending_digest_insight_count().unwrap(), 0);

    let _ = std::fs::remove_file(path);
}

#[test]
fn joiner_rebuilds_local_row_from_card_content_and_acts() {
    let (path, store) = insight_store();
    let conversation_id = main_conversation_id(&store);
    let content = crate::storage::digest_insights::digest_insight_card_text(
        "散步叠加拉伸更省成本",
        "午饭后散步已连续记录 12 天。",
        Some("散步后直接就地进行小腿拉伸"),
        "habit/walk",
    );
    let message_id = store
        .insert_message_with_kind(
            &conversation_id,
            "assistant",
            &content,
            None,
            None,
            "digest_insight",
            Some("surfacing:remote-1"),
        )
        .unwrap();
    // 无行动建议的卡片同样可解析（建议段整体缺席）。
    let plain = crate::storage::digest_insights::digest_insight_card_text(
        "只有观察",
        "观察正文。",
        None,
        "insight/plain",
    );
    let parsed = crate::storage::digest_insights::parse_digest_insight_card_text(&plain).unwrap();
    assert_eq!(parsed.2, None);

    // 本机没有台账行（副设备）：动作按卡片内容就地重建（同 id）。
    let out = store.act_on_digest_insight(&message_id, "todo").unwrap();
    assert_eq!(out.id, "remote-1");
    assert_eq!(
        (out.status.as_str(), out.action_type.as_deref()),
        ("acted", Some("todo"))
    );
    let title: String = store
        .connection
        .query_row(
            "SELECT title FROM todos ORDER BY created_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(title, "散步后直接就地进行小腿拉伸");
    let row = store
        .get_digest_insight_surfacing("remote-1")
        .unwrap()
        .unwrap();
    assert_eq!(row.title, "散步叠加拉伸更省成本");
    assert_eq!(row.page_slug, "habit/walk");
    assert!(!row.page_active, "页面在本机不存在时计算字段应为未生效");

    // 内容不可解析 → 明确报错，不凭猜测造动作。
    let bad = store
        .insert_message_with_kind(
            &conversation_id,
            "assistant",
            "普通文本",
            None,
            None,
            "digest_insight",
            Some("surfacing:remote-2"),
        )
        .unwrap();
    assert!(store.act_on_digest_insight(&bad, "dismiss").is_err());

    let _ = std::fs::remove_file(path);
}

#[test]
fn cognition_profile_background_injects_recent_high_value_pages_and_isolates_modes() {
    let (path, store) = insight_store();
    let conversation = main_conversation_id(&store);
    assert!(
        crate::ai::memory::user_cognition_profile_background(&store, &conversation)
            .unwrap()
            .is_none(),
        "无素材不注入"
    );
    entity_page(&store, "habit/walk", "habit", "午饭后散步");
    let message = crate::ai::memory::user_cognition_profile_background(&store, &conversation)
        .unwrap()
        .expect("有习惯/洞察素材应注入画像");
    assert!(message.content.contains("习惯规律"));
    assert!(message.content.contains("午饭后散步"));
    assert!(message.content.chars().count() <= 340, "画像应受长度约束");

    // knowledge_mentor / goal_mentor / 知识页会话隔离。
    store
        .connection
        .execute(
            "UPDATE conversations SET assistant_mode='goal_mentor' WHERE id=?1",
            [&conversation],
        )
        .unwrap();
    assert!(
        crate::ai::memory::user_cognition_profile_background(&store, &conversation)
            .unwrap()
            .is_none(),
        "目标讨论不注入画像"
    );

    let _ = std::fs::remove_file(path);
}
