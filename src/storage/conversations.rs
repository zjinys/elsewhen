//! 会话与消息存取（从 `Store` 抽出）。
//!
//! conversations（含主对话流/wiki 页对话/归档）+ messages（父子链、token_usage）。
//! 会话标题缺省时由首条用户消息派生（derive_conversation_title）。

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use super::{ConversationSummary, DailyTokenUsage, MessageSummary, RecentUserMessage, Store};

impl Store {
    pub fn recent_user_messages(
        &self,
        limit: usize,
        max_chars: usize,
    ) -> Result<Vec<RecentUserMessage>> {
        let mut statement = self.connection.prepare(
            "SELECT conversation_id, substr(content, 1, ?2) FROM messages
         WHERE role = 'user'
         ORDER BY created_at DESC
         LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit as i64, max_chars as i64], |row| {
            Ok(RecentUserMessage {
                conversation_id: row.get(0)?,
                content: row.get(1)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    // Conversation management

    pub fn create_conversation(&self, title: Option<&str>, tag: Option<&str>) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
        "INSERT INTO conversations (id, title, tag, assistant_mode, created_at, updated_at) VALUES (?1, ?2, ?3, 'personal_secretary', ?4, ?4)",
        params![id, title, tag, now],
    )?;
        Ok(id)
    }

    /// 查询某个知识页是否已有处理会话（页内 AI 聊天），返回会话 id
    pub fn find_wiki_chat_conversation(&self, wiki_page_slug: &str) -> Result<Option<String>> {
        let mut statement = self.connection.prepare(
            "SELECT id FROM conversations
         WHERE wiki_page_slug = ?1 AND archived = 0
         ORDER BY updated_at DESC LIMIT 1",
        )?;
        let id = statement
            .query_row(params![wiki_page_slug], |row| row.get::<_, String>(0))
            .optional()?;
        Ok(id)
    }

    /// 为知识页创建处理会话（带 wiki_page_slug 关联）
    pub fn create_wiki_chat_conversation(
        &self,
        wiki_page_slug: &str,
        title: &str,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
        "INSERT INTO conversations (id, title, tag, wiki_page_slug, assistant_mode, created_at, updated_at)
         VALUES (?1, ?2, 'idea', ?3, 'knowledge_mentor', ?4, ?4)",
        params![id, title, wiki_page_slug, now],
    )?;
        Ok(id)
    }

    pub fn list_conversations(&self) -> Result<Vec<ConversationSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT c.id, c.title, c.tag, c.created_at, c.updated_at, c.archived,
                c.wiki_page_slug, c.assistant_mode,
                COUNT(m.id) as message_count,
                (SELECT m2.content FROM messages m2
                 WHERE m2.conversation_id = c.id
                 ORDER BY m2.created_at DESC, m2.rowid DESC LIMIT 1) as last_message
         FROM conversations c
         LEFT JOIN messages m ON m.conversation_id = c.id
         WHERE c.archived = 0 AND c.wiki_page_slug IS NULL
         GROUP BY c.id
         ORDER BY c.updated_at DESC, c.id DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ConversationSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                tag: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                message_count: row.get(8)?,
                last_message_preview: row.get(9)?,
                archived: row.get(5)?,
                wiki_page_slug: row.get(6)?,
                assistant_mode: row.get(7)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Archived conversations, newest first.
    pub fn list_archived_conversations(&self) -> Result<Vec<ConversationSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT c.id, c.title, c.tag, c.created_at, c.updated_at, c.archived,
                c.wiki_page_slug, c.assistant_mode,
                COUNT(m.id) as message_count,
                (SELECT m2.content FROM messages m2
                 WHERE m2.conversation_id = c.id
                 ORDER BY m2.created_at DESC, m2.rowid DESC LIMIT 1) as last_message
         FROM conversations c
         LEFT JOIN messages m ON m.conversation_id = c.id
         WHERE c.archived = 1 AND c.wiki_page_slug IS NULL
         GROUP BY c.id
         ORDER BY c.updated_at DESC, c.id DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ConversationSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                tag: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                message_count: row.get(8)?,
                last_message_preview: row.get(9)?,
                archived: row.get(5)?,
                wiki_page_slug: row.get(6)?,
                assistant_mode: row.get(7)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn rename_conversation(&self, conversation_id: &str, title: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, conversation_id],
        )?;
        Ok(())
    }

    pub fn set_conversation_archived(&self, conversation_id: &str, archived: bool) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "UPDATE conversations SET archived = ?1, updated_at = ?2 WHERE id = ?3",
            params![archived as i64, now, conversation_id],
        )?;
        Ok(())
    }

    /// 删除已归档的普通对话；知识页专用会话和未归档对话均拒绝删除。
    pub fn delete_archived_conversation(&self, conversation_id: &str) -> Result<bool> {
        let eligible: bool = self.connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1 AND archived=1 AND wiki_page_slug IS NULL)",
        [conversation_id], |row| row.get(0))?;
        if !eligible {
            return Ok(false);
        }
        Ok(self
            .connection
            .execute("DELETE FROM conversations WHERE id=?1", [conversation_id])?
            > 0)
    }

    pub fn get_conversation(&self, conversation_id: &str) -> Result<Option<ConversationSummary>> {
        self.connection
            .query_row(
                "SELECT c.id, c.title, c.tag, c.created_at, c.updated_at, c.archived,
                    c.wiki_page_slug, c.assistant_mode,
                    COUNT(m.id) as message_count,
                    (SELECT m2.content FROM messages m2
                     WHERE m2.conversation_id = c.id
                     ORDER BY m2.created_at DESC, m2.rowid DESC LIMIT 1) as last_message
             FROM conversations c
             LEFT JOIN messages m ON m.conversation_id = c.id
             WHERE c.id = ?1
             GROUP BY c.id",
                [conversation_id],
                |row| {
                    Ok(ConversationSummary {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        tag: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                        message_count: row.get(8)?,
                        last_message_preview: row.get(9)?,
                        archived: row.get(5)?,
                        wiki_page_slug: row.get(6)?,
                        assistant_mode: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    // Message management
    pub fn send_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        parent_message_id: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let transaction = self.connection.unchecked_transaction()?;

        // Insert message
        transaction.execute(
        "INSERT INTO messages (id, conversation_id, parent_message_id, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, conversation_id, parent_message_id, role, content, now],
    )?;

        // Update conversation timestamp
        transaction.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now, conversation_id],
        )?;

        transaction.commit()?;
        Ok(id)
    }

    // Token usage tracking
    /// 记录一次 AI 调用的 token 用量（provider 返回的 usage；缺失时由调用方本地估算兜底）
    pub fn record_token_usage(
        &self,
        conversation_id: Option<&str>,
        prompt_tokens: i64,
        completion_tokens: i64,
        total_tokens: i64,
        model: Option<&str>,
    ) -> Result<()> {
        self.connection.execute(
        "INSERT INTO token_usage (id, conversation_id, prompt_tokens, completion_tokens, total_tokens, model, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            Uuid::new_v4().to_string(),
            conversation_id,
            prompt_tokens,
            completion_tokens,
            total_tokens,
            model,
            chrono::Utc::now().to_rfc3339(),
        ],
    )?;
        Ok(())
    }

    /// 按天聚合最近 N 天的 token 用量（含当天），按日期倒序
    pub fn daily_token_usage(&self, days: u32) -> Result<Vec<DailyTokenUsage>> {
        let offset = format!("-{} days", days);
        let mut statement = self.connection.prepare(
            "SELECT substr(created_at, 1, 10) AS day,
                SUM(prompt_tokens), SUM(completion_tokens), SUM(total_tokens), COUNT(*)
         FROM token_usage
         WHERE created_at >= datetime('now', ?1)
         GROUP BY day
         ORDER BY day DESC",
        )?;
        let rows = statement.query_map(params![offset], |row| {
            Ok(DailyTokenUsage {
                date: row.get(0)?,
                prompt_tokens: row.get(1)?,
                completion_tokens: row.get(2)?,
                total_tokens: row.get(3)?,
                call_count: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn list_messages(&self, conversation_id: &str) -> Result<Vec<MessageSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT id, conversation_id, parent_message_id, role, content, created_at
         FROM messages
         WHERE conversation_id = ?1
         ORDER BY created_at ASC, id ASC",
        )?;
        let rows = statement.query_map([conversation_id], |row| {
            Ok(MessageSummary {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                parent_message_id: row.get(2)?,
                role: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    // Get child messages of a specific message (for branching conversations)
    pub fn get_child_messages(&self, parent_id: &str) -> Result<Vec<MessageSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT id, conversation_id, parent_message_id, role, content, created_at
         FROM messages
         WHERE parent_message_id = ?1
         ORDER BY created_at ASC, id ASC",
        )?;
        let rows = statement.query_map([parent_id], |row| {
            Ok(MessageSummary {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                parent_message_id: row.get(2)?,
                role: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    // Get the message chain from root to a specific message
    pub fn get_message_chain(&self, message_id: &str) -> Result<Vec<MessageSummary>> {
        let mut chain = Vec::new();
        let mut current_id = Some(message_id.to_string());

        while let Some(id) = current_id {
            let message: MessageSummary = self.connection.query_row(
                "SELECT id, conversation_id, parent_message_id, role, content, created_at
             FROM messages WHERE id = ?1",
                [&id],
                |row| {
                    Ok(MessageSummary {
                        id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        parent_message_id: row.get(2)?,
                        role: row.get(3)?,
                        content: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                },
            )?;

            current_id = message.parent_message_id.clone();
            chain.push(message);
        }

        chain.reverse(); // Root to leaf order
        Ok(chain)
    }
}
