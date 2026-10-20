//! 页面写入面：派生创建、upsert、资产采集、标签与项目路径维护、回填。

use super::*;

impl Store {
    /// 创建一个「派生产物」页：对某页加工（总结/提炼观点/文案…）的成果。
    /// 挂靠原文（based_on + content_type），area=derivative，不进主列表；
    /// 不修改原文页的任何内容。
    pub fn create_derivative(
        &self,
        based_on_slug: &str,
        content_type: &str,
        title: &str,
        content_md: &str,
        reason: &str,
    ) -> Result<WikiPage> {
        self.create_derivative_with_generation(
            based_on_slug,
            content_type,
            title,
            content_md,
            reason,
            None,
        )
    }

    pub(crate) fn create_derivative_with_generation(
        &self,
        based_on_slug: &str,
        content_type: &str,
        title: &str,
        content_md: &str,
        reason: &str,
        conversation: Option<&str>,
    ) -> Result<WikiPage> {
        anyhow::ensure!(
            !content_type.trim().is_empty()
                && content_type.chars().count() <= 80
                && !title.trim().is_empty()
                && title.chars().count() <= 200,
            "产物类型或标题无效"
        );
        let tx = self.begin_write_transaction()?;
        let base = self.get_wiki_page(based_on_slug)?.with_context(|| {
            format!("知识页不存在：{based_on_slug}（不能对不存在的页面创建派生产物）")
        })?;
        let content_md = content_md.trim().to_string();
        if content_md.is_empty() {
            anyhow::bail!("派生产物正文为空，无法保存");
        }
        let id = Uuid::new_v4().to_string();
        let slug = format!("der-{}", &id[..8]);
        let summary: String = content_md
            .chars()
            .take(120)
            .collect::<String>()
            .trim_end()
            .to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let tags_raw = serde_json::to_string(&vec!["派生产物".to_string()])?;
        self.connection.execute(
            "INSERT INTO wiki_pages
         (id, slug, kind, title, summary, content_md, tags, source_event_ids,
          evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
          source_url, area, based_on, content_type)
         VALUES (?1, ?2, 'derivative', ?3, ?4, ?5, ?6, '[]', 1,
                 ?7, ?7, 'active', ?7, ?7, NULL, 'derivative', ?8, ?9)",
            params![
                id,
                slug,
                title.trim(),
                summary,
                content_md,
                tags_raw,
                now,
                base.slug,
                content_type.trim(),
            ],
        )?;
        self.record_wiki_revision(&id, &content_md, reason, None)?;
        let snapshots = self
            .page_source_snapshots(&base.slug)?
            .into_iter()
            .map(|s| s.id)
            .collect::<Vec<_>>();
        self.bind_page_sources(&id, &snapshots)?;
        let bases = crate::knowledge::dependencies::capture(self, &[base.slug.clone()])?;
        crate::knowledge::dependencies::bind(self, &id, &bases)?;
        self.connection.execute(
            "UPDATE wiki_pages SET source_event_ids=?1,evidence_count=?2 WHERE id=?3",
            params![
                serde_json::to_string(&base.source_event_ids)?,
                base.source_event_ids.len() as i64,
                id
            ],
        )?;
        self.connection.execute("INSERT INTO knowledge_artifact_versions(page_id,parent_id,content_type,version,created_at) SELECT ?1,?2,?3,COALESCE(MAX(version),0)+1,?4 FROM knowledge_artifact_versions WHERE parent_id=?2 AND content_type=?3",params![id,base.id,content_type.trim(),now])?;
        let page = self
            .get_wiki_page(&slug)?
            .context("派生产物创建后读取失败")?;
        if let Some(conversation) = conversation {
            crate::knowledge::workflows::attach_generation(self, &page, conversation)?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(page)
    }

    /// 新建 wiki 页时按 kind / slug / 来源自动推导「来源/用途」分区。
    /// - `network`：人物（person）、事情/项目（topic/ 前缀）——关系网实体；
    /// - `imported`：外部素材（kind=source 或有来源 URL，以及 tweet-/note-/import- 前缀的导入页）；
    /// - `derivative`：派生产物（不经过 draft 新建，但兜底）；
    /// - 其余（AI 对话沉淀的知识、规则等）→ `insight`。
    fn derive_wiki_area(kind: &str, slug: &str, source_url: Option<&str>) -> String {
        if kind == "derivative" {
            return "derivative".to_string();
        }
        if kind == "person"
            || kind == "project"
            || slug.starts_with("person/")
            || slug.starts_with("project/")
            || slug.starts_with("topic/")
        {
            return "network".to_string();
        }
        if kind == "source"
            || source_url.is_some()
            || slug.starts_with("tweet-")
            || slug.starts_with("note-")
            || slug.starts_with("import-")
        {
            return "imported".to_string();
        }
        "insight".to_string()
    }

    /// 创建或更新一个 wiki 页面。核心做确定性合并：
    /// 已存在 → 更新内容 + 事件 id 并集 + evidence_count = 并集长度；不存在 → 新建。
    /// 每次写回都记录一条 revision（页面与 revision 同事务提交）。
    pub fn upsert_wiki_page(
        &self,
        draft: &WikiPageDraft,
        policy: ContentPolicy,
    ) -> Result<WikiUpsertOutcome> {
        let tx = self.begin_write_transaction()?;
        let outcome = self.upsert_wiki_page_in_tx(draft, policy)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(outcome)
    }

    /// [`upsert_wiki_page`] 的事务内版本：不自开事务，由调用方持有事务并决定提交。
    /// 知识消化批次用它把多页写入、日志与任务确认收拢到同一事务（全成或全不成）。
    pub(crate) fn upsert_wiki_page_in_tx(
        &self,
        draft: &WikiPageDraft,
        policy: ContentPolicy,
    ) -> Result<WikiUpsertOutcome> {
        let mut resolved = draft.clone();
        if matches!(draft.kind.as_str(), "source" | "note") {
            if let Some(existing) =
                self.matching_source_page(draft.source_url.as_deref(), &draft.content_md)?
            {
                resolved.slug = existing.slug;
            }
        }
        let draft = &resolved;
        let now = chrono::Utc::now().to_rfc3339();
        let tags_raw = serde_json::to_string(&draft.tags)?;

        let existing = self.get_wiki_page(&draft.slug)?;
        if let Some(page) = existing {
            // 人工持有判定：仅 PreserveHumanEdits 拦截——human_edited_at 非空
            // （人工编辑过的档案/派生页）或 kind∈{source,note}（采集素材，对人和
            // AI 都只读正文，仅素材导入流用 Always 刷新）。
            // Always 是显式授权路径（素材导入；AI 草拟→用户确认的修订/建档），不拦。
            // 保护时正文（content_md/title/summary/tags）不变，只并集证据 + 刷新 last_seen_at。
            let human_held = match policy {
                ContentPolicy::Always => false,
                ContentPolicy::PreserveHumanEdits => {
                    page.human_edited_at.is_some()
                        || matches!(page.kind.as_str(), "source" | "note")
                }
            };
            let content_changed = page.content_md != draft.content_md;
            // 合并（确定性，不允许 LLM 直接改数字）
            let mut all_ids = page.source_event_ids.clone();
            for id in &draft.source_event_ids {
                if !all_ids.contains(id) {
                    all_ids.push(id.clone());
                }
            }
            let evidence_count = all_ids.len() as i64;
            let sources_raw = serde_json::to_string(&all_ids)?;
            if human_held {
                if content_changed
                    && !matches!(page.kind.as_str(), "source" | "note")
                    && !draft.source_event_ids.is_empty()
                {
                    let mut proposal = draft.clone();
                    proposal.source_event_ids = all_ids.clone();
                    let snapshots = self
                        .page_source_snapshots(&page.slug)?
                        .into_iter()
                        .map(|s| s.id)
                        .collect::<Vec<_>>();
                    self.record_proposal(
                        &proposal,
                        &self.knowledge_metadata(&page.slug)?.applicable_when,
                        &snapshots,
                        Some(&page),
                        "新证据建议补充人工编辑页",
                    )?;
                }
                // 只累加证据
                self.connection.execute(
                    "UPDATE wiki_pages
                 SET source_event_ids=?1, evidence_count=?2, last_seen_at=?3, updated_at=?3
                 WHERE id=?4",
                    params![sources_raw, evidence_count, now, page.id,],
                )?;
                let updated = self.get_wiki_page(&draft.slug)?.unwrap();
                return Ok(WikiUpsertOutcome {
                    created: false,
                    page: updated,
                    protected: true,
                });
            }
            self.connection.execute(
                "UPDATE wiki_pages
             SET title=?1, summary=?2, content_md=?3, tags=?4, source_event_ids=?5,
                 evidence_count=?6, last_seen_at=?7, status=?8, updated_at=?7,
                 source_url=COALESCE(?10, source_url)
             WHERE id=?9",
                params![
                    draft.title,
                    draft.summary,
                    draft.content_md,
                    tags_raw,
                    sources_raw,
                    evidence_count,
                    now,
                    draft.status,
                    page.id,
                    draft.source_url,
                ],
            )?;
            self.sync_record_wiki_page(&page.id, false)?;
            // 空正文不落 revision：协议校验拒收空 revision
            // （sync_wiki_revision_empty），空内容亦无信息量——页面本体已随
            // WikiPageChanged 同步，快记追加后再有非空 revision。
            if !draft.content_md.is_empty() {
                Self::record_wiki_revision_on(
                    self,
                    &page.id,
                    &draft.content_md,
                    &draft.reason,
                    None,
                )?;
            }
            let updated = self.get_wiki_page(&draft.slug)?.unwrap();
            crate::storage::knowledge::capture_source_on(&self.connection, &updated)?;
            Ok(WikiUpsertOutcome {
                created: false,
                page: updated,
                protected: false,
            })
        } else {
            let id = Uuid::new_v4().to_string();
            let sources_raw = serde_json::to_string(&draft.source_event_ids)?;
            let evidence_count = draft.source_event_ids.len() as i64;
            // 新建页自动推导分区（旧页保留原分区，见上方 UPDATE 分支）
            let area =
                Self::derive_wiki_area(&draft.kind, &draft.slug, draft.source_url.as_deref());
            self.connection.execute(
                "INSERT INTO wiki_pages
             (id, slug, kind, title, summary, content_md, tags, source_event_ids,
              evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
              source_url, area)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11, ?10, ?10, ?12, ?13)",
                params![
                    id,
                    draft.slug,
                    draft.kind,
                    draft.title,
                    draft.summary,
                    draft.content_md,
                    tags_raw,
                    sources_raw,
                    evidence_count,
                    now,
                    draft.status,
                    draft.source_url,
                    area,
                ],
            )?;
            self.sync_record_wiki_page(&id, false)?;
            // 同上：空正文（记事本建档即空页）不落 revision，否则协议校验
            // sync_wiki_revision_empty 直接拒绝建档事务。
            if !draft.content_md.is_empty() {
                Self::record_wiki_revision_on(self, &id, &draft.content_md, &draft.reason, None)?;
            }
            // 占位符引用的图片字节随页入库（文件在导入下载阶段已落盘）。
            let _ = self.ingest_wiki_page_assets();
            let page = self.get_wiki_page(&draft.slug)?.unwrap();
            crate::storage::knowledge::capture_source_on(&self.connection, &page)?;
            Ok(WikiUpsertOutcome {
                created: true,
                page,
                protected: false,
            })
        }
    }

