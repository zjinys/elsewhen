//! pending action 查询与清理：按会话/标题/id 与事件存在性。

use super::*;

impl Store {
    // ── pending actions (continued) ─────────────────────────────────────
    /// 列出某个对话里待确认的动作（按时间先后）
    pub fn pending_actions_for_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<PendingAction>> {
        let mut statement = self.connection.prepare(
            "SELECT id, conversation_id, action, args_json, created_at
         FROM pending_actions
         WHERE conversation_id = ?1 AND status = 'pending'
         ORDER BY created_at ASC",
        )?;
        let rows = statement
            .query_map(params![conversation_id], |row| {
                Ok(PendingAction {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    action: row.get(2)?,
                    args_json: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// 判断本会话是否已有同一动作、同一标题的待确认草稿。
    /// 标题是知识页创建的业务幂等键；正文允许在确认前继续完善，但不应因此产生第二个页面。
    pub fn pending_action_for_title(
        &self,
        conversation_id: &str,
        action: &str,
        title: &str,
    ) -> Result<Option<PendingAction>> {
        let actions = self.pending_actions_for_conversation(conversation_id)?;
        Ok(actions.into_iter().find(|pa| {
            if pa.action != action {
                return false;
            }
            serde_json::from_str::<serde_json::Value>(&pa.args_json)
                .ok()
                .and_then(|args| {
                    args.get("title")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .map(String::from)
                })
                .is_some_and(|pending_title| pending_title.eq_ignore_ascii_case(title.trim()))
        }))
    }

    pub fn pending_action_by_id(
        &self,
        conversation_id: &str,
        id: &str,
    ) -> Result<Option<PendingAction>> {
        Ok(self
            .pending_actions_for_conversation(conversation_id)?
            .into_iter()
            .find(|action| action.id == id))
    }

    pub fn action_exists_for_event(
        &self,
        conversation_id: &str,
        action: &str,
        event_id: &str,
    ) -> Result<bool> {
        let needle = format!("%{}%", event_id);
        Ok(self.connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pending_actions WHERE conversation_id=?1 AND action=?2 AND args_json LIKE ?3)",
        params![conversation_id, action, needle], |row| row.get(0))?)
    }

    /// 删除一条待确认动作（执行完或用户拒绝后清理）
    pub fn delete_pending_action(&self, id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM pending_actions WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn update_pending_action_args(&self, id: &str, args_json: &str) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE pending_actions SET args_json=?1 WHERE id=?2 AND status='pending'",
            params![args_json, id],
        )? > 0)
    }

    pub fn decline_pending_action(&self, id: &str) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE pending_actions SET status='declined' WHERE id=?1 AND status='pending'",
            [id],
        )? > 0)
    }

    /// 清空某个对话的全部待确认动作，返回删除条数
    pub fn delete_pending_actions_for_conversation(&self, conversation_id: &str) -> Result<usize> {
        let affected = self.connection.execute(
            "DELETE FROM pending_actions WHERE conversation_id = ?1",
            params![conversation_id],
        )?;
        Ok(affected)
    }
}
