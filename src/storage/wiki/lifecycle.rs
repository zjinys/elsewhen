//! 页面生命周期：删除/创建/追加正文与再整合（reconsolidation）作业。

use super::*;

impl Store {
    /// P5b：Wiki 页面业务删除。与本页所有写路径同一事务纪律：
    /// 先在行仍存在时捕获 `deleted=true` 同步变更（含 head 推进），再物理
    /// 删除本行（wiki_revisions / knowledge_source_pages 等随外键级联）。
    /// 删除后离线对端的迟到编辑会在应用端得到 `wiki_page_deleted` 冲突候选，
    /// 不会被静默覆盖。返回 false 表示页面不存在（幂等）。
    pub fn delete_wiki_page(&self, slug: &str) -> Result<bool> {
        let Some(page_id) = self.get_wiki_page(slug)?.map(|page| page.id) else {
            return Ok(false);
        };
        let tx = self.begin_write_transaction()?;
        self.sync_record_wiki_page(&page_id, true)?;
        self.connection
            .execute("DELETE FROM wiki_pages WHERE id=?1", params![page_id])?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(true)
    }

    /// 人类直接创建一张 wiki 页（当前唯一使用方是 kind=memo 记事本，评审
    /// 3.3：新页成为人类专属内容的唯一建档入口）。无 migration、无 AI 参与；
    /// 与所有写路径同纪律（同事务写页+revision+同步捕获+log）。
    /// slug 冲突时自动加数字后缀（title 相同时复用已有 slug，对齐 unique_slug 语义）。
    pub fn create_wiki_page(
        &self,
        kind: &str,
        title: &str,
        content_md: &str,
        tags: &[String],
    ) -> Result<WikiPage> {
        let title = title.trim();
        if title.is_empty() {
            anyhow::bail!("标题不能为空");
        }
        if kind == "memo" {
            // 记事本是人类专属（评审 D1）：只允许本路径建档，digest 与 AI
            // 工具一律不碰。本函数不拦其他 kind，但 UI 当前只暴露 memo。
        }
        let base_slug = format!("memo/{}", crate::wiki::slugify(title));
        let slug = crate::wiki::unique_slug(self, &base_slug, title)?;
        let draft = WikiPageDraft {
            slug,
            kind: kind.to_string(),
            title: title.to_string(),
            summary: String::new(),
            content_md: content_md.trim().to_string(),
            tags: tags.to_vec(),
            source_event_ids: Vec::new(),
            status: "confirmed".to_string(),
            reason: "人工创建记事本".to_string(),
            source_url: None,
        };
        // upsert 内部已同事务记录 revision + 同步捕获（ContentPolicy::Always：
        // 人类显式建档是有显式授权的写入路径）。
        let outcome = self.upsert_wiki_page(&draft, ContentPolicy::Always)?;
        self.append_wiki_log(&format!("人工创建记事本：{}", outcome.page.slug))?;
        Ok(outcome.page)
    }

    /// 单事务追加正文（memo 快记框的唯一写入口，评审 3.1：读-改-写两个
    /// 事务的乐观锁路径对「连击快记」会丢写，必须读-拼-写同一事务）。
    /// 与 save_wiki_page_content 同上限（64k 字符），附加「追加块」上限 8k。
    pub fn append_wiki_page_content(
        &self,
        slug: &str,
        append_md: &str,
        reason: &str,
    ) -> Result<WikiPage> {
        let append_md = append_md.trim();
        if append_md.is_empty() {
            anyhow::bail!("追加内容为空");
        }
        let append_chars = append_md.chars().count();
        if append_chars > 8192 {
            anyhow::bail!("单次追加超过 8k 字符上限（当前 {append_chars} 字符）");
        }
        let tx = self.begin_write_transaction()?;
        let page = self
            .get_wiki_page(slug)?
            .with_context(|| format!("知识页不存在: {slug}"))?;
        if matches!(page.kind.as_str(), "source" | "note") {
            anyhow::bail!("素材页（kind={}）只读，不支持追加正文", page.kind);
        }
        let mut content_md = page.content_md.clone();
        if !content_md.is_empty() && !content_md.ends_with('\n') {
            content_md.push('\n');
        }
        content_md.push_str(append_md);
        content_md.push('\n');
        let total = content_md.chars().count();
        if total > 65536 {
            anyhow::bail!("追加后正文将超过 64k 字符上限（当前 {total} 字符），请先归档或新开一页");
        }
        let now = chrono::Utc::now().to_rfc3339();
        self.connection.execute(
            "UPDATE wiki_pages SET content_md=?1, human_edited_at=?2, updated_at=?2 WHERE id=?3",
            params![content_md, now, page.id],
        )?;
        self.sync_record_wiki_page(&page.id, false)?;
        let reason = reason.trim();
        self.record_wiki_revision(
            &page.id,
            &content_md,
            &format!(
                "[human] {}",
                if reason.is_empty() {
                    "追加快记"
                } else {
                    reason
                }
            ),
            None,
        )?;
        self.append_wiki_log(&format!("人工追加正文：{slug}（+{append_chars} 字符）"))?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        self.get_wiki_page(slug)?.context("追加保存后读取失败")
    }

