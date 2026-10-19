//! 提醒时间解析（确定性，不依赖模型）。输入是本地自然语言时间，
//! 输出是 RFC3339 due_at。解析不出明确时间返回 None，由调用方回显
//! 「无法确定时间」确认卡片，绝不静默乱猜（评审 3.4）。

/// 从文本中解析出提醒时间（相对 now，返回 RFC3339 UTC）。
pub fn parse_reminder_time(text: &str, now: chrono::DateTime<chrono::Local>) -> Option<String> {
    let lower = text.to_lowercase();
    // 相对时长：N 分钟/小时/天 后
    if let Some(mins) = parse_relative(&lower) {
        return Some((now + chrono::Duration::minutes(mins)).with_timezone(&chrono::Utc).to_rfc3339());
    }
    // 「明早/明晚/今晚/下午 N 点」等口语
    if let Some(dt) = parse_named(&lower, now) {
        return Some(dt.with_timezone(&chrono::Utc).to_rfc3339());
    }
    // 「HH:MM」当天/次日最近一个该时刻
    if let Some(dt) = parse_clock(&lower, now) {
        return Some(dt.with_timezone(&chrono::Utc).to_rfc3339());
    }
    None
}

fn parse_relative(text: &str) -> Option<i64> {
    let num = |s: &str| -> Option<i64> {
        s.trim().parse::<i64>().ok().or(match s.trim() {
            "一" => Some(1), "两" | "二" => Some(2), "三" => Some(3),
            "五" => Some(5), "十" => Some(10), "半" => None,
            _ => None,
        })
    };
    // N 分钟后
    for (unit, mult) in [("分钟", 1), ("小时", 60), ("天", 1440)] {
        if let Some(pos) = text.find(&format!("{unit}后")) {
            let before = text[..pos].chars().rev().take_while(|c| c.is_ascii_digit() || "一两二三五十年".contains(*c)).collect::<String>();
            let before: String = before.chars().rev().collect();
            if let Some(n) = num(&before) { return Some(n * mult); }
        }
    }
    // 半小时后
    if text.contains("半小时后") { return Some(30); }
    None
}

fn parse_named(text: &str, now: chrono::DateTime<chrono::Local>) -> Option<chrono::DateTime<chrono::Local>> {
    use chrono::Timelike;
    let day_offset = if text.contains("后天") { 2 } else if text.contains("明") { 1 } else { 0 };
    let hour = if text.contains("早上") || text.contains("早晨") || text.contains("明早") { 8 }
        else if text.contains("中午") { 12 }
        else if text.contains("下午") { 15 }
        else if text.contains("晚上") || text.contains("今晚") || text.contains("明晚") { 20 }
        else { return None; };
    // 「晚上 N 点」
    if let Some(pos) = text.find('点') {
        let before: String = text[..pos].chars().rev().take_while(|c| c.is_ascii_digit()).collect::<String>().chars().rev().collect();
        if let Ok(h) = before.parse::<u32>() {
            let pm = text.contains("下午") || text.contains("晚") || text.contains("夜");
            let h = if pm && h < 12 { h + 12 } else { h };
            let date = (now + chrono::Duration::days(day_offset)).date_naive();
            return date.and_hms_opt(h, 0, 0).and_then(|naive| naive.and_local_timezone(chrono::Local).single());
        }
    }
    let date = (now + chrono::Duration::days(day_offset)).date_naive();
    let base = date.and_hms_opt(hour, 0, 0)?.and_local_timezone(chrono::Local).single()?;
    // 已过的「今天」推到明天
    if day_offset == 0 && base <= now { return date.and_hms_opt(hour,0,0)?.and_local_timezone(chrono::Local).single()?.checked_add_days(chrono::Days::new(1)).map(|d| d.with_hour(hour).unwrap_or(d)); }
    Some(base)
}

fn parse_clock(text: &str, now: chrono::DateTime<chrono::Local>) -> Option<chrono::DateTime<chrono::Local>> {
    let bytes = text.as_bytes();
    for i in 0..bytes.len().saturating_sub(4) {
        if bytes[i].is_ascii_digit() && bytes[i+1].is_ascii_digit() && bytes[i+2] == b':'
            && bytes[i+3].is_ascii_digit() && bytes[i+4].is_ascii_digit() {
            let h: u32 = text[i..i+2].parse().ok()?;
            let m: u32 = text[i+3..i+5].parse().ok()?;
            if h < 24 && m < 60 {
                let date = now.date_naive();
                let candidate = date.and_hms_opt(h, m, 0)?.and_local_timezone(chrono::Local).single()?;
                return Some(if candidate > now { candidate } else { candidate + chrono::Duration::days(1) });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> chrono::DateTime<chrono::Local> {
        // 2026-10-19 14:00 本地
        chrono::Local.with_ymd_and_hms(2026, 10, 19, 14, 0, 0).single().unwrap()
    }

    #[test]
    fn parses_relative_minutes() {
        let r = parse_reminder_time("提醒我20分钟后喝水", now()).unwrap();
        let dt = chrono::DateTime::parse_from_rfc3339(&r).unwrap();
        assert_eq!(dt.with_timezone(&chrono::Local).format("%H:%M").to_string(), "14:20");
    }

    #[test]
    fn parses_half_hour() {
        let r = parse_reminder_time("半小时后提醒我", now()).unwrap();
        let dt = chrono::DateTime::parse_from_rfc3339(&r).unwrap();
        assert_eq!(dt.with_timezone(&chrono::Local).format("%H:%M").to_string(), "14:30");
    }

    #[test]
    fn parses_clock_today_future() {
        let r = parse_reminder_time("提醒我 16:30", now()).unwrap();
        let dt = chrono::DateTime::parse_from_rfc3339(&r).unwrap();
        let local = dt.with_timezone(&chrono::Local);
        assert_eq!(local.format("%H:%M").to_string(), "16:30");
        assert_eq!(local.format("%m-%d").to_string(), "10-19", "16:30 未过，应是今天");
    }

    #[test]
    fn parses_clock_past_rolls_to_tomorrow() {
        let r = parse_reminder_time("提醒我 09:00", now()).unwrap();
        let dt = chrono::DateTime::parse_from_rfc3339(&r).unwrap();
        let local = dt.with_timezone(&chrono::Local);
        assert_eq!(local.format("%m-%d").to_string(), "10-20", "09:00 已过，应推到明天");
    }

    #[test]
    fn parses_named_tonight() {
        let r = parse_reminder_time("今晚提醒我吃药", now()).unwrap();
        let dt = chrono::DateTime::parse_from_rfc3339(&r).unwrap();
        assert_eq!(dt.with_timezone(&chrono::Local).format("%H:%M").to_string(), "20:00");
    }

    #[test]
    fn parses_evening_n_o_clock_pm() {
        let r = parse_reminder_time("晚上7点提醒我", now()).unwrap();
        let dt = chrono::DateTime::parse_from_rfc3339(&r).unwrap();
        assert_eq!(dt.with_timezone(&chrono::Local).format("%H:%M").to_string(), "19:00");
    }

    #[test]
    fn returns_none_when_no_time() {
        assert!(parse_reminder_time("提醒我一下", now()).is_none());
        assert!(parse_reminder_time("随便看看", now()).is_none());
    }
}
