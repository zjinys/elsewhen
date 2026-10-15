use super::*;
use crate::ai::memory::{MemoryProvider, SimpleMemory, SlidingWindowMemory};

struct FixedClock(DateTime<FixedOffset>);

impl ContextClock for FixedClock {
    fn now(&self) -> DateTime<FixedOffset> {
        self.0
    }
    fn localize(&self, time: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
        time.with_timezone(self.0.offset())
    }
}

fn at(text: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(text).unwrap()
}

fn user(time: &str, text: &str) -> ContextMessage {
    let mut m = ContextMessage::new("user", text);
    m.message_time = Some(at(time));
    m
}

fn hints(messages: &[ContextMessage]) -> Vec<&ContextMessage> {
    messages.iter().filter(|m| m.time_reminder).collect()
}

#[test]
fn first_user_gets_anchor_but_short_intervals_do_not_repeat_it() {
    let clock = FixedClock(at("2026-09-30T12:00:00+08:00"));
    let messages = enrich_history(
        vec![
            ContextMessage::new("system", "stable"),
            user("2026-09-30T10:00:00+08:00", "今天的原文"),
            ContextMessage::new("assistant", "答复"),
            user("2026-09-30T10:05:00+08:00", "再补充"),
        ],
        &clock,
    );
    assert_eq!(messages[0].content, "stable");
    assert_eq!(messages[1].content, "今天的原文");
    assert_eq!(hints(&messages).len(), 1);
    assert!(
        messages[2]
            .content
            .contains("2026-09-30 10:00:00 星期三 UTC+08:00")
    );
    assert_eq!(messages.last().unwrap().content, "再补充");
}

#[test]
fn reminder_threshold_is_strictly_more_than_ten_minutes() {
    let clock = FixedClock(at("2026-09-30T12:00:00Z"));
    let messages = enrich_history(
        vec![
            user("2026-09-30T10:00:00Z", "a"),
            user("2026-09-30T10:10:00Z", "b"),
            user("2026-09-30T10:20:01Z", "c"),
        ],
        &clock,
    );
    assert_eq!(hints(&messages).len(), 2);
    assert!(hints(&messages)[1].content.contains("10:20:01"));
}

#[test]
fn midnight_gets_a_reminder_even_for_a_two_minute_gap() {
    let clock = FixedClock(at("2026-10-01T00:02:00+08:00"));
    let messages = enrich_history(
        vec![
            user("2026-09-30T23:59:00+08:00", "昨天的记录"),
            user("2026-10-01T00:01:00+08:00", "今天的记录"),
        ],
        &clock,
    );
    assert_eq!(hints(&messages).len(), 2);
    assert!(hints(&messages)[0].content.contains("2026-09-30"));
    assert!(hints(&messages)[1].content.contains("2026-10-01"));
}

#[test]
fn historical_dst_offsets_are_resolved_at_the_message_instant() {
    struct DstClock;
    impl ContextClock for DstClock {
        fn now(&self) -> DateTime<FixedOffset> {
            at("2026-11-01T01:02:00-05:00")
        }
        fn localize(&self, time: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
            let hours = if time < at("2026-11-01T06:00:00Z") {
                4
            } else {
                5
            };
            time.with_timezone(&FixedOffset::west_opt(hours * 3600).unwrap())
        }
    }
    let messages = enrich_history(
        vec![
            user("2026-11-01T05:59:00Z", "before"),
            user("2026-11-01T06:01:00Z", "after"),
        ],
        &DstClock,
    );
    assert_eq!(hints(&messages).len(), 2);
    assert!(
        hints(&messages)[0]
            .content
            .contains("01:59:00 星期日 UTC-04:00")
    );
    assert!(
        hints(&messages)[1]
            .content
            .contains("01:01:00 星期日 UTC-05:00")
    );
}

#[test]
fn replaying_enrichment_does_not_duplicate_synthetic_messages() {
    let clock = FixedClock(at("2026-09-30T12:00:00Z"));
    let once = enrich_history(vec![user("2026-09-30T10:00:00Z", "hi")], &clock);
    let twice = enrich_history(once.clone(), &clock);
    assert_eq!(
        once.iter().map(|m| &m.content).collect::<Vec<_>>(),
        twice.iter().map(|m| &m.content).collect::<Vec<_>>()
    );
}

#[test]
fn delayed_question_retains_send_date_separately_from_current_date() {
    let clock = FixedClock(at("2026-10-01T00:01:00+08:00"));
    let mut context = vec![
        ContextMessage::new("system", "stable prefix"),
        user("2026-09-30T23:59:00+08:00", "昨天做了什么"),
    ];
    refresh_current_time(&mut context, &clock);
    assert_eq!(context[0].content, "stable prefix");
    assert_eq!(context[1].content, "昨天做了什么");
    let block = &context.last().unwrap().content;
    assert!(block.contains("Current time: 2026-10-01 00:01:00"));
    assert!(block.contains("User sent: 2026-09-30 23:59:00"));
    assert!(block.contains("message time, not now"));
    assert!(block.contains("time is not evidence"));
}