    /// 页面整理观测（提案 P1「可观测」，2026-10-20）：最近一次正文整理 =
    /// wiki_revisions 最新时间（digest 增量修补与手动重整理都写修订，天然覆盖）；
    /// 新增证据 = 来源事件里晚于该点发生的个数。全部派生读、不新增列——
    /// P2 的再汇总阈值若需要精确基线，再引入 last_consolidated_* 列。
    /// 返回 (最近整理时间, 新增证据数)。
    pub fn page_consolidation_status(&self, slug: &str) -> Result<(Option<String>, i64)> {
        let last = self.connection.query_row(
            "SELECT MAX(r.created_at) FROM wiki_revisions r \
         JOIN wiki_pages p ON p.id = r.page_id WHERE p.slug = ?1",
            [slug],
            |row| row.get::<_, Option<String>>(0),
        )?;
        let new_evidence = match &last {
            Some(ts) => self.connection.query_row(
                "SELECT COUNT(*) FROM events e \
             JOIN wiki_pages w ON w.slug = ?1 \
             JOIN json_each(w.source_event_ids) je ON je.value = e.id \
             WHERE e.recorded_at > ?2",
                rusqlite::params![slug, ts],
                |row| row.get(0),
            )?,
            None => 0,
        };
        Ok((last, new_evidence))
    }

