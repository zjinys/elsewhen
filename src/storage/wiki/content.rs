//! 正文编辑会话：保存正文/观点、改名、revision 历史与 wiki_log 流水。

use super::*;

impl Store {
    /// 人类编辑保存一页正文（「人类直接编辑」主线入口）。
    ///
    /// - 仅允许可编辑 kind（person/project/capability/recurring_cost/topic/…）；
    ///   采集素材 kind（source/note）只读，直接拒绝。
    /// - 非空、长度上限 64k 字符。
    /// - 乐观锁（§11 Q3）：`expected_updated_at` 提供时须与当前 `updated_at` 一致，
    ///   否则报「编辑冲突」——编辑会话期间页面被后台 digest 写回时，拒绝静默覆盖。
    /// - 写 revision（reason 前缀 `[human]`）+ wiki_log 审计；
    /// - 置 `human_edited_at=now`：此后该页被 AI digest 视为「人工持有」，不再整篇覆盖正文。
    pub fn save_wiki_page_content(
        &self,
        slug: &str,
        content_md: &str,
        reason: &str,
        expected_updated_at: Option<&str>,
    ) -> Result<WikiPage> {
        let page = self
            .get_wiki_page(slug)?
            .with_context(|| format!("知识页不存在: {slug}（可能已被改名或删除）"))?;
        if matches!(page.kind.as_str(), "source" | "note") {
            anyhow::bail!(
                "素材页（kind={}）只读，不支持人工编辑正文；只能表态评价（认可/不认可）",
                page.kind
            );
        }
        if let Some(expected) = expected_updated_at {
            // 乐观锁按毫秒精度比较解析后的时间戳，而非字符串相等：
            // Dart 侧 DateTime.parse 会把纳秒截断为微秒并转本地时区，
            // 字符串往返不可能精确还原 rfc3339（"Z" vs "+00:00"、精度位数）。
            // 解析失败（调用方传非 rfc3339）按冲突处理——fail-closed 优于静默覆盖。
            let expected_ts = chrono::DateTime::parse_from_rfc3339(expected);
            let current_ts = chrono::DateTime::parse_from_rfc3339(&page.updated_at);
            let consistent = matches!(
                (expected_ts, current_ts),
                (Ok(e), Ok(c)) if e.timestamp_millis() == c.timestamp_millis()
            );
            if !consistent {
                anyhow::bail!(
                    "编辑冲突：页面在你编辑期间已被更新（加载于 {}，当前 {}）；请重新加载后合并修改，或强制覆盖",
                    expected,
                    page.updated_at
                );
            }
        }
        let content_md = content_md.trim().to_string();
        if content_md.is_empty() {
            anyhow::bail!("正文为空，无法保存");
        }
        let char_count = content_md.chars().count();
        if char_count > 65536 {
            anyhow::bail!("正文超过 64k 字符上限（当前 {char_count} 字符）");
        }
        let now = chrono::Utc::now().to_rfc3339();
        let transaction = self.begin_write_transaction()?;
        self.connection.execute(
            "UPDATE wiki_pages SET content_md=?1, human_edited_at=?2, updated_at=?2 WHERE id=?3",
            params![content_md, now, page.id],
        )?;
        let reason = reason.trim();
        // 页 op 先于修订 op 捕获：收端按流内序 drain，修订的页存在依赖
        // 必须先被页 op 满足。
        self.sync_record_wiki_page(&page.id, false)?;
        self.record_wiki_revision(
            &page.id,
            &content_md,
            &format!(
                "[human] {}",
                if reason.is_empty() {
                    "人工编辑正文"
                } else {
                    reason
                }
            ),
            None,
        )?;
        self.append_wiki_log(&format!(
            "人工编辑正文：{slug}（{}）",
            if reason.is_empty() {
                "无备注"
            } else {
                reason
            }
        ))?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        self.get_wiki_page(slug)?.context("人工编辑保存后读取失败")
    }

