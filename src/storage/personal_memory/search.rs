use super::*;

impl Store {
    pub(crate) fn search_memories(&self, conversation: &str, filter: &Value) -> Result<Vec<Value>> {
        self.search_memories_bounded(conversation, filter, 20)
    }

    pub(super) fn search_memories_bounded(
        &self,
        conversation: &str,
        filter: &Value,
        maximum: u64,
    ) -> Result<Vec<Value>> {
        if !grants::conversation_current(self, conversation)? {
            return Ok(vec![]);
        }
        let query = filter["query"].as_str().unwrap_or("");
        ensure!(query.chars().count() <= 3000, "查询过长");
        let terms = crate::knowledge::query_terms(query);
        let historical = filter["include_history"].as_bool().unwrap_or(false);
        let include_excluded = filter["include_excluded"].as_bool().unwrap_or(false);
        let inspection = if include_excluded {
            self.memory_inspection_context(conversation)?
        } else {
            Value::Null
        };
        let conversation_data = self.get_conversation(conversation)?.context("会话不存在")?;
        let focus_goal = filter["focus_goal_id"]
            .as_str()
            .or(conversation_data.goal_id.as_deref());
        let goal_context = focus_goal
            .map(|goal| self.memory_goal_context(goal))
            .transpose()?
            .flatten();
        let personal = conversation_data.assistant_mode == "personal_secretary"
            && conversation_data.wiki_page_slug.is_none()
            && conversation_data.goal_id.is_none();
        let sql = format!(
            "SELECT {COLUMNS} FROM personal_memory_claims WHERE state<>'deleted' AND (?1 IS NULL OR kind=?1) AND (?2 IS NULL OR subject=?2) AND (?3 IS NULL OR state=?3) AND (?4 IS NULL OR support=?4) AND (?5 IS NULL OR (confirmed_hash IS NOT NULL)=?5) AND (conversation_id=?6 OR (?7 AND scope='personal')) AND (?8 OR excluded=0) ORDER BY updated_at DESC,id LIMIT 200"
        );
        let claims = self
            .connection
            .prepare(&sql)?
            .query_map(
                params![
                    filter["kind"].as_str(),
                    filter["subject"].as_str(),
                    filter["state"].as_str(),
                    filter["support"].as_str(),
                    filter["confirmed"].as_bool(),
                    conversation,
                    personal,
                    include_excluded
                ],
                row_claim,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut results = Vec::new();
        for claim in claims {
            if claim.excluded && !include_excluded {
                continue;
            }
            let owner = claim.conversation_id == conversation;
            if !owner && !(personal && claim.scope == "personal") {
                continue;
            }
            let validity = self.memory_validity(&claim)?;
            if !historical
                && (claim.state != "active"
                    || validity != "current"
                    || claim.confirmed_hash.as_deref() != Some(&claim.body_hash))
            {
                continue;
            }
            if filter["validity"]
                .as_str()
                .is_some_and(|wanted| wanted != validity)
            {
                continue;
            }
            let page = self.get_wiki_page(&claim.page_slug)?;
            if page.is_none() && claim.content.is_none() && !historical {
                continue;
            }
            let memory_text = claim.content.as_deref().or_else(|| page.as_ref().map(|page| page.content_md.as_str())).unwrap_or("");
            let searchable = format!(
                "{} {} {} {}",
                claim.subject,
                claim.predicate,
                page.as_ref().map(|page| page.title.as_str()).unwrap_or(""),
                memory_text
            )
            .to_lowercase();
            let score = terms
                .iter()
                .filter(|term| searchable.contains(term.as_str()))
                .count();
            let links = self.memory_goal_links(&claim.id)?;
            let goal_match = goal_context.as_ref().is_some_and(|goal| {
                goal["status"] == "active" && links.iter().any(|link| link["goal_id"] == goal["id"])
            });
            if !terms.is_empty() && score == 0 && !goal_match {
                continue;
            }
            let content_current = hash(memory_text) == claim.body_hash;
            let excerpt = if content_current && validity == "current" {
                Some(
                    crate::knowledge::select::matching_excerpt(
                        memory_text,
                        &terms,
                        480,
                    )
                    .0,
                )
            } else {
                None
            };
            let mut sources = Vec::new();
            for basis in &claim.basis {
                let usable = self.memory_basis_current(basis)?;
                sources.push(json!({"kind":basis.kind,"id":basis.id,"version":basis.version,"offset":basis.offset,"quote":if usable {Some(&basis.quote)} else {None},"current":usable,"independent_group":basis.independent_group}));
            }
            let latest_review: Option<String>=self.connection.query_row("SELECT detail FROM knowledge_background_runs WHERE task='memory-review' AND input_key=?1 AND status='succeeded' ORDER BY started_at DESC LIMIT 1",[format!("{}:{}",claim.id,claim.version)],|row|row.get(0)).optional()?;
            let latest_review =
                latest_review.and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
            let salience = match claim.salience.as_str() {
                "high" => 2,
                "low" => 0,
                _ => 1,
            };
            results.push(((score,claim.pinned,goal_match,salience),json!({"id":claim.id,"version":claim.version,"page_slug":claim.page_slug,"title":page.as_ref().map(|page|page.title.as_str()).unwrap_or(&claim.page_slug),"kind":claim.kind,"subject":claim.subject,"predicate":claim.predicate,"state":claim.state,"scope":claim.scope,"confirmed":claim.confirmed_hash.as_deref()==Some(&claim.body_hash),"support":claim.support,"validity":validity,"valid_from":claim.valid_from,"valid_until":claim.valid_until,"superseded_by":claim.superseded_by,"excerpt":excerpt,"complete":content_current&&memory_text.chars().count()<=480,"sources":sources,"latest_review":latest_review,"independence":"unknown_unless_explicitly_sourced","execution_permission":false,"pinned":claim.pinned,"excluded":claim.excluded,"excluded_review":claim.excluded&&include_excluded,"inspection":if claim.excluded {&inspection} else {&Value::Null},"salience":claim.salience,"goal_links":links,"goal_context":goal_context})));
        }
        results.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        Ok(results
            .into_iter()
            .take(filter["limit"].as_u64().unwrap_or(8).clamp(1, maximum) as usize)
            .map(|(_, value)| value)
            .collect())
    }

    pub(crate) fn memory_receipt(
        &self,
        request: &str,
        attempt: &str,
        claim: &Claim,
        kind: &str,
        outcome: &str,
    ) -> Result<()> {
        self.connection.execute("INSERT INTO personal_memory_receipts(attempt_id,request_id,claim_id,claim_version,access_kind,outcome,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(attempt_id,claim_id,claim_version,access_kind) DO UPDATE SET outcome=excluded.outcome",params![attempt,request,claim.id,claim.version,kind,outcome,now()])?;
        Ok(())
    }

    pub(crate) fn memory_page_access(
        &self,
        slug: &str,
        request: &str,
        attempt: &str,
        kind: &str,
    ) -> Result<()> {
        let claims = self
            .connection
            .prepare(&format!(
                "SELECT {COLUMNS} FROM personal_memory_claims WHERE page_slug=?1 AND state='active'"
            ))?
            .query_map([slug], row_claim)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for claim in claims {
            if claim.confirmed_hash.as_deref() == Some(&claim.body_hash)
                && self.memory_validity(&claim)? == "current"
            {
                self.memory_receipt(request, attempt, &claim, kind, "sent")?;
            }
        }
        Ok(())
    }

    pub(crate) fn memory_rebuild(&self) -> Result<Value> {
        let claims = self
            .connection
            .prepare(&format!(
                "SELECT {COLUMNS} FROM personal_memory_claims ORDER BY id"
            ))?
            .query_map([], row_claim)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut current = 0;
        let mut suppressed = 0;
        for claim in &claims {
            if matches!(claim.state.as_str(), "dismissed" | "deleted") {
                suppressed += 1;
                continue;
            }
            if self.memory_validity(claim)? == "current" {
                current += 1;
            }
        }
        Ok(
            json!({"claims":claims.len(),"current":current,"suppressed":suppressed,"decisions_preserved":true,"cache":"computed_from_current_sources"}),
        )
    }

    pub(crate) fn memory_page_current(&self, slug: &str) -> Result<bool> {
        let claims = self
            .connection
            .prepare(&format!(
                "SELECT {COLUMNS} FROM personal_memory_claims WHERE page_slug=?1"
            ))?
            .query_map([slug], row_claim)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if claims.is_empty() {
            return Ok(true);
        }
        for claim in claims {
            if claim.state == "active"
                && claim.confirmed_hash.as_deref() == Some(&claim.body_hash)
                && self.memory_validity(&claim)? == "current"
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn memory_page_background_current(
        &self,
        slug: &str,
        conversation: &str,
    ) -> Result<bool> {
        let registered: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM personal_memory_claims WHERE page_slug=?1)",
            [slug],
            |row| row.get(0),
        )?;
        if !registered {
            return Ok(true);
        }
        let claims = self.connection.prepare(&format!("SELECT {COLUMNS} FROM personal_memory_claims WHERE page_slug=?1 AND state='active' AND scope<>'reference'"))?.query_map([slug], row_claim)?.collect::<rusqlite::Result<Vec<_>>>()?;
        let main = self.get_conversation(conversation)?.is_some_and(|data| {
            data.assistant_mode == "personal_secretary"
                && data.wiki_page_slug.is_none()
                && data.goal_id.is_none()
        });
        for claim in claims {
            if (claim.conversation_id == conversation || (main && claim.scope == "personal"))
                && !claim.excluded
                && claim.confirmed_hash.as_deref() == Some(&claim.body_hash)
                && self.memory_validity(&claim)? == "current"
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn memory_message_suppressed(&self, id: &str) -> Result<bool> {
        let inspected: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM personal_memory_inspections i JOIN personal_memory_claims c ON c.id=i.claim_id WHERE i.input_id=?1 AND (c.excluded=1 OR c.state IN ('superseded','dismissed','deleted','conflicted')))",[id],|row|row.get(0))?;
        if inspected {
            return Ok(true);
        }
        let claims = self.connection.prepare(&format!("SELECT {COLUMNS} FROM personal_memory_claims WHERE excluded=1 OR state IN ('superseded','dismissed','deleted','conflicted')"))?.query_map([],row_claim)?.collect::<rusqlite::Result<Vec<_>>>()?;
        for claim in claims {
            for basis in claim.basis {
                if basis.kind == "message" && basis.id == id {
                    return Ok(true);
                }
                if basis.kind == "event" {
                    let linked: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM input_records WHERE message_id=?1 AND event_id=?2 UNION ALL SELECT 1 FROM chat_input_effects WHERE input_message_id=?1 AND event_id=?2)",params![id,basis.id],|row|row.get(0))?;
                    if linked {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    pub(crate) fn memory_source_suppressed(&self, kind: &str, id: &str) -> Result<bool> {
        let inspected: bool = self.connection.query_row("SELECT EXISTS(SELECT 1 FROM personal_memory_inspections i JOIN personal_memory_claims c ON c.id=i.claim_id WHERE (c.excluded=1 OR c.state IN ('superseded','dismissed','deleted','conflicted')) AND ((?1='message' AND i.input_id=?2) OR (?1='event' AND (EXISTS(SELECT 1 FROM input_records r WHERE r.message_id=i.input_id AND r.event_id=?2) OR EXISTS(SELECT 1 FROM chat_input_effects e WHERE e.input_message_id=i.input_id AND e.event_id=?2)))))",params![kind,id],|row|row.get(0))?;
        if inspected {
            return Ok(true);
        }
        Ok(self.connection.query_row("SELECT EXISTS(SELECT 1 FROM personal_memory_claims c,json_each(c.basis_json) b WHERE (c.excluded=1 OR c.state IN ('superseded','dismissed','deleted','conflicted')) AND ((json_extract(b.value,'$.kind')=?1 AND json_extract(b.value,'$.id')=?2) OR (?1='event' AND json_extract(b.value,'$.kind')='message' AND (EXISTS(SELECT 1 FROM input_records i WHERE i.message_id=json_extract(b.value,'$.id') AND i.event_id=?2) OR EXISTS(SELECT 1 FROM chat_input_effects e WHERE e.input_message_id=json_extract(b.value,'$.id') AND e.event_id=?2)))))",params![kind,id],|row|row.get(0))?)
    }
}
