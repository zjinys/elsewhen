//! 规则读写：列表、启停、增改删与 pending 晋升/丢弃。

use super::*;

impl Store {
    // Rules（个人经验规则库）

    /// 列出规则；status 为 None 时返回全部（active + pending）。
    /// conversation_id 为 None 时列出全局；为 Some 时只列该会话提出的 pending
    /// （v13 前遗留 conversation_id IS NULL 的规则始终包含，保证兼容）。
    pub fn list_rules(
        &self,
        status: Option<RuleStatus>,
        conversation_id: Option<&str>,
    ) -> Result<Vec<RuleSummary>> {
        let mut sql =
            "SELECT id, content, status, created_at, trigger_scene, trigger_keywords, essence FROM rules"
                .to_string();
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(s) = status {
            conditions.push("status = ?".to_string());
            params.push(Box::new(s.as_str().to_string()));
        }
        if let Some(cid) = conversation_id {
            conditions.push("(conversation_id = ? OR conversation_id IS NULL)".to_string());
            params.push(Box::new(cid.to_string()));
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY created_at ASC");
        let mut statement = self.connection.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
        let rows = statement.query_map(rusqlite::params_from_iter(param_refs), |row| {
            let status_str: String = row.get(2)?;
            Ok(RuleSummary {
                id: row.get(0)?,
                content: row.get(1)?,
                status: if status_str == "pending" {
                    RuleStatus::Pending
                } else {
                    RuleStatus::Active
                },
                created_at: row.get(3)?,
                trigger_scene: row.get(4)?,
                trigger_keywords: row.get(5)?,
                essence: row.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 已生效的规则（注入 AI 回复上下文用）
    pub fn list_active_rules(&self) -> Result<Vec<RuleSummary>> {
        self.list_rules(Some(RuleStatus::Active), None)
    }

    /// 已生效规则的数量（基座规模播报用；避免为计数拉全量行）。
    pub fn count_active_rules(&self) -> Result<i64> {
        Ok(self.connection.query_row(
            "SELECT COUNT(*) FROM rules WHERE status='active'",
            [],
            |r| r.get(0),
        )?)
    }

    /// 设置/清除规则的触发配置（决策辅助 D1）。
    /// trigger_keywords 为 JSON 字符串数组或 None（None = 不参与预筛）。
    /// trigger_scene 枚举（capture/conversation/daily_review）或 None（全场景）。
    pub fn set_rule_trigger(
        &self,
        rule_id: &str,
        trigger_scene: Option<&str>,
        trigger_keywords: Option<&[String]>,
    ) -> Result<()> {
        if let Some(scene) = trigger_scene {
            if !matches!(scene, "capture" | "conversation" | "daily_review") {
                anyhow::bail!("trigger_scene 必须是 capture/conversation/daily_review");
            }
        }
        if let Some(keywords) = trigger_keywords {
            if keywords.iter().any(|kw| kw.trim().is_empty()) {
                anyhow::bail!("触发关键词不能为空串");
            }
        }
        let keywords_json = trigger_keywords
            .map(|kw| serde_json::to_string(kw))
            .transpose()?;
        let updated = self.connection.execute(
            "UPDATE rules SET trigger_scene=?2, trigger_keywords=?3, updated_at=?4 WHERE id=?1",
            params![
                rule_id,
                trigger_scene,
                keywords_json,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        if updated == 0 {
            anyhow::bail!("规则不存在：{rule_id}");
        }
        Ok(())
    }

    /// 新增一条规则。
    /// conversation_id：来源于哪个会话（pending 确认门按会话隔离用）；None 表示不归属
    pub fn add_rule(
        &self,
        content: &str,
        status: RuleStatus,
        conversation_id: Option<&str>,
    ) -> Result<String> {
        self.add_rule_with_essence(content, status, conversation_id, None)
    }

    /// 新增一条规则（带语义内核）。
    /// essence 是 AI 在提议时提取的一句话语义标签，用于对话装配时的轻量匹配注入。
    pub fn add_rule_with_essence(
        &self,
        content: &str,
        status: RuleStatus,
        conversation_id: Option<&str>,
        essence: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        self.connection.execute(
            "INSERT INTO rules (id, content, status, conversation_id, essence, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![id, content, status.as_str(), conversation_id, essence, now],
        )?;
        self.sync_record_rule(&id, false)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(id)
    }

    /// 把本会话的待确认规则升级为生效（含 v13 前遗留的无归属规则）。
    /// 升级的每一条都在同事务捕获（规则行状态迁移随流复制）。
    pub fn promote_pending_rules(&self, conversation_id: &str) -> Result<usize> {
        let now = chrono::Utc::now().to_rfc3339();
        let tx = self.begin_write_transaction()?;
        let ids: Vec<String> = {
            let mut statement = self.connection.prepare(
                "SELECT id FROM rules WHERE status = 'pending'
                 AND (conversation_id = ?1 OR conversation_id IS NULL)",
            )?;
            statement
                .query_map([conversation_id], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for id in &ids {
            self.connection.execute(
                "UPDATE rules SET status = 'active', updated_at = ?1 WHERE id = ?2",
                params![now, id],
            )?;
            self.sync_record_rule(id, false)?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(ids.len())
    }

    /// 丢弃本会话的待确认规则（用户明确拒绝；含 v13 前遗留的无归属规则）。
    /// 逐条先捕获（deleted，行在）后删——删除语义全端物理删除。
    pub fn discard_pending_rules(&self, conversation_id: &str) -> Result<usize> {
        let tx = self.begin_write_transaction()?;
        let ids: Vec<String> = {
            let mut statement = self.connection.prepare(
                "SELECT id FROM rules WHERE status = 'pending'
                 AND (conversation_id = ?1 OR conversation_id IS NULL)",
            )?;
            statement
                .query_map([conversation_id], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for id in &ids {
            self.sync_record_rule(id, true)?;
            self.connection
                .execute("DELETE FROM rules WHERE id = ?1", [id])?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(ids.len())
    }

    /// 更新规则的语义内核（essence）。
    /// 用于 AI 回填：旧规则在迁移 56 之前没有 essence 字段。
    pub fn update_rule_essence(&self, rule_id: &str, essence: &str) -> Result<()> {
        let updated = self.connection.execute(
            "UPDATE rules SET essence = ?2, updated_at = ?3 WHERE id = ?1",
            params![rule_id, essence, chrono::Utc::now().to_rfc3339()],
        )?;
        if updated == 0 {
            anyhow::bail!("规则不存在：{rule_id}");
        }
        self.sync_record_rule(rule_id, false)?;
        Ok(())
    }

    /// 删除一条规则；返回是否存在并删除。先捕获（deleted，行在）后删。
    pub fn delete_rule(&self, id: &str) -> Result<bool> {
        let tx = self.begin_write_transaction()?;
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM rules WHERE id = ?1)",
            [id],
            |r| r.get(0),
        )?;
        if exists {
            self.sync_record_rule(id, true)?;
            self.connection
                .execute("DELETE FROM rules WHERE id = ?1", [id])?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(exists)
    }
}
