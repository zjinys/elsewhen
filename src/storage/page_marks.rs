//! 翻篇标记存储（迁移 46；P2 手动 / P3 自动档）。
//!
//! 摘要本体在 chat_summary_nodes，标记只记「边界 + 引用 + 生效开关」。
//! 装配层据 active mark 跳过边界前原文；撤销只改开关不回滚任何事实
//! （原文在消息表、知识页在 wiki，均不受影响）。

use super::{chat_summaries::SummaryBody, MessageSummary, Store};
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};

#[derive(Debug, Clone, serde::Serialize)]
pub struct PageMark {
    pub id: String,
    pub conversation_id: String,
    pub boundary_message_id: String,
    pub summary_node_id: String,
    /// 用户预览编辑后的摘要（SummaryBody JSON）；None = 用节点原文渲染。
    pub override_json: Option<String>,
    pub kind: String,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

// FFI 文本传输面（frb 不走 pub(crate) 类型）。
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct PageMarkRow {
    pub id: String,
    pub conversation_id: String,
    pub boundary_message_id: String,
    pub summary_node_id: String,
    pub summary_preview: String,
    pub body_json: String,
    pub kind: String,
    pub active: bool,
    pub created_at: String,
}

const KINDS: [&str; 2] = ["manual", "auto"];
/// 翻篇校验：边界后必须留有原文（「不吞最新一轮」的可视化约束）。
const MIN_TAIL_MESSAGES: usize = 1;

fn row_to_mark(row: &rusqlite::Row) -> rusqlite::Result<PageMark> {
    Ok(PageMark {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        boundary_message_id: row.get(2)?,
        summary_node_id: row.get(3)?,
        override_json: row.get(4)?,
        kind: row.get(5)?,
        active: row.get::<_, i64>(6)? != 0,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

impl Store {
    pub(crate) fn list_page_marks(&self, conversation_id: &str) -> Result<Vec<PageMark>> {
        let mut statement = self.connection.prepare(
            "SELECT id,conversation_id,boundary_message_id,summary_node_id,override_json,kind,active,created_at,updated_at
             FROM conversation_page_marks WHERE conversation_id=?1 ORDER BY created_at DESC, id DESC",
        )?;
        let rows = statement.query_map([conversation_id], row_to_mark)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("读取翻篇标记")
    }

    /// 当前生效标记（最新 active 一条；理论至多一条，防御性取最新）。
    pub(crate) fn active_page_mark(&self, conversation_id: &str) -> Result<Option<PageMark>> {
        Ok(self
            .connection
            .query_row(
                "SELECT id,conversation_id,boundary_message_id,summary_node_id,override_json,kind,active,created_at,updated_at
                 FROM conversation_page_marks WHERE conversation_id=?1 AND active=1
                 ORDER BY created_at DESC, id DESC LIMIT 1",
                [conversation_id],
                row_to_mark,
            )
            .optional()
            .context("读取生效翻篇标记")?)
    }

    pub fn page_mark_by_id(&self, id: &str) -> Result<Option<PageMark>> {
        Ok(self
            .connection
            .query_row(
                "SELECT id,conversation_id,boundary_message_id,summary_node_id,override_json,kind,active,created_at,updated_at
                 FROM conversation_page_marks WHERE id=?1",
                [id],
                row_to_mark,
            )
            .optional()
            .context("读取翻篇标记")?)
    }

    /// 新建标记。调用方保证 summary_node 已落数据库且覆盖到边界。
    /// auto：同会话旧 auto 标记一律失活（每话题只留最新一份）；manual
    /// 不自动失活别的 manual——多话题多层翻篇留待 P4（多摘要链）处理。
    pub(crate) fn create_page_mark(
        &self,
        conversation_id: &str,
        boundary_message_id: &str,
        summary_node_id: &str,
        kind: &str,
    ) -> Result<String> {
        if !KINDS.contains(&kind) {
            bail!("未知翻篇类型：{kind}");
        }
        // 边界后必须留有原文。
        let messages = self.list_messages(conversation_id)?;
        let Some(position) = messages
            .iter()
            .position(|m| m.id == boundary_message_id)
        else {
            bail!("边界消息不在当前会话中（可能已被删除）");
        };
        ensure_page_boundary(&messages, position)?;
        let now = chrono::Utc::now().to_rfc3339();
        let transaction = self.begin_write_transaction()?;
        if kind == "auto" {
            self.connection.execute(
                "UPDATE conversation_page_marks SET active=0, updated_at=?2
                 WHERE conversation_id=?1 AND kind='auto' AND active=1",
                params![conversation_id, now],
            )?;
        }
        let id = uuid::Uuid::new_v4().to_string();
        match self
            .connection
            .execute(
                "INSERT INTO conversation_page_marks
                     (id,conversation_id,boundary_message_id,summary_node_id,override_json,kind,active,created_at,updated_at)
                 VALUES (?1,?2,?3,?4,NULL,?5,1,?6,?6)",
                params![id, conversation_id, boundary_message_id, summary_node_id, kind, now],
            ) {
            Ok(_) => {}
            Err(error) => {
                if error
                    .to_string()
                    .contains("UNIQUE constraint failed: conversation_page_marks.conversation_id, conversation_page_marks.boundary_message_id")
                {
                    bail!("该边界已存在翻篇标记；重复翻篇无新信息");
                }
                return Err(error.into());
            }
        }
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(id)
    }

    /// 预览编辑：覆盖摘要条目（SummaryBody JSON，仍过正文校验）。
    pub(crate) fn save_page_mark_override(&self, id: &str, body_json: &str) -> Result<()> {
        let body: SummaryBody = serde_json::from_str(body_json)
            .context("编辑后的摘要不是合法的结构化 JSON")?;
        body.validate().context("编辑后的摘要未通过校验")?;
        let updated = self.connection.execute(
            "UPDATE conversation_page_marks SET override_json=?2, updated_at=?3 WHERE id=?1",
            params![id, body_json, chrono::Utc::now().to_rfc3339()],
        )?;
        if updated == 0 {
            bail!("未找到翻篇标记（id={id}）");
        }
        Ok(())
    }

    /// 撤销：置 active=0。原文与 wiki 均不受影响。
    pub(crate) fn set_page_mark_active(&self, id: &str, active: bool) -> Result<()> {
        let updated = self.connection.execute(
            "UPDATE conversation_page_marks SET active=?2, updated_at=?3 WHERE id=?1",
            params![id, active as i64, chrono::Utc::now().to_rfc3339()],
        )?;
        if updated == 0 {
            bail!("未找到翻篇标记（id={id}）");
        }
        Ok(())
    }

    /// FFI 列表：带渲染好的摘要预览（override 优先）。
    pub(crate) fn list_page_marks_with_preview(&self, conversation_id: &str) -> Result<Vec<PageMarkRow>> {
        let marks = self.list_page_marks(conversation_id)?;
        marks
            .iter()
            .map(|mark| {
                let (summary_preview, body_json) = crate::ai::summary::render_mark_text(self, mark)?;
                Ok(PageMarkRow {
                    id: mark.id.clone(),
                    conversation_id: mark.conversation_id.clone(),
                    boundary_message_id: mark.boundary_message_id.clone(),
                    summary_node_id: mark.summary_node_id.clone(),
                    summary_preview,
                    body_json,
                    kind: mark.kind.clone(),
                    active: mark.active,
                    created_at: mark.created_at.clone(),
                })
            })
            .collect()
    }
}

/// 边界位置约束：翻篇点之后必须还有原文（不吞最新一轮）。
pub(crate) fn ensure_page_boundary(
    messages: &[MessageSummary],
    boundary_position: usize,
) -> Result<()> {
    let tail = messages.len() - boundary_position - 1;
    if tail < MIN_TAIL_MESSAGES {
        bail!(
            "翻篇点之后必须保留至少 {MIN_TAIL_MESSAGES} 条消息；不吞最新一轮是硬约束"
        );
    }
    Ok(())
}
