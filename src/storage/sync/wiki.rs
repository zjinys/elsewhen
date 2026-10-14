//! P5a：wiki 页面的复制底座。slug 是对外身份锚、id 是行身份；任何一端的
//! 漂移都必须显式冲突（`slug_owned_by_other_id` / `slug_changed`），不得
//! 静默重挂。证据（source_event_ids）引用的事件未抵达本地时等待补齐，
//! 不推断历史依据（S05）。

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use super::{Apply, Store};
use crate::sync::protocol::{SyncMutation, SyncOperation, WikiPageRecord};

fn parse_tags(raw: &str) -> std::result::Result<Vec<String>, serde_json::Error> {
    serde_json::from_str(raw)
}

fn parse_event_ids(raw: &str) -> std::result::Result<Vec<String>, serde_json::Error> {
    serde_json::from_str(raw)
}

/// 单行读出后把 JSON 列一次性解析，避免在 rusqlite 闭包内 anyhow 错误转换。
fn wiki_page_record_from_row(
    id: String,
    slug: String,
    kind: String,
    title: String,
    summary: String,
    content_md: String,
    tags: String,
    source_event_ids: String,
    status: String,
    area: String,
    based_on: Option<String>,
    content_type: Option<String>,
    source_url: Option<String>,
    human_edited_at: Option<String>,
    opinion: Option<String>,
    created_at: String,
    updated_at: String,
) -> std::result::Result<WikiPageRecord, serde_json::Error> {
    Ok(WikiPageRecord {
        id,
        slug,
        kind,
        title,
        summary,
        content_md,
        tags: parse_tags(&tags)?,
        source_event_ids: parse_event_ids(&source_event_ids)?,
        status,
        area,
        based_on,
        content_type,
        source_url,
        human_edited_at,
        opinion,
        created_at,
        updated_at,
    })
}

