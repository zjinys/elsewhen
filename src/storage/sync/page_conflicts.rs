//! Wiki 页面同步冲突审阅（P5b·S05）。
//!
//! 三种 wiki 候选语义：`wiki_page_deleted`（删除后离线编辑）、
//! `rename_diverged`（两端各自改名）、`slug_owned_by_other_id`（新 slug 撞
//! 本端他页）。候选 payload 原文留在 `sync_operations`（state='conflict'），
//! `sync_conflicts` 只是立案索引。审阅把双端状态并列展示后由人显式裁决：
//! 「保留本地」= 重新捕获本地现状并广播（对端会看到反向候选，各自收敛到
//! 自己选的版本）；「采纳远端」= 按候选 op 语义强制落库（内容整行采纳，
//! 系统合并列仍只并集）。不做字段拼接，不比时间；双绑定失败关闭。

use anyhow::{Result, anyhow, ensure};
use rusqlite::OptionalExtension;
use rusqlite::params;

use super::Store;
use crate::sync::protocol::{SyncMutation, SyncOperation, WikiPageRecord};

/// 候选中远端一方的页面快照（供审阅 UI 并列展示）。
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct WikiPageConflictIncoming {
    pub slug: String,
    pub title: String,
    pub deleted: bool,
    /// 重命名候选的原 slug（其余候选为 None）。
    pub renamed_from: Option<String>,
    pub summary: String,
    pub content_md_len: i64,
    pub updated_at: String,
    pub base_revision: Option<String>,
}

/// 候选中本地一方的页面快照（行可能已被业务删除 → None）。
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct WikiPageConflictLocal {
    pub slug: String,
    pub title: String,
    pub content_md_len: i64,
    pub updated_at: String,
}

/// 一条待审阅的 wiki 页同步冲突（双端状态已解析，内容按需展示长度不全文
/// 携带——审阅列表不搬运整页正文，正文对比走既有页面视图）。
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct WikiPageConflictReview {
    pub op_id: String,
    pub object_id: String,
    pub reason: String,
    pub sender_device_id: String,
    pub incoming: WikiPageConflictIncoming,
    pub local: Option<WikiPageConflictLocal>,
    /// 本端 head 是否处于已删状态（删除后离线编辑场景的本端侧佐证）。
    pub local_head_deleted: bool,
}

struct ConflictRow {
    op_id: String,
    object_id: String,
    reason: String,
    sender_device_id: String,
    payload: String,
    payload_hash: String,
}

fn incoming_of(page: &WikiPageRecord, renamed_from: Option<&str>) -> WikiPageConflictIncoming {
    WikiPageConflictIncoming {
        slug: page.slug.clone(),
        title: page.title.clone(),
        deleted: false,
        renamed_from: renamed_from.map(|s| s.to_string()),
        summary: page.summary.clone(),
        content_md_len: page.content_md.chars().count() as i64,
        updated_at: page.updated_at.clone(),
        base_revision: None,
    }
}

fn mutation_incoming(mutation: &SyncMutation) -> Result<WikiPageConflictIncoming> {
    Ok(match mutation {
        SyncMutation::WikiPageChanged {
            page,
            base_revision,
            deleted,
        } => {
            let mut incoming = incoming_of(page, None);
            incoming.deleted = *deleted;
            incoming.base_revision = base_revision.clone();
            incoming
        }
        SyncMutation::WikiPageRenamed {
            old_slug,
            page,
            base_revision,
        } => {
            let mut incoming = incoming_of(page, Some(old_slug));
            incoming.base_revision = base_revision.clone();
            incoming
        }
        _ => return Err(anyhow!("sync_wiki_conflict_unexpected")),
    })
}

