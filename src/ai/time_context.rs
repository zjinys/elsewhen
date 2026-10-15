//! Application time and message time are context, never new user records.
use super::memory::ContextMessage;
use chrono::{DateTime, Duration, FixedOffset, Local, Weekday};

const REMINDER_GAP: Duration = Duration::minutes(10);

pub(crate) trait ContextClock {
    fn now(&self) -> DateTime<FixedOffset>;
    fn localize(&self, time: DateTime<FixedOffset>) -> DateTime<FixedOffset>;
}

pub(crate) struct SystemClock;

impl ContextClock for SystemClock {
    fn now(&self) -> DateTime<FixedOffset> {
        Local::now().fixed_offset()
    }

    fn localize(&self, time: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
        time.with_timezone(&Local).fixed_offset()
    }
}

fn display_time(time: DateTime<FixedOffset>) -> String {
    use chrono::Datelike;
    let weekday = match time.weekday() {
        Weekday::Mon => "星期一",
        Weekday::Tue => "星期二",
        Weekday::Wed => "星期三",
        Weekday::Thu => "星期四",
        Weekday::Fri => "星期五",
        Weekday::Sat => "星期六",
        Weekday::Sun => "星期日",
    };
    format!(
        "{} {weekday} UTC{}",
        time.format("%Y-%m-%d %H:%M:%S"),
        time.format("%:z")
    )
}

fn changed_period(previous: DateTime<FixedOffset>, current: DateTime<FixedOffset>) -> bool {
    current.date_naive() != previous.date_naive()
        || current.offset() != previous.offset()
        || current.signed_duration_since(previous) > REMINDER_GAP
        || current < previous
}

/// One sampled clock block per logical turn. Retries keep its bytes until its
/// date/offset changes, ten minutes pass, or the application clock moves back.
pub(crate) fn refresh_current_time(context: &mut Vec<ContextMessage>, clock: &dyn ContextClock) {
    let now = clock.now();
    let sent_at = context
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .and_then(|m| m.message_time);
    if let Some(previous) = context.iter().find(|m| m.sampled_time.is_some()) {
        if previous.message_time == sent_at && !changed_period(previous.sampled_time.unwrap(), now)
        {
            return;
        }
    }
    let user_time = sent_at
        .map(|time| format!("\nUser sent: {}.", display_time(clock.localize(time))))
        .unwrap_or_else(|| {
            "\nUser send time unknown; do not infer historical relative dates.".into()
        });
    context.retain(|m| m.sampled_time.is_none());
    let mut message = ContextMessage::new(
        "system",
        format!(
            "<current_time>Current time: {}.{}\nApp-local timezone; original send zone/event time unknown. Relative dates refer to message time, not now; time is not evidence, query records. No scheduled reminders.</current_time>",
            display_time(now),
            user_time
        ),
    );
    message.sampled_time = Some(now);
    message.message_time = sent_at;
    context.push(message);
}

/// Add reminders only after selection, so the first retained user gets an
/// anchor. A reminder follows its owner and is removed with that user turn by
/// the request budget. Metadata is never serialized as provider fields.
pub(crate) fn reminder_for(
    message: &ContextMessage,
    clock: &dyn ContextClock,
) -> Option<ContextMessage> {
    if message.role != "user" {
        return None;
    }
    let time = clock.localize(message.message_time?);
    let mut reminder = ContextMessage::new(
        "system",
        format!(
            "<message_time>User sent: {}</message_time>",
            display_time(time)
        ),
    );
    reminder.time_reminder = true;
    Some(reminder)
}

/// The current block already carries the latest user's exact send time.
pub(crate) fn current_anchor_index(messages: &[ContextMessage]) -> Option<usize> {
    let index = messages.iter().rposition(|m| m.role == "user")?;
    let time = messages[index].message_time?;
    messages
        .iter()
        .any(|m| m.sampled_time.is_some() && m.message_time == Some(time))
        .then_some(index)
}

pub(crate) fn enrich_history(
    messages: Vec<ContextMessage>,
    clock: &dyn ContextClock,
) -> Vec<ContextMessage> {
    let messages: Vec<_> = messages.into_iter().filter(|m| !m.time_reminder).collect();
    let covered = current_anchor_index(&messages);
    let mut enriched = Vec::with_capacity(messages.len());
    let mut previous = None;
    for (index, message) in messages.into_iter().enumerate() {
        let time = if message.role == "user" {
            message.message_time.map(|t| clock.localize(t))
        } else {
            None
        };
        let reminder = time
            .filter(|time| {
                Some(index) != covered && previous.is_none_or(|last| changed_period(last, *time))
            })
            .and_then(|_| reminder_for(&message, clock));
        enriched.push(message);
        if let Some(reminder) = reminder {
            enriched.push(reminder);
        }
        if let Some(time) = time {
            previous = Some(time);
        }
    }
    enriched
}

#[cfg(test)]
mod tests;
