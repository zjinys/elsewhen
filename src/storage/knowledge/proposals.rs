//! 知识提案库：metadata 维护、proposal 记录/查找/裁决与事务内发布。

use super::*;

impl Store {
    pub fn knowledge_metadata(&self, slug: &str) -> Result<KnowledgeMetadata> {
        Ok(self
            .connection
            .query_row(
                "SELECT m.applicable_when,m.strength,m.confirmed_at
            FROM knowledge_metadata m JOIN wiki_pages p ON p.id=m.page_id WHERE p.slug=?1",
                [slug],
                |r| {
                    Ok(KnowledgeMetadata {
                        applicable_when: r.get(0)?,
                        strength: r.get(1)?,
                        confirmed_at: r.get(2)?,
                    })
                },
            )
            .optional()?
            .unwrap_or(KnowledgeMetadata {
                strength: "reference".into(),
                ..Default::default()
            }))
    }

    /// Only the explicit GUI confirmation endpoint calls this; model tools do not.
    pub fn set_knowledge_metadata(
        &self,
        slug: &str,
        applicable_when: &str,
        strength: &str,
    ) -> Result<()> {
        if !matches!(strength, "reference" | "method" | "rule") {
            bail!("无效的知识强度");
        }
        if applicable_when.chars().count() > 600 {
            bail!("适用条件最多 600 字");
        }
        let page = self.get_wiki_page(slug)?.context("知识页不存在")?;
        if !matches!(page.kind.as_str(), "method" | "case" | "principle") {
            bail!("仅方法、案例、规律页可设置适用条件与强度");
        }
        if strength != "reference" && applicable_when.trim().is_empty() {
            bail!("请先填写适用条件");
        }
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        self.connection.execute("INSERT INTO knowledge_metadata(page_id,applicable_when,strength,confirmed_at)
            VALUES (?1,?2,?3,?4) ON CONFLICT(page_id) DO UPDATE SET
            applicable_when=excluded.applicable_when,strength=excluded.strength,confirmed_at=excluded.confirmed_at",
            params![page.id,applicable_when.trim(),strength,chrono::Utc::now().to_rfc3339()])?;
        self.append_wiki_log(&format!(
            "知识适用条件/强度由用户确认：{slug} → {strength}；{applicable_when}"
        ))?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn record_proposal(
        &self,
        draft: &WikiPageDraft,
        applicable_when: &str,
        snapshots: &[String],
        base: Option<&WikiPage>,
        reason: &str,
    ) -> Result<String> {
        self.record_proposal_with_origin(draft, applicable_when, snapshots, base, reason, "manual")
    }

    pub(crate) fn record_proposal_with_origin(
        &self,
        draft: &WikiPageDraft,
        applicable_when: &str,
        snapshots: &[String],
        base: Option<&WikiPage>,
        reason: &str,
        origin: &str,
    ) -> Result<String> {
        if draft.content_md.trim().is_empty() {
            bail!("建议正文为空");
        }
        if snapshots.is_empty() && draft.source_event_ids.is_empty() {
            bail!("知识建议缺少真实来源");
        }
        for id in snapshots {
            self.source_snapshot(id)?.context("建议引用的来源不存在")?;
        }
        for id in &draft.source_event_ids {
            if !self.recordable_event(id)? {
                bail!("建议包含不存在或不可记录的事件");
            }
        }
        let mut source_keys = snapshots.to_vec();
        source_keys.sort();
        source_keys.dedup();
        let mut event_keys = draft.source_event_ids.clone();
        event_keys.sort();
        event_keys.dedup();
        // Same input version + same target is a single decision. Rejected
        // suggestions cannot reappear merely because the model rephrases them.
        let base_state = base
            .map(|p| {
                crate::knowledge::authoring::revision_basis(self, p)
                    .and_then(|v| Ok(serde_json::to_string(&v)?))
            })
            .transpose()?;
        let key = content_hash(&format!(
            "{}|{}|{}|{}",
            base.map_or(draft.slug.as_str(), |p| p.id.as_str()),
            source_keys.join(","),
            event_keys.join(","),
            format!(
                "{}|{}",
                base_state.as_deref().unwrap_or_default(),
                if reason.starts_with("恢复历史正文") {
                    content_hash(&draft.content_md)
                } else {
                    String::new()
                }
            )
        ));
        self.connection.execute(
            "INSERT OR IGNORE INTO knowledge_proposals
            (id,dedupe_key,page_id,target_slug,kind,title,content_md,summary,applicable_when,
             snapshot_ids,event_ids,base_hash,reason,created_at,origin,base_state)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            params![
                Uuid::new_v4().to_string(),
                key,
                base.map(|p| &p.id),
                draft.slug,
                draft.kind,
                draft.title,
                draft.content_md,
                draft.summary,
                applicable_when,
                serde_json::to_string(&source_keys)?,
                serde_json::to_string(&event_keys)?,
                base.map(|p| content_hash(&p.content_md)),
                reason,
                chrono::Utc::now().to_rfc3339(),
                origin,
                base_state
            ],
        )?;
        Ok(self.connection.query_row(
            "SELECT id FROM knowledge_proposals WHERE dedupe_key=?1",
            [&key],
            |r| r.get(0),
        )?)
    }

    pub fn knowledge_proposals(&self, slug: Option<&str>) -> Result<Vec<KnowledgeProposal>> {
        self.find_knowledge_proposals(slug, None)
    }

    /// Navigate the existing provenance graph, including pages compiled before
    /// the UI exposed these links. Never invent a single based_on for many sources.
    pub fn knowledge_origin_pages(&self, slug: &str) -> Result<Vec<WikiPage>> {
        let sql = format!(
            "SELECT {WIKI_PAGE_COLS} FROM wiki_pages WHERE slug<>?1 AND id IN (
            SELECT origin.page_id FROM knowledge_source_pages origin
            JOIN knowledge_snapshots s ON s.source_id=origin.source_id
            JOIN knowledge_page_sources cited ON cited.snapshot_id=s.id
            JOIN wiki_pages target ON target.id=cited.page_id WHERE target.slug=?1
            UNION SELECT d.upstream_id FROM knowledge_dependencies d JOIN wiki_pages target ON target.id=d.page_id WHERE target.slug=?1
            UNION SELECT r.old_page_id FROM knowledge_topic_replacements r JOIN wiki_pages target ON target.id=r.new_page_id WHERE target.slug=?1)
            ORDER BY title,slug"
        );
        let mut stmt = self.connection.prepare(&sql)?;
        let rows = stmt.query_map([slug], map_wiki_page)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn knowledge_output_pages(&self, slug: &str) -> Result<Vec<WikiPage>> {
        let sql = format!(
            "SELECT {WIKI_PAGE_COLS} FROM wiki_pages WHERE slug<>?1 AND id IN (
            SELECT cited.page_id FROM knowledge_page_sources cited
            JOIN knowledge_snapshots s ON s.id=cited.snapshot_id
            JOIN knowledge_source_pages origin ON origin.source_id=s.source_id
            JOIN wiki_pages material ON material.id=origin.page_id WHERE material.slug=?1
            UNION SELECT d.page_id FROM knowledge_dependencies d JOIN wiki_pages original ON original.id=d.upstream_id WHERE original.slug=?1
            UNION SELECT r.new_page_id FROM knowledge_topic_replacements r JOIN wiki_pages original ON original.id=r.old_page_id WHERE original.slug=?1)
            ORDER BY updated_at DESC,slug"
        );
        let mut stmt = self.connection.prepare(&sql)?;
        let rows = stmt.query_map([slug], map_wiki_page)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub(crate) fn find_knowledge_proposals(
        &self,
        slug: Option<&str>,
        id: Option<&str>,
    ) -> Result<Vec<KnowledgeProposal>> {
        let mut stmt = self.connection.prepare("SELECT k.id,k.page_id,COALESCE(p.slug,k.target_slug),k.kind,k.title,
            k.content_md,k.applicable_when,k.snapshot_ids,k.event_ids,k.base_hash,k.reason,k.status,k.created_at,k.summary
            FROM knowledge_proposals k LEFT JOIN wiki_pages p ON p.id=k.page_id
            WHERE (?2 IS NULL OR k.id=?2) AND (?1 IS NULL OR p.slug=?1 OR k.target_slug=?1 OR EXISTS (
                SELECT 1 FROM json_each(k.snapshot_ids) j JOIN knowledge_snapshots s ON s.id=j.value
                JOIN knowledge_source_pages m ON m.source_id=s.source_id JOIN wiki_pages origin ON origin.id=m.page_id
                WHERE origin.slug=?1))
            ORDER BY (k.status='pending') DESC,k.created_at ASC LIMIT 200")?;
        let rows = stmt
            .query_map(params![slug, id], |r| {
                Ok(KnowledgeProposal {
                    id: r.get(0)?,
                    page_id: r.get(1)?,
                    target_slug: r.get(2)?,
                    kind: r.get(3)?,
                    title: r.get(4)?,
                    content_md: r.get(5)?,
                    applicable_when: r.get(6)?,
                    snapshot_ids: serde_json::from_str(&r.get::<_, String>(7)?).unwrap_or_default(),
                    event_ids: serde_json::from_str(&r.get::<_, String>(8)?).unwrap_or_default(),
                    base_hash: r.get(9)?,
                    reason: r.get(10)?,
                    status: r.get(11)?,
                    created_at: r.get(12)?,
                    summary: r.get(13)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn resolve_knowledge_proposal(&self, id: &str, accept: bool) -> Result<Option<WikiPage>> {
        self.apply_knowledge_proposal(id, accept, true)
    }

    /// Only the automatic source compiler uses this path. Its result remains a
    /// reference, and can never overwrite a human decision or edit.
    pub(crate) fn save_automatic_reference(&self, id: &str) -> Result<Option<WikiPage>> {
        self.apply_knowledge_proposal(id, true, false)
    }

    fn apply_knowledge_proposal(
        &self,
        id: &str,
        accept: bool,
        confirmed: bool,
    ) -> Result<Option<WikiPage>> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        if !confirmed {
            self.require_background_processor()?;
        }
        let page = self.apply_knowledge_proposal_in_tx(id, accept, confirmed)?;
        tx.commit()?;
        Ok(page)
    }

    pub(crate) fn reference_is_unprotected(&self, slug: &str) -> Result<bool> {
        let Some(page) = self.get_wiki_page(slug)? else {
            return Ok(true);
        };
        let metadata = self.knowledge_metadata(slug)?;
        Ok(page.human_edited_at.is_none()
            && page.status != "archived"
            && page.opinion.as_deref() != Some("reject")
            && !matches!(page.kind.as_str(), "source" | "note")
            && metadata.confirmed_at.is_none()
            && metadata.strength == "reference")
    }

    /// Keep old manual decisions and concurrent edits pending, while fresh,
    /// unprotected reference output can be published within the caller's batch.
    pub(crate) fn publish_reference_in_tx(&self, id: &str) -> Result<Option<WikiPage>> {
        let proposal = self
            .find_knowledge_proposals(None, Some(id))?
            .into_iter()
            .next()
            .context("建议不存在")?;
        let origin: String = self.connection.query_row(
            "SELECT origin FROM knowledge_proposals WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        if origin != "automatic" || proposal.status == "rejected" {
            return Ok(None);
        }
        if proposal.status == "accepted" {
            return self.get_wiki_page(&proposal.target_slug);
        }
        if !self.reference_is_unprotected(&proposal.target_slug)? {
            return Ok(None);
        }
        let base = self.get_wiki_page(&proposal.target_slug)?;
        if base.as_ref().map(|p| &p.id) != proposal.page_id.as_ref()
            || base.as_ref().map(|p| content_hash(&p.content_md)) != proposal.base_hash
        {
            return Ok(None);
        }
        let blocked: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM knowledge_proposals WHERE id<>?1 AND target_slug=?2
             AND (status='rejected' OR (status='pending' AND origin='manual')))",
            params![id, proposal.target_slug],
            |r| r.get(0),
        )?;
        if blocked {
            return Ok(None);
        }
        self.apply_knowledge_proposal_in_tx(id, true, false)
    }

    pub(crate) fn apply_knowledge_proposal_in_tx(
        &self,
        id: &str,
        accept: bool,
        confirmed: bool,
    ) -> Result<Option<WikiPage>> {
        if !confirmed {
            let origin: String = self.connection.query_row(
                "SELECT origin FROM knowledge_proposals WHERE id=?1",
                [id],
                |r| r.get(0),
            )?;
            if origin != "automatic" {
                bail!("人工待审建议不能自动采纳");
            }
        }
        let proposal = self
            .find_knowledge_proposals(None, Some(id))?
            .into_iter()
            .next()
            .context("建议不存在或已过期")?;
        if !confirmed {
            let manual_choice: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM knowledge_proposals other
                 JOIN json_each(other.snapshot_ids) j JOIN knowledge_snapshots s ON s.id=j.value
                 WHERE other.id<>?1 AND other.target_slug=?3 AND (other.status='rejected' OR (other.status='pending' AND other.origin='manual'))
                 AND s.source_id IN (SELECT source_id FROM knowledge_snapshots WHERE id IN (SELECT value FROM json_each(?2))))",
                params![id,serde_json::to_string(&proposal.snapshot_ids)?,proposal.target_slug], |r| r.get(0))?;
            if manual_choice {
                bail!("整理期间有新的人工决定，保留待审状态");
            }
        }
        if proposal.status != "pending" {
            if accept && proposal.status == "accepted" {
                return self.get_wiki_page(&proposal.target_slug);
            }
            if !accept && proposal.status == "rejected" {
                return Ok(None);
            }
            bail!("建议已经处理，不能更改历史决定");
        }
        let previous_content = self
            .get_wiki_page(&proposal.target_slug)?
            .map(|p| p.content_md);
        let page = if accept {
            let base = self.get_wiki_page(&proposal.target_slug)?;
            let raw_dependencies: Option<String> = self.connection.query_row(
                "SELECT dependency_bases FROM knowledge_proposals WHERE id=?1",
                [id],
                |r| r.get(0),
            )?;
            let dependencies: Option<Vec<crate::knowledge::dependencies::Basis>> = raw_dependencies
                .map(|raw| serde_json::from_str(&raw))
                .transpose()?;
            if let Some(bases) = &dependencies {
                crate::knowledge::dependencies::validate(self, bases)?;
            } else if let Some(base) = &base {
                anyhow::ensure!(
                    !crate::knowledge::dependencies::stale(self, &base.id)?,
                    "上游知识已变化，请重新检查更新"
                );
            }
            if !confirmed && !self.reference_is_unprotected(&proposal.target_slug)? {
                bail!("已有人工编辑，自动整理保留原知识页");
            }
            if !confirmed {
                let metadata = self.knowledge_metadata(&proposal.target_slug)?;
                if metadata.confirmed_at.is_some() || metadata.strength != "reference" {
                    bail!("人工确认的适用条件与强度不能自动改写");
                }
            }
            if proposal.page_id.is_some() {
                let base = base.as_ref().context("目标页面已删除")?;
                let basis: Option<String> = self.connection.query_row(
                    "SELECT base_state FROM knowledge_proposals WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )?;
                let basis = basis
                    .map(|raw| serde_json::from_str::<serde_json::Value>(&raw))
                    .transpose()?;
                crate::knowledge::authoring::compare_revision_basis(self, base, basis.as_ref())?;
                if Some(&base.id) != proposal.page_id.as_ref()
                    || Some(content_hash(&base.content_md)) != proposal.base_hash
                {
                    bail!("页面已被修改，请重新审阅，不能覆盖新的编辑");
                }
            } else if base.is_some() {
                bail!("同名页面已存在，请重新生成修订建议");
            }
            for id in &proposal.snapshot_ids {
                let snap = self.source_snapshot(id)?.context("来源不存在")?;
                let newest: i64 = self.connection.query_row(
                    "SELECT MAX(version) FROM knowledge_snapshots WHERE source_id=?1",
                    [&snap.source_id],
                    |r| r.get(0),
                )?;
                let source_active = snap
                    .page_slug
                    .as_deref()
                    .map(|slug| self.get_wiki_page(slug))
                    .transpose()?
                    .flatten()
                    .is_some_and(|p| p.status != "archived");
                if snap.version != newest
                    || snap.opinion.as_deref() == Some("reject")
                    || !source_active
                {
                    bail!("来源已更新或被拒绝，请重新生成建议");
                }
            }
            for id in &proposal.event_ids {
                if !self.recordable_event(id)? {
                    bail!("来源事件已被标为讨论，请重新审阅");
                }
            }
            let tags = base
                .as_ref()
                .map(|p| p.tags.clone())
                .unwrap_or_else(|| vec!["编译知识".into()]);
            let draft = WikiPageDraft {
                slug: proposal.target_slug.clone(),
                kind: proposal.kind.clone(),
                title: proposal.title.clone(),
                summary: if proposal.summary.trim().is_empty() {
                    proposal.content_md.chars().take(120).collect()
                } else {
                    proposal.summary.chars().take(240).collect()
                },
                content_md: proposal.content_md.clone(),
                tags,
                source_event_ids: proposal.event_ids.clone(),
                status: "active".into(),
                reason: format!(
                    "{}：{}",
                    if confirmed {
                        "用户确认知识建议"
                    } else {
                        "自动整理参考知识"
                    },
                    proposal.reason
                ),
                source_url: None,
            };
            let result = self
                .upsert_wiki_page_in_tx(&draft, ContentPolicy::Always)?
                .page;
            self.connection.execute("UPDATE wiki_pages SET human_edited_at=?1,source_event_ids=?3,evidence_count=?4 WHERE id=?2",
                params![confirmed.then(|| chrono::Utc::now().to_rfc3339()),result.id,serde_json::to_string(&proposal.event_ids)?,proposal.event_ids.len() as i64])?;
            self.connection.execute(
                "DELETE FROM knowledge_page_sources WHERE page_id=?1",
                [&result.id],
            )?;
            self.bind_page_sources(&result.id, &proposal.snapshot_ids)?;
            if let Some(bases) = &dependencies {
                crate::knowledge::dependencies::bind(self, &result.id, bases)?;
            }
            // Confirming a revision must not silently downgrade a personal rule.
            let strength = if confirmed && base.is_some() {
                self.knowledge_metadata(&result.slug)?.strength
            } else {
                "reference".into()
            };
            self.connection.execute("INSERT INTO knowledge_metadata(page_id,applicable_when,strength,confirmed_at)
                VALUES (?1,?2,?4,?3) ON CONFLICT(page_id) DO UPDATE SET
                applicable_when=excluded.applicable_when,strength=excluded.strength,confirmed_at=excluded.confirmed_at",
                params![result.id,proposal.applicable_when,confirmed.then(|| chrono::Utc::now().to_rfc3339()),strength])?;
            self.append_wiki_log(&format!(
                "{} {} → {}",
                if confirmed {
                    "确认知识建议"
                } else {
                    "自动整理参考知识"
                },
                proposal.id,
                result.slug
            ))?;
            Some(
                self.get_wiki_page(&result.slug)?
                    .context("保存后页面不存在")?,
            )
        } else {
            self.append_wiki_log(&format!("拒绝知识建议 {}（保留历史）", proposal.id))?;
            None
        };
        self.connection.execute(
            "UPDATE knowledge_proposals SET status=?1,resolved_at=?2 WHERE id=?3",
            params![
                if accept { "accepted" } else { "rejected" },
                chrono::Utc::now().to_rfc3339(),
                id
            ],
        )?;
        let revision: Option<String> = if let Some(p) = &page {
            self.connection.query_row("SELECT id FROM wiki_revisions WHERE page_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT 1",[&p.id],|r|r.get(0)).optional()?
        } else {
            None
        };
        self.connection.execute("INSERT OR IGNORE INTO knowledge_review_decisions(proposal_id,original_content,original_applicable,selected_parts,issue_ids,created_at,before_content,result_content,result_applicable,revision_id)
            VALUES(?1,?2,?3,'[]','[]',?4,?5,?6,?7,?8)",params![id,proposal.content_md,proposal.applicable_when,chrono::Utc::now().to_rfc3339(),previous_content,page.as_ref().map(|p|&p.content_md),accept.then_some(&proposal.applicable_when),revision])?;
        Ok(page)
    }
}
