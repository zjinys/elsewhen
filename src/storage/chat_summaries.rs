//! Derived conversation summaries verify live SQL rows, never frozen snapshots.
use super::{MessageSummary, Store, knowledge::content_hash};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct HistorySpan {
    pub message_id: String,
    pub start_char: usize,
    pub end_char: usize,
    pub message_basis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SummaryBody {
    pub goals: Vec<String>,
    pub constraints: Vec<String>,
    pub facts: Vec<String>,
    pub decisions: Vec<String>,
    pub unfinished: Vec<String>,
}

impl SummaryBody {
    pub(crate) fn validate(&self) -> Result<()> {
        let groups = [
            &self.goals,
            &self.constraints,
            &self.facts,
            &self.decisions,
            &self.unfinished,
        ];
        ensure!(
            groups.iter().all(|items| items.len() <= 12),
            "摘要条目数量超限"
        );
        ensure!(
            groups
                .iter()
                .flat_map(|items| items.iter())
                .all(|item| !item.trim().is_empty() && item.chars().count() <= 512),
            "摘要条目为空或过长"
        );
        let size: usize = groups
            .iter()
            .flat_map(|items| items.iter())
            .map(|item| item.chars().count())
            .sum();
        ensure!(size > 0 && size <= 4096, "摘要为空或超过结构化结果上限");
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SummaryNode {
    pub id: String,
    pub conversation_id: String,
    pub basis_hash: String,
    pub spans: Vec<HistorySpan>,
    pub children: Vec<String>,
    pub strategy: String,
    pub provider_basis: String,
    pub model: Option<String>,
    pub body: SummaryBody,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct HistoryExcerpt {
    pub message_id: String,
    pub role: String,
    pub created_at: String,
    pub start_char: usize,
    pub end_char: usize,
    pub total_chars: usize,
    pub has_more: bool,
    pub text: String,
}

pub(crate) fn message_basis(message: &MessageSummary) -> String {
    content_hash(
        &serde_json::json!([
            message.id,
            message.conversation_id,
            message.parent_message_id,
            message.role,
            message.content,
            message.created_at
        ])
        .to_string(),
    )
}

/// Legacy null-parent history is linear; explicit edges must follow this path.
pub(crate) fn validate_history_path(messages: &[MessageSummary]) -> Result<()> {
    let mut seen = HashSet::new();
    for (index, message) in messages.iter().enumerate() {
        ensure!(seen.insert(message.id.as_str()), "摘要历史包含重复消息或环");
        if let Some(parent) = &message.parent_message_id {
            ensure!(
                index > 0 && messages[index - 1].id == *parent,
                "摘要历史存在分支或不明确父链，未合并不同路径"
            );
        }
        ensure!(
            index == 0 || message.conversation_id == messages[0].conversation_id,
            "摘要历史包含其他会话"
        );
    }
    Ok(())
}

fn live_message(store: &Store, conversation: &str, id: &str) -> Result<MessageSummary> {
    store.connection.query_row(
        "SELECT id,conversation_id,parent_message_id,role,content,created_at,NULL,kind,ref FROM messages WHERE id=?1 AND conversation_id=?2",
        params![id, conversation], |row| Ok(MessageSummary {
            id: row.get(0)?, conversation_id: row.get(1)?, parent_message_id: row.get(2)?,
            role: row.get(3)?, content: row.get(4)?, created_at: row.get(5)?,
            input_message_id: row.get(6)?, kind: row.get(7)?, message_ref: row.get(8)?,
        }),
    ).optional()?.context("摘要原始消息不存在或不属于此会话")
}

impl Store {
    pub(crate) fn validate_history_spans(
        &self,
        conversation: &str,
        spans: &[HistorySpan],
    ) -> Result<()> {
        ensure!(
            !spans.is_empty() && spans.len() <= 8192,
            "摘要覆盖范围为空或超限"
        );
        let mut seen = HashSet::new();
        let mut previous: Option<(&HistorySpan, usize)> = None;
        for span in spans {
            let message = live_message(self, conversation, &span.message_id)?;
            ensure!(
                message_basis(&message) == span.message_basis,
                "摘要原始消息版本已变化"
            );
            let chars = message.content.chars().count();
            ensure!(
                span.start_char <= span.end_char && span.end_char <= chars,
                "摘要原文范围无效"
            );
            if let Some((prev, previous_length)) = previous {
                if prev.message_id == span.message_id {
                    ensure!(prev.end_char == span.start_char, "摘要覆盖存在重叠或空洞");
                } else {
                    ensure!(
                        prev.end_char == previous_length && span.start_char == 0,
                        "摘要跨消息覆盖不完整"
                    );
                    ensure!(
                        seen.insert(span.message_id.as_str()),
                        "摘要覆盖重复或消息路径乱序"
                    );
                }
            } else {
                seen.insert(span.message_id.as_str());
            }
            previous = Some((span, chars));
        }
        Ok(())
    }

    pub(crate) fn summary_node(
        &self,
        conversation: &str,
        basis: &str,
        strategy: &str,
        provider_basis: &str,
    ) -> Result<Option<SummaryNode>> {
        let id: Option<String> = self.connection.query_row(
            "SELECT id FROM chat_summary_nodes WHERE conversation_id=?1 AND basis_hash=?2 AND strategy=?3 AND provider_basis=?4",
            params![conversation, basis, strategy, provider_basis], |row| row.get(0),
        ).optional()?;
        id.map(|id| self.summary_node_by_id(conversation, &id))
            .transpose()
            .map(Option::flatten)
    }

    pub(crate) fn summary_node_by_id(
        &self,
        conversation: &str,
        id: &str,
    ) -> Result<Option<SummaryNode>> {
        let record = self.connection.query_row(
            "SELECT id,conversation_id,basis_hash,spans_json,children_json,strategy,provider_basis,model,body_json,prompt_tokens,completion_tokens,elapsed_ms FROM chat_summary_nodes WHERE id=?1 AND conversation_id=?2",
            params![id, conversation], |r| Ok((
                r.get::<_,String>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?,
                r.get::<_,String>(3)?, r.get::<_,String>(4)?, r.get::<_,String>(5)?,
                r.get::<_,String>(6)?, r.get::<_,Option<String>>(7)?, r.get::<_,String>(8)?,
                r.get::<_,Option<i64>>(9)?, r.get::<_,Option<i64>>(10)?, r.get::<_,i64>(11)?,
            )),
        ).optional()?;
        let Some((
            id,
            conversation_id,
            basis_hash,
            spans,
            children,
            strategy,
            provider_basis,
            model,
            body,
            prompt_tokens,
            completion_tokens,
            elapsed_ms,
        )) = record
        else {
            return Ok(None);
        };
        let node = SummaryNode {
            id,
            conversation_id,
            basis_hash,
            spans: serde_json::from_str(&spans)?,
            children: serde_json::from_str(&children)?,
            strategy,
            provider_basis,
            model,
            body: serde_json::from_str(&body)?,
            prompt_tokens: prompt_tokens.map(u64::try_from).transpose()?,
            completion_tokens: completion_tokens.map(u64::try_from).transpose()?,
            elapsed_ms: u64::try_from(elapsed_ms)?,
        };
        node.body.validate()?;
        self.validate_history_spans(conversation, &node.spans)?;
        Ok(Some(node))
    }

    pub(crate) fn commit_summary_node(&self, node: &SummaryNode) -> Result<()> {
        let tx = self.begin_write_transaction()?;
        crate::ai::generation::check(self)?;
        node.body.validate()?;
        self.validate_history_spans(&node.conversation_id, &node.spans)?;
        ensure!(node.children.len() <= 4, "摘要归并子节点数量超限");
        if !node.children.is_empty() {
            let mut coverage = Vec::new();
            for child in &node.children {
                let child = self
                    .summary_node_by_id(&node.conversation_id, child)?
                    .context("摘要子节点不存在")?;
                ensure!(
                    child.strategy == node.strategy && child.provider_basis == node.provider_basis,
                    "摘要子节点策略或模型配置不一致"
                );
                coverage.extend(child.spans);
            }
            ensure!(coverage == node.spans, "摘要归并覆盖范围与子节点不一致");
        }
        self.connection.execute(
            "INSERT OR IGNORE INTO chat_summary_nodes(id,conversation_id,basis_hash,spans_json,children_json,strategy,provider_basis,model,body_json,prompt_tokens,completion_tokens,elapsed_ms,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![node.id,node.conversation_id,node.basis_hash,serde_json::to_string(&node.spans)?,serde_json::to_string(&node.children)?,node.strategy,node.provider_basis,node.model,serde_json::to_string(&node.body)?,node.prompt_tokens.map(i64::try_from).transpose()?,node.completion_tokens.map(i64::try_from).transpose()?,i64::try_from(node.elapsed_ms)?,chrono::Utc::now().to_rfc3339()],
        )?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// Read only within the frozen run input; never silently substitute new text.
    pub(crate) fn read_conversation_history_excerpt(
        &self,
        conversation: &str,
        message: &str,
        start: usize,
        max_chars: usize,
    ) -> Result<HistoryExcerpt> {
        let history = self.list_messages(conversation)?;
        let frozen = history
            .iter()
            .find(|item| item.id == message)
            .context("消息不在本次会话的输入范围内")?;
        let current = live_message(self, conversation, message)?;
        ensure!(
            message_basis(frozen) == message_basis(&current),
            "原始消息版本已变化，请重新生成"
        );
        let total_chars = current.content.chars().count();
        ensure!(start <= total_chars, "原文起点超出消息范围");
        let text: String = current
            .content
            .chars()
            .skip(start)
            .take(max_chars.clamp(1, 2400))
            .collect();
        let end_char = start + text.chars().count();
        Ok(HistoryExcerpt {
            message_id: current.id,
            role: current.role,
            created_at: current.created_at,
            start_char: start,
            end_char,
            total_chars,
            has_more: end_char < total_chars,
            text,
        })
    }

    pub(crate) fn summary_history_spans(
        &self,
        conversation: &str,
        node: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<HistorySpan>> {
        let node = self
            .summary_node_by_id(conversation, node)?
            .context("摘要不存在或不属于此会话")?;
        let allowed: HashSet<_> = self
            .list_messages(conversation)?
            .into_iter()
            .map(|message| message.id)
            .collect();
        ensure!(
            node.spans
                .iter()
                .all(|span| allowed.contains(&span.message_id)),
            "摘要超出本次运行的输入范围"
        );
        Ok(node
            .spans
            .into_iter()
            .skip(offset)
            .take(limit.clamp(1, 32))
            .collect())
    }
}

#[cfg(test)]
mod tests;
