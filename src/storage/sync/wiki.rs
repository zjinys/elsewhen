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

    /// slug 足迹：只有真实出现过的 slug 才记录（不推断）；页面物理删除后
    /// 仍保留，供 wiki 专属会话的引用锚等待判定。
    pub(crate) fn sync_mark_wiki_slug_seen(&self, slug: &str) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO sync_wiki_slug_seen(slug) VALUES(?1)",
            [slug],
        )?;
        Ok(())
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
        self.sync_mark_wiki_slug_seen(&page.slug)?;
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

    /// 修订行追加的捕获点：与本地修订写放同一领域事务提交（record_wiki_
    /// revision 及其改名/upsert 调用方均已包tx）。修订不设 head——不参与
    /// CAS，只随流到达；收端应用见 apply_sync_wiki_revision。
    pub(crate) fn sync_record_wiki_revision(
        &self,
        revision: &crate::sync::protocol::WikiRevisionRecord,
    ) -> Result<()> {
        if self.sync_identity()?.is_none() {
            return Ok(());
        }
        self.append_sync_mutation(SyncMutation::WikiPageRevisionAppended {
            revision: revision.clone(),
        })?;
        Ok(())
    }

    /// 重命名前读取当前 head（作为 base_revision 的 CAS 锚点）。
    pub(crate) fn sync_wiki_base_revision(&self, id: &str) -> Result<Option<String>> {
        Ok(self.sync_object_head("wiki_page", id)?.map(|h| h.0))
    }

    /// rename_wiki_page 的捕获点：变更与本端迁移同事务提交。page 记录为
    /// 重命名后的整行（按 id 读取）；old_slug 是迁移前的 slug（换 slug 才
    /// 走这里，标题-only 修改走 sync_record_wiki_page）。
    pub(crate) fn sync_record_wiki_rename(
        &self,
        id: &str,
        old_slug: &str,
        base_revision: Option<String>,
    ) -> Result<()> {
        if self.sync_identity()?.is_none() {
            return Ok(());
        }
        let page = self
            .sync_wiki_page_record(id)?
            .context("sync_wiki_page_missing")?;
        self.sync_mark_wiki_slug_seen(&page.slug)?;
        if let Some(op_id) = self.append_sync_mutation(SyncMutation::WikiPageRenamed {
            old_slug: old_slug.to_string(),
            page,
            base_revision,
        })? {
            self.sync_set_head("wiki_page", id, &op_id, false)?;
        }
        Ok(())
    }

    /// 重命名（slug 迁移）的应用端：
    /// 1) head 已删 → 删除后到达的重命名保留为 wiki_page_deleted 候选；
    /// 2) base CAS 未知/不匹配 → Waiting / base_revision_changed；
    /// 3) 本端没有该 id 行 → Waiting（页本体 op 可能还在途）；
    /// 4) 本端行 slug == 新 slug → 幂等重放：只推进 head；
    /// 5) 本端行 slug != old_slug → 两端各自改名的显式冲突（不自动选边）；
    /// 6) 新 slug 已被别的 id 持有 → slug_owned_by_other_id；
    /// 7) 通过后：换 slug + 标题 + summary（base CAS 保证双端行内容一致），
    ///    并按本端 rename_wiki_page 同语义迁移关系与会话引用。
    pub(super) fn apply_sync_wiki_rename(
        &self,
        op: &SyncOperation,
        old_slug: &str,
        page: &crate::sync::protocol::WikiPageRecord,
        base_revision: &Option<String>,
    ) -> Result<Apply> {
        let head = self.sync_object_head("wiki_page", &page.id)?;
        // 重命名的判序以 slug 为准，先于 CAS：两端各自改名时 head 必然
        // 双双推进，base_revision_changed Waiting/blame 都会把真正的分歧
        // （old_slug 对不上本端行）掩盖掉。
        if head.as_ref().is_some_and(|h| h.1) {
            return Ok(Apply::Conflict {
                kind: "wiki_page",
                id: page.id.clone(),
                revision: head.as_ref().map(|h| h.0.clone()),
                reason: "wiki_page_deleted",
            });
        }
        let local_slug: Option<String> = self
            .connection
            .query_row(
                "SELECT slug FROM wiki_pages WHERE id=?1",
                params![page.id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(local_slug) = local_slug else {
            // 本端没有该页：页本体 op 可能在途，等齐再判。
            return Ok(Apply::Waiting);
        };
        if local_slug == page.slug {
            // 幂等重放（或本端已独立改到同名 slug，殊途同归）。
            self.sync_set_head("wiki_page", &page.id, &op.op_id, false)?;
            return Ok(Apply::Applied);
        }
        if local_slug != old_slug {
            return Ok(Apply::Conflict {
                kind: "wiki_page",
                id: page.id.clone(),
                revision: head.as_ref().map(|h| h.0.clone()),
                reason: "wiki_page_rename_diverged",
            });
        }
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
        if head.as_ref().map(|h| h.0.as_str()) != base_revision.as_deref() {
            return Ok(Apply::Conflict {
                kind: "wiki_page",
                id: page.id.clone(),
                revision: head.as_ref().map(|h| h.0.clone()),
                reason: "base_revision_changed",
            });
        }
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
                revision: head.as_ref().map(|h| h.0.clone()),
                reason: "slug_owned_by_other_id",
            });
        }
        self.connection.execute(
            "UPDATE wiki_pages SET slug=?1, title=?2, summary=?3, updated_at=?4 WHERE id=?5",
            params![
                page.slug,
                page.title,
                page.summary,
                page.updated_at,
                page.id
            ],
        )?;
        self.migrate_wiki_slug_refs(old_slug, &page.slug)?;
        self.sync_set_head("wiki_page", &page.id, &op.op_id, false)?;
        Ok(Apply::Applied)
    }

    /// 修订行应用（起源端权威历史）：页在站、证据在站才落；已删页头上到
    /// 达的修订与内容 op 同罪，落 wiki_page_deleted 候选。行只追加，重复
    /// 投递按主键幂等（receive 层已保证单 op 单应用，此处 OR IGNORE 兜底）。
    pub(super) fn apply_sync_wiki_revision(
        &self,
        op: &SyncOperation,
        revision: &crate::sync::protocol::WikiRevisionRecord,
    ) -> Result<Apply> {
        let head = self.sync_object_head("wiki_page", &revision.page_id)?;
        if head.as_ref().is_some_and(|h| h.1) {
            // 已删页头下的迟到修订：不立案（同批页 op 已报 wiki_page_deleted
            // 候选，重复立案只是噪音），也不落行（页待人工处置，历史不复活）。
            return Ok(Apply::Applied);
        }
        let page_exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM wiki_pages WHERE id=?1)",
            params![revision.page_id],
            |r| r.get(0),
        )?;
        if !page_exists {
            // 页本体 op 在途（写路径保证页 op 的流内序先于修订 op）。
            return Ok(Apply::Waiting);
        }
        if let Some(event_id) = &revision.source_event_id {
            let evidence: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE id=?1)",
                params![event_id],
                |r| r.get(0),
            )?;
            if !evidence {
                return Ok(Apply::Waiting);
            }
        }
        self.connection.execute(
            "INSERT OR IGNORE INTO wiki_revisions
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
        // 修订不推进 head：不是 CAS 对象；落行即可（否则删除头会被误清、
        // 后续页 op 的 base 判定会被打乱）。
        let _ = op;
        Ok(Apply::Applied)
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
            self.sync_mark_wiki_slug_seen(&page.slug)?;
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
        self.sync_mark_wiki_slug_seen(&page.slug)?;
        self.replace_sync_wiki_row(page)?;
        self.sync_set_head("wiki_page", &page.id, &op.op_id, false)?;
        Ok(Apply::Applied)
    }

    /// 页行的整行落库（内容列随记录替换；tags/source_event_ids 与
    /// last_seen_at 是系统合并列：只并集、不回退；行不存在时插入）。
    /// 冲突审阅的「采纳远端」复用同一语义（P5b·S05）。
    pub(super) fn replace_sync_wiki_row(&self, page: &WikiPageRecord) -> Result<()> {
        // 与写侧收口同一条不变式：落库正文里的推文配图统一为占位符形态。
        // 远端内容（旧端、或回填前的存量行）可能带着本机绝对路径，落地前折叠。
        let mut owned = page.clone();
        owned.content_md = crate::storage::wiki_images::fold_tweet_image_refs(&owned.content_md);
        let page = &owned;
        let existing: Option<(String, String, String)> = self
            .connection
            .query_row(
                "SELECT tags,last_seen_at,source_event_ids FROM wiki_pages WHERE id=?1",
                params![page.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((local_tags, local_seen, local_evidence)) = existing {
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
        Ok(())
    }

    /// slug 引用迁移（relations + 页内会话），本地 rename_wiki_page 与冲突
    /// 审阅的「采纳远端重命名」共用同一语义。
    pub(super) fn migrate_wiki_slug_refs(&self, old_slug: &str, new_slug: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE relations SET from_slug=?1 WHERE from_slug=?2",
            params![new_slug, old_slug],
        )?;
        self.connection.execute(
            "UPDATE relations SET to_slug=?1 WHERE to_slug=?2",
            params![new_slug, old_slug],
        )?;
        self.connection.execute(
            "UPDATE conversations SET wiki_page_slug=?1 WHERE wiki_page_slug=?2",
            params![new_slug, old_slug],
        )?;
        // S05：待办的页面引用与页面同迁——旧页引用不得默默指向同名新页。
        self.connection.execute(
            "UPDATE todos SET related_wiki_slug=?1 WHERE related_wiki_slug=?2",
            params![new_slug, old_slug],
        )?;
        Ok(())
    }
}
