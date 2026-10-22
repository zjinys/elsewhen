//! 自 wiki.rs 机械分段平移（行为零变化）。

use crate::storage::{ContentPolicy, RelationDraft, Store, WikiPageDraft};
use anyhow::{Context, Result};
use serde::Deserialize;

use crate::wiki::{slugify, unique_slug};
// ── 联系人关系（AI 从对话识别「人 ↔ 事情/项目」，用户确认后落地） ───────────

/// 执行「联系人 + 关系」草拟（用户已确认）。
/// 输入 args：`{ "people": [{"name","role_note"}], "relations": [{"person","target","relation","note"}] }`
/// - 每个新联系人建 kind=person 页（slug `person/<名>`，标题查重，不重复建档）
/// - 每个关联目标按标题查重，不存在则建 kind=topic 页（slug `topic/<名>`）
/// - 再写入结构化关系（(from,to,relation) 唯一）
/// 返回执行摘要。
pub fn apply_people_relations(
    args: &serde_json::Value,
    store: &Store,
    conversation_id: &str,
) -> Result<String> {
    #[derive(Deserialize)]
    struct PersonDraft {
        name: String,
        #[serde(default)]
        role_note: String,
    }
    #[derive(Deserialize)]
    struct RelationDraftArg {
        #[serde(default)]
        person: String,
        #[serde(default)]
        target: String,
        #[serde(default)]
        relation: String,
        #[serde(default)]
        note: String,
        #[serde(default)]
        from_slug: Option<String>,
        #[serde(default)]
        to_slug: Option<String>,
    }
    #[derive(Deserialize)]
    struct PeopleRelationsArgs {
        #[serde(default)]
        people: Vec<PersonDraft>,
        #[serde(default)]
        relations: Vec<RelationDraftArg>,
        #[serde(default)]
        source_event_id: Option<String>,
    }

    let parsed: PeopleRelationsArgs = serde_json::from_value(args.clone())?;
    if parsed.people.is_empty() && parsed.relations.is_empty() {
        anyhow::bail!("没有需要保存的联系人或关系");
    }

    let tx = store.begin_write_transaction()?;
    for relation in &parsed.relations {
        for (name, selected) in [
            (&relation.person, &relation.from_slug),
            (&relation.target, &relation.to_slug),
        ] {
            if let Some(slug) = selected.as_deref() {
                let candidates = store.find_wiki_pages_by_title_or_alias(name.trim())?;
                anyhow::ensure!(
                    candidates.iter().any(|p| p.slug == slug),
                    "明确选择的目标 {slug} 已失效或不再对应 {name}，请重新消歧审阅"
                );
            }
        }
    }

    // 1) 联系人建档：标题精确查重 → 不存在才建 person 页
    let mut person_entries: Vec<(String, String, String)> = Vec::new(); // (name, slug, kind)
    for p in &parsed.people {
        let name = p.name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        if person_entries.iter().any(|(n, _, _)| *n == name) {
            continue;
        }
        let matches = store.find_wiki_pages_by_title_or_alias(&name)?;
        let selected_slug = parsed
            .relations
            .iter()
            .find(|relation| relation.person.trim() == name)
            .and_then(|relation| relation.from_slug.clone());
        if matches.len() > 1 && selected_slug.is_none() {
            anyhow::bail!(
                "联系人「{name}」存在多个同名页面（{}），请先在知识库中消歧后再确认",
                matches
                    .iter()
                    .map(|page| page.slug.as_str())
                    .collect::<Vec<_>>()
                    .join("、")
            );
        }
        let selected = match selected_slug.as_deref() {
            Some(slug) => Some(
                matches
                    .iter()
                    .find(|page| page.slug == slug)
                    .cloned()
                    .with_context(|| format!("选中的联系人 {slug} 已变化，请重新审阅"))?,
            ),
            None => matches.first().cloned(),
        };
        if let Some(page) = selected {
            person_entries.push((name, page.slug.clone(), page.kind.clone()));
            continue;
        }
        let slug = unique_slug(store, &format!("person/{}", slugify(&name)), &name)?;
        let note = p.role_note.trim().to_string();
        let summary = if note.is_empty() {
            format!("{name}（对话中出现的联系人）")
        } else {
            note.clone()
        };
        let content_md = if note.is_empty() {
            format!("{name} 是对话中出现的联系人。")
        } else {
            format!("# {name}\n\n{note}\n\n---\n由 AI 从对话中识别，用户确认后建档。")
        };
        let draft = WikiPageDraft {
            slug: slug.clone(),
            kind: "person".to_string(),
            title: name.clone(),
            summary,
            content_md,
            tags: vec![name.clone()],
            source_event_ids: vec![],
            status: "active".to_string(),
            reason: "AI 从对话识别联系人，用户确认".to_string(),
            source_url: None,
        };
        store.upsert_wiki_page(&draft, ContentPolicy::PreserveHumanEdits)?;
        person_entries.push((name, slug, "person".to_string()));
    }

    // 2) 关联目标：标题查重 → 不存在建 topic 页
    let mut target_entries: Vec<(String, String, String)> = Vec::new(); // (target, slug, kind)
    for r in &parsed.relations {
        let target = r.target.trim().to_string();
        if target.is_empty() || target_entries.iter().any(|(t, _, _)| *t == target) {
            continue;
        }
        let target_matches = store.find_wiki_pages_by_title_or_alias(&target)?;
        let selected_target_slug = parsed
            .relations
            .iter()
            .find(|relation| relation.target.trim() == target)
            .and_then(|relation| relation.to_slug.clone());
        if target_matches.len() > 1 && selected_target_slug.is_none() {
            anyhow::bail!(
                "事项「{target}」存在多个同名页面（{}），请先在知识库中消歧后再确认",
                target_matches
                    .iter()
                    .map(|page| page.slug.as_str())
                    .collect::<Vec<_>>()
                    .join("、")
            );
        }
        let selected = match selected_target_slug.as_deref() {
            Some(slug) => Some(
                target_matches
                    .iter()
                    .find(|page| page.slug == slug)
                    .cloned()
                    .with_context(|| format!("选中的事项 {slug} 已变化，请重新审阅"))?,
            ),
            None => target_matches.first().cloned(),
        };
        let (slug, kind) = match selected {
            Some(page) => (page.slug, page.kind),
            None => {
                let slug = unique_slug(store, &format!("topic/{}", slugify(&target)), &target)?;
                let draft = WikiPageDraft {
                    slug: slug.clone(),
                    kind: "topic".to_string(),
                    title: target.clone(),
                    summary: format!("{target}（由联系人关系确认时自动建档）"),
                    content_md: format!(
                        "# {target}\n\n（由联系人关系确认时自动创建，待补充内容。）"
                    ),
                    tags: vec![],
                    source_event_ids: vec![],
                    status: "active".to_string(),
                    reason: "AI 联系人关系确认时自动建档".to_string(),
                    source_url: None,
                };
                store.upsert_wiki_page(&draft, ContentPolicy::PreserveHumanEdits)?;
                (slug, "topic".to_string())
            }
        };
        target_entries.push((target, slug, kind));
    }

    // 3) 写关系
    let mut saved = 0usize;
    let mut relation_lines: Vec<String> = Vec::new();
    for r in &parsed.relations {
        let person = r.person.trim().to_string();
        let target = r.target.trim().to_string();
        let Some((_, inferred_from_slug, inferred_from_kind)) =
            person_entries.iter().find(|(n, _, _)| *n == person)
        else {
            continue;
        };
        let Some((_, inferred_to_slug, inferred_to_kind)) =
            target_entries.iter().find(|(t, _, _)| *t == target)
        else {
            continue;
        };
        let from_slug = r
            .from_slug
            .as_deref()
            .unwrap_or(inferred_from_slug)
            .to_string();
        let from_kind = if r.from_slug.is_some() {
            store
                .get_wiki_page(&from_slug)?
                .context("选中的联系人已失效")?
                .kind
        } else {
            inferred_from_kind.clone()
        };
        let to_slug = r.to_slug.as_deref().unwrap_or(inferred_to_slug).to_string();
        let to_kind = if r.to_slug.is_some() {
            store
                .get_wiki_page(&to_slug)?
                .context("选中的事项已失效")?
                .kind
        } else {
            inferred_to_kind.clone()
        };
        let relation = if r.relation.trim().is_empty() {
            "参与".to_string()
        } else {
            r.relation.trim().to_string()
        };
        let note = if r.note.trim().is_empty() {
            None
        } else {
            Some(r.note.trim().to_string())
        };
        store.upsert_relation(&RelationDraft {
            from_slug: from_slug.clone(),
            from_kind: from_kind.clone(),
            to_slug: to_slug.clone(),
            to_kind: to_kind.clone(),
            relation: relation.clone(),
            note,
            confidence: 3,
            source_conversation_id: Some(conversation_id.to_string()),
            source_event_id: parsed.source_event_id.clone(),
        })?;
        if let Some(event_id) = parsed.source_event_id.as_deref() {
            let occurred_at = store
                .event_analysis_detail(event_id)?
                .map(|detail| detail.recorded_at)
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
            let fact = format!("与{target}的关系：{relation}");
            store.upsert_entity_fact(&from_kind, &from_slug, &fact, &occurred_at, 3, event_id)?;
            let target_fact = format!("{person}：{relation}");
            store.upsert_entity_fact(
                &to_kind,
                &to_slug,
                &target_fact,
                &occurred_at,
                3,
                event_id,
            )?;
        }
        saved += 1;
        relation_lines.push(format!("{person} —— {relation} —— {target}"));
    }

    let mut out = String::new();
    if !person_entries.is_empty() {
        let names = person_entries
            .iter()
            .map(|(n, _, _)| n.clone())
            .collect::<Vec<_>>()
            .join("、");
        out.push_str(&format!("联系人 {}：{names}\n", person_entries.len()));
    }
    if saved > 0 {
        out.push_str(&format!(
            "关系 {} 条：\n{}",
            saved,
            relation_lines.join("\n")
        ));
    } else {
        out.push_str("（没有落地的关系）");
    }
    if let Some(tx) = tx {
        tx.commit()?;
    }
    Ok(out)
}
