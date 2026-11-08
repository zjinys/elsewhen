//! Prompt administration: drafts and activation are separate operations.
use crate::storage::{Store, prompts::catalog};
use anyhow::Result;
fn store() -> Result<Store> {
    let s = Store::open(&crate::config::AppConfig::load()?.database_path)?;
    s.register_prompts()?;
    Ok(s)
}
pub struct PromptDefinitionDto {
    pub key: String,
    pub title: String,
    pub editable: bool,
    pub locale: String,
    pub resolved_locale: String,
    pub current_revision_id: String,
    pub current_body: String,
    pub builtin_body: String,
    pub generation: i32,
    pub fallback: String,
    pub tokens: i32,
}
pub struct PromptRevisionDto {
    pub id: String,
    pub body: String,
    pub origin: String,
    pub note: String,
    pub created_at: String,
}
pub fn list_prompt_definitions(locale: String) -> Result<Vec<PromptDefinitionDto>> {
    let s = store()?;
    catalog()
        .into_iter()
        .map(|d| {
            let v = s.resolve_prompt(d.key, &locale)?;
            Ok(PromptDefinitionDto {
                key: d.key.into(),
                title: d.title.into(),
                editable: d.editable,
                locale: locale.clone(),
                resolved_locale: v.revision.locale,
                current_revision_id: v.revision.id,
                tokens: crate::ai::budget::text_tokens("unknown", &v.revision.body) as i32,
                current_body: v.revision.body,
                builtin_body: s.resolve_builtin_prompt(d.key, &locale)?.revision.body,
                generation: v.generation.try_into()?,
                fallback: v.fallback,
            })
        })
        .collect()
}
pub fn list_prompt_revisions(
    key: String,
    locale: String,
    offset: i32,
) -> Result<Vec<PromptRevisionDto>> {
    Ok(store()?
        .prompt_history(&key, &locale, offset.into())?
        .into_iter()
        .map(|r| PromptRevisionDto {
            id: r.id,
            body: r.body,
            origin: r.origin,
            note: r.note,
            created_at: r.created_at,
        })
        .collect())
}
pub fn save_prompt_revision(
    key: String,
    locale: String,
    body: String,
    parent_revision_id: Option<String>,
    note: String,
) -> Result<String> {
    store()?.save_prompt_revision(&key, &locale, &body, parent_revision_id.as_deref(), &note)
}
pub fn activate_prompt_revision(
    key: String,
    locale: String,
    revision_id: Option<String>,
    expected_generation: i32,
) -> Result<()> {
    store()?.activate_prompt(
        &key,
        &locale,
        revision_id.as_deref(),
        expected_generation.into(),
    )
}
pub struct PromptBundleDto {
    pub id: String,
    pub owner_kind: String,
    pub owner_id: String,
    pub created_at: String,
    pub ui_locale: String,
    pub instruction_locale: String,
    pub response_policy_json: String,
    pub knowledge_output_locale: String,
    pub language_policy_version: String,
    pub revision_ids: Vec<String>,
    pub keys: Vec<String>,
    pub bodies: Vec<String>,
    pub requested_locales: Vec<String>,
    pub resolved_locales: Vec<String>,
    pub fallback_reasons: Vec<String>,
}
pub fn list_prompt_bundles(offset: i32) -> Result<Vec<PromptBundleDto>> {
    let s = store()?;
    let mut q = s.connection.prepare("SELECT id,owner_kind,owner_id,created_at,ui_locale,requested_instruction_locale,response_policy_json,knowledge_output_locale,locale_policy_version FROM prompt_bundles ORDER BY rowid DESC LIMIT 20 OFFSET ?1")?;
    let rows = q
        .query_map([offset.max(0)], |r| {
            Ok(PromptBundleDto {
                id: r.get(0)?,
                owner_kind: r.get(1)?,
                owner_id: r.get(2)?,
                created_at: r.get(3)?,
                ui_locale: r.get(4)?,
                instruction_locale: r.get(5)?,
                response_policy_json: r.get(6)?,
                knowledge_output_locale: r.get(7)?,
                language_policy_version: r.get(8)?,
                revision_ids: Vec::new(),
                keys: Vec::new(),
                bodies: Vec::new(),
                requested_locales: Vec::new(),
                resolved_locales: Vec::new(),
                fallback_reasons: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|mut bundle| {
        let mut q = s.connection.prepare("SELECT e.prompt_key,r.id,r.body,e.requested_locale,e.resolved_locale,coalesce(e.fallback_reason,'') FROM prompt_bundle_entries e JOIN prompt_revisions r ON r.id=e.revision_id WHERE e.bundle_id=?1 ORDER BY e.ordinal")?;
        for entry in q.query_map([&bundle.id], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?)))? {
            let (key,id,body,requested,resolved,fallback) = entry?;
            bundle.keys.push(key); bundle.revision_ids.push(id); bundle.bodies.push(body);
            bundle.requested_locales.push(requested); bundle.resolved_locales.push(resolved);
            bundle.fallback_reasons.push(fallback);
        }
        Ok(bundle)
    }).collect()
}
