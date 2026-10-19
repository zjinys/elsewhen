//! 周期窗口与调度判据：report_window、due_reports、日期/门禁判定。

use super::*;

pub(crate) fn report_window(
    period_type: &str,
    period: NaiveDate,
) -> Result<(NaiveDate, NaiveDate)> {
    match period_type {
        "week" => Ok((period - Duration::days(6), period)),
        "month" => {
            let start =
                NaiveDate::from_ymd_opt(period.year(), period.month(), 1).context("月窗口起点")?;
            Ok((start, period))
        }
        other => anyhow::bail!("未知 period_type：{other}"),
    }
}

/// 最近一个完整周期（period 锚 = 周期末日）。
/// 今天周一 → 昨天是上周日；其他日子 → 同理取最近一个已过去的周日/上月末。
pub(crate) fn latest_complete_week(today: NaiveDate) -> NaiveDate {
    today - Duration::days((today.weekday().num_days_from_monday() as i64) + 1)
}

pub(crate) fn latest_complete_month(today: NaiveDate) -> NaiveDate {
    // 上个月末 = 本月 1 日的昨天。
    let first = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).expect("本月 1 日");
    first - Duration::days(1)
}

fn gate_is_passed(now: DateTime<Local>, day: NaiveDate) -> bool {
    // 周期结束次日的 08:00（本地）是生成时刻下限；时区跳跃日取不到该时刻
    // 时保守跳过本 tick，下个 tick 自然重试。
    match day
        .and_hms_opt(8, 0, 0)
        .map(|naive| Local.from_local_datetime(&naive))
    {
        Some(chrono::LocalResult::Single(gate)) => now >= gate,
        _ => false,
    }
}

/// 当前时刻已应生成、且仅限「最新完整周期」的待补报告清单。
/// `now` 为本地时刻（生成时刻下限：周期结束次日 08:00 后）。
pub(crate) fn due_reports(now: DateTime<Local>) -> Vec<(&'static str, NaiveDate)> {
    let today = now.date_naive();
    let mut due = Vec::new();
    let week = latest_complete_week(today);
    if gate_is_passed(now, week + Duration::days(1)) {
        due.push(("week", week));
    }
    let month = latest_complete_month(today);
    if gate_is_passed(now, month + Duration::days(1)) {
        due.push(("month", month));
    }
    due
}
