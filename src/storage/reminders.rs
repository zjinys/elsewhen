//! 定时提醒（migration 50）：到期弹出、确认与推迟。
//!
//! 纪律（评审已通过，见 notes/proposed/architecture/2026-10-19-timed-reminders.md §8）：
//! - `todo_reminder_fires` 是本机去重表，**不进快照**（多设备各弹各的）；
//! - 弹出条件 `reminder=1 AND status='open' AND due_at<=now AND 本机未触发过`；
//! - **snooze 必须在同一事务里更新 due_at 并删除 fire 行**——否则「本机未触发过」
//!   永假，snooze 后该提醒在本设备永不重响（设计稿 D2 曾漏了这一步，评审修掉）；
//! - 确认（完成）也删 fire 行：若用户事后把待办改回 open，允许它在新的到期时刻再响。

use anyhow::Result;
use rusqlite::params;

use super::{Store, Todo};

/// 到期应弹的提醒（展示用最小字段面）。
#[derive(Debug, Clone)]
pub struct DueReminder {
    pub todo_id: String,
    pub title: String,
    pub due_at: String,
}

impl Store {
    /// 查询当前应弹出的提醒（不含已触发记录），不改任何状态。
    /// 先写 fire 行再弹出由调用方（API 层）负责，两步之间应用退出最多重弹一次。
    pub fn due_reminders(&self, now: &str, device_id: &str) -> Result<Vec<DueReminder>> {
        let mut statement = self.connection.prepare(
            "SELECT t.id, t.title, t.due_at
             FROM todos t
             WHERE t.reminder = 1
               AND t.status = 'open'
               AND t.due_at IS NOT NULL
               AND t.due_at <= ?1
               AND NOT EXISTS (
                   SELECT 1 FROM todo_reminder_fires f
                   WHERE f.todo_id = t.id AND f.device_id = ?2
               )
             ORDER BY t.due_at",
        )?;
        let rows = statement.query_map(params![now, device_id], |row| {
            Ok(DueReminder {
                todo_id: row.get(0)?,
                title: row.get(1)?,
                due_at: row.get(2)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 记录「已在本机弹出」（弹出窗口前调用）。重复调用幂等（同一设备一次）。
    pub fn record_reminder_fire(&self, todo_id: &str, device_id: &str) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO todo_reminder_fires (todo_id, fired_at, device_id)
             VALUES (?1, ?2, ?3)",
            params![todo_id, chrono::Utc::now().to_rfc3339(), device_id],
        )?;
        Ok(())
    }

    /// 确认提醒 = 完成该待办，并清除本机触发记录（同事务）。
    pub fn confirm_reminder(&self, todo_id: &str, device_id: &str) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let changed = self.connection.execute(
            "UPDATE todos SET status='done', updated_at=?1 WHERE id=?2",
            params![chrono::Utc::now().to_rfc3339(), todo_id],
        )?;
        if changed > 0 {
            self.sync_record_todo(todo_id, false)?;
        }
        self.connection.execute(
            "DELETE FROM todo_reminder_fires WHERE todo_id=?1 AND device_id=?2",
            params![todo_id, device_id],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 推迟提醒：**同事务**把 due_at 顺延并删除本机触发记录。
    /// 分两步就会踩「due_at 已推后但 fire 行还在 → 本机永不再弹」的坑（评审 3.1）。
    pub fn snooze_reminder(&self, todo_id: &str, minutes: i64, device_id: &str) -> Result<Todo> {
        let until = (chrono::Utc::now() + chrono::Duration::minutes(minutes.max(1))).to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let changed = self.connection.execute(
            "UPDATE todos SET due_at=?1, updated_at=?2 WHERE id=?3",
            params![until, chrono::Utc::now().to_rfc3339(), todo_id],
        )?;
        if changed == 0 {
            anyhow::bail!("未找到待办（id={todo_id}）");
        }
        self.sync_record_todo(todo_id, false)?;
        self.connection.execute(
            "DELETE FROM todo_reminder_fires WHERE todo_id=?1 AND device_id=?2",
            params![todo_id, device_id],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        self.get_todo(todo_id)?
            .ok_or_else(|| anyhow::anyhow!("snooze_reminder: 刚更新的行读不回"))
    }

    /// 标记/取消「提醒」（待办 ↔ 提醒的身份切换）。打开时若 due_at 已过期则拒绝，
    /// 提示先改一个未来时间——否则保存下一拍到期弹出会立刻响起（评审 3.2）。
    pub fn set_todo_reminder(&self, id: &str, reminder: bool) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        if reminder {
            let due_at: Option<String> =
                self.connection
                    .query_row("SELECT due_at FROM todos WHERE id=?1", [id], |row| {
                        row.get(0)
                    })?;
            match due_at {
                Some(due) => {
                    let parsed = chrono::DateTime::parse_from_rfc3339(&due);
                    let past = matches!(parsed, Ok(ts) if ts <= chrono::Utc::now());
                    anyhow::ensure!(
                        !past,
                        "该待办的截止时间已过（{due}），请先设一个未来的提醒时间"
                    );
                }
                None => anyhow::bail!("请先为该待办设置截止时间，再开启提醒"),
            }
        }
        let changed = self.connection.execute(
            "UPDATE todos SET reminder=?1, updated_at=?2 WHERE id=?3",
            params![reminder as i64, chrono::Utc::now().to_rfc3339(), id],
        )?;
        if changed > 0 {
            self.sync_record_todo(id, false)?;
        }
        // 关掉提醒时清掉本机触发记录，重新开启后可在新时刻再响。
        if !reminder {
            self.connection
                .execute("DELETE FROM todo_reminder_fires WHERE todo_id=?1", [id])?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
}