    /// 把本机文件系统里被正文占位符引用的图片记入 `wiki_assets` 台账
    /// （字节不入库，§31b；快照文件段按台账导出时读盘搬运）。幂等：已有行按 rel 跳过。
    /// 扫描全部页面（个人量级为个位数），文件缺失的引用静默跳过——
    /// 字节本就无法凭空恢复，显示层自愈时自然退化。
    pub(crate) fn ingest_wiki_page_assets(&self) -> Result<usize> {
        let data_dir = self
            .path
            .parent()
            .map(std::path::Path::to_path_buf)
            .context("数据目录不可定位，无法收集图片资产")?;
        let mut relations: i64 = 0;
        let pages: Vec<String> = {
            let mut q = self
                .connection
                .prepare("SELECT content_md FROM wiki_pages")?;
            let rows = q.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        for content_md in pages {
            for rel in crate::storage::wiki_images::collect_tweet_image_rels(&content_md) {
                let existing: bool = self.connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM wiki_assets WHERE rel=?1)",
                    [&rel],
                    |r| r.get(0),
                )?;
                if existing {
                    continue;
                }
                let Some(ext) = rel.rsplit('.').next() else {
                    continue;
                };
                let Some(mime) = crate::storage::wiki_images::mime_for_ext(ext) else {
                    continue;
                };
                let Ok(bytes) = std::fs::read(data_dir.join(&rel)) else {
                    continue;
                };
                let sha = {
                    use ring::digest::{Digest, SHA256};
                    let d: Digest = ring::digest::digest(&SHA256, &bytes);
                    d.as_ref()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                };
                self.connection.execute(
                    "INSERT OR REPLACE INTO wiki_assets(rel,sha256,mime,size) VALUES(?1,?2,?3,?4)",
                    rusqlite::params![rel, sha, mime, bytes.len() as i64],
                )?;
                relations += 1;
            }
        }
        Ok(relations as usize)
    }

    /// 用户手动重写一页的标签（元数据组织用）。
    /// 规范化：去 `#` 前缀、去首尾空白、去重、保序，最多保留 24 个。
    /// 标签变更会更新 `updated_at` 并追加一条 revision（正文不变，便于审计）。
    pub fn update_wiki_tags(&self, slug: &str, tags: &[String]) -> Result<WikiPage> {
        let page = self
            .get_wiki_page(slug)?
            .with_context(|| format!("知识页不存在: {slug}"))?;
        let mut cleaned: Vec<String> = Vec::new();
        for raw in tags {
            let t = raw.trim().trim_start_matches('#').trim().to_string();
            if t.is_empty() || cleaned.contains(&t) {
                continue;
            }
            cleaned.push(t);
            if cleaned.len() >= 24 {
                break;
            }
        }
        let now = chrono::Utc::now().to_rfc3339();
        let tags_raw = serde_json::to_string(&cleaned)?;
        let transaction = self.begin_write_transaction()?;
        self.connection.execute(
            "UPDATE wiki_pages SET tags = ?1, updated_at = ?2 WHERE id = ?3",
            params![tags_raw, now, page.id],
        )?;
        // 审计：标签变更也留一条 revision（正文沿用当前内容，reason 记录本次动作）
        let reason = if cleaned.is_empty() {
            "标签更新：（清空）".to_string()
        } else {
            format!("标签更新：{}", cleaned.join(", "))
        };
        self.record_wiki_revision(&page.id, &page.content_md, &reason, None)?;
        self.sync_record_wiki_page(&page.id, false)?;
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        self.get_wiki_page(slug)?.context("标签更新后读取失败")
    }

    /// 修改一张项目页关联的本地目录（目录搬家后在这里纠正路径）。
    /// - 仅 kind=project；新路径必须存在且是目录（否则拒绝，避免指到空处）；
    /// - 路径记进 `source_url`（file:// 规范形式），正文里的「项目目录：`...`」快照行同步改掉；
    /// - 留 revision + wiki_log，不置 `human_edited_at`（元数据修正，不是正文创作）。
    pub fn update_project_path(&self, slug: &str, new_path: &str) -> Result<WikiPage> {
        let page = self
            .get_wiki_page(slug)?
            .with_context(|| format!("知识页不存在: {slug}"))?;
        if page.kind != "project" {
            anyhow::bail!(
                "只有项目页（kind=project）可以修改本地路径，当前 kind={}",
                page.kind
            );
        }
        let trimmed = new_path.trim();
        if trimmed.is_empty() {
            anyhow::bail!("新路径不能为空");
        }
        let raw = std::path::PathBuf::from(trimmed);
        if !raw.is_dir() {
            anyhow::bail!("目录不存在或不可访问: {trimmed}");
        }
        // 规范化（解引用符号链接）：同一目录永远得到同一 file://，去重不漂移。
        let canonical = raw.canonicalize().unwrap_or(raw);
        let new_url = crate::wiki::path_to_file_url(&canonical);
        let display = canonical.display().to_string();
        // 正文快照行同步：只换「项目目录：`...`」这一行的反引号内路径。
        let mut content = page.content_md.clone();
        if let Some(pos) = content.find("项目目录：") {
            let rest = &content[pos..];
            if let Some(open) = rest.find('`') {
                let after_open = pos + open + 1;
                if let Some(close_rel) = content[after_open..].find('`') {
                    content.replace_range(after_open..after_open + close_rel, &display);
                }
            }
        }
        let old_url = page.source_url.clone().unwrap_or_default();
        let now = chrono::Utc::now().to_rfc3339();
        // 修订捕获要求领域事务（P5b：修订历史随流复制）。
        let tx = self.begin_write_transaction()?;
        self.connection.execute(
            "UPDATE wiki_pages SET source_url = ?1, content_md = ?2, updated_at = ?3 WHERE id = ?4",
            params![new_url, content, now, page.id],
        )?;
        let reason = if old_url.is_empty() {
            format!("项目路径设置：{display}")
        } else {
            format!("项目路径修改：{old_url} → {new_url}")
        };
        self.record_wiki_revision(&page.id, &content, &reason, None)?;
        self.sync_record_wiki_page(&page.id, false)?;
        self.append_wiki_log(&format!("项目路径修改：{slug} → {display}"))?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        self.get_wiki_page(slug)?.context("项目路径修改后读取失败")
    }

    /// 存量回填（幂等）：目录导入时代久远的项目页只有正文快照、没有 `source_url`，
    /// 从「项目目录：`...`」解析出路径并记进 `source_url`（file://）。
    /// 只命中 `source_url IS NULL` 的 project 页，填过后不再重复执行，无副作用。
    /// 启动期一次性回填：存量页正文里的本机绝对路径/杂形态引用折叠为
    /// 占位符（§31 之前导入的推文配图全是绝对路径），并收集其字节入
    /// `wiki_assets`。裸 UPDATE + wiki_log（同 backfill_project_source_urls
    /// 惯例：不记 revision、不发 op），对端靠下一份快照收敛。幂等。
    pub fn backfill_wiki_image_placeholders(&self) -> Result<usize> {
        let rows: Vec<(String, String, String)> = {
            let mut statement = self
                .connection
                .prepare("SELECT id, slug, content_md FROM wiki_pages")?;
            let mapped = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            mapped.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut changed = 0;
        for (id, slug, content_md) in rows {
            let folded = crate::storage::wiki_images::fold_tweet_image_refs(&content_md);
            if folded == content_md {
                continue;
            }
            self.connection.execute(
                "UPDATE wiki_pages SET content_md = ?1 WHERE id = ?2",
                params![folded, id],
            )?;
            self.append_wiki_log(&format!("图片引用回填：{slug} 占位符归一化"))?;
            changed += 1;
        }
        // 顺手把存量引用的字节收进资产表（文件还在本机就能收齐）。
        let collected = self.ingest_wiki_page_assets()?;
        if changed > 0 || collected > 0 {
            self.append_wiki_log(&format!(
                "图片资产回填：{changed} 页引用归一化，{collected} 字节入库"
            ))?;
        }
        Ok(changed)
    }

    pub fn backfill_project_source_urls(&self) -> Result<usize> {
        let mut statement = self.connection.prepare(
            "SELECT id, slug, content_md FROM wiki_pages
         WHERE kind = 'project' AND source_url IS NULL",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        let mut filled = 0;
        for (id, slug, content) in rows {
            let Some(path) = Self::parse_project_dir_snapshot(&content) else {
                continue;
            };
            let url = crate::wiki::path_to_file_url(&path);
            let now = chrono::Utc::now().to_rfc3339();
            self.connection.execute(
                "UPDATE wiki_pages SET source_url = ?1, updated_at = ?2 WHERE id = ?3",
                params![url, now, id],
            )?;
            self.append_wiki_log(&format!("项目路径回填：{slug} → {}", path.display()))?;
            filled += 1;
        }
        Ok(filled)
    }

    /// 从项目页正文快照解析目录（`项目目录：` + 反引号路径，analyze_project 的固定格式）。
    fn parse_project_dir_snapshot(content: &str) -> Option<std::path::PathBuf> {
        let pos = content.find("项目目录：")?;
        let rest = &content[pos..];
        let open = rest.find('`')?;
        let after = &rest[open + 1..];
        let close = after.find('`')?;
        let path = after[..close].trim();
        if path.is_empty() || !path.starts_with('/') {
            return None;
        }
        Some(std::path::PathBuf::from(path))
    }
}
