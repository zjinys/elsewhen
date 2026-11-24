use super::*;

fn required<'a>(value: &'a Value, name: &str, max: usize) -> Result<&'a str> {
    let text = value[name]
        .as_str()
        .context(format!("缺少参数 {name}"))?
        .trim();
    ensure!(
        !text.is_empty() && text.chars().count() <= max,
        "参数 {name} 长度不合法"
    );
    Ok(text)
}

pub(super) fn review_claim(store: &Store, claim: &Claim) -> Result<Value> {
    let mut value = serde_json::to_value(claim)?;
    if matches!(claim.state.as_str(), "deleted" | "dismissed") { value["content"] = Value::Null; }
    for (source, basis) in value["basis"]
        .as_array_mut()
        .context("记忆依据无效")?
        .iter_mut()
        .zip(&claim.basis)
    {
        if matches!(claim.state.as_str(), "deleted" | "dismissed")
            || !store.memory_basis_current(basis)?
        {
            source["quote"] = Value::Null;
        }
    }
    Ok(value)
}

impl Store {
    /// 写入不依赖知识页的对话记忆。正文就是后续召回内容。
    pub(crate) fn propose_direct_memory(&self, conversation: &str, args: &Value) -> Result<Claim> {
        let transaction = self.begin_write_transaction()?;
        let claim = self.propose_direct_memory_inner(conversation,args)?;
        if let Some(transaction) = transaction { transaction.commit()?; }
        Ok(claim)
    }