#[test]
fn ordinary_retries_keep_current_block_bytes() {
    let now = at("2026-09-30T12:00:00Z");
    let mut context = vec![user("2026-09-30T11:59:00Z", "hello")];
    refresh_current_time(&mut context, &FixedClock(now));
    let before = context.last().unwrap().content.clone();
    refresh_current_time(&mut context, &FixedClock(now + Duration::minutes(9)));
    assert_eq!(context.last().unwrap().content, before);
    assert_eq!(
        context.iter().filter(|m| m.sampled_time.is_some()).count(),
        1
    );
}

#[test]
fn stale_current_block_is_replaced_without_accumulating() {
    let now = at("2026-09-30T12:00:00Z");
    let mut context = vec![user("2026-09-30T11:59:00Z", "hello")];
    refresh_current_time(&mut context, &FixedClock(now));
    refresh_current_time(&mut context, &FixedClock(now + Duration::minutes(11)));
    assert_eq!(context.len(), 2);
    assert!(context[1].content.contains("12:11:00"));
    refresh_current_time(&mut context, &FixedClock(at("2026-10-01T00:00:00Z")));
    assert_eq!(context.len(), 2);
    assert!(context[1].content.contains("2026-10-01"));
}

#[test]
fn current_offset_change_refreshes_without_assuming_east_eight() {
    let mut context = vec![user("2026-09-30T10:00:00Z", "hello")];
    refresh_current_time(&mut context, &FixedClock(at("2026-09-30T12:00:00Z")));
    refresh_current_time(&mut context, &FixedClock(at("2026-09-30T07:00:00-05:00")));
    assert_eq!(context.len(), 2);
    assert!(context[1].content.contains("UTC-05:00"));
    assert!(!context[1].content.contains("UTC+08:00"));
}

#[test]
fn clock_rollback_replaces_sample_without_negative_elapsed_claims() {
    let mut context = vec![user("2026-09-30T12:00:00Z", "hello")];
    refresh_current_time(&mut context, &FixedClock(at("2026-09-30T12:01:00Z")));
    refresh_current_time(&mut context, &FixedClock(at("2026-09-30T11:59:00Z")));
    assert_eq!(context.len(), 2);
    assert!(
        context[1]
            .content
            .contains("Current time: 2026-09-30 11:59:00")
    );
    assert!(!context[1].content.contains("分钟前"));
}

#[test]
fn missing_or_invalid_timestamp_is_not_invented() {
    let message = crate::storage::MessageSummary {
        id: "id".into(),
        conversation_id: "conversation".into(),
        parent_message_id: None,
        role: "user".into(),
        content: "昨天".into(),
        created_at: "not a timestamp".into(),
    };
    let mut context = vec![ContextMessage::from_record(message)];
    let clock = FixedClock(at("2026-09-30T12:00:00Z"));
    refresh_current_time(&mut context, &clock);
    let context = enrich_history(context, &clock);
    assert!(hints(&context).is_empty());
    assert_eq!(context[0].content, "昨天");
    assert!(context[1].content.contains("send time unknown"));
}

#[test]
fn both_memory_paths_keep_timestamps_without_writing_synthetic_records() {
    let path = std::env::temp_dir().join(format!("elsewhen-time-{}.db", uuid::Uuid::new_v4()));
    let store = crate::storage::Store::open(&path).unwrap();
    let id = store
        .create_conversation(Some("clock fixture"), None)
        .unwrap();
    store.send_message(&id, "user", "昨天的原文", None).unwrap();
    let before = store.list_messages(&id).unwrap();
    let clock = FixedClock(at("2026-10-01T00:01:00+08:00"));
    for memory in [
        Box::new(SimpleMemory::new(10)) as Box<dyn MemoryProvider>,
        Box::new(SlidingWindowMemory::new(32768)),
    ] {
        let mut context = memory.prepare_context(&id, &store).unwrap();
        let user = context.iter().find(|m| m.role == "user").unwrap();
        assert_eq!(user.message_time, Some(at(&before[0].created_at)));
        refresh_current_time(&mut context, &clock);
        let enriched = enrich_history(context, &clock);
        assert_eq!(hints(&enriched).len(), 0);
        assert_eq!(
            enriched.iter().filter(|m| m.sampled_time.is_some()).count(),
            1
        );
    }
    let after = store.list_messages(&id).unwrap();
    assert_eq!(after.len(), before.len());
    assert_eq!(after[0].content, before[0].content);
    assert_eq!(after[0].created_at, before[0].created_at);
    assert!(store.list_events().unwrap().is_empty());
    drop(store);
    let _ = std::fs::remove_file(path);
}
