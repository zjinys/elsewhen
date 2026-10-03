//! Saved task scope belongs to an input, not to whichever page is current later.
use super::{Store, WikiPage, knowledge::content_hash};
use crate::knowledge::authoring;
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PageBasis {
    pub slug: String,
    pub revision: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PageTask {
    pub kind: String,
    pub pages: Vec<PageBasis>,
    pub start_char: Option<usize>,
    pub end_char: Option<usize>,
}

impl PageTask {
    pub(crate) fn validate(&self, store: &Store) -> Result<Vec<WikiPage>> {
        ensure!(
            matches!(
                self.kind.as_str(),
                "local_question"
                    | "full_summary"
                    | "compare"
                    | "local_edit"
                    | "full_revision"
                    | "none"
            ),
            "未知页面任务范围"
        );
        if self.kind == "none" {
            ensure!(
                self.pages.is_empty() && self.start_char.is_none() && self.end_char.is_none(),
                "独立问题不能暗带旧页面材料"
            );
            return Ok(vec![]);
        }
        ensure!(
            (1..=4).contains(&self.pages.len()),
            "任务需要 1–4 份明确材料"
        );
        ensure!(
            self.kind == "compare" || self.pages.len() == 1,
            "只有跨来源对照可以选择多页"
        );
        ensure!(
            self.kind != "compare" || self.pages.len() >= 2,
            "跨来源对照至少选择两页"
        );
        let mut seen = std::collections::HashSet::new();
        let mut pages = Vec::new();
        for basis in &self.pages {
            ensure!(seen.insert(&basis.slug), "材料重复");
            let page = store
                .get_wiki_page(&basis.slug)?
                .context("任务材料已删除或改名")?;
            ensure!(page.status != "archived", "任务材料已归档");
            authoring::compare_revision_basis(store, &page, Some(&basis.revision))?;
            pages.push(page);
        }
        match (self.start_char, self.end_char) {
            (Some(start), Some(end)) => {
                ensure!(
                    matches!(self.kind.as_str(), "local_question" | "local_edit"),
                    "此任务不接受局部选区"
                );
                ensure!(
                    start < end && end <= pages[0].content_md.chars().count(),
                    "选区超出显示的页面范围"
                );
                ensure!(end - start <= 6000, "局部选区最多 6000 字，请缩小选区");
            }
            (None, None) => ensure!(self.kind != "local_edit", "局部编辑必须明确选择原文"),
            _ => anyhow::bail!("选区起止必须同时提供"),
        }
        Ok(pages)
    }
}

impl Store {
    pub(crate) fn save_page_task_input(
        &self,
        conversation: &str,
        text: &str,
        key: &str,
        task: &PageTask,
    ) -> Result<String> {
        ensure!(
            !text.trim().is_empty() && !key.is_empty(),
            "输入及幂等键不能为空"
        );
        let tx = self.begin_write_transaction()?;
        let old: Option<(String, String, String)> = self.connection.query_row(
            "SELECT t.input_message_id,t.task_json,m.content FROM chat_page_tasks t JOIN messages m ON m.id=t.input_message_id WHERE t.conversation_id=?1 AND t.idempotency_key=?2",
            params![conversation,key], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((id, raw, content)) = old {
            ensure!(
                serde_json::from_str::<PageTask>(&raw)? == *task && content == text,
                "幂等键已绑定其他输入或材料版本"
            );
            return Ok(id);
        }
        let conv = self.get_conversation(conversation)?.context("会话不存在")?;
        ensure!(!conv.archived, "会话已归档");
        let slug = conv.wiki_page_slug.context("页面任务只能用于知识页会话")?;
        ensure!(
            task.kind == "none" || task.pages.first().is_some_and(|p| p.slug == slug),
            "任务不属于当前知识页"
        );
        task.validate(self)?;
        let id = self.send_message(conversation, "user", text, None)?;
        self.connection.execute("INSERT INTO chat_page_tasks(input_message_id,conversation_id,idempotency_key,task_json,created_at) VALUES(?1,?2,?3,?4,?5)",
            params![id,conversation,key,serde_json::to_string(task)?,chrono::Utc::now().to_rfc3339()])?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(id)
    }

    /// The generation Store exposes its immutable input cutoff through list_messages.
    /// Never inherit a task merely because an unrelated later input is short.
    pub(crate) fn current_page_task(&self, conversation: &str) -> Result<Option<PageTask>> {
        let input = self
            .list_messages(conversation)?
            .into_iter()
            .rev()
            .find(|m| m.role == "user");
        let Some(input) = input else {
            return Ok(None);
        };
        let raw: Option<String> = self.connection.query_row("SELECT task_json FROM chat_page_tasks WHERE input_message_id=?1 AND conversation_id=?2",params![input.id,conversation],|r|r.get(0)).optional()?;
        if let Some(raw) = raw {
            return Ok(Some(serde_json::from_str(&raw)?));
        }
        Ok(self
            .captured_page_task
            .borrow()
            .as_ref()
            .filter(|(id, _)| id == &input.id)
            .map(|(_, task)| task.clone()))
    }

    /// Restore only the last input's persisted scope, including its old tokens.
    /// A later legacy input never inherits an earlier task or a freshly read basis.
    pub(crate) fn latest_page_task_json(&self, conversation: &str) -> Result<Option<String>> {
        let input = self
            .list_messages(conversation)?
            .into_iter()
            .rev()
            .find(|message| message.role == "user");
        let Some(input) = input else {
            return Ok(None);
        };
        Ok(self.connection.query_row(
            "SELECT task_json FROM chat_page_tasks WHERE input_message_id=?1 AND conversation_id=?2",
            params![input.id, conversation], |row| row.get(0),
        ).optional()?)
    }

    pub(crate) fn page_task_basis(&self, slug: &str) -> Result<(WikiPage, PageBasis)> {
        let page = self.get_wiki_page(slug)?.context("知识页不存在")?;
        let basis = PageBasis {
            slug: slug.into(),
            revision: authoring::revision_basis(self, &page)?,
        };
        Ok((page, basis))
    }
}

pub(crate) fn validate_proposal_task(store: &Store, args: &Value) -> Result<Vec<WikiPage>> {
    let Some(raw) = args.get("page_task").filter(|v| !v.is_null()) else {
        return Ok(vec![]);
    };
    let task: PageTask = serde_json::from_value(raw.clone())?;
    let pages = task.validate(store)?;
    ensure!(
        pages
            .first()
            .is_some_and(|page| Some(page.slug.as_str()) == args["slug"].as_str()),
        "提议目标不属于所选材料"
    );
    for (page, basis) in pages.iter().zip(&task.pages) {
        authoring::validate_revision_basis(store, page, Some(&basis.revision))?;
    }
    Ok(pages)
}

/// Called within the acceptance transaction, after every displayed basis is checked.
/// A comparison artifact depends on all selected materials, not only its UI parent.
pub(crate) fn bind_proposal_materials(
    store: &Store,
    artifact: &WikiPage,
    materials: &[WikiPage],
) -> Result<()> {
    if materials.is_empty() {
        return Ok(());
    }
    let mut snapshots = Vec::new();
    let mut events = artifact.source_event_ids.clone();
    for material in materials {
        snapshots.extend(
            store
                .page_source_snapshots(&material.slug)?
                .into_iter()
                .map(|s| s.id),
        );
        events.extend(material.source_event_ids.clone());
    }
    snapshots.sort();
    snapshots.dedup();
    events.sort();
    events.dedup();
    ensure!(
        snapshots.len() <= 8,
        "成果来源超出 8 份原料，请拆分对照任务"
    );
    let bases = crate::knowledge::dependencies::capture(
        store,
        &materials
            .iter()
            .map(|page| page.slug.clone())
            .collect::<Vec<_>>(),
    )?;
    store.bind_page_sources(&artifact.id, &snapshots)?;
    crate::knowledge::dependencies::bind(store, &artifact.id, &bases)?;
    store.connection.execute(
        "UPDATE wiki_pages SET source_event_ids=?1,evidence_count=?2 WHERE id=?3",
        params![
            serde_json::to_string(&events)?,
            events.len() as i64,
            artifact.id
        ],
    )?;
    Ok(())
}

pub(crate) fn task_identity(task: &PageTask) -> Result<String> {
    Ok(content_hash(&serde_json::to_string(task)?))
}
