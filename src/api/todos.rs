//! 个人待办 FRB 门面。

use super::WikiPageDto;
use crate::storage::Store;
use anyhow::Result;

// ── 个人待办（todo） ──────────────────────────────────────────────────

/// 待办 DTO
#[derive(Clone, Debug)]
pub struct TodoDto {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub due_at: Option<String>,
    pub related_event_id: Option<String>,
    pub related_wiki_slug: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<crate::storage::Todo> for TodoDto {
    fn from(t: crate::storage::Todo) -> Self {
        Self {
            id: t.id,
            title: t.title,
            status: t.status.as_str().to_string(),
            priority: t.priority,
            due_at: t.due_at,
            related_event_id: t.related_event_id,
            related_wiki_slug: t.related_wiki_slug,
            note: t.note,
            created_at: t.created_at,
            updated_at: t.updated_at,
        }
    }
}

/// 列出待办（status 过滤：open/done/archived；None 时列出 open+done）
pub fn list_todos(status: Option<String>) -> Result<Vec<TodoDto>> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let todos = store.list_todos(status.as_deref())?;
    Ok(todos.into_iter().map(TodoDto::from).collect())
}

/// 新建待办（用户手动创建，直接生效）
pub fn create_todo(
    title: String,
    due_at: Option<String>,
    priority: Option<String>,
    related_wiki_slug: Option<String>,
    note: Option<String>,
) -> Result<TodoDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    if title.trim().is_empty() {
        anyhow::bail!("待办内容不能为空");
    }
    let t = store.create_todo(
        title.trim(),
        priority.as_deref().unwrap_or("normal"),
        due_at.as_deref(),
        None,
        related_wiki_slug.as_deref(),
        note.as_deref(),
    )?;
    Ok(TodoDto::from(t))
}

/// 更新待办状态（open/done/archived）
pub fn update_todo_status(id: String, status: String) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.update_todo_status(&id, crate::storage::TodoStatus::parse(&status))
}

/// 打开待办对应的可讨论工作项；无关联页时按需创建并回写关联。
pub fn open_todo_work_item(id: String) -> Result<WikiPageDto> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    let todo = store
        .get_todo(&id)?
        .ok_or_else(|| anyhow::anyhow!("未找到待办（id={id}）"))?;
    if let Some(slug) = todo.related_wiki_slug.as_deref() {
        return store
            .get_wiki_page(slug)?
            .map(WikiPageDto::from)
            .ok_or_else(|| anyhow::anyhow!("待办关联页面不存在（slug={slug}）"));
    }
    let marker = format!("work-item-id:{}", todo.id);
    if let Some(existing) = store.find_wiki_page_by_tag(&marker)? {
        store.set_todo_related_wiki_slug(&todo.id, &existing.slug)?;
        return Ok(WikiPageDto::from(existing));
    }
    let mut content = format!("# {}\n\n", todo.title);
    content.push_str("## 工作项状态\n\n");
    content.push_str(&format!("- 状态：{}\n", todo.status.as_str()));
    content.push_str(&format!("- 优先级：{}\n", todo.priority));
    if let Some(due_at) = todo.due_at.as_deref() {
        content.push_str(&format!("- 截止：{}\n", due_at));
    }
    if let Some(note) = todo.note.as_deref() {
        content.push_str(&format!("\n## 说明\n\n{}\n", note));
    }
    let page = crate::wiki::save_text_page(
        &content,
        Some(&todo.title),
        &["work-item".to_string(), marker],
        &store,
    )?;
    store.set_todo_related_wiki_slug(&todo.id, &page.slug)?;
    Ok(WikiPageDto::from(page))
}

/// 更新待办的可编辑字段（标题 / 补充 / 优先级 / 截止时间）。
/// 可选字段传 None 表示清除（如结束拖延、去掉截止时间）。
pub fn update_todo(
    id: String,
    title: String,
    note: Option<String>,
    priority: Option<String>,
    due_at: Option<String>,
) -> Result<()> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.update_todo(
        &id,
        &title,
        note.as_deref(),
        priority.as_deref(),
        due_at.as_deref(),
    )
}

/// 删除一条待办
pub fn delete_todo(id: String) -> Result<bool> {
    let config = crate::config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;
    store.delete_todo(&id)
}
