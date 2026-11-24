use super::*;
use crate::storage::approvals::{ChatDecision, ChatProposal};

fn conversation_kind(store: &Store, conversation: &str) -> Result<&'static str> {
    Ok(match store.get_conversation(conversation)? {
        Some(value) if value.goal_id.is_some() => "goal",
        Some(value) if value.wiki_page_slug.is_some() => "wiki",
        Some(_) => "main",
        None => "unavailable",
    })
}

impl Store {
    pub(crate) fn edit_direct_memory(&self, id: &str, expected_version: i64, content: &str) -> Result<()> {
        let content = content.trim();
        ensure!(!content.is_empty() && content.chars().count() <= 6000, "记忆正文长度不合法");
        let transaction = self.begin_write_transaction()?;
        let claim = self.memory_claim(id)?.context("记忆不存在")?;
        ensure!(!matches!(claim.state.as_str(), "deleted" | "dismissed" | "superseded"), "请先恢复记忆再编辑");
        ensure!(claim.content.is_some(), "旧知识页记忆需先迁移后编辑");
        ensure!(claim.version == expected_version, "记忆已变化，请刷新后重试");
        let now = chrono::Utc::now().to_rfc3339();
        let updated = self.connection.execute("UPDATE personal_memory_claims SET content=?1,body_hash=?2,version=version+1,confirmed_hash=?2,state='active',updated_at=?3 WHERE id=?4 AND version=?5", rusqlite::params![content, hash(content), now, id, expected_version])?;
        ensure!(updated == 1, "记忆已变化，请刷新后重试");
        self.connection.execute("INSERT INTO personal_memory_decisions(id,claim_id,conversation_id,operation,expected_version,args_json,created_at) VALUES(?1,?2,?3,'edit_content',?4,?5,?6)", params![uuid::Uuid::new_v4().to_string(),id,claim.conversation_id,expected_version,json!({"previous_content":claim.content,"content":content}).to_string(),now])?;
        if let Some(transaction) = transaction { transaction.commit()?; }
        Ok(())
    }
    fn profile_memory_item(&self) -> Result<Option<Value>> {
        let Ok((profile, updated_at)) = self.planning_profile() else {
            return Ok(None);
        };
        let fields = [
            ("技能", profile.skills),
            ("资源", profile.resources),
            ("限制", profile.limits),
            ("偏好", profile.preferences),
        ];
        let content = fields
            .iter()
            .filter(|(_, value)| !value.trim().is_empty())
            .map(|(label, value)| format!("{label}\n{value}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        if content.is_empty() {
            return Ok(None);
        }
        Ok(Some(json!({
            "id": "profile/me",
            "version": 1,
            "title": "关于我",
            "subject": "user",
            "predicate": "personal_profile",
            "kind": "profile",
            "state": "active",
            "scope": "personal",
            "support": "supported",
            "confirmed": true,
            "validity": "current",
            "pinned": false,
            "excluded": false,
            "salience": "normal",
            "conversation_id": "",
            "conversation_title": null,
            "conversation_kind": "main",
            "content": content,
            "updated_at": updated_at,
        })))
    }

    pub(crate) fn memory_management_list(
        &self,
        filter: &Value,
        offset: u32,
        limit: u32,
    ) -> Result<Value> {
        let query = filter["query"].as_str().unwrap_or("").trim();
        ensure!(query.chars().count() <= 300, "查询至多 300 字符");
        let category = filter["conversation_kind"].as_str();
        let scope = filter["scope"].as_str();
        ensure!(
            scope.is_none_or(|value| matches!(value, "reference" | "conversation" | "personal")),
            "适用范围不合法"
        );
        let condition = "FROM personal_memory_claims m
            LEFT JOIN wiki_pages w ON w.slug=m.page_slug
            LEFT JOIN conversations c ON c.id=m.conversation_id
            WHERE (?1='' OR instr(lower(m.subject||' '||m.predicate||' '||coalesce(w.title,m.page_slug)||' '||coalesce(m.content,'')),lower(?1))>0)
            AND (?2 IS NULL OR m.state=?2) AND (?3 IS NULL OR m.kind=?3)
            AND (?4 IS NULL OR m.excluded=?4)
            AND (?5 IS NULL OR CASE WHEN c.goal_id IS NOT NULL THEN 'goal' WHEN c.wiki_page_slug IS NOT NULL THEN 'wiki' ELSE 'main' END=?5)
            AND (?6 IS NULL OR m.scope=?6)
            AND (?7 IS NULL OR m.conversation_id=?7)
            AND (?8 IS NULL OR m.subject=?8) AND (?9 IS NULL OR m.predicate=?9)";
        let parameters = params![
            query,
            filter["state"].as_str(),
            filter["kind"].as_str(),
            filter["excluded"].as_bool(),
            category,
            scope,
            filter["conversation_id"].as_str(),
            filter["subject"].as_str(),
            filter["predicate"].as_str(),
        ];
        let total: i64 = self.connection.query_row(
            &format!("SELECT count(*) {condition}"),
            parameters,
            |row| row.get(0),
        )?;
        let show_profile = filter["query"].as_str().unwrap_or("").trim().is_empty()
            && filter["state"].is_null()
            && filter["kind"].is_null()
            && filter["excluded"].is_null()
            && filter["conversation_kind"].is_null()
            && filter["scope"].is_null()
            && filter["conversation_id"].is_null()
            && filter["subject"].is_null()
            && filter["predicate"].is_null();
        let profile = show_profile
            .then(|| self.profile_memory_item())
            .transpose()?
            .flatten();
        let profile_offset = if profile.is_some() { 1 } else { 0 };
        let total = total + i64::from(profile_offset);
        let limit = limit.clamp(1, 50);
        let claim_offset = offset.saturating_sub(profile_offset);
        let ids = self.connection.prepare(&format!(
            "SELECT m.id {condition} ORDER BY m.updated_at DESC,m.id LIMIT {limit} OFFSET {claim_offset}"
        ))?.query_map(parameters, |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut items = Vec::new();
        for id in ids {
            let claim = self.memory_claim(&id)?.context("记忆不存在")?;
            let page = self.get_wiki_page(&claim.page_slug)?;
            let conversation = self.get_conversation(&claim.conversation_id)?;
            let hidden = matches!(claim.state.as_str(), "deleted" | "dismissed");
            let title = if hidden {
                "已移除的记忆".to_owned()
            } else {
                claim.content.as_deref().map(|text| text.chars().take(48).collect())
                    .or_else(|| page.as_ref().map(|page| page.title.clone()))
                    .unwrap_or_else(|| claim.page_slug.clone())
            };
            items.push(json!({
                "id":claim.id,"version":claim.version,"page_slug":claim.page_slug,
                "title":title,
                // The text shown in the settings screen is the memory itself,
                // rather than a lossy UI-only summary. Keep it bounded for the
                // list; the detail endpoint returns the complete editable text.
                "content":if hidden { None } else { claim.content.as_deref().map(|text| text.chars().take(600).collect::<String>()) },
                "subject":claim.subject,"predicate":claim.predicate,"kind":claim.kind,
                "state":claim.state,"scope":claim.scope,"support":claim.support,
                "confirmed":claim.confirmed_hash.as_deref()==Some(&claim.body_hash),
                "validity":self.memory_validity(&claim)?,"pinned":claim.pinned,
                "excluded":claim.excluded,"salience":claim.salience,
                "conversation_id":claim.conversation_id,
                "conversation_title":conversation.and_then(|value| value.title),
                "conversation_kind":conversation_kind(self,&claim.conversation_id)?,
            }));
        }
        if offset == 0 {
            if let Some(profile) = profile {
                items.insert(0, profile);
                items.truncate(limit as usize);
            }
        }
        Ok(json!({"items":items,"total":total,"offset":offset,"limit":limit}))
    }

    pub(crate) fn memory_management_detail(&self, id: &str) -> Result<Value> {
        if id == "profile/me" {
            let (profile, updated_at) = self.planning_profile()?;
            let content = [
                ("技能", profile.skills),
                ("资源", profile.resources),
                ("限制", profile.limits),
                ("偏好", profile.preferences),
            ]
            .into_iter()
            .filter(|(_, value)| !value.trim().is_empty())
            .map(|(label, value)| format!("{label}\n{value}"))
            .collect::<Vec<_>>()
            .join("\n\n");
            let mut detail = json!({
                "kind": "profile",
                "id": id,
                "title": "关于我",
                "content": content,
                "complete": true,
                "can_edit": true,
                "updated_at": updated_at,
            });
            detail["detail_token"] = hash(&detail.to_string()).into();
            return Ok(detail);
        }
        let claim = self.memory_claim(id)?.context("记忆不存在")?;
        let page = self.get_wiki_page(&claim.page_slug)?;
        let readable = !matches!(claim.state.as_str(), "deleted" | "dismissed")
            && page
                .as_ref()
                .is_some_and(|page| hash(&page.content_md) == claim.body_hash);
        let hidden = matches!(claim.state.as_str(), "deleted" | "dismissed");
        let mut decisions = self.connection.prepare(
            "SELECT operation,args_json,created_at FROM personal_memory_decisions WHERE claim_id=?1 ORDER BY rowid DESC LIMIT 20"
        )?.query_map([id], |row| Ok(json!({"operation":row.get::<_,String>(0)?,"args":serde_json::from_str::<Value>(&row.get::<_,String>(1)?).unwrap_or(Value::Null),"created_at":row.get::<_,String>(2)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        if hidden {
            for decision in &mut decisions { decision["args"] = Value::Null; }
        }
        let sources = claim.basis.iter().map(|basis| {
            let current = self.memory_basis_current(basis)?;
            Ok(json!({"kind":basis.kind,"id":basis.id,"version":basis.version,"offset":basis.offset,"current":current,
                "quote":if readable && current {Some(&basis.quote)} else {None}}))
        }).collect::<Result<Vec<_>>>()?;
        let mut detail = json!({
            "claim":writes::review_claim(self,&claim)?,
            "title":if hidden { "已移除的记忆".to_owned() } else { claim.content.as_deref().map(|text| text.chars().take(48).collect()).or_else(||page.as_ref().map(|page|page.title.clone())).unwrap_or_else(||claim.page_slug.clone()) },
            "content":if matches!(claim.state.as_str(), "deleted" | "dismissed") { None } else if let Some(content) = &claim.content { Some(content.chars().take(6000).collect::<String>()) } else if readable {page.as_ref().map(|page|page.content_md.chars().take(6000).collect::<String>())} else {None},
            "complete":claim.content.as_ref().map_or(readable && page.as_ref().is_some_and(|page|page.content_md.chars().count()<=6000), |content|content.chars().count()<=6000),
            "validity":self.memory_validity(&claim)?,"sources":sources,
            "goal_links":self.memory_goal_links(id)?,"decisions":decisions,
            "can_review":grants::conversation_current(self,&claim.conversation_id)?,
            "conversation_kind":conversation_kind(self,&claim.conversation_id)?,
            "conversation_title":self.get_conversation(&claim.conversation_id)?.and_then(|value|value.title),
        });
        detail["detail_token"] = hash(&detail.to_string()).into();
        Ok(detail)
    }

    pub(crate) fn prepare_memory_management_action(
        &self,
        id: &str,
        expected_version: i64,
        detail_token: &str,
        args: &Value,
    ) -> Result<ChatProposal> {
        let transaction = self.begin_write_transaction()?;
        let detail = self.memory_management_detail(id)?;
        ensure!(
            detail["claim"]["version"].as_i64() == Some(expected_version)
                && detail["detail_token"].as_str() == Some(detail_token),
            "记忆或来源已变化，请刷新详情后重新审阅"
        );
        ensure!(args.is_object(), "操作参数无效");
        ensure!(
            args.as_object()
                .is_some_and(|args| args.keys().all(|key| matches!(
                    key.as_str(),
                    "operation"
                        | "scope"
                        | "reason"
                        | "salience"
                        | "goal_id"
                        | "replacement_id"
                        | "valid_from"
                        | "valid_until"
                ))),
            "操作包含未知参数"
        );
        ensure!(
            matches!(
                args["operation"].as_str(),
                Some(
                    "confirm"
                        | "dismiss"
                        | "delete"
                        | "restore"
                        | "conflict"
                        | "supersede"
                        | "pin"
                        | "unpin"
                        | "exclude"
                        | "include"
                        | "set_salience"
                        | "link_goal"
                        | "unlink_goal"
                )
            ),
            "管理页不支持此操作"
        );
        let conversation = detail["claim"]["conversation_id"]
            .as_str()
            .context("缺少所属会话")?;
        let mut prepared = args.clone();
        prepared["claim_id"] = id.into();
        prepared["reviewed_basis"] = self.memory_action_basis(conversation, &prepared)?;
        prepared["title"] = detail["title"].clone();
        let proposal_id =
            self.create_pending_action(conversation, "review_memory", &prepared.to_string())?;
        let proposal = self
            .chat_proposal("action", &proposal_id, Some(conversation))?
            .context("审阅卡不存在")?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(proposal)
    }

    pub(crate) fn decide_memory_management_action(
        &self,
        id: &str,
        conversation: &str,
        version: i64,
        approval_token: &str,
        accept: bool,
    ) -> Result<ChatDecision> {
        let action = self
            .chat_proposal("action", id, Some(conversation))?
            .map(|proposal| proposal.action)
            .or(self
                .chat_decision("action", id, Some(conversation))?
                .map(|decision| decision.action));
        ensure!(action.as_deref() == Some("review_memory"), "不是记忆审阅卡");
        crate::ai::approval::resolve_chat_proposal(
            self,
            "action",
            id,
            Some(conversation),
            version,
            approval_token,
            if accept { "accept" } else { "reject" },
        )
    }
}