    fn propose_direct_memory_inner(&self, conversation: &str, args: &Value) -> Result<Claim> {
        let content = required(args, "content", 6000)?.trim().to_owned();
        let message = required(args, "source_message_id", 200)?;
        let kind = required(args, "kind", 20)?;
        ensure!(matches!(kind, "semantic" | "procedural"), "记忆类型不合法");
        ensure!(self.memory_policy(&format!("blocked:{kind}"))?.as_deref() != Some("true"), "用户已禁止此类记忆候选");
        ensure!(grants::conversation_current(self, conversation)?, "会话不可用");
        let subject = required(args, "subject", 160)?.to_lowercase();
        let predicate = required(args, "predicate", 160)?.to_lowercase();
        let text: String = self.connection.query_row("SELECT content FROM messages WHERE id=?1 AND conversation_id=?2 AND role='user'", params![message, conversation], |row| row.get(0)).context("依据必须是本会话用户消息")?;
        ensure!(crate::ai::memory::history::eligible(self, message)?, "消息依据不可用");
        let body_hash = hash(&content);
        let existing = self.connection.query_row(&format!("SELECT {COLUMNS} FROM personal_memory_claims WHERE kind=?1 AND subject=?2 AND predicate=?3 AND body_hash=?4"), params![kind, subject, predicate, body_hash], row_claim).optional()?;
        if let Some(claim) = existing { return Ok(claim); }
        let suppressed: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM personal_memory_claims WHERE kind=?1 AND subject=?2 AND predicate=?3 AND content=?4 AND state IN ('dismissed','deleted'))", params![kind, subject, predicate, content], |row| row.get(0))?;
        ensure!(!suppressed, "已有用户决定不再保存此记忆");
        let normalize = |text: &str| text.chars().filter(|c| !c.is_whitespace() && !"，。！？、；：,.!?;:".contains(*c)).flat_map(char::to_lowercase).collect::<String>();
        let normalized = normalize(&content);
        let suppressed_claims = self.connection.prepare(&format!("SELECT {COLUMNS} FROM personal_memory_claims WHERE conversation_id=?1 AND kind=?2 AND subject=?3 AND predicate=?4 AND content IS NOT NULL AND (excluded=1 OR state IN ('deleted','dismissed','superseded')) LIMIT 200"))?.query_map(params![conversation,kind,subject,predicate],row_claim)?.collect::<rusqlite::Result<Vec<_>>>()?;
        for suppressed in suppressed_claims {
            if suppressed.content.as_deref().is_some_and(|text| normalize(text) == normalized) { return Ok(suppressed); }
        }
        let candidates = self.connection.prepare(&format!("SELECT {COLUMNS} FROM personal_memory_claims WHERE conversation_id=?1 AND kind=?2 AND subject=?3 AND predicate=?4 AND content IS NOT NULL AND excluded=0 AND state='pending' AND confirmed_hash IS NULL LIMIT 80"))?.query_map(params![conversation,kind,subject,predicate],row_claim)?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut supplemented = None;
        for candidate in candidates {
            if candidate.content.as_deref().is_some_and(|text| normalize(text) == normalized) {
                return Ok(candidate);
            }
            if candidate.basis.len() < 8 && self.memory_validity(&candidate)? == "current" && candidate.content.as_deref().is_some_and(|old| {
                let old = normalize(old);
                old.chars().count() >= 8 && normalized.starts_with(&old)
                    && content.chars().count() <= 600
                    && !content.contains(['不', '未', '无', '否'])
            }) { supplemented = Some(candidate); }
        }
        let id = uuid::Uuid::new_v4().to_string();
        let now = now();
        let mut basis = vec![Basis { kind: "message".into(), id: message.into(), version: hash(&text), quote: text.chars().take(480).collect(), offset: 0, independent_group: None }];
        if let Some(old) = &supplemented {
            // A lexical supplement is still derived from both statements.
            // Preserve the old evidence rather than silently replacing it.
            for source in &old.basis {
                if !basis.iter().any(|entry| entry.kind == source.kind && entry.id == source.id && entry.version == source.version) {
                    basis.push(source.clone());
                }
            }
        }
        self.connection.execute("INSERT INTO personal_memory_claims(id,page_slug,body_hash,content,kind,subject,predicate,conversation_id,basis_json,updated_at,review_after) VALUES(?1,'',?2,?3,?4,?5,?6,?7,?8,?9,?9)", params![id, body_hash, content, kind, subject, predicate, conversation, serde_json::to_string(&basis)?, now])?;
        if let Some(old) = supplemented {
            self.connection.execute("UPDATE personal_memory_claims SET state='superseded',superseded_by=?1,version=version+1,updated_at=?2 WHERE id=?3 AND state='pending' AND confirmed_hash IS NULL", params![id,now,old.id])?;
        }
        // Only flag exact opposite preferences. Broad semantic overlap is not
        // enough to replace a user-confirmed fact.
        let opposite = if let Some(value) = content.strip_prefix("用户不喜欢") {
            Some(format!("用户喜欢{value}"))
        } else { content.strip_prefix("用户喜欢").map(|value| format!("用户不喜欢{value}")) };
        if let Some(opposite) = opposite {
            let conflict: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM personal_memory_claims WHERE conversation_id=?1 AND content=?2 AND excluded=0 AND state IN ('pending','active'))", params![conversation,opposite], |row| row.get(0))?;
            if conflict { self.connection.execute("UPDATE personal_memory_claims SET state='conflicted' WHERE id=?1", [&id])?; }
        }
        self.memory_claim(&id)?.context("写入记忆后无法读取")
    }

    pub(crate) fn propose_memory(&self, conversation: &str, args: &Value) -> Result<Claim> {
        let transaction = self.begin_write_transaction()?;
        let result = self.propose_memory_inner(conversation, args)?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(result)
    }

    fn propose_memory_inner(&self, conversation: &str, args: &Value) -> Result<Claim> {
        let slug = required(args, "page_slug", 200)?;
        let kind = required(args, "kind", 20)?;
        ensure!(matches!(kind, "semantic" | "procedural"), "记忆类型不合法");
        ensure!(
            self.memory_policy(&format!("blocked:{kind}"))?.as_deref() != Some("true"),
            "用户已禁止此类记忆候选"
        );
        ensure!(
            grants::conversation_current(self, conversation)?,
            "会话不可用"
        );
        let subject = required(args, "subject", 160)?.to_lowercase();
        let predicate = required(args, "predicate", 160)?.to_lowercase();
        let direct_content = args["content"].as_str().map(str::trim).filter(|v| !v.is_empty());
        let page = self.get_wiki_page(slug)?.context("知识页不存在")?;
        ensure!(
            page.status != "archived" && page.opinion.as_deref() != Some("reject"),
            "知识页不可用"
        );
        let content = direct_content.unwrap_or(&page.content_md);
        let body_hash = hash(content);
        let suppressed = self.connection.query_row(&format!("SELECT {COLUMNS} FROM personal_memory_claims WHERE (page_slug=?4 OR body_hash=?5 OR (kind=?1 AND subject=?2 AND predicate=?3)) AND state IN ('dismissed','deleted') ORDER BY updated_at DESC LIMIT 1"),params![kind,subject,predicate,slug,body_hash],row_claim).optional()?;
        if let Some(suppressed) = suppressed {
            ensure!(
                suppressed.conversation_id == conversation,
                "已有人工决定不在本会话范围"
            );
            return Ok(suppressed);
        }
        let existing = self.connection.query_row(&format!("SELECT {COLUMNS} FROM personal_memory_claims WHERE kind=?1 AND subject=?2 AND predicate=?3 AND body_hash=?4"),params![kind,subject,predicate,body_hash],row_claim).optional()?;
        let mut basis = Vec::new();
        if let Some(message) = args["source_message_id"].as_str() {
            let text: String = self.connection.query_row("SELECT content FROM messages WHERE id=?1 AND conversation_id=?2 AND role='user'",params![message,conversation],|row|row.get(0)).context("依据必须是本会话用户消息")?;
            if let Some(snapshot) = &self.conversation_snapshot {
                ensure!(
                    snapshot.conversation_id == conversation
                        && snapshot.messages.iter().any(|item| item.id == message
                            && item.role == "user"
                            && item.content == text),
                    "来源消息不在本次允许输入中"
                );
            }
            ensure!(
                crate::ai::memory::history::eligible(self, message)?,
                "消息依据不可用"
            );
            basis.push(Basis {
                kind: "message".into(),
                id: message.into(),
                version: hash(&text),
                quote: text.chars().take(480).collect(),
                offset: 0,
                independent_group: None,
            });
        }
        for event in page
            .source_event_ids
            .iter()
            .take(8usize.saturating_sub(basis.len()))
        {
            ensure!(self.recordable_event(event)?, "事件依据不可用");
            let text: String = self.connection.query_row(
                "SELECT raw_text FROM events WHERE id=?1",
                [event],
                |row| row.get(0),
            )?;
            basis.push(Basis {
                kind: "event".into(),
                id: event.clone(),
                version: hash(&text),
                quote: text.chars().take(480).collect(),
                offset: 0,
                independent_group: None,
            });
        }
        for snapshot in self
            .page_source_snapshots(slug)?
            .into_iter()
            .take(8usize.saturating_sub(basis.len()))
        {
            let basis_item = Basis {
                kind: "snapshot".into(),
                id: snapshot.id,
                version: snapshot.content_hash,
                quote: snapshot.content_md.chars().take(480).collect(),
                offset: 0,
                independent_group: Some(snapshot.source_id),
            };
            ensure!(self.memory_basis_current(&basis_item)?, "来源快照已失效");
            basis.push(basis_item);
        }
        ensure!(
            !basis.is_empty(),
            "记忆需要有效的用户消息或事件依据，不能虚构来源"
        );
        let explicit = args["explicit_preference"].as_bool().unwrap_or(false);
        if explicit {
            let statement = page.content_md.trim();
            ensure!(
                kind == "procedural"
                    && subject == "user"
                    && statement.chars().count() <= 160
                    && ["我的偏好是", "我偏好", "我希望使用", "I prefer "]
                        .iter()
                        .any(|prefix| statement.starts_with(prefix))
                    && !['\n', '\r', '"', '“', '”', '\'', '？', '?', '：', ':']
                        .iter()
                        .any(|character| statement.contains(*character))
                    && ![
                        "如果", "假设", "也许", "可能", "别人", "他说", "她说", "例如", "举例",
                        "假如", "转述", " if ", " might "
                    ]
                    .iter()
                    .any(|word| statement.to_lowercase().contains(word))
                    && basis.iter().any(|source| source.kind == "message"
                        && source.quote.trim() == statement
                        && source.version == hash(&source.quote)),
                "仅原文完整匹配的简短直接偏好自述可记为显式输入；其他情况保留候选并审阅"
            );
        }
        if let Some(mut existing) = existing {
            ensure!(
                existing.conversation_id == conversation,
                "读取许可不授予条目修改权"
            );
            let previous = existing.basis.len();
            for source in basis {
                if existing.basis.len() < 8
                    && !existing.basis.iter().any(|old| {
                        old.kind == source.kind
                            && old.id == source.id
                            && old.version == source.version
                    })
                {
                    existing.basis.push(source);
                }
            }
            if existing.basis.len() != previous {
                self.connection.execute("UPDATE personal_memory_claims SET basis_json=?1,support='unassessed',version=version+1,updated_at=?2,review_after=?2 WHERE id=?3",params![serde_json::to_string(&existing.basis)?,now(),existing.id])?;
            }
            if explicit && existing.state == "pending" {
                let mut action = json!({"operation":"confirm","claim_id":existing.id,"scope":"conversation","reason":"explicit_user_input"});
                action["reviewed_basis"] = self.memory_action_basis(conversation, &action)?;
                self.apply_memory_action_inner(conversation, &action)?;
            }
            return self.memory_claim(&existing.id)?.context("记忆条目不存在");
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.connection.execute("INSERT INTO personal_memory_claims(id,page_slug,body_hash,content,kind,subject,predicate,conversation_id,basis_json,updated_at,review_after) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",params![id,slug,body_hash,direct_content,kind,subject,predicate,conversation,serde_json::to_string(&basis)?,now()])?;
        if explicit {
            let mut action = json!({"operation":"confirm","claim_id":id,"scope":"conversation","reason":"explicit_user_input"});
            action["reviewed_basis"] = self.memory_action_basis(conversation, &action)?;
            self.apply_memory_action_inner(conversation, &action)?;
        }
        self.memory_claim(&id)?.context("记忆写入后不存在")
    }

    pub(crate) fn memory_action_basis(&self, conversation: &str, args: &Value) -> Result<Value> {
        let operation = required(args, "operation", 40)?;
        if let Some(reason) = args.get("reason") {
            ensure!(
                reason
                    .as_str()
                    .is_some_and(|text| text.chars().count() <= 600),
                "操作说明至多 600 字符"
            );
        }
        ensure!(
            grants::conversation_current(self, conversation)?,
            "当前会话不可用"
        );
        match operation {
            "share_conversation" => self
                .memory_grant_basis(required(args, "source_conversation_id", 200)?, conversation),
            "revoke_conversation" => {
                let source = required(args, "source_conversation_id", 200)?;
                let current=self.connection.query_row("SELECT message_bound,source_hash,version FROM personal_memory_grants WHERE source_id=?1 AND target_id=?2",params![source,conversation],|row|Ok(json!({"message_bound":row.get::<_,i64>(0)?,"source_hash":row.get::<_,String>(1)?,"grant_version":row.get::<_,i64>(2)?}))).optional()?;
                Ok(current.unwrap_or_else(
                    || json!({"message_bound":0,"source_hash":"","grant_version":0}),
                ))
            }
            "block_kind" | "allow_kind" => {
                let kind = required(args, "kind", 20)?;
                ensure!(matches!(kind, "semantic" | "procedural"), "记忆类型不合法");
                Ok(json!({"policy":kind,"value":self.memory_policy(&format!("blocked:{kind}"))?}))
            }
            "enable_index" | "disable_index" => {
                Ok(json!({"index":self.memory_policy("compact_index")?}))
            }
            "pause_reviews" | "resume_reviews" => {
                Ok(json!({"reviews":self.memory_policy("reviews_paused")?}))
            }
            "rebuild_index" => self.memory_rebuild(),
            "clear_usage" => self.memory_usage_basis(),
            "confirm" | "dismiss" | "delete" | "restore" | "supersede" | "conflict" | "assess"
            | "pin" | "unpin" | "exclude" | "include" | "set_salience" | "link_goal"
            | "unlink_goal" => {
                let claim = self
                    .memory_claim(required(args, "claim_id", 200)?)?
                    .context("记忆不存在")?;
                ensure!(
                    claim.conversation_id == conversation,
                    "请在记忆所属会话审阅，跨会话读取许可不授予修改权"
                );
                if operation == "set_salience" {
                    ensure!(
                        matches!(args["salience"].as_str(), Some("low" | "normal" | "high")),
                        "显著性不合法"
                    );
                }
                let goal = if matches!(operation, "link_goal" | "unlink_goal") {
                    let goal_id = required(args, "goal_id", 200)?;
                    let goal = self.memory_goal_context(goal_id)?;
                    if operation == "link_goal" {
                        ensure!(
                            goal.as_ref().is_some_and(|goal| goal["status"] == "active"),
                            "只能关联活跃目标"
                        );
                        let reason = required(args, "reason", 600)?;
                        ensure!(!reason.is_empty(), "关联需要说明适用关系");
                    }
                    Some(json!({"goal":goal,"links":self.memory_goal_links(&claim.id)?}))
                } else {
                    None
                };
                let page = self.get_wiki_page(&claim.page_slug)?;
                let content_excerpt = if matches!(claim.state.as_str(), "deleted" | "dismissed") {
                    None
                } else {
                    claim.content.as_deref().or_else(|| page.as_ref().map(|page|page.content_md.as_str()))
                        .map(|text| text.chars().take(2400).collect::<String>())
                };
                let complete = claim.content.as_deref().or_else(|| page.as_ref().map(|page|page.content_md.as_str())).is_some_and(|text|text.chars().count() <= 2400);
                let replacement = if operation == "supersede" {
                    let replacement = self
                        .memory_claim(required(args, "replacement_id", 200)?)?
                        .context("替代条目不存在")?;
                    ensure!(
                        replacement.id != claim.id
                            && replacement.conversation_id == conversation
                            && replacement.kind == claim.kind
                            && replacement.subject == claim.subject
                            && replacement.predicate == claim.predicate,
                        "替代条目必须是同范围同主体关系的新版本"
                    );
                    let page = self.get_wiki_page(&replacement.page_slug)?;
                    let readable = !matches!(replacement.state.as_str(), "deleted" | "dismissed");
                    Some(
                        json!({"claim":review_claim(self,&replacement)?,"validity":self.memory_validity(&replacement)?,"content_excerpt":if readable {replacement.content.as_deref().or_else(||page.as_ref().map(|page|page.content_md.as_str())).map(|text|text.chars().take(2400).collect::<String>())} else {None},"complete":replacement.content.as_deref().or_else(||page.as_ref().map(|page|page.content_md.as_str())).is_some_and(|text|text.chars().count()<=2400)}),
                    )
                } else {
                    None
                };
                let assessment = if operation == "assess" {
                    let run = required(args, "assessment_run_id", 200)?;
                    let report: String=self.connection.query_row("SELECT detail FROM knowledge_background_runs WHERE id=?1 AND task='memory-review' AND status='succeeded' AND input_key=?2",params![run,format!("{}:{}",claim.id,claim.version)],|row|row.get(0)).context("自审结果不存在或对应旧版本")?;
                    Some(serde_json::from_str::<Value>(&report)?)
                } else {
                    None
                };
                Ok(
                    json!({"claim":review_claim(self,&claim)?,"page_hash":page.as_ref().map(|page|hash(&page.content_md)),"title":page.map(|page|page.title),"content_excerpt":content_excerpt,"complete":complete,"validity":self.memory_validity(&claim)?,"replacement":replacement,"assessment":assessment,"goal":goal}),
                )
            }
            _ => anyhow::bail!("未知记忆操作"),
        }
    }

    pub(crate) fn apply_memory_action(&self, conversation: &str, args: &Value) -> Result<Value> {
        let transaction = self.begin_write_transaction()?;
        let result = self.apply_memory_action_inner(conversation, args)?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(result)
    }

    fn apply_memory_action_inner(&self, conversation: &str, args: &Value) -> Result<Value> {
        let reviewed = args.get("reviewed_basis").context("缺少审阅版本")?;
        ensure!(
            &self.memory_action_basis(conversation, args)? == reviewed,
            "记忆或授权范围已变化，请重新审阅"
        );
        let operation = required(args, "operation", 40)?;
        let timestamp = now();
        let mut claim_id = None;
        let mut expected = None;
        match operation {
            "share_conversation" | "revoke_conversation" => {
                let source = required(args, "source_conversation_id", 200)?;
                let enabled = operation == "share_conversation";
                let expires = date(args["expires_at"].as_str())?;
                ensure!(
                    !enabled
                        || expires
                            .as_deref()
                            .is_none_or(|expires| expires > timestamp.as_str()),
                    "授权已过期"
                );
                self.connection.execute("INSERT INTO personal_memory_grants(source_id,target_id,message_bound,source_hash,enabled,expires_at) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(source_id,target_id) DO UPDATE SET message_bound=excluded.message_bound,source_hash=excluded.source_hash,enabled=excluded.enabled,expires_at=excluded.expires_at,version=version+1",params![source,conversation,reviewed["message_bound"].as_i64(),reviewed["source_hash"].as_str(),enabled,expires])?;
            }
            "block_kind" | "allow_kind" | "enable_index" | "disable_index" | "pause_reviews"
            | "resume_reviews" => {
                let (key, enabled) = if operation.ends_with("index") {
                    ("compact_index".into(), operation == "enable_index")
                } else if operation.ends_with("reviews") {
                    ("reviews_paused".into(), operation == "pause_reviews")
                } else {
                    (
                        format!("blocked:{}", required(args, "kind", 20)?),
                        operation == "block_kind",
                    )
                };
                self.connection.execute("INSERT INTO personal_memory_policy(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,enabled.to_string()])?;
            }
            "rebuild_index" => {
                self.memory_rebuild()?;
            }
            "clear_usage" => {
                self.connection
                    .execute("DELETE FROM personal_memory_receipts", [])?;
            }
            "pin" | "unpin" | "exclude" | "include" | "set_salience" | "link_goal"
            | "unlink_goal" => {
                let claim = self
                    .memory_claim(required(args, "claim_id", 200)?)?
                    .context("记忆不存在")?;
                ensure!(
                    !matches!(claim.state.as_str(), "deleted" | "dismissed"),
                    "需先明确恢复被删除或否定的条目"
                );
                claim_id = Some(claim.id.clone());
                expected = Some(claim.version);
                match operation {
                    "pin" | "unpin" => {
                        self.connection.execute(
                            "UPDATE personal_memory_claims SET pinned=?1 WHERE id=?2",
                            params![operation == "pin", claim.id],
                        )?;
                    }
                    "exclude" | "include" => {
                        self.connection.execute(
                            "UPDATE personal_memory_claims SET excluded=?1 WHERE id=?2",
                            params![operation == "exclude", claim.id],
                        )?;
                    }
                    "set_salience" => {
                        self.connection.execute(
                            "UPDATE personal_memory_claims SET salience=?1 WHERE id=?2",
                            params![args["salience"].as_str(), claim.id],
                        )?;
                    }
                    "link_goal" => {
                        let links = self.memory_goal_links(&claim.id)?;
                        ensure!(
                            links.len() < 8
                                || links.iter().any(|link| link["goal_id"] == args["goal_id"]),
                            "每条记忆至多关联 8 个目标"
                        );
                        self.connection.execute("INSERT INTO personal_memory_goal_links(claim_id,goal_id,reason,created_at) VALUES(?1,?2,?3,?4) ON CONFLICT(claim_id,goal_id) DO UPDATE SET reason=excluded.reason",params![claim.id,required(args,"goal_id",200)?,required(args,"reason",600)?,timestamp])?;
                    }
                    "unlink_goal" => {
                        self.connection.execute("DELETE FROM personal_memory_goal_links WHERE claim_id=?1 AND goal_id=?2",params![claim.id,required(args,"goal_id",200)?])?;
                    }
                    _ => unreachable!(),
                }
                self.connection.execute(
                    "UPDATE personal_memory_claims SET version=version+1,updated_at=?1 WHERE id=?2",
                    params![timestamp, claim.id],
                )?;
            }
            "assess" => {
                let claim = self
                    .memory_claim(required(args, "claim_id", 200)?)?
                    .context("记忆不存在")?;
                ensure!(
                    !matches!(claim.state.as_str(), "dismissed" | "deleted" | "superseded")
                        && self.memory_validity(&claim)? == "current",
                    "条目已失效"
                );
                claim_id = Some(claim.id.clone());
                expected = Some(claim.version);
                self.connection.execute("UPDATE personal_memory_claims SET support=?1,version=version+1,updated_at=?2 WHERE id=?3",params![reviewed["assessment"]["assessment"]["support"].as_str().context("自审缺少支持判断")?,timestamp,claim.id])?;
            }
            _ => {
                let claim = self
                    .memory_claim(required(args, "claim_id", 200)?)?
                    .context("记忆不存在")?;
                claim_id = Some(claim.id.clone());
                expected = Some(claim.version);
                let state = match operation {
                    "confirm" => "active",
                    "dismiss" => "dismissed",
                    "delete" => "deleted",
                    "restore" => "pending",
                    "supersede" => "superseded",
                    "conflict" => "conflicted",
                    _ => unreachable!(),
                };
                if matches!(operation, "confirm" | "supersede") {
                    ensure!(
                        !matches!(claim.state.as_str(), "deleted" | "dismissed"),
                        "需先明确恢复被删除或否定的条目"
                    );
                    let mut source_basis = claim.clone();
                    source_basis.valid_from = None;
                    source_basis.valid_until = None;
                    ensure!(
                        self.memory_validity(&source_basis)? == "current",
                        "来源或条目版本不可用"
                    );
                }
                let scope = args["scope"].as_str().unwrap_or("conversation");
                ensure!(
                    matches!(scope, "reference" | "conversation" | "personal"),
                    "使用范围不合法"
                );
                if scope == "personal" {
                    let conversation_data = self
                        .get_conversation(conversation)?
                        .context("所属主对话不存在")?;
                    ensure!(
                        conversation_data.wiki_page_slug.is_none()
                            && conversation_data.goal_id.is_none(),
                        "个人长期记忆只能在主对话中确认"
                    );
                }
                let from = date(args["valid_from"].as_str())?;
                let until = date(args["valid_until"].as_str())?;
                ensure!(
                    from.as_ref()
                        .zip(until.as_ref())
                        .is_none_or(|(from, until)| from < until),
                    "适用时间范围不合法"
                );
                if operation == "supersede" {
                    let replacement = self
                        .memory_claim(required(args, "replacement_id", 200)?)?
                        .context("替代条目不存在")?;
                    let mut source_basis = replacement.clone();
                    source_basis.valid_from = None;
                    source_basis.valid_until = None;
                    ensure!(
                        !matches!(replacement.state.as_str(), "deleted" | "dismissed")
                            && self.memory_validity(&source_basis)? == "current",
                        "替代条目不可用"
                    );
                    self.connection.execute("UPDATE personal_memory_claims SET state='active',scope=?1,confirmed_hash=body_hash,version=version+1,updated_at=?2,valid_from=?3,valid_until=?4 WHERE id=?5",params![scope,timestamp,from,until,replacement.id])?;
                }
                self.connection.execute("UPDATE personal_memory_claims SET state=?1,scope=CASE WHEN ?2='confirm' THEN ?3 ELSE scope END,confirmed_hash=CASE WHEN ?2='confirm' THEN body_hash WHEN ?2='restore' THEN NULL ELSE confirmed_hash END,version=version+1,updated_at=?4,superseded_by=?5,valid_from=CASE WHEN ?2='confirm' THEN ?6 ELSE valid_from END,valid_until=CASE WHEN ?2='confirm' THEN ?7 ELSE valid_until END WHERE id=?8",params![state,operation,scope,timestamp,args["replacement_id"].as_str(),from,until,claim.id])?;
                if operation == "confirm" {
                    self.connection.execute("UPDATE personal_memory_claims SET state='conflicted',version=version+1,updated_at=?1 WHERE kind=?2 AND subject=?3 AND predicate=?4 AND conversation_id=?5 AND state='active' AND id<>?6 AND body_hash<>?7",params![timestamp,claim.kind,claim.subject,claim.predicate,conversation,claim.id,claim.body_hash])?;
                    let conflict: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM personal_memory_claims WHERE kind=?1 AND subject=?2 AND predicate=?3 AND conversation_id=?4 AND state='conflicted')",params![claim.kind,claim.subject,claim.predicate,conversation],|row|row.get(0))?;
                    if conflict {
                        self.connection.execute(
                            "UPDATE personal_memory_claims SET state='conflicted' WHERE id=?1",
                            [&claim.id],
                        )?;
                    }
                }
            }
        }
        self.connection.execute("INSERT INTO personal_memory_decisions(id,claim_id,conversation_id,operation,expected_version,args_json,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![uuid::Uuid::new_v4().to_string(),claim_id,conversation,operation,expected,serde_json::to_string(&json!({"scope":args["scope"],"reason":args["reason"],"replacement_id":args["replacement_id"],"source_conversation_id":args["source_conversation_id"],"assessment_run_id":args["assessment_run_id"],"valid_from":args["valid_from"],"valid_until":args["valid_until"],"expires_at":args["expires_at"],"salience":args["salience"],"goal_id":args["goal_id"]}))?,timestamp])?;
        Ok(json!({"operation":operation,"status":"applied","claim_id":claim_id}))
    }
}
