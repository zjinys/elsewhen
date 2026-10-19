//! 页面读取面：按 slug/来源/标题/别名/标签检索、列表与派生关系查询。

use super::*;

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

    /// 实体名 → canonical slug（D3 归一管道）：标题/别名不区分大小写命中；
    /// 恰好命中一页且 kind 相符、页面 active 才算归一成功（多义或无命中 →
    /// None，调用方降级到草拟确认/人工消歧，绝不猜）。
    /// 已合并页不算命中：其别名在合并时已迁到目标页，标题指向旧页是历史。
    pub fn resolve_entity_slug(&self, kind: &str, name: &str) -> Result<Option<String>> {
        let exact: Vec<WikiPage> = self
            .find_wiki_pages_by_title_or_alias(name)?
            .into_iter()
            .filter(|p| p.kind == kind && p.status == "active")
            .collect();
        Ok(if exact.len() == 1 {
            Some(exact[0].slug.clone())
        } else {
            None
        })
    }

    /// 列出知识库页面（主列表）。
    /// - `kind`：按内容类型过滤；`area`：按来源/用途分区过滤（imported/network/insight）。
    /// - 默认排除派生产物（area=derivative，它们只经 `list_derivatives` 按原文展开读取）。
    /// - 顺序：有来源 URL 的（素材）在前，其余按最近更新时间倒序。
    /// 列表元数据视图（sidebar/列表专用）：无 content_md、无图片解析、
    /// SQL 级排序 + LIMIT 有界。与 `list_wiki_pages` 同款派生产物排除；
    /// 语义装饰（AI 提炼 kind 排除、slug 前缀过滤、archived 状态）留在
    /// Dart 侧 `_isSidebarVisiblePage`——单一真源不过双写。
    pub fn list_wiki_page_metas(&self, limit: i64) -> Result<Vec<WikiPageMeta>> {
        let mut statement = self.connection.prepare_cached(&format!(
            "SELECT {WIKI_PAGE_META_COLS} FROM wiki_pages \
             WHERE COALESCE(area, 'insight') != 'derivative' \
             ORDER BY updated_at DESC, rowid DESC LIMIT ?1"
        ))?;
        let rows = statement.query_map([limit], map_wiki_page_meta)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

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
}