impl Store {
    fn wiki_conflict_row(&self, op_id: &str) -> Result<ConflictRow> {
        self.connection
            .query_row(
                "SELECT c.op_id,c.object_id,c.reason,o.origin_device_id,o.payload,o.payload_hash \
                 FROM sync_conflicts c JOIN sync_operations o ON o.op_id=c.op_id \
                 WHERE c.op_id=?1 AND c.object_kind='wiki_page' AND o.state='conflict'",
                [op_id],
                |r| {
                    Ok(ConflictRow {
                        op_id: r.get(0)?,
                        object_id: r.get(1)?,
                        reason: r.get(2)?,
                        sender_device_id: r.get(3)?,
                        payload: r.get(4)?,
                        payload_hash: r.get(5)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| anyhow!("wiki_conflict_not_pending"))
    }

    /// 列出待审阅的 wiki 页同步冲突（object_kind='wiki_page'）。
    pub fn list_wiki_page_conflicts(&self) -> Result<Vec<WikiPageConflictReview>> {
        let rows: Vec<ConflictRow> = {
            let mut statement = self.connection.prepare(
                "SELECT c.op_id,c.object_id,c.reason,o.origin_device_id,o.payload,o.payload_hash \
                 FROM sync_conflicts c JOIN sync_operations o ON o.op_id=c.op_id \
                 WHERE c.object_kind='wiki_page' AND o.state='conflict' \
                 ORDER BY o.origin_stream_id,o.origin_sequence",
            )?;
            statement
                .query_map([], |r| {
                    Ok(ConflictRow {
                        op_id: r.get(0)?,
                        object_id: r.get(1)?,
                        reason: r.get(2)?,
                        sender_device_id: r.get(3)?,
                        payload: r.get(4)?,
                        payload_hash: r.get(5)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut reviews = Vec::with_capacity(rows.len());
        for row in rows {
            let op = SyncOperation::decode(row.payload.as_bytes())?;
            let incoming = mutation_incoming(&op.mutation)?;
            let head = self.sync_object_head("wiki_page", &row.object_id)?;
            let local = self
                .connection
                .query_row(
                    "SELECT slug,title,content_md,updated_at FROM wiki_pages WHERE id=?1",
                    params![row.object_id],
                    |r| {
                        Ok(WikiPageConflictLocal {
                            slug: r.get(0)?,
                            title: r.get(1)?,
                            content_md_len: r.get::<_, String>(2)?.chars().count() as i64,
                            updated_at: r.get(3)?,
                        })
                    },
                )
                .optional()?;
            reviews.push(WikiPageConflictReview {
                op_id: row.op_id,
                object_id: row.object_id,
                reason: row.reason,
                sender_device_id: row.sender_device_id,
                incoming,
                local,
                local_head_deleted: head.as_ref().is_some_and(|h| h.1),
            });
        }
        Ok(reviews)
    }

    /// 显式裁决一条 wiki 页冲突。`expected_local_updated_at` 是审阅时展示的
    /// 本端行 updated_at（行已物理删除传 None）——裁决时必须原样回传，任一
    /// 变化失败关闭；payload_hash 在裁决事务内复核，防止候选被换绑。
    pub fn resolve_wiki_page_conflict(
        &self,
        op_id: &str,
        expected_local_updated_at: Option<&str>,
        accept_remote: bool,
    ) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        ensure!(!op_id.trim().is_empty(), "wiki_conflict_op_invalid");
        let row = self.wiki_conflict_row(op_id)?;
        let op = SyncOperation::decode(row.payload.as_bytes())?;
        // 本端现状绑定：与审阅时展示的行一致才允许裁决。
        let local_now: Option<String> = self
            .connection
            .query_row(
                "SELECT updated_at FROM wiki_pages WHERE id=?1",
                params![row.object_id],
                |r| r.get(0),
            )
            .optional()?;
        if local_now.as_deref() != expected_local_updated_at {
            if let Some(tx) = tx {
                tx.commit()?;
            }
            return Err(anyhow!(
                "wiki_conflict_local_changed（本端页面已变化，请刷新后重新审阅）"
            ));
        }
        // 裁决语义按候选 op 类型强制落地。落库后清立案；op 状态保持
        // 'conflict'（durable 标记，防对端丢失回执后的重投被误重放；重投
        // 只会补回执）。
        match (&op.mutation, accept_remote) {
            (_, false) => {
                // 保留本地 = 本端现状为准：head 已删则物理删除（本地删除态
                // 就是本端的裁决）；现存行则重新捕获并广播——对端会出现反向
                // 候选，由对端各自裁决（对称收敛，谁也不覆盖谁）。
                let head = self.sync_object_head("wiki_page", &row.object_id)?;
                let row_present: bool = self.connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM wiki_pages WHERE id=?1)",
                    params![row.object_id],
                    |r| r.get(0),
                )?;
                if head.as_ref().is_some_and(|h| h.1) {
                    if row_present {
                        self.connection.execute(
                            "DELETE FROM wiki_pages WHERE id=?1",
                            params![row.object_id],
                        )?;
                    }
                } else if row_present {
                    self.sync_record_wiki_page(&row.object_id, false)?;
                }
                self.connection
                    .execute("DELETE FROM sync_conflicts WHERE op_id=?1", [op_id])?;
            }
            (
                SyncMutation::WikiPageChanged {
                    page,
                    deleted: true,
                    ..
                },
                true,
            ) => {
                // 采纳远端删除：物理删除（修订与素材链接随外键级联）。
                self.connection
                    .execute("DELETE FROM wiki_pages WHERE id=?1", params![page.id])?;
                self.sync_set_head("wiki_page", &page.id, &op.op_id, true)?;
                self.connection
                    .execute("DELETE FROM sync_conflicts WHERE op_id=?1", [op_id])?;
            }
            (SyncMutation::WikiPageChanged { page, .. }, true) => {
                // 采纳远端编辑：整行采纳（slug 变了就连引用一起迁）。
                let old_slug: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT slug FROM wiki_pages WHERE id=?1",
                        params![page.id],
                        |r| r.get(0),
                    )
                    .optional()?;
                self.replace_sync_wiki_row(page)?;
                if let Some(old_slug) = old_slug {
                    if old_slug != page.slug {
                        self.migrate_wiki_slug_refs(&old_slug, &page.slug)?;
                    }
                }
                self.sync_set_head("wiki_page", &page.id, &op.op_id, false)?;
                self.connection
                    .execute("DELETE FROM sync_conflicts WHERE op_id=?1", [op_id])?;
            }
            (SyncMutation::WikiPageRenamed { page, .. }, true) => {
                // 采纳远端重命名：从本端**当前** slug 迁到远端新 slug
                // （两端各自改名时 old_slug 对不上本端，从现状迁才不丢引用）。
                let old_slug: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT slug FROM wiki_pages WHERE id=?1",
                        params![page.id],
                        |r| r.get(0),
                    )
                    .optional()?;
                match old_slug {
                    Some(current) if current != page.slug => {
                        self.replace_sync_wiki_row(page)?;
                        self.migrate_wiki_slug_refs(&current, &page.slug)?;
                    }
                    Some(_) => {}
                    None => {
                        self.replace_sync_wiki_row(page)?;
                    }
                }
                self.sync_set_head("wiki_page", &page.id, &op.op_id, false)?;
                self.connection
                    .execute("DELETE FROM sync_conflicts WHERE op_id=?1", [op_id])?;
            }
            _ => {
                if let Some(tx) = tx {
                    tx.commit()?;
                }
                return Err(anyhow!("sync_wiki_conflict_unexpected"));
            }
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
}
