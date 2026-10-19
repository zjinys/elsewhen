//! 事件写入与输入关联：insert_event 单事务边界、input_record 幂等、路由更新。

use super::*;

pub(crate) fn insert_raw_event_on(
    connection: &Connection,
    id: &str,
    occurred_at: &str,
    recorded_at: &str,
    raw_text: &str,
    source: &str,
    created_at: &str,
) -> Result<()> {
    anyhow::ensure!(!connection.is_autocommit(), "event_requires_transaction");
    connection.execute(
        "INSERT INTO events (id,occurred_at,recorded_at,raw_text,source,status,created_at,updated_at)
         VALUES (?1,?2,?3,?4,?5,'pending',?6,?6)",
        params![id,occurred_at,recorded_at,raw_text,source,created_at],
    )?;
    Ok(())
}

impl Store {
    pub fn insert_event(&self, event: NewEvent<'_>) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now_at = chrono::Utc::now();
        let now = now_at.to_rfc3339();
        let transaction = self.begin_write_transaction()?;
        let input_id = self
            .conversation_snapshot
            .as_ref()
            .filter(|_| event.source == "tool")
            .map(|snapshot| snapshot.input_message_id.as_str());
        if let Some(input_id) = input_id {
            let existing = self
                .connection
                .query_row(
                    "SELECT event_id FROM chat_input_effects WHERE input_message_id=?1",
                    [input_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?;
            if let Some(existing) = existing {
                let existing = existing.context("原记录已被移除，不会在生成重试时重新创建")?;
                if let Some(transaction) = transaction {
                    transaction.commit()?;
                }
                return Ok(existing);
            }
            let captured = self.connection.query_row(
                "SELECT event_id FROM input_records WHERE message_id=?1 AND event_id IS NOT NULL LIMIT 1",
                [input_id], |row| row.get::<_, String>(0),
            ).optional()?;
            if let Some(captured) = captured {
                if let Some(transaction) = transaction {
                    transaction.commit()?;
                }
                return Ok(captured);
            }
        }
        insert_raw_event_on(
            &self.connection,
            &id,
            &event.occurred_at.to_rfc3339(),
            &event.recorded_at.to_rfc3339(),
            event.raw_text,
            event.source,
            &now,
        )?;
        self.connection.execute(
        "INSERT INTO analysis_jobs (id, event_id, status, attempts, available_at, created_at, updated_at)
         VALUES (?1, ?2, 'pending', 0, ?3, ?3, ?3)",
        params![Uuid::new_v4().to_string(), id, now],
    )?;
        enqueue_digest_job_on(&self.connection, &id, now_at)?;
        // 规则触发预筛（D1）：确定性关键词匹配，失败静默放行（预警绝不阻塞捕获）。
        crate::storage::rule_triggers::screen_event_on(&self.connection, &id, event.raw_text);
        if let Some(input_id) = input_id {
            self.connection.execute("INSERT INTO chat_input_effects(input_message_id,event_id,created_at) VALUES(?1,?2,?3)", params![input_id,id,now])?;
        }
        self.sync_record_capture(&id, None)?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(id)
    }

    pub fn create_input_record(
        &self,
        raw_text: &str,
        source: &str,
        idempotency_key: Option<&str>,
    ) -> Result<InputRecord> {
        let raw_text = raw_text.trim();
        let source = source.trim();
        if raw_text.is_empty() {
            anyhow::bail!("input raw_text 不能为空");
        }
        if source.is_empty() {
            anyhow::bail!("input source 不能为空");
        }
        let key = idempotency_key.map(str::trim).filter(|key| !key.is_empty());
        if let Some(key) = key {
            // 幂等写入原子化（P2-3）：不做「先查后插」（check-then-insert 有并发窗口，
            // 两个同键请求会同时判定不存在并双双 INSERT），直接用部分唯一索引兜底——
            // 同名键并发时 INSERT OR IGNORE 让后写者静默跳过，再回读既有记录返回，
            // 且首个请求按 pending 落库，与原来的插入行为一致。
            self.connection.execute(
                "INSERT OR IGNORE INTO input_records
             (id,raw_text,source,route_status,idempotency_key,created_at,updated_at)
             VALUES (?1,?2,?3,'pending',?4,?5,?5)",
                params![
                    Uuid::new_v4().to_string(),
                    raw_text,
                    source,
                    key,
                    chrono::Utc::now().to_rfc3339()
                ],
            )?;
            return self
                .get_input_record_by_idempotency_key(key)?
                .context("input record 幂等键写入后读取既有记录失败");
        }

        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "INSERT INTO input_records
         (id,raw_text,source,route_status,idempotency_key,created_at,updated_at)
         VALUES (?1,?2,?3,'pending',?4,?5,?5)",
            params![id, raw_text, source, key, now],
        )?;
        self.get_input_record(&id)?
            .context("input record 创建后读取失败")
    }

    /// 普通个人输入的最小统一提交路径：input record、不可变 event 与分析任务
    /// 在同一事务内提交。网络和 AI 均不参与此路径。
    pub fn submit_input_as_event(
        &self,
        raw_text: &str,
        source: &str,
        idempotency_key: Option<&str>,
    ) -> Result<InputRecord> {
        let raw_text = raw_text.trim();
        let source = source.trim();
        if raw_text.is_empty() || source.is_empty() {
            anyhow::bail!("input raw_text 和 source 不能为空");
        }
        let key = idempotency_key.map(str::trim).filter(|key| !key.is_empty());
        if let Some(key) = key {
            if let Some(existing) = self.get_input_record_by_idempotency_key(key)? {
                return Ok(existing);
            }
        }

        let input_id = Uuid::new_v4().to_string();
        let event_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let now_text = now.to_rfc3339();
        let transaction = self.connection.unchecked_transaction()?;
        insert_raw_event_on(
            &transaction,
            &event_id,
            &now_text,
            &now_text,
            raw_text,
            source,
            &now_text,
        )?;
        transaction.execute(
            "INSERT INTO analysis_jobs
         (id,event_id,status,attempts,available_at,created_at,updated_at)
         VALUES (?1,?2,'pending',0,?3,?3,?3)",
            params![Uuid::new_v4().to_string(), event_id, now_text],
        )?;
        enqueue_digest_job_on(&transaction, &event_id, now)?;
        // 幂等键的并发窗口（先查后插存在时间差）由部分唯一索引兜底：后到的
        // 重复键在这里触发 UNIQUE 冲突——必须回滚整个事务（否则 event+job 已成
        // 孤儿行），再按幂等键回读既有记录返回（P2-3）。
        let inserted = transaction.execute(
            "INSERT INTO input_records
         (id,raw_text,source,route_status,idempotency_key,event_id,created_at,updated_at)
         VALUES (?1,?2,?3,'routed',?4,?5,?6,?6)",
            params![input_id, raw_text, source, key, event_id, now_text],
        );
        match inserted {
            Ok(_) => {}
            Err(e) if e.to_string().contains("input_records.idempotency_key") && key.is_some() => {
                transaction.rollback()?;
                return self
                    .get_input_record_by_idempotency_key(key.unwrap())?
                    .context("并发重复提交：回滚后读取既有 input record 失败");
            }
            Err(e) => return Err(e.into()),
        }
        self.sync_record_capture(&event_id, Some(&input_id))?;
        transaction.commit()?;
        self.get_input_record(&input_id)?
            .context("统一输入提交后读取失败")
    }

    /// 主对话输入：同一事务保存用户消息与个人事件，并用 input record 关联。
    pub fn submit_conversation_input(
        &self,
        conversation_id: &str,
        raw_text: &str,
        idempotency_key: Option<&str>,
    ) -> Result<InputRecord> {
        let raw_text = raw_text.trim();
        if raw_text.is_empty() {
            anyhow::bail!("input raw_text 不能为空");
        }
        let key = idempotency_key.map(str::trim).filter(|key| !key.is_empty());
        if let Some(key) = key {
            if let Some(existing) = self.get_input_record_by_idempotency_key(key)? {
                return Ok(existing);
            }
        }
        if self.get_conversation(conversation_id)?.is_none() {
            anyhow::bail!("Conversation not found: {conversation_id}");
        }

        let input_id = Uuid::new_v4().to_string();
        let event_id = Uuid::new_v4().to_string();
        let message_id = Uuid::new_v4().to_string();
        let now_at = chrono::Utc::now();
        let now = now_at.to_rfc3339();
        let transaction = self.connection.unchecked_transaction()?;
        insert_raw_event_on(
            &transaction,
            &event_id,
            &now,
            &now,
            raw_text,
            "conversation",
            &now,
        )?;
        transaction.execute(
            "INSERT INTO analysis_jobs
         (id,event_id,status,attempts,available_at,created_at,updated_at)
         VALUES (?1,?2,'pending',0,?3,?3,?3)",
            params![Uuid::new_v4().to_string(), event_id, now],
        )?;
        enqueue_digest_job_on(&transaction, &event_id, now_at)?;
        transaction.execute(
            "INSERT INTO messages (id,conversation_id,role,content,created_at)
         VALUES (?1,?2,'user',?3,?4)",
            params![message_id, conversation_id, raw_text, now],
        )?;
        transaction.execute(
            "UPDATE conversations SET updated_at=?1 WHERE id=?2",
            params![now, conversation_id],
        )?;
        // 与 submit_input_as_event 相同的幂等并发兜底（P2-3）：重复键冲突时
        // 回滚整个事务（event+job+message 一并撤销），再回读既有记录。
        let inserted = transaction.execute(
            "INSERT INTO input_records
         (id,raw_text,source,route_status,idempotency_key,event_id,message_id,created_at,updated_at)
         VALUES (?1,?2,'conversation','routed',?3,?4,?5,?6,?6)",
            params![input_id, raw_text, key, event_id, message_id, now],
        );
        match inserted {
            Ok(_) => {}
            Err(e) if e.to_string().contains("input_records.idempotency_key") && key.is_some() => {
                transaction.rollback()?;
                return self
                    .get_input_record_by_idempotency_key(key.unwrap())?
                    .context("并发重复提交：回滚后读取既有 input record 失败");
            }
            Err(e) => return Err(e.into()),
        }
        self.sync_record_capture(&event_id, Some(&input_id))?;
        transaction.commit()?;
        self.get_input_record(&input_id)?
            .context("对话统一输入提交后读取失败")
    }

    pub fn get_input_record(&self, id: &str) -> Result<Option<InputRecord>> {
        self.connection
            .query_row(
                "SELECT id,raw_text,source,route_status,idempotency_key,event_id,message_id,
                    wiki_page_slug,todo_id,created_at,updated_at
             FROM input_records WHERE id=?1",
                [id],
                map_input_record,
            )
            .optional()
            .context("read input record")
    }

    pub fn latest_event_id_for_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Option<String>> {
        if let Some(snapshot) = &self.conversation_snapshot {
            if snapshot.conversation_id == conversation_id {
                return self.connection.query_row(
                    "SELECT event_id FROM input_records WHERE message_id=?1 AND event_id IS NOT NULL
                     UNION ALL SELECT event_id FROM chat_input_effects WHERE input_message_id=?1 LIMIT 1",
                    [&snapshot.input_message_id], |row| row.get::<_, Option<String>>(0),
                ).optional().map(Option::flatten).map_err(Into::into);
            }
        }
        let event_id = self
            .connection
            .query_row(
                "SELECT i.event_id FROM messages m
         LEFT JOIN input_records i ON i.message_id=m.id
         WHERE m.conversation_id=?1 AND m.role='user'
         ORDER BY m.created_at DESC, m.id DESC LIMIT 1",
                [conversation_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(anyhow::Error::from)?;
        Ok(event_id.flatten())
    }

    pub fn conversation_id_for_event(&self, event_id: &str) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT m.conversation_id FROM input_records i
             JOIN messages m ON m.id=i.message_id
             WHERE i.event_id=?1 LIMIT 1",
                [event_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    fn get_input_record_by_idempotency_key(&self, key: &str) -> Result<Option<InputRecord>> {
        self.connection
            .query_row(
                "SELECT id,raw_text,source,route_status,idempotency_key,event_id,message_id,
                    wiki_page_slug,todo_id,created_at,updated_at
             FROM input_records WHERE idempotency_key=?1",
                [key],
                map_input_record,
            )
            .optional()
            .context("read input record by idempotency key")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_input_route(
        &self,
        id: &str,
        route_status: &str,
        event_id: Option<&str>,
        message_id: Option<&str>,
        wiki_page_slug: Option<&str>,
        todo_id: Option<&str>,
    ) -> Result<InputRecord> {
        if !matches!(
            route_status,
            "pending" | "routed" | "needs_confirmation" | "failed"
        ) {
            anyhow::bail!("非法 input route_status: {route_status}");
        }
        let changed = self.connection.execute(
            "UPDATE input_records
         SET route_status=?2,event_id=COALESCE(?3,event_id),message_id=COALESCE(?4,message_id),
             wiki_page_slug=COALESCE(?5,wiki_page_slug),todo_id=COALESCE(?6,todo_id),
             updated_at=?7 WHERE id=?1",
            params![
                id,
                route_status,
                event_id,
                message_id,
                wiki_page_slug,
                todo_id,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        if changed == 0 {
            anyhow::bail!("input record 不存在: {id}");
        }
        self.get_input_record(id)?
            .context("input route 更新后读取失败")
    }
}
