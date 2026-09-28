//! 知识库（wiki）存取（从 `Store` 抽出）。
//!
//! wiki_pages（含 area 分区 / based_on 派生 / content_type）+ wiki_revisions
//! （每次正文变更落版本）+ wiki_log（操作流水）。合并/改名经 record_wiki_revision
//! 记录历史，slug 唯一性由 unique_slug 类逻辑保证。

use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use super::{
    map_wiki_page, ContentPolicy, RenameWikiOutcome, Store, WikiPage, WikiPageDraft,
    WikiUpsertOutcome, WIKI_PAGE_COLS,
};

impl Store {
    pub fn get_wiki_page(&self, slug: &str) -> Result<Option<WikiPage>> {
        let mut statement = self.connection.prepare(
        "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                source_url, COALESCE(area, 'insight'), based_on, content_type, human_edited_at, opinion
         FROM wiki_pages WHERE slug = ?1",
    )?;
        let page = statement
            .query_row(params![slug], |row| map_wiki_page(row))
            .optional()?;
        Ok(page)
    }

    /// 按来源 URL 查已导入页面（URL 去重用）
    pub fn find_wiki_page_by_source_url(&self, source_url: &str) -> Result<Option<WikiPage>> {
        let mut statement = self.connection.prepare(
        "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                source_url, COALESCE(area, 'insight'), based_on, content_type, human_edited_at, opinion
         FROM wiki_pages WHERE source_url = ?1
         ORDER BY updated_at DESC LIMIT 1",
    )?;
        let page = statement
            .query_row(params![source_url], |row| map_wiki_page(row))
            .optional()?;
        Ok(page)
    }

    /// 按标题精确查已存在页面（标题去重用；先于确定性 slug 判断，避免同名页重复建档）
    pub fn find_wiki_page_by_title(&self, title: &str) -> Result<Option<WikiPage>> {
        let mut statement = self.connection.prepare(
        "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                source_url, COALESCE(area, 'insight'), based_on, content_type, human_edited_at, opinion
         FROM wiki_pages WHERE lower(trim(title)) = lower(trim(?1))
         ORDER BY updated_at DESC LIMIT 1",
    )?;
        let page = statement
            .query_row(params![title], |row| map_wiki_page(row))
            .optional()?;
        Ok(page)
    }