impl Store {
    pub(crate) fn sync_wiki_page_record(&self, id: &str) -> Result<Option<WikiPageRecord>> {
        Ok(self
            .connection
            .query_row(
                "SELECT id,slug,kind,title,summary,content_md,tags,source_event_ids,status,area, \
                 based_on,content_type,source_url,human_edited_at,opinion,created_at,updated_at \
                 FROM wiki_pages WHERE id=?1",
                params![id],
                |r| {
                    let tags: String = r.get(6)?;
                    let evidence: String = r.get(7)?;
                    wiki_page_record_from_row(
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        tags,
                        evidence,
                        r.get(8)?,
                        r.get(9)?,
                        r.get(10)?,
                        r.get(11)?,
                        r.get(12)?,
                        r.get(13)?,
                        r.get(14)?,
                        r.get(15)?,
                        r.get(16)?,
                    )
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            6,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })
                },
            )
            .optional()?)
    }

    /// 本地 wiki 编辑入站的捕获点。与会话捕获同模式：非同步库是 no-op，
    /// 操作与业务写入同事务提交，head 以本操作为修订。
    pub(crate) fn sync_record_wiki_page(&self, id: &str, deleted: bool) -> Result<()> {
        if self.sync_identity()?.is_none() {
            return Ok(());
        }
        let page = self
            .sync_wiki_page_record(id)?
            .context("sync_wiki_page_missing")?;
        let base_revision = self.sync_object_head("wiki_page", id)?.map(|h| h.0);
        if let Some(op_id) = self.append_sync_mutation(SyncMutation::WikiPageChanged {
            page,
            base_revision,
            deleted,
        })? {
            self.sync_set_head("wiki_page", id, &op_id, deleted)?;
        }
        Ok(())
    }

    pub(super) fn apply_sync_wiki_page(
        &self,
        op: &SyncOperation,
        page: &WikiPageRecord,
        base_revision: &Option<String>,
        deleted: bool,
    ) -> Result<Apply> {
        let head = self.sync_object_head("wiki_page", &page.id)?;
        if let Some(base) = base_revision {
            let known: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM sync_operations WHERE op_id=?1 AND state IN ('applied','conflict') \
                 UNION ALL SELECT 1 FROM sync_known_revisions WHERE object_kind='wiki_page' AND object_id=?2 AND revision_id=?1)",
                params![base, page.id],
                |r| r.get(0),
            )?;
            if !known && head.as_ref().map(|h| &h.0) != Some(base) {
                return Ok(Apply::Waiting);
            }
        }
        if head.as_ref().is_some_and(|h| h.1) {
            // 已删页头上到达的任何编辑都是「删除后离线编辑」候选，优先于
            // base CAS 报告——否则原因会被 generic 的 base_revision_changed 掩盖。
            return Ok(Apply::Conflict {
                kind: "wiki_page",
                id: page.id.clone(),
                revision: head.as_ref().map(|h| h.0.clone()),
                reason: "wiki_page_deleted",
            });
        }
        if head.as_ref().map(|h| h.0.as_str()) != base_revision.as_deref() {
            return Ok(Apply::Conflict {
                kind: "wiki_page",
                id: page.id.clone(),
                revision: head.as_ref().map(|h| h.0.clone()),
                reason: "base_revision_changed",
            });
        }
        let existing_slug: Option<String> = self
            .connection
            .query_row(
                "SELECT slug FROM wiki_pages WHERE id=?1",
                params![page.id],
                |r| r.get(0),
            )
            .optional()?;
        if existing_slug.is_some() {
            ensure!(
                existing_slug.as_deref() == Some(page.slug.as_str()),
                "sync_wiki_slug_changed"
            );
        }
        if deleted {
            ensure!(existing_slug.is_some(), "sync_wiki_delete_missing");
            self.connection
                .execute("DELETE FROM wiki_pages WHERE id=?1", params![page.id])?;
            self.sync_set_head("wiki_page", &page.id, &op.op_id, true)?;
            return Ok(Apply::Applied);
        }
        // 同 slug 已被另一 id 持有：显式冲突（潜在重复页/同名新页），不重挂；
        // 旧页引用不得被默默指向新页（S05）。
        let slug_owner: Option<String> = self
            .connection
            .query_row(
                "SELECT id FROM wiki_pages WHERE slug=?1",
                params![page.slug],
                |r| r.get(0),
            )
            .optional()?;
        if slug_owner.is_some_and(|owner| owner != page.id) {
            return Ok(Apply::Conflict {
                kind: "wiki_page",
                id: page.id.clone(),
                revision: head.map(|h| h.0),
                reason: "slug_owned_by_other_id",
            });
        }
        // 证据闭包：引用的事件必须已在本地。未到先等待，不推断。
        for event_id in &page.source_event_ids {
            let known: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE id=?1)",
                params![event_id],
                |r| r.get(0),
            )?;
            if !known {
                return Ok(Apply::Waiting);
            }
        }
        if existing_slug.is_some() {
            // 内容列随记录整行替换：对端这版编辑就是新 head；证据
            // tags/source_event_ids 与 last_seen_at 是系统合并列：只并集、不回退。
            let (local_tags, local_seen, local_evidence): (String, String, String) =
                self.connection.query_row(
                    "SELECT tags,last_seen_at,source_event_ids FROM wiki_pages WHERE id=?1",
                    params![page.id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?;
            let mut tags_union = parse_tags(&local_tags)?;
            for tag in &page.tags {
                if !tags_union.contains(tag) {
                    tags_union.push(tag.clone());
                }
            }
            let mut evidence_union = parse_event_ids(&local_evidence)?;
            for ev in &page.source_event_ids {
                if !evidence_union.contains(ev) {
                    evidence_union.push(ev.clone());
                }
            }
            let last_seen = if local_seen > page.updated_at {
                local_seen
            } else {
                page.updated_at.clone()
            };
            self.connection.execute(
                "UPDATE wiki_pages SET slug=?2,kind=?3,title=?4,summary=?5,content_md=?6,tags=?7, \
                 source_event_ids=?8,status=?9,area=?10,based_on=?11,content_type=?12,source_url=?13, \
                 human_edited_at=?14,opinion=?15,last_seen_at=?16,updated_at=?17 WHERE id=?1",
                params![
                    page.id,
                    page.slug,
                    page.kind,
                    page.title,
                    page.summary,
                    page.content_md,
                    serde_json::to_string(&tags_union)?,
                    serde_json::to_string(&evidence_union)?,
                    page.status,
                    page.area,
                    page.based_on,
                    page.content_type,
                    page.source_url,
                    page.human_edited_at,
                    page.opinion,
                    last_seen,
                    page.updated_at,
                ],
            )?;
        } else {
            self.connection.execute(
                "INSERT INTO wiki_pages(id,slug,kind,title,summary,content_md,tags,source_event_ids, \
                 evidence_count,first_seen_at,last_seen_at,status,created_at,updated_at,source_url,area, \
                 based_on,content_type,human_edited_at,opinion) \
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
                params![
                    page.id,
                    page.slug,
                    page.kind,
                    page.title,
                    page.summary,
                    page.content_md,
                    serde_json::to_string(&page.tags)?,
                    serde_json::to_string(&page.source_event_ids)?,
                    page.source_event_ids.len().max(1) as i64,
                    page.created_at,
                    page.updated_at,
                    page.status,
                    page.created_at,
                    page.updated_at,
                    page.source_url,
                    page.area,
                    page.based_on,
                    page.content_type,
                    page.human_edited_at,
                    page.opinion,
                ],
            )?;
        }
        self.sync_set_head("wiki_page", &page.id, &op.op_id, false)?;
        Ok(Apply::Applied)
    }
}