    /// 素材页观点评价（素材唯一的交互入口）。
    ///
    /// - 仅允许采集素材 kind（source/note），非素材页拒绝；
    /// - `None` = 清空回未表态；对 `source`/`note` 页即回到「孵化中」
    ///   （2026-10-20 起缺省语义收紧：NULL 不参与编译与自动召回）；
    ///   `Some("endorse")`/`Some("reject")` = 认可 / 不认可；
    /// - 不改变正文、不置位 `human_edited_at`，只写 wiki_log 审计。
    pub fn set_wiki_opinion(&self, slug: &str, opinion: Option<&str>) -> Result<WikiPage> {
        let page = self
            .get_wiki_page(slug)?
            .with_context(|| format!("知识页不存在: {slug}（可能已被改名或删除）"))?;
        if !matches!(page.kind.as_str(), "source" | "note") {
            anyhow::bail!(
                "只有采集素材页（source/note）可以表态评价，当前 kind={}",
                page.kind
            );
        }
        let value: Option<String> = match opinion {
            Some(o) => {
                let o = o.trim();
                if o != "endorse" && o != "reject" {
                    anyhow::bail!("评价只接受 endorse / reject，收到：{o}");
                }
                Some(o.to_string())
            }
            None => None,
        };
        let now = chrono::Utc::now().to_rfc3339();
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        self.connection.execute(
            "UPDATE knowledge_sources SET opinion=?1 WHERE id IN
            (SELECT source_id FROM knowledge_source_pages WHERE page_id=?2)",
            params![value, page.id],
        )?;
        self.connection.execute(
            "UPDATE wiki_pages SET opinion=?1, updated_at=?2 WHERE id=?3 OR id IN
                (SELECT m.page_id FROM knowledge_source_pages m WHERE m.source_id IN
                 (SELECT source_id FROM knowledge_source_pages WHERE page_id=?3))",
            params![value, now, page.id],
        )?;
        let label = match value.as_deref() {
            Some("endorse") => "认可",
            Some("reject") => "不认可",
            _ => "清空（回归未表态，缺省认可）",
        };
        self.append_wiki_log(&format!("素材评价：{slug} → {label}"))?;
        let affected: Vec<String> = {
            let mut stmt = self.connection.prepare(
                "SELECT id FROM wiki_pages WHERE id=?1 OR id IN\
                    (SELECT m.page_id FROM knowledge_source_pages m WHERE m.source_id IN\
                     (SELECT source_id FROM knowledge_source_pages WHERE page_id=?1))",
            )?;
            let rows = stmt.query_map(params![page.id], |r| r.get::<_, String>(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        for id in affected {
            self.sync_record_wiki_page(&id, false)?;
        }
        tx.commit()?;
        self.get_wiki_page(slug)?.context("评价保存后读取失败")
    }

    /// 重命名知识页：标题 + slug 一起换，事务内原子迁移关系引用与页内聊天会话，并追加一条修订记录。
    ///
    /// - 带前缀的 slug（`person/…`、`topic/…` 等）按前缀 + 新标题重算新 slug（撞名自动避让）；
    /// - 无前缀的 slug（`tweet-…`、`kb-…` 等）保留原 slug，只改标题（源资料引用不能断）；
    /// - 标题未变时直接返回 `changed=false`，不做任何写操作。
    pub fn rename_wiki_page(
        &self,
        slug: &str,
        new_title: &str,
        reason: &str,
    ) -> Result<RenameWikiOutcome> {
        let tx = self.begin_write_transaction()?;
        let page = self.get_wiki_page(slug)?.with_context(|| {
        format!(
            "知识页不存在：{slug}（可能已被改名或删除——如果刚改过名，请用新名字操作，可在对话里列出知识库确认当前名称）"
        )
    })?;
        let new_title = new_title.trim().to_string();
        if new_title.is_empty() {
            anyhow::bail!("新标题不能为空");
        }
        let old_title = page.title.clone();
        let changed = new_title != old_title;
        if !changed {
            return Ok(RenameWikiOutcome {
                old_slug: slug.to_string(),
                new_slug: slug.to_string(),
                old_title,
                new_title,
                changed: false,
                relations_moved: 0,
                chats_moved: 0,
            });
        }
        // P5b：重命名同步的 base 锚点必须在本地变更前读取（CAS 锚）。
        let sync_base_revision = self.sync_wiki_base_revision(&page.id)?;
        // 计算新 slug：带前缀的页面重算（撞名避让），无前缀的保留原 slug
        let new_slug = match slug.rsplit_once('/') {
            Some((prefix, _)) => crate::wiki::unique_slug(
                self,
                &format!("{prefix}/{}", crate::wiki::slugify(&new_title)),
                &new_title,
            )?,
            None => slug.to_string(),
        };
        let now = chrono::Utc::now().to_rfc3339();
        // 1) 页面本体：换 slug + 标题，summary 里的旧名同步替换
        let new_summary = if old_title.is_empty() {
            page.summary.clone()
        } else {
            page.summary.replace(&old_title, &new_title)
        };
        self.connection.execute(
            "UPDATE wiki_pages SET slug=?1, title=?2, summary=?3, updated_at=?4 WHERE id=?5",
            params![new_slug, new_title, new_summary, now, page.id],
        )?;
        // 2) 关系引用迁移
        let mut relations_moved = 0usize;
        if new_slug != slug {
            relations_moved += self.connection.execute(
                "UPDATE relations SET from_slug=?1 WHERE from_slug=?2",
                params![new_slug, slug],
            )? as usize;
            relations_moved += self.connection.execute(
                "UPDATE relations SET to_slug=?1 WHERE to_slug=?2",
                params![new_slug, slug],
            )? as usize;
        }
        // 3) 页内聊天会话迁移
        let chats_moved = if new_slug != slug {
            self.connection.execute(
                "UPDATE conversations SET wiki_page_slug=?1 WHERE wiki_page_slug=?2",
                params![new_slug, slug],
            )? as usize
        } else {
            0
        };
        // 4) 习惯问候计划跟随 slug 迁移（唯一索引按 page_slug 绑定）。
        if new_slug != slug {
            self.move_habit_nudges_page(slug, Some(&new_slug))?;
        }
        // 5) 审计修订记录
        self.connection.execute(
        "INSERT INTO wiki_revisions (id, page_id, content_md, reason, source_event_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            Uuid::new_v4().to_string(),
            page.id,
            page.content_md,
            reason,
            Option::<String>::None,
            now,
        ],
    )?;
        self.append_wiki_log(&format!(
            "知识页重命名：{old_title}（{slug}）→ {new_title}（{new_slug}）"
        ))?;
        // 捕获在领域事务内：真正换 slug 走专用 rename 变体（收端做同样的
        // 关系/会话迁移）；标题-only（前缀页保留原 slug）走整行捕获。
        if new_slug != slug {
            self.sync_record_wiki_rename(&page.id, slug, sync_base_revision)?;
        } else {
            self.sync_record_wiki_page(&page.id, false)?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(RenameWikiOutcome {
            old_slug: slug.to_string(),
            new_slug,
            old_title,
            new_title,
            changed: true,
            relations_moved,
            chats_moved,
        })
    }

    pub(crate) fn record_wiki_revision(
        &self,
        page_id: &str,
        content_md: &str,
        reason: &str,
        source_event_id: Option<&str>,
    ) -> Result<()> {
        Self::record_wiki_revision_on(self, page_id, content_md, reason, source_event_id)
    }

    /// 在指定连接上追加一条 wiki revision；事务通过 Deref 传入 `&Transaction` 亦可。
    /// upsert_wiki_page 把「页面写入 + revision」放进同一事务原子提交，
    /// 避免页面改了但 revision 没记（或反之）导致审计断链。
    /// 写一条修订并捕获同步 op（修订历史随流复制，起源端权威）。事务约束
    /// 由调用方满足：所有写路径（upsert/改名/正文保存/标签/项目路径）都
    /// 已在 begin_write_transaction 包裹内或自动提交单写。
    pub(crate) fn record_wiki_revision_on(
        store: &Store,
        page_id: &str,
        content_md: &str,
        reason: &str,
        source_event_id: Option<&str>,
    ) -> Result<()> {
        let conn = &store.connection;
        let revision = crate::sync::protocol::WikiRevisionRecord {
            id: Uuid::new_v4().to_string(),
            page_id: page_id.to_string(),
            content_md: content_md.to_string(),
            reason: reason.to_string(),
            source_event_id: source_event_id.map(|s| s.to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        conn.execute(
            "INSERT INTO wiki_revisions
         (id, page_id, content_md, reason, source_event_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                revision.id,
                revision.page_id,
                revision.content_md,
                revision.reason,
                revision.source_event_id,
                revision.created_at,
            ],
        )?;
        store.sync_record_wiki_revision(&revision)?;
        Ok(())
    }

    pub fn list_wiki_revisions(&self, slug: &str) -> Result<Vec<(String, String, String)>> {
        // (created_at, content_md, reason)
        let mut statement = self.connection.prepare(
            "SELECT r.created_at, r.content_md, r.reason
         FROM wiki_revisions r JOIN wiki_pages p ON p.id = r.page_id
         WHERE p.slug = ?1 ORDER BY r.created_at DESC",
        )?;
        let rows = statement.query_map(params![slug], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn append_wiki_log(&self, entry: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO wiki_log (ts, entry) VALUES (?1, ?2)",
            params![chrono::Utc::now().to_rfc3339(), entry],
        )?;
        Ok(())
    }

    pub fn list_wiki_log(&self, limit: i64) -> Result<Vec<(String, String)>> {
        let mut statement = self
            .connection
            .prepare("SELECT ts, entry FROM wiki_log ORDER BY id DESC LIMIT ?1")?;
        let rows = statement.query_map(params![limit], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }
}