    pub fn find_wiki_pages_by_title(&self, title: &str) -> Result<Vec<WikiPage>> {
        let mut statement = self.connection.prepare(
        "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                source_url, COALESCE(area, 'insight'), based_on, content_type, human_edited_at, opinion
         FROM wiki_pages WHERE lower(trim(title)) = lower(trim(?1))
         ORDER BY updated_at DESC",
    )?;
        let rows = statement.query_map(params![title], map_wiki_page)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn find_wiki_pages_by_title_or_alias(&self, name: &str) -> Result<Vec<WikiPage>> {
        let mut statement = self.connection.prepare(
        "SELECT DISTINCT p.id, p.slug, p.kind, p.title, p.summary, p.content_md, p.tags, p.source_event_ids,
                p.evidence_count, p.first_seen_at, p.last_seen_at, p.status, p.created_at, p.updated_at,
                p.source_url, COALESCE(p.area, 'insight'), p.based_on, p.content_type,
                p.human_edited_at, p.opinion
         FROM wiki_pages p LEFT JOIN entity_aliases a ON a.entity_slug=p.slug AND a.entity_kind=p.kind
         WHERE lower(trim(p.title))=lower(trim(?1)) OR lower(trim(a.alias))=lower(trim(?1))
         ORDER BY p.updated_at DESC",
    )?;
        let rows = statement.query_map(params![name], map_wiki_page)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 列出知识库页面（主列表）。
    /// - `kind`：按内容类型过滤；`area`：按来源/用途分区过滤（imported/network/insight）。
    /// - 默认排除派生产物（area=derivative，它们只经 `list_derivatives` 按原文展开读取）。
    /// - 顺序：有来源 URL 的（素材）在前，其余按最近更新时间倒序。
    pub fn list_wiki_pages(&self, kind: Option<&str>, area: Option<&str>) -> Result<Vec<WikiPage>> {
        let mut sql = String::from("SELECT ");
        sql.push_str(WIKI_PAGE_COLS);
        sql.push_str(" FROM wiki_pages WHERE 1=1");
        let mut owned: Vec<String> = Vec::new();
        match area {
            Some(a) => {
                owned.push(a.to_string());
                sql.push_str(" AND COALESCE(area, 'insight') = ?");
            }
            None => sql.push_str(" AND COALESCE(area, 'insight') != 'derivative'"),
        }
        if let Some(k) = kind {
            owned.push(k.to_string());
            sql.push_str(" AND kind = ?");
        }
        sql.push_str(" ORDER BY source_url IS NULL, last_seen_at DESC");
        let arg_refs: Vec<&dyn rusqlite::ToSql> =
            owned.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(arg_refs.as_slice(), map_wiki_page)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 某页的派生产物列表（AI 加工成果：总结/提炼/文案…），按创建时间倒序。
    pub fn list_derivatives(&self, based_on: &str) -> Result<Vec<WikiPage>> {
        let mut statement = self.connection.prepare(
        "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                source_url, COALESCE(area, 'insight'), based_on, content_type, human_edited_at, opinion
         FROM wiki_pages WHERE based_on = ?1 AND area = 'derivative'
         ORDER BY created_at DESC",
    )?;
        let rows = statement.query_map(params![based_on], map_wiki_page)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// 按标签精确匹配定位知识页（tags 为 JSON 数组字符串，用 instr 做含有匹配，
    /// 避免路径分隔字符演义的 LIKE 转义问题）。上限取最近更新的一页。
    pub fn find_wiki_page_by_tag(&self, tag: &str) -> Result<Option<WikiPage>> {
        let mut statement = self.connection.prepare(
        "SELECT id, slug, kind, title, summary, content_md, tags, source_event_ids,
                evidence_count, first_seen_at, last_seen_at, status, created_at, updated_at,
                source_url, COALESCE(area, 'insight'), based_on, content_type, human_edited_at, opinion
         FROM wiki_pages
         WHERE instr(tags, ?1) > 0
         ORDER BY updated_at DESC LIMIT 1",
    )?;
        let rows = statement.query_map(params![tag], map_wiki_page)?;
        let mut pages = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(pages.pop())
    }

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
        self.get_wiki_page(&slug)?.context("派生产物创建后读取失败")
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
        let tx = self.connection.unchecked_transaction()?;
        let outcome = self.upsert_wiki_page_in_tx(draft, policy)?;
        tx.commit()?;
        Ok(outcome)
    }

    /// [`upsert_wiki_page`] 的事务内版本：不自开事务，由调用方持有事务并决定提交。
    /// 知识消化批次用它把多页写入、日志与任务确认收拢到同一事务（全成或全不成）。
    pub(crate) fn upsert_wiki_page_in_tx(
        &self,
        draft: &WikiPageDraft,
        policy: ContentPolicy,
    ) -> Result<WikiUpsertOutcome> {
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
            if human_held && content_changed {
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
            Self::record_wiki_revision_on(
                &self.connection,
                &page.id,
                &draft.content_md,
                &draft.reason,
                None,
            )?;
            let updated = self.get_wiki_page(&draft.slug)?.unwrap();
            Ok(WikiUpsertOutcome {
                created: false,
                page: updated,
                protected: false,
            })
        } else {
            let id = Uuid::new_v4().to_string();
            let sources_raw = serde_json::to_string(&draft.source_event_ids)?;
            let evidence_count = draft.source_event_ids.len().max(1) as i64;
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
            Self::record_wiki_revision_on(
                &self.connection,
                &id,
                &draft.content_md,
                &draft.reason,
                None,
            )?;
            Ok(WikiUpsertOutcome {
                created: true,
                page: self.get_wiki_page(&draft.slug)?.unwrap(),
                protected: false,
            })
        }
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
        self.append_wiki_log(&format!("项目路径修改：{slug} → {display}"))?;
        self.get_wiki_page(slug)?.context("项目路径修改后读取失败")
    }

    /// 存量回填（幂等）：目录导入时代久远的项目页只有正文快照、没有 `source_url`，
    /// 从「项目目录：`...`」解析出路径并记进 `source_url`（file://）。
    /// 只命中 `source_url IS NULL` 的 project 页，填过后不再重复执行，无副作用。
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
        self.connection.execute(
            "UPDATE wiki_pages SET content_md=?1, human_edited_at=?2, updated_at=?2 WHERE id=?3",
            params![content_md, now, page.id],
        )?;
        let reason = reason.trim();
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
        self.get_wiki_page(slug)?.context("人工编辑保存后读取失败")
    }

    /// 素材页观点评价（素材唯一的交互入口）。
    ///
    /// - 仅允许采集素材 kind（source/note），非素材页拒绝；
    /// - `None` = 清空回未表态（读取时按缺省认可 'endorse' 处理）；
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
        self.connection.execute(
            "UPDATE wiki_pages SET opinion=?1, updated_at=?2 WHERE id=?3",
            params![value, now, page.id],
        )?;
        let label = match value.as_deref() {
            Some("endorse") => "认可",
            Some("reject") => "不认可",
            _ => "清空（回归未表态，缺省认可）",
        };
        self.append_wiki_log(&format!("素材评价：{slug} → {label}"))?;
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
        let tx = self.connection.unchecked_transaction()?;
        // 1) 页面本体：换 slug + 标题，summary 里的旧名同步替换
        let new_summary = if old_title.is_empty() {
            page.summary.clone()
        } else {
            page.summary.replace(&old_title, &new_title)
        };
        tx.execute(
            "UPDATE wiki_pages SET slug=?1, title=?2, summary=?3, updated_at=?4 WHERE id=?5",
            params![new_slug, new_title, new_summary, now, page.id],
        )?;
        // 2) 关系引用迁移
        let mut relations_moved = 0usize;
        if new_slug != slug {
            relations_moved += tx.execute(
                "UPDATE relations SET from_slug=?1 WHERE from_slug=?2",
                params![new_slug, slug],
            )? as usize;
            relations_moved += tx.execute(
                "UPDATE relations SET to_slug=?1 WHERE to_slug=?2",
                params![new_slug, slug],
            )? as usize;
        }
        // 3) 页内聊天会话迁移
        let chats_moved = if new_slug != slug {
            tx.execute(
                "UPDATE conversations SET wiki_page_slug=?1 WHERE wiki_page_slug=?2",
                params![new_slug, slug],
            )? as usize
        } else {
            0
        };
        // 4) 审计修订记录
        tx.execute(
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
        tx.commit()?;
        self.append_wiki_log(&format!(
            "知识页重命名：{old_title}（{slug}）→ {new_title}（{new_slug}）"
        ))?;
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

    fn record_wiki_revision(
        &self,
        page_id: &str,
        content_md: &str,
        reason: &str,
        source_event_id: Option<&str>,
    ) -> Result<()> {
        Self::record_wiki_revision_on(
            &self.connection,
            page_id,
            content_md,
            reason,
            source_event_id,
        )
    }

    /// 在指定连接上追加一条 wiki revision；事务通过 Deref 传入 `&Transaction` 亦可。
    /// upsert_wiki_page 把「页面写入 + revision」放进同一事务原子提交，
    /// 避免页面改了但 revision 没记（或反之）导致审计断链。
    fn record_wiki_revision_on(
        conn: &rusqlite::Connection,
        page_id: &str,
        content_md: &str,
        reason: &str,
        source_event_id: Option<&str>,
    ) -> Result<()> {
        conn.execute(
            "INSERT INTO wiki_revisions
         (id, page_id, content_md, reason, source_event_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                Uuid::new_v4().to_string(),
                page_id,
                content_md,
                reason,
                source_event_id,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
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
