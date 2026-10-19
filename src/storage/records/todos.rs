//! 待办与 pending action：创建、查询、更新与删除。

use super::*;

impl Store {
    /// 新增一条待确认动作（写类工具的草拟阶段）
    pub fn create_pending_action(
        &self,
        conversation_id: &str,
        action: &str,
        args_json: &str,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO pending_actions (id, conversation_id, action, args_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, conversation_id, action, args_json, now],
        )?;
        Ok(id)
    }

    // ── 个人待办 ────────────────────────────────────────────────────────

    pub fn create_todo(
        &self,
        title: &str,
        priority: &str,
        due_at: Option<&str>,
        related_event_id: Option<&str>,
        related_wiki_slug: Option<&str>,
        note: Option<&str>,
        reminder: bool,
    ) -> Result<Todo> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        self.connection.execute(
        "INSERT INTO todos (id, title, status, priority, due_at, related_event_id, related_wiki_slug, note, reminder, created_at, updated_at)
         VALUES (?1, ?2, 'open', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![id, title, priority, due_at, related_event_id, related_wiki_slug, note, reminder as i64, now],
    )?;
        self.sync_record_todo(&id, false)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(Todo {
            id,
            title: title.to_string(),
            status: TodoStatus::Open,
            priority: priority.to_string(),
            due_at: due_at.map(|s| s.to_string()),
            reminder,
            related_event_id: related_event_id.map(|s| s.to_string()),
            related_wiki_slug: related_wiki_slug.map(|s| s.to_string()),
            note: note.map(|s| s.to_string()),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub fn list_todos(&self, status_filter: Option<&str>) -> Result<Vec<Todo>> {
        let sql = match status_filter {
        Some(_) => "SELECT id, title, status, priority, due_at, reminder, related_event_id, related_wiki_slug, note, created_at, updated_at
                    FROM todos WHERE status = ?1 ORDER BY created_at DESC",
        None => "SELECT id, title, status, priority, due_at, reminder, related_event_id, related_wiki_slug, note, created_at, updated_at
                FROM todos WHERE status != 'archived' ORDER BY status, created_at DESC",
    };
        let mut statement = self.connection.prepare(sql)?;
        let mapper = |row: &rusqlite::Row| -> rusqlite::Result<Todo> {
            Ok(Todo {
                id: row.get(0)?,
                title: row.get(1)?,
                status: TodoStatus::parse(&row.get::<_, String>(2)?),
                priority: row.get(3)?,
                due_at: row.get(4)?,
                reminder: row.get::<_, i64>(5)? != 0,
                related_event_id: row.get(6)?,
                related_wiki_slug: row.get(7)?,
                note: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        };
        let rows = match status_filter {
            Some(s) => statement.query_map(params![s], mapper)?,
            None => statement.query_map([], mapper)?,
        };
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Fetch a todo by id, including archived history. Used by on-demand
    /// work-item migration so archived todos are never silently lost.
    pub fn get_todo(&self, id: &str) -> Result<Option<Todo>> {
        // 单行直查：不做两遍全表 list 再内存 find（P2-5）
        let mapper = |row: &rusqlite::Row| -> rusqlite::Result<Todo> {
            Ok(Todo {
                id: row.get(0)?,
                title: row.get(1)?,
                status: TodoStatus::parse(&row.get::<_, String>(2)?),
                priority: row.get(3)?,
                due_at: row.get(4)?,
                reminder: row.get::<_, i64>(5)? != 0,
                related_event_id: row.get(6)?,
                related_wiki_slug: row.get(7)?,
                note: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        };
        self.connection
            .query_row(
                "SELECT id, title, status, priority, due_at, reminder, related_event_id,
                    related_wiki_slug, note, created_at, updated_at
             FROM todos WHERE id = ?1",
                [id],
                mapper,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn update_todo_status(&self, id: &str, status: TodoStatus) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let changed = self.connection.execute(
            "UPDATE todos SET status=?1, updated_at=?2 WHERE id=?3",
            params![status.as_str(), now, id],
        )?;
        if changed > 0 {
            self.sync_record_todo(id, false)?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    pub fn set_todo_related_wiki_slug(&self, id: &str, slug: &str) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        let changed = self.connection.execute(
            "UPDATE todos SET related_wiki_slug=?1, updated_at=?2 WHERE id=?3",
            params![slug, chrono::Utc::now().to_rfc3339(), id],
        )?;
        if changed == 0 {
            anyhow::bail!("未找到待办（id={id}）");
        }
        self.sync_record_todo(id, false)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// 更新待办的可编辑字段（标题 / 补充 / 优先级 / 截止时间）。
    /// 状态与关联字段保持不变；note/due_at 传 None 表示清除；
    /// priority 传 None 时回落到默认值 "normal"（列 NOT NULL）。
    pub fn update_todo(
        &self,
        id: &str,
        title: &str,
        note: Option<&str>,
        priority: Option<&str>,
        due_at: Option<&str>,
    ) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            anyhow::bail!("待办内容不能为空");
        }
        let priority = priority.unwrap_or("normal");
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let changed = self.connection.execute(
            "UPDATE todos SET title=?1, note=?2, priority=?3, due_at=?4, updated_at=?5 WHERE id=?6",
            params![title, note, priority, due_at, now, id],
        )?;
        if changed > 0 {
            self.sync_record_todo(id, false)?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    pub fn delete_todo(&self, id: &str) -> Result<bool> {
        let tx = self.begin_write_transaction()?;
        // 删除语义 = 全端物理删除（与 wiki 删除同款）：先在行还在时捕获，
        // 再删行；不存在则幂等。
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM todos WHERE id=?1)",
            [id],
            |r| r.get(0),
        )?;
        if exists {
            self.sync_record_todo(id, true)?;
            self.connection
                .execute("DELETE FROM todos WHERE id=?1", [id])?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(exists)
    }

    // ── 人物关系 ────────────────────────────────────────────────────────
}