    /// 某页全部来源事件原文（按记录时间升序）——页级再汇总的增量输入。
    pub fn page_source_events(&self, slug: &str) -> Result<Vec<EventRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT e.id, e.recorded_at, e.raw_text FROM events e \
         JOIN wiki_pages w ON w.slug = ?1 \
         JOIN json_each(w.source_event_ids) je ON je.value = e.id \
         ORDER BY e.recorded_at",
        )?;
        let rows = statement.query_map([slug], |row| {
            Ok(EventRecord {
                id: row.get(0)?,
                recorded_at: row.get(1)?,
                raw_text: row.get(2)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 把触发阈值命中的参考页排进再汇总队列（P2，提案 §6）。
    /// 派生口径与 P1 观测一致：整理点（最新 wiki_revisions）之后
    /// 新到来源事件 ≥3 → evidence_delta；整理点超 30 天且有新证据 →
    /// scheduled。人工编辑页不排（worker 也会拦一道）。进行中/失败的
    /// 任务不重复排；已有 succeeded 行在再次触发时重新入队。
    pub fn reconsolidation_enqueue_candidates(&self, now: &str) -> Result<usize> {
        let mut stmt = self.connection.prepare(
            "SELECT p.id FROM wiki_pages p
             WHERE p.status != 'archived'
               AND p.human_edited_at IS NULL
               AND COALESCE(p.area, 'insight') = 'insight'
               AND (p.kind NOT IN ('source', 'note') OR p.opinion = 'endorse')
               AND (
                 (SELECT COUNT(*) FROM json_each(p.source_event_ids) je
                    JOIN events e ON e.id = je.value
                   WHERE e.recorded_at > COALESCE(
                     (SELECT MAX(r.created_at) FROM wiki_revisions r WHERE r.page_id = p.id),
                     '1970')) >= 3
                 OR
                 ((SELECT MAX(r.created_at) FROM wiki_revisions r WHERE r.page_id = p.id)
                     IS NOT NULL
                  AND julianday(?2) - julianday(
                     (SELECT MAX(r.created_at) FROM wiki_revisions r WHERE r.page_id = p.id)) >= 30
                  AND (SELECT COUNT(*) FROM json_each(p.source_event_ids) je
                    JOIN events e ON e.id = je.value
                   WHERE e.recorded_at > COALESCE(
                     (SELECT MAX(r.created_at) FROM wiki_revisions r WHERE r.page_id = p.id),
                     '1970')) >= 1)
               )
               AND NOT EXISTS(SELECT 1 FROM knowledge_reconsolidation_jobs j
                 WHERE j.page_id = p.id AND j.status IN ('pending', 'running', 'retry'))
               AND NOT EXISTS(SELECT 1 FROM knowledge_reconsolidation_jobs j
                 WHERE j.page_id = p.id AND j.status = 'succeeded'
                   AND (j.last_completed_at IS NULL OR j.last_completed_at >=
                     (SELECT MAX(r.created_at) FROM wiki_revisions r WHERE r.page_id = p.id)))",
        )?;
        // 先收再写，避免语句交互。
        let due = stmt
            .query_map(rusqlite::params![now, now], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut enqueued = 0;
        for page_id in due {
            // 触发分支各自命名：evidence_delta（增量≥3）优先于 scheduled
            //（30 天+新证据），便于观测面板看「为什么整理这页」。
            let trigger: String = self.connection.query_row(
                "SELECT CASE WHEN (
                   SELECT COUNT(*) FROM json_each(p.source_event_ids) je
                     JOIN events e ON e.id = je.value
                    WHERE e.recorded_at > COALESCE(
                      (SELECT MAX(r.created_at) FROM wiki_revisions r WHERE r.page_id = p.id),
                      '1970')) >= 3
                 THEN 'evidence_delta' ELSE 'scheduled' END
                 FROM wiki_pages p WHERE p.id = ?1",
                [&page_id],
                |row| row.get(0),
            )?;
            enqueued += self.connection.execute(
                "INSERT INTO knowledge_reconsolidation_jobs(page_id, trigger, available_at)
                 VALUES(?1, ?2, ?3)
                 ON CONFLICT(page_id) DO UPDATE SET
                   trigger = excluded.trigger, status = 'pending',
                   generation = knowledge_reconsolidation_jobs.generation + 1,
                   attempts = 0, available_at = excluded.available_at
                 WHERE knowledge_reconsolidation_jobs.status = 'succeeded'",
                rusqlite::params![page_id, trigger, now],
            )?;
        }
        Ok(enqueued)
    }

    /// 领取一条到期任务并上心跳锁（与 refresh 同一 generation 语义）。
    /// 返回 None = 当前无可处理任务。
    pub fn claim_reconsolidation_job(&self, now: &str) -> Result<Option<(String, i64)>> {
        let Some(candidate) = self
            .connection
            .prepare(
                "SELECT page_id, generation FROM knowledge_reconsolidation_jobs
             WHERE status IN ('pending', 'retry')
               AND julianday(available_at) <= julianday(?1)
             ORDER BY attempts, available_at, page_id LIMIT 1",
            )?
            .query_row([now], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .optional()?
        else {
            return Ok(None);
        };
        let (page_id, generation) = candidate;
        let heartbeat = (chrono::DateTime::parse_from_rfc3339(now)?
            + chrono::Duration::minutes(10))
        .to_rfc3339();
        let changed = self.connection.execute(
            "UPDATE knowledge_reconsolidation_jobs SET status = 'running',
             attempts = attempts + 1, available_at = ?3
             WHERE page_id = ?1 AND generation = ?2
               AND status IN ('pending', 'retry')",
            rusqlite::params![page_id, generation, heartbeat],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        Ok(Some((page_id, generation)))
    }

    /// 任务收尾：状态、重试退避由 worker 负责，这里只做带 generation 的定稿。
    pub fn finish_reconsolidation_job(
        &self,
        page_id: &str,
        status: &str,
        detail: &str,
    ) -> Result<()> {
        self.connection.execute(
            "UPDATE knowledge_reconsolidation_jobs SET status = ?2, detail = ?3,
             last_completed_at = CASE WHEN ?2 = 'succeeded' THEN ?4 ELSE last_completed_at END,
             last_evidence_count = CASE WHEN ?2 = 'succeeded' THEN
               (SELECT COUNT(*) FROM json_each(p.source_event_ids) je JOIN events e ON e.id = je.value
                WHERE e.recorded_at > COALESCE((SELECT MAX(r.created_at) FROM wiki_revisions r
                  WHERE r.page_id = p.id), '1970'))
             ELSE last_evidence_count END
             FROM wiki_pages p
             WHERE knowledge_reconsolidation_jobs.page_id = ?1",
            rusqlite::params![page_id, status, detail, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }
}
