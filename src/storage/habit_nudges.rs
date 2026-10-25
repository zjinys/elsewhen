//! 习惯主动问候（migration 57）：计划 CRUD、到期查询、消息落库与回答。
//!
//! 纪律：
//! - `habit_nudges` 是同步行，捕获必须与业务写同事务；
//! - `habit_nudge_fires` 是本机去重/回答状态，不进快照；
//! - 问候消息进入主对话历史，`kind='habit_nudge'`，回答 `kind='habit_checkin'`；
//! - 触发只做本地墙钟判断，不调用模型；应用关闭时跨天不补、22:00 后不补。

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use super::{HabitNudge, HabitNudgeFire, NewEvent, Store};
use anyhow::{Context, Result, ensure};

fn row_to_nudge(row: &rusqlite::Row<'_>) -> rusqlite::Result<HabitNudge> {
    Ok(HabitNudge {
        id: row.get(0)?,
        page_slug: row.get(1)?,
        question: row.get(2)?,
        done_label: row.get(3)?,
        done_text: row.get(4)?,
        missed_label: row.get(5)?,
        missed_text: row.get(6)?,
        weekdays: row.get(7)?,
        local_time: row.get(8)?,
        enabled: row.get::<_, i64>(9)? != 0,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn row_to_fire(row: &rusqlite::Row<'_>) -> rusqlite::Result<HabitNudgeFire> {
    Ok(HabitNudgeFire {
        nudge_id: row.get(0)?,
        local_date: row.get(1)?,
        fired_at: row.get(2)?,
        ask_count: row.get(3)?,
        snooze_until: row.get(4)?,
        response: row.get(5)?,
        device_id: row.get(6)?,
        message_id: row.get(7)?,
    })
}

const NUDGE_COLS: &str = "id,page_slug,question,done_label,done_text,missed_label,missed_text,weekdays,local_time,enabled,created_at,updated_at";
const NUDGE_COLS_N: &str = "n.id,n.page_slug,n.question,n.done_label,n.done_text,n.missed_label,n.missed_text,n.weekdays,n.local_time,n.enabled,n.created_at,n.updated_at";

fn validate_plan(
    page_slug: &str,
    question: &str,
    done_label: &str,
    done_text: &str,
    missed_label: &str,
    missed_text: &str,
    weekdays: i32,
    local_time: &str,
) -> Result<()> {
    ensure!(
        !page_slug.trim().is_empty() && !page_slug.chars().any(char::is_control),
        "习惯页 slug 不合法"
    );
    for (label, value, max) in [
        ("问法", question, 500usize),
        ("完成按钮", done_label, 12usize),
        ("完成记录", done_text, 500usize),
        ("未完成按钮", missed_label, 12usize),
        ("未完成记录", missed_text, 500usize),
    ] {
        ensure!(
            !value.trim().is_empty() && value.chars().count() <= max,
            "{label}不能为空且不能超过 {max} 字"
        );
    }
    ensure!((1..=127).contains(&weekdays), "重复星期不合法");
    let parsed = NaiveTime::parse_from_str(local_time, "%H:%M")
        .with_context(|| format!("时间格式不合法：{local_time}"))?;
    ensure!(
        parsed >= NaiveTime::from_hms_opt(7, 0, 0).unwrap()
            && parsed <= NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
        "问候时间需在 07:00–22:00 之间"
    );
    Ok(())
}

fn local_date(now: DateTime<Local>) -> String {
    now.format("%Y-%m-%d").to_string()
}

fn weekday_bit(now: DateTime<Local>) -> i32 {
    1 << (now.weekday().number_from_monday() - 1)
}

fn parse_ref(reference: &str) -> Option<(&str, &str)> {
    let mut parts = reference.splitn(3, ':');
    let nudge = parts.next()?;
    let date = parts.next()?;
    if nudge.is_empty() || date.len() != 10 {
        return None;
    }
    Some((nudge, date))
}

fn device_id(store: &Store) -> String {
    store
        .sync_identity()
        .ok()
        .flatten()
        .map(|identity| identity.device_id)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "local".to_string())
}

impl Store {
    pub fn list_habit_nudges(&self) -> Result<Vec<HabitNudge>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {NUDGE_COLS} FROM habit_nudges ORDER BY created_at,id"
        ))?;
        let rows = statement.query_map([], row_to_nudge)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn get_habit_nudge(&self, id: &str) -> Result<Option<HabitNudge>> {
        self.connection
            .query_row(
                &format!("SELECT {NUDGE_COLS} FROM habit_nudges WHERE id=?1"),
                [id],
                row_to_nudge,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn get_habit_nudge_for_page(&self, page_slug: &str) -> Result<Option<HabitNudge>> {
        self.connection
            .query_row(
                &format!("SELECT {NUDGE_COLS} FROM habit_nudges WHERE page_slug=?1"),
                [page_slug],
                row_to_nudge,
            )
            .optional()
            .map_err(Into::into)
    }

    /// 新建或更新一个习惯页的唯一计划。只有 `kind='habit'` 的页面可写。
    #[allow(clippy::too_many_arguments)]
    pub fn save_habit_nudge(
        &self,
        page_slug: &str,
        question: &str,
        done_label: &str,
        done_text: &str,
        missed_label: &str,
        missed_text: &str,
        weekdays: i32,
        local_time: &str,
        enabled: bool,
    ) -> Result<HabitNudge> {
        validate_plan(
            page_slug,
            question,
            done_label,
            done_text,
            missed_label,
            missed_text,
            weekdays,
            local_time,
        )?;
        let kind: Option<String> = self
            .connection
            .query_row(
                "SELECT kind FROM wiki_pages WHERE slug=?1",
                [page_slug],
                |row| row.get(0),
            )
            .optional()?;
        ensure!(
            kind.as_deref() == Some("habit"),
            "只有习惯页可以设置定期问候"
        );

        let now = chrono::Utc::now().to_rfc3339();
        let transaction = self.begin_write_transaction()?;
        let existing = self.get_habit_nudge_for_page(page_slug)?;
        let id = if let Some(existing) = &existing {
            self.connection.execute(
                "UPDATE habit_nudges
                 SET question=?1,done_label=?2,done_text=?3,missed_label=?4,missed_text=?5,
                     weekdays=?6,local_time=?7,enabled=?8,updated_at=?9
                 WHERE id=?10",
                params![
                    question.trim(),
                    done_label.trim(),
                    done_text.trim(),
                    missed_label.trim(),
                    missed_text.trim(),
                    weekdays,
                    local_time,
                    enabled as i64,
                    now,
                    existing.id
                ],
            )?;
            existing.id.clone()
        } else {
            let id = Uuid::new_v4().to_string();
            self.connection.execute(
                "INSERT INTO habit_nudges
                 (id,page_slug,question,done_label,done_text,missed_label,missed_text,weekdays,local_time,enabled,created_at,updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)",
                params![
                    id,
                    page_slug,
                    question.trim(),
                    done_label.trim(),
                    done_text.trim(),
                    missed_label.trim(),
                    missed_text.trim(),
                    weekdays,
                    local_time,
                    enabled as i64,
                    now
                ],
            )?;
            id
        };
        self.sync_record_habit_nudge(&id, false)?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        self.get_habit_nudge(&id)?.context("习惯问候保存后不存在")
    }

    pub fn delete_habit_nudge(&self, id: &str) -> Result<bool> {
        let transaction = self.begin_write_transaction()?;
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM habit_nudges WHERE id=?1)",
            [id],
            |row| row.get(0),
        )?;
        if exists {
            self.sync_record_habit_nudge(id, true)?;
            self.connection
                .execute("DELETE FROM habit_nudges WHERE id=?1", [id])?;
        }
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(exists)
    }

    /// 页面删除/改名时维护计划引用；返回被处理的计划 id。
    pub(crate) fn move_habit_nudges_page(
        &self,
        old_slug: &str,
        new_slug: Option<&str>,
    ) -> Result<Vec<String>> {
        let ids = self
            .connection
            .prepare("SELECT id FROM habit_nudges WHERE page_slug=?1")?
            .query_map([old_slug], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if ids.is_empty() {
            return Ok(ids);
        }
        let transaction = self.begin_write_transaction()?;
        for id in &ids {
            if let Some(new_slug) = new_slug {
                self.connection.execute(
                    "UPDATE habit_nudges SET page_slug=?1,updated_at=?2 WHERE id=?3",
                    params![new_slug, chrono::Utc::now().to_rfc3339(), id],
                )?;
                self.sync_record_habit_nudge(id, false)?;
            } else {
                self.sync_record_habit_nudge(id, true)?;
                self.connection
                    .execute("DELETE FROM habit_nudges WHERE id=?1", [id])?;
                self.connection
                    .execute("DELETE FROM habit_nudge_fires WHERE nudge_id=?1", [id])?;
            }
        }
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(ids)
    }

    fn list_due_habit_nudges(
        &self,
        now: DateTime<Local>,
    ) -> Result<Vec<(HabitNudge, Option<HabitNudgeFire>)>> {
        let date = local_date(now);
        let mut statement = self.connection.prepare(&format!(
            "SELECT {NUDGE_COLS_N},
                    f.nudge_id,f.local_date,f.fired_at,f.ask_count,f.snooze_until,f.response,f.device_id,f.message_id
             FROM habit_nudges n
             JOIN wiki_pages p ON p.slug=n.page_slug
             LEFT JOIN habit_nudge_fires f ON f.nudge_id=n.id AND f.local_date=?1
             WHERE n.enabled=1 AND p.kind='habit' AND p.status='active'
             ORDER BY n.local_time,n.created_at,n.id"
        ))?;
        let rows = statement.query_map([date], |row| {
            let nudge = row_to_nudge(row)?;
            let fire = if row.get::<_, Option<String>>(12)?.is_some() {
                Some(HabitNudgeFire {
                    nudge_id: row.get(12)?,
                    local_date: row.get(13)?,
                    fired_at: row.get(14)?,
                    ask_count: row.get(15)?,
                    snooze_until: row.get(16)?,
                    response: row.get(17)?,
                    device_id: row.get(18)?,
                    message_id: row.get(19)?,
                })
            } else {
                None
            };
            Ok((nudge, fire))
        })?;
        let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        let bit = weekday_bit(now);
        let now_time = now.time();
        let mut due = Vec::new();
        for (nudge, fire) in rows {
            if nudge.weekdays & bit == 0 {
                continue;
            }
            let Ok(scheduled) = NaiveTime::parse_from_str(&nudge.local_time, "%H:%M") else {
                continue;
            };
            if now_time < scheduled || now_time > NaiveTime::from_hms_opt(22, 0, 0).unwrap() {
                continue;
            }
            if let Some(fire) = &fire {
                if fire.response.is_some() {
                    continue;
                }
                let snoozed_due = fire
                    .snooze_until
                    .as_deref()
                    .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                    .map(|value| value.with_timezone(&Local) <= now)
                    .unwrap_or(false);
                if fire.ask_count < 2 && snoozed_due {
                    due.push((nudge, Some(fire.clone())));
                }
                continue;
            }
            due.push((nudge, None));
        }
        Ok(due)
    }

    fn existing_nudge_message(&self, reference: &str) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT id FROM messages WHERE kind IN ('habit_nudge','habit_nudge_suggest') AND ref=?1 ORDER BY created_at,id LIMIT 1",
                [reference],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    fn main_conversation_id(&self) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT id FROM conversations
                 WHERE archived=0 AND wiki_page_slug IS NULL AND assistant_mode != 'knowledge_mentor'
                 ORDER BY CASE WHEN title='主对话流' THEN 0 ELSE 1 END, created_at, id
                 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// 5s tick 调用：最多插入一条问候，返回本次可见的问候消息 id（可能是
    /// 另一台设备已同步过来的同 ref 消息，用于唤醒 UI，不重复插入）。
    pub fn tick_habit_nudges(&self) -> Result<Vec<String>> {
        self.tick_habit_nudges_at(Local::now())
    }

    pub(crate) fn tick_habit_nudges_at(&self, now: DateTime<Local>) -> Result<Vec<String>> {
        // 多设备同步（A 方案）：只有 owner（UI 的「主设备」）负责写入问候；
        // 其他设备只同步展示 owner 写下的消息，避免同一天各自插入一条。
        // 单机没有同步权威时本机就是唯一设备，照常触发。
        if !self.is_sync_owner_device()? {
            return Ok(Vec::new());
        }
        let Some(conversation_id) = self.main_conversation_id()? else {
            return Ok(Vec::new());
        };
        let due = self.list_due_habit_nudges(now)?;
        let Some((nudge, fire)) = due.into_iter().next() else {
            return Ok(Vec::new());
        };

        let date = local_date(now);
        // 产品原则：全局每天最多触发 2 条。统计当天首次问候与建议卡（ref 恰好是 nudge_id 加冒号加 date 的消息），补问与已关闭的不占额度。
        if fire.is_none() && self.habit_nudge_messages_on(&date)? >= 2 {
            return Ok(Vec::new());
        }
        // 连续忽略：同一习惯连续 3 天被「今天不问」后，改发确定性建议卡，不替用户改计划。
        let suggestion = fire.is_none() && self.consecutive_dismissed_days(&nudge.id, &date)? >= 3;
        let base_ref = format!("{}:{date}", nudge.id);
        let (reference, parent_message_id, ask_count) = match &fire {
            None => (base_ref.clone(), None, 1),
            Some(fire) => {
                let parent = fire
                    .message_id
                    .clone()
                    .or_else(|| self.existing_nudge_message(&base_ref).ok().flatten());
                (format!("{base_ref}:2"), parent, 2)
            }
        };

        // 多设备竞态：同 ref 已有消息时只补本机 fire 行，不再插入。
        if let Some(existing) = self.existing_nudge_message(&reference)? {
            let transaction = self.begin_write_transaction()?;
            self.upsert_fire(&nudge.id, &date, ask_count, None, None, &existing, now)?;
            if let Some(transaction) = transaction {
                transaction.commit()?;
            }
            return Ok(vec![existing]);
        }

        let transaction = self.begin_write_transaction()?;
        let (content, kind, response) = if suggestion {
            (
                format!(
                    "这条问候已经连续几天被忽略了。要不要把「{}」调个时间，或者先暂停几天？",
                    nudge.question
                ),
                "habit_nudge_suggest",
                Some("suggested"),
            )
        } else {
            (nudge.question.clone(), "habit_nudge", None)
        };
        let message_id = self.insert_message_with_kind(
            &conversation_id,
            "assistant",
            &content,
            parent_message_id.as_deref(),
            None,
            kind,
            Some(&reference),
        )?;
        self.upsert_fire(
            &nudge.id,
            &date,
            ask_count,
            None,
            response,
            &message_id,
            now,
        )?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(vec![message_id])
    }

    /// 当天已触发的首次问候与建议卡条数（全局限流用）。只数 ref 恰好是 nudge_id:date 的消息；补问（:2 后缀）与其他日期都不算。
    fn habit_nudge_messages_on(&self, date: &str) -> Result<i64> {
        self.connection
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE kind IN (?1,?2) AND length(ref)=instr(ref,char(58))+10 AND substr(ref,instr(ref,char(58))+1)=?3",
                params!["habit_nudge", "habit_nudge_suggest", date],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    /// date 之前最近 7 天里，该习惯的问候连续被「今天不问」的天数。遇到没触发过的日子或任何其它回答（done/missed/suggested）即中断。
    fn consecutive_dismissed_days(&self, nudge_id: &str, date: &str) -> Result<i64> {
        let Ok(base) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
            return Ok(0);
        };
        let start = (base - Duration::days(7)).format("%Y-%m-%d").to_string();
        let mut statement = self.connection.prepare(
            "SELECT local_date,response FROM habit_nudge_fires WHERE nudge_id=?1 AND local_date<?2 AND local_date>=?3",
        )?;
        let rows = statement.query_map(params![nudge_id, date, start], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        let mut responses = std::collections::HashMap::new();
        for row in rows {
            let (day, response) = row?;
            responses.insert(day, response);
        }
        let mut count = 0i64;
        for offset in 1..=7i64 {
            let day = (base - Duration::days(offset))
                .format("%Y-%m-%d")
                .to_string();
            match responses.get(&day).and_then(|r| r.as_deref()) {
                Some("dismissed") => count += 1,
                _ => break,
            }
        }
        Ok(count)
    }

    fn upsert_fire(
        &self,
        nudge_id: &str,
        local_date: &str,
        ask_count: i32,
        snooze_until: Option<&str>,
        response: Option<&str>,
        message_id: &str,
        now: DateTime<Local>,
    ) -> Result<()> {
        let device = device_id(self);
        self.connection.execute(
            "INSERT INTO habit_nudge_fires
             (nudge_id,local_date,fired_at,ask_count,snooze_until,response,device_id,message_id)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(nudge_id,local_date) DO UPDATE SET
               fired_at=excluded.fired_at,
               ask_count=MAX(habit_nudge_fires.ask_count,excluded.ask_count),
               snooze_until=excluded.snooze_until,
               response=COALESCE(excluded.response, habit_nudge_fires.response),
               message_id=excluded.message_id",
            params![
                nudge_id,
                local_date,
                now.with_timezone(&chrono::Utc).to_rfc3339(),
                ask_count,
                snooze_until,
                response,
                device,
                message_id
            ],
        )?;
        Ok(())
    }

    pub fn get_habit_nudge_fire(
        &self,
        nudge_id: &str,
        local_date: &str,
    ) -> Result<Option<HabitNudgeFire>> {
        self.connection
            .query_row(
                "SELECT nudge_id,local_date,fired_at,ask_count,snooze_until,response,device_id,message_id
                 FROM habit_nudge_fires WHERE nudge_id=?1 AND local_date=?2",
                params![nudge_id, local_date],
                row_to_fire,
            )
            .optional()
            .map_err(Into::into)
    }

    /// 用户按下完成/未完成。返回 (回答消息 id, 是否新建事件)。
    pub fn answer_habit_nudge(
        &self,
        nudge_message_id: &str,
        response: &str,
    ) -> Result<(String, bool)> {
        ensure!(
            matches!(response, "done" | "missed"),
            "习惯回答只能是 done 或 missed"
        );
        let (conversation_id, reference, parent_id) = self
            .connection
            .query_row(
                "SELECT conversation_id,ref,id FROM messages WHERE id=?1 AND kind='habit_nudge'",
                [nudge_message_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .context("习惯问候消息不存在")?;
        let reference = reference.context("习惯问候缺少 ref")?;
        let (nudge_id, date) = parse_ref(&reference).context("习惯问候 ref 不合法")?;
        let nudge = self
            .get_habit_nudge(nudge_id)?
            .context("习惯问候计划不存在")?;
        let text = if response == "done" {
            nudge.done_text
        } else {
            nudge.missed_text
        };

        let transaction = self.begin_write_transaction()?;
        let existing: Option<String> = self
            .connection
            .query_row(
                "SELECT id FROM messages WHERE kind='habit_checkin' AND ref=?1 AND conversation_id=?2 LIMIT 1",
                params![reference, conversation_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            if let Some(transaction) = transaction {
                transaction.commit()?;
            }
            return Ok((existing, false));
        }

        let checkin_id = self.insert_message_with_kind(
            &conversation_id,
            "user",
            &text,
            Some(&parent_id),
            None,
            "habit_checkin",
            Some(&reference),
        )?;
        let mut created_event = false;
        if response == "done" {
            let now = chrono::Utc::now();
            self.insert_event(NewEvent {
                raw_text: &text,
                occurred_at: now,
                recorded_at: now,
                source: "habit_nudge",
            })?;
            created_event = true;
        }
        self.upsert_fire(
            nudge_id,
            date,
            1,
            None,
            Some(response),
            nudge_message_id,
            Local::now(),
        )?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok((checkin_id, created_event))
    }

    pub fn snooze_habit_nudge(&self, nudge_message_id: &str) -> Result<()> {
        self.snooze_habit_nudge_at(nudge_message_id, Local::now())
    }

    pub(crate) fn snooze_habit_nudge_at(
        &self,
        nudge_message_id: &str,
        now: DateTime<Local>,
    ) -> Result<()> {
        let (reference, _) = self.nudge_message_identity(nudge_message_id)?;
        let (nudge_id, date) = parse_ref(&reference).context("习惯问候 ref 不合法")?;
        let fire = self
            .get_habit_nudge_fire(nudge_id, date)?
            .context("今天还没有触发过这条问候")?;
        ensure!(fire.ask_count < 2, "今天已经问过一次了");
        ensure!(fire.response.is_none(), "这条问候已经回答过了");
        let until = now + Duration::minutes(60);
        self.connection.execute(
            "UPDATE habit_nudge_fires SET snooze_until=?1, fired_at=?2 WHERE nudge_id=?3 AND local_date=?4",
            params![
                until.with_timezone(&chrono::Utc).to_rfc3339(),
                now.with_timezone(&chrono::Utc).to_rfc3339(),
                nudge_id,
                date
            ],
        )?;
        Ok(())
    }

    pub fn dismiss_habit_nudge(&self, nudge_message_id: &str) -> Result<()> {
        let (reference, _) = self.nudge_message_identity(nudge_message_id)?;
        let (nudge_id, date) = parse_ref(&reference).context("习惯问候 ref 不合法")?;
        self.upsert_fire(
            nudge_id,
            date,
            1,
            None,
            Some("dismissed"),
            nudge_message_id,
            Local::now(),
        )?;
        Ok(())
    }

    fn nudge_message_identity(&self, message_id: &str) -> Result<(String, String)> {
        let row: Option<(Option<String>, String)> = self
            .connection
            .query_row(
                "SELECT ref,conversation_id FROM messages WHERE id=?1 AND kind='habit_nudge'",
                [message_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (reference, conversation_id) = row.context("习惯问候消息不存在")?;
        Ok((reference.context("习惯问候缺少 ref")?, conversation_id))
    }

    /// 今日未处理问候数（侧边栏未读点）。已回答/已忽略/过期的都不算。
    ///
    /// 必须按 ref 里的日期过滤：昨天没回答的问候不能一直把未读点挂在那里。
    pub fn pending_habit_nudge_count(&self) -> Result<i32> {
        let today = Local::now().format("%Y-%m-%d").to_string();
        self.pending_habit_nudge_count_at(&today)
    }

    pub(crate) fn pending_habit_nudge_count_at(&self, date: &str) -> Result<i32> {
        self.connection
            .query_row(
                "SELECT COUNT(*) FROM messages n
                 WHERE n.kind='habit_nudge'
                   AND substr(n.ref,instr(n.ref,':')+1,10)=?1
                   AND NOT EXISTS(
                     SELECT 1 FROM messages c
                     WHERE c.kind='habit_checkin' AND c.ref=n.ref
                   )
                   AND NOT EXISTS(
                     SELECT 1 FROM habit_nudge_fires f
                     WHERE f.nudge_id=substr(n.ref,1,instr(n.ref,':')-1)
                       AND f.local_date=substr(n.ref,instr(n.ref,':')+1,10)
                       AND f.response IN ('done','missed','dismissed')
                   )",
                [date],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    /// 今日问候状态（UI 卡片显示「已记录 / 已推迟到…」）。
    pub fn list_habit_nudge_fires(&self, date: &str) -> Result<Vec<HabitNudgeFire>> {
        let mut statement = self.connection.prepare(
            "SELECT nudge_id,local_date,fired_at,ask_count,snooze_until,response,device_id,message_id
             FROM habit_nudge_fires WHERE local_date=?1 ORDER BY fired_at,nudge_id",
        )?;
        let rows = statement.query_map([date], row_to_fire)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }
}
