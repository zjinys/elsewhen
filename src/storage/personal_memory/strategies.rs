use super::*;
use std::collections::{BTreeMap, BTreeSet};

impl Store {
    pub(crate) fn memory_inspection_context(&self, conversation: &str) -> Result<Value> {
        let input = if let Some(snapshot) = self
            .conversation_snapshot
            .as_ref()
            .filter(|snapshot| snapshot.conversation_id == conversation)
        {
            Some(snapshot.input_message_id.clone())
        } else {
            self.connection.query_row("SELECT id FROM messages WHERE conversation_id=?1 AND role='user' ORDER BY rowid DESC LIMIT 1",[conversation],|row|row.get::<_,String>(0)).optional()?
        };
        Ok(json!({"conversation_id":conversation,"input_id":input}))
    }

    pub(crate) fn memory_inspection_current(&self, value: &Value) -> Result<bool> {
        let Some(conversation) = value["inspection"]["conversation_id"].as_str() else {
            return Ok(false);
        };
        if value["excluded_review"] != true
            || value["inspection"]["input_id"].as_str().is_none()
            || self
                .conversation_snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.conversation_id != conversation)
        {
            return Ok(false);
        }
        Ok(value["inspection"] == self.memory_inspection_context(conversation)?)
    }

    pub(crate) fn memory_record_inspection(&self, claim: &Claim, value: &Value) -> Result<()> {
        ensure!(
            claim.excluded && self.memory_inspection_current(value)?,
            "被排除条目的查看范围已变化"
        );
        self.connection.execute("INSERT OR IGNORE INTO personal_memory_inspections(claim_id,input_id,created_at) VALUES(?1,?2,?3)",params![claim.id,value["inspection"]["input_id"].as_str(),now()])?;
        Ok(())
    }

    pub(crate) fn memory_goal_context(&self, goal: &str) -> Result<Option<Value>> {
        Ok(self.connection.query_row(
            "SELECT id,content,status,updated_at FROM goals WHERE id=?1",
            [goal],
            |row| Ok(json!({"id":row.get::<_,String>(0)?,"title":row.get::<_,String>(1)?.chars().take(120).collect::<String>(),"status":row.get::<_,String>(2)?,"updated_at":row.get::<_,String>(3)?})),
        ).optional()?)
    }

    pub(crate) fn memory_goal_links(&self, claim: &str) -> Result<Vec<Value>> {
        Ok(self.connection.prepare("SELECT l.goal_id,l.reason,g.content,g.status,g.updated_at FROM personal_memory_goal_links l LEFT JOIN goals g ON g.id=l.goal_id WHERE l.claim_id=?1 ORDER BY l.goal_id LIMIT 8")?
            .query_map([claim], |row| Ok(json!({"goal_id":row.get::<_,String>(0)?,"reason":row.get::<_,String>(1)?,"title":row.get::<_,Option<String>>(2)?.map(|text|text.chars().take(120).collect::<String>()),"status":row.get::<_,Option<String>>(3)?,"updated_at":row.get::<_,Option<String>>(4)?})))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub(crate) fn memory_goal_context_current(&self, value: &Value) -> Result<bool> {
        if let Some(links) = value.get("goal_links") {
            let Some(claim) = value["id"].as_str() else {
                return Ok(false);
            };
            if links.as_array() != Some(&self.memory_goal_links(claim)?) {
                return Ok(false);
            }
        }
        let Some(context) = value
            .get("goal_context")
            .filter(|context| !context.is_null())
        else {
            return Ok(true);
        };
        let Some(goal) = context["id"].as_str() else {
            return Ok(false);
        };
        Ok(self.memory_goal_context(goal)?.as_ref() == Some(context))
    }

    pub(crate) fn memory_page_auto_current(&self, slug: &str) -> Result<bool> {
        let Some(page) = self.get_wiki_page(slug)? else {
            return Ok(false);
        };
        let excluded: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM personal_memory_claims WHERE page_slug=?1 AND body_hash=?2 AND excluded=1)",
            params![slug,hash(&page.content_md)], |row| row.get(0),
        )?;
        Ok(!excluded && self.memory_page_current(slug)?)
    }

    pub(crate) fn memory_usage_basis(&self) -> Result<Value> {
        Ok(self.connection.query_row(
            "SELECT COUNT(*),COALESCE(MAX(rowid),0) FROM personal_memory_receipts",
            [],
            |row| Ok(json!({"receipts":row.get::<_,i64>(0)?,"through_rowid":row.get::<_,i64>(1)?})),
        )?)
    }

    pub(crate) fn memory_maintenance(&self, conversation: &str) -> Result<Value> {
        let claims = self.search_memories_bounded(
            conversation,
            &json!({"include_history":true,"include_excluded":true,"limit":200}),
            200,
        )?;
        let mut hashes: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut sources: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut broken = Vec::new();
        let mut excluded = 0;
        let mut pinned = 0;
        let mut body_chars = 0;
        let mut source_chars = 0;
        let mut measured_sources = BTreeSet::new();
        let mut measured_pages = BTreeSet::new();
        let mut near = Vec::new();
        let mut signatures: Vec<(String, BTreeSet<String>)> = Vec::new();
        for value in &claims {
            let id = value["id"].as_str().context("记忆缺少标识")?.to_owned();
            let claim = self.memory_claim(&id)?.context("记忆不存在")?;
            excluded += usize::from(claim.excluded);
            pinned += usize::from(claim.pinned);
            if value["validity"] != "current" {
                broken.push(
                    json!({"id":id,"page_slug":claim.page_slug,"validity":value["validity"]}),
                );
            }
            hashes.entry(claim.body_hash).or_default().push(id.clone());
            for basis in &claim.basis {
                let key = format!("{}:{}:{}", basis.kind, basis.id, basis.version);
                sources.entry(key.clone()).or_default().insert(id.clone());
                if measured_sources.insert(key) {
                    source_chars += basis.quote.chars().count();
                }
            }
            if value["validity"] == "current" {
                let body = if let Some(content) = &claim.content {
                    Some((format!("claim:{}", claim.id), content.clone()))
                } else {
                    self.get_wiki_page(&claim.page_slug)?.map(|page| (page.id, page.content_md))
                };
                if let Some((body_id, content)) = body {
                    if measured_pages.insert(body_id) {
                        body_chars += content.chars().count();
                    }
                    let signature = crate::knowledge::query_terms(
                        &content.chars().take(3000).collect::<String>(),
                    )
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                    if signature.len() >= 4 {
                        for (other, previous) in &signatures {
                            let intersection = signature.intersection(previous).count();
                            let union = signature.union(previous).count();
                            if intersection * 100 >= union * 85 && near.len() < 20 {
                                near.push(json!({"ids":[other,id],"hint":"lexical_overlap_not_semantic_equivalence"}));
                            }
                        }
                        signatures.push((id, signature));
                    }
                }
            }
        }
        Ok(
            json!({"coverage":"latest_at_most_200_visible_non_deleted_claims","scanned":claims.len(),"pinned":pinned,"excluded":excluded,"unavailable_or_stale":broken.into_iter().take(20).collect::<Vec<_>>(),"exact_body_groups":hashes.into_values().filter(|ids|ids.len()>1).take(20).collect::<Vec<_>>(),"shared_source_groups":sources.into_values().filter(|ids|ids.len()>1).take(20).collect::<Vec<_>>(),"near_duplicate_hints":near,"derived_body_chars":body_chars,"unique_basis_excerpt_chars":source_chars,"measurement":"bounded_excerpts_not_full_source_compression_ratio","automatic_mutation":false,"decisions_preserved":true}),
        )
    }
}
