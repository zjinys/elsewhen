//! Personal research reuses synchronized human-held memo revisions. Cache reset cannot erase it.
use crate::storage::{ContentPolicy, Store, WikiPageDraft};
use anyhow::{Result, ensure};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};

fn identity(instance: &str, product: &str) -> String {
    format!(
        "scout-research-{}",
        token(&format!("{instance}\0{product}"))
    )
}
pub(super) fn token(content: &str) -> String {
    use ring::digest::{SHA256, digest};
    digest(&SHA256, content.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub(super) fn decode(content: &str) -> Result<Value> {
    let body = content
        .split("```json\n")
        .nth(1)
        .and_then(|s| s.split("\n```").next())
        .ok_or_else(|| anyhow::anyhow!("research_document_changed"))?;
    Ok(serde_json::from_str(body)?)
}
impl Store {
    pub fn scout_research_list(&self) -> Result<Value> {
        let mut query=self.connection.prepare("SELECT slug,content_md FROM wiki_pages WHERE slug LIKE 'scout-research-%' AND status!='archived' ORDER BY updated_at DESC LIMIT 1000")?;
        let mut items = Vec::new();
        for row in query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (slug, content) = row?;
            if let Ok(mut value) = decode(&content) {
                value["slug"] = json!(slug);
                value["token"] = json!(token(&content));
                items.push(value);
            }
        }
        Ok(json!(items))
    }
    pub fn save_scout_research(&self, mut value: Value, expected: Option<&str>) -> Result<Value> {
        let instance = value["instance_id"].as_str().unwrap_or("").to_owned();
        let product = value["product_id"].as_str().unwrap_or("").to_owned();
        ensure!(
            !instance.is_empty() && !product.is_empty(),
            "research_identity_required"
        );
        ensure!(
            matches!(
                value["state"].as_str(),
                Some("research" | "validating" | "watching" | "paused" | "rejected")
            ),
            "research_invalid_state"
        );
        if let Some(themes) = value.get("themes") {
            let themes = themes
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("research_invalid_themes"))?;
            let mut names = std::collections::HashSet::new();
            ensure!(
                themes.len() <= 10
                    && themes
                        .iter()
                        .all(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()
                            && s.chars().count() <= 180
                            && names.insert(s))),
                "research_invalid_themes"
            );
        }
        if let Some(evidence) = value.get("materials").and_then(Value::as_array) {
            ensure!(evidence.len() <= 20, "research_evidence_limit");
            let mut ids = std::collections::HashSet::new();
            for e in evidence {
                let id = e["id"].as_str().unwrap_or("");
                let body = e["body"].as_str().unwrap_or("");
                ensure!(
                    !id.is_empty() && ids.insert(id) && (8..=8000).contains(&body.chars().count()),
                    "research_invalid_evidence"
                );
                let url = reqwest::Url::parse(e["url"].as_str().unwrap_or(""))?;
                ensure!(
                    matches!(url.scheme(), "https" | "http")
                        && url.username().is_empty()
                        && url.password().is_none(),
                    "research_invalid_source"
                );
            }
        }
        if let Some(selected) = value
            .get("analysis_selected_basis")
            .and_then(Value::as_str)
            .map(str::to_owned)
        {
            let language = self
                .prompt_language_preferences()?
                .instruction_locale
                .unwrap_or_else(|| "zh-Hans".into());
            let prompt = self
                .resolve_prompt("scout.fixed_contract", &language)?
                .revision
                .body;
            ensure!(
                super::analysis::basis(self, &value, &prompt)? == selected,
                "scout_analysis_basis_changed"
            );
            let candidate:Option<String>=self.connection.query_row("SELECT result_json FROM scout_analysis_jobs WHERE study_slug=?1 AND basis_token=?2 AND status='succeeded'",rusqlite::params![identity(&instance,&product),selected],|r|r.get(0)).optional()?;
            ensure!(candidate.is_some(), "scout_analysis_candidate_unavailable");
            value["accepted_analysis"] = serde_json::from_str(candidate.as_deref().unwrap())?;
            value["accepted_analysis_evidence"] = json!(super::analysis::evidence(&value));
            value["analysis_basis"] = json!(selected);
        }
        value
            .as_object_mut()
            .unwrap()
            .remove("analysis_selected_basis");
        let title = value["title"]
            .as_str()
            .unwrap_or("Research")
            .chars()
            .take(180)
            .collect::<String>()
            .replace(['\n', '\r'], " ");
        self.save_scout_document(
            &identity(&instance, &product),
            &title,
            value,
            expected,
            "scout-research",
        )
    }

    pub fn scout_research_drafts(&self) -> Result<Value> {
        let mut query = self.connection.prepare("SELECT document_json FROM scout_research_drafts ORDER BY updated_at DESC,study_key LIMIT 1000")?;
        let items = query
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|row| Ok(serde_json::from_str::<Value>(&row?)?))
            .collect::<Result<Vec<_>>>()?;
        Ok(json!(items))
    }
    pub fn scout_research_draft(&self, instance: &str, product: &str) -> Result<Value> {
        let body: Option<String> = self
            .connection
            .query_row(
                "SELECT document_json FROM scout_research_drafts WHERE study_key=?1",
                [identity(instance, product)],
                |r| r.get(0),
            )
            .optional()?;
        Ok(body
            .map(|body| serde_json::from_str(&body))
            .transpose()?
            .unwrap_or(Value::Null))
    }
    pub fn save_scout_research_draft(&self, value: Value) -> Result<()> {
        let instance = value["instance_id"].as_str().unwrap_or("");
        let product = value["product_id"].as_str().unwrap_or("");
        ensure!(
            !instance.is_empty()
                && !product.is_empty()
                && instance.len() <= 1000
                && product.len() <= 1000,
            "research_identity_required"
        );
        let body = serde_json::to_string(&value)?;
        ensure!(body.len() <= 100000, "research_too_large");
        self.connection.execute("INSERT INTO scout_research_drafts(study_key,document_json) VALUES(?1,?2) ON CONFLICT(study_key) DO UPDATE SET document_json=excluded.document_json,updated_at=datetime('now')",rusqlite::params![identity(instance,product),body])?;
        Ok(())
    }
    pub fn delete_scout_research_draft(&self, instance: &str, product: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM scout_research_drafts WHERE study_key=?1",
            [identity(instance, product)],
        )?;
        Ok(())
    }

    /// Preferences are explicitly authored, synchronized and independent of any Scout server.
    pub fn scout_preferences(&self) -> Result<Value> {
        let Some(page) = self.get_wiki_page("scout-personal-preferences")? else {
            return Ok(json!({}));
        };
        let mut value = decode(&page.content_md)?;
        value["token"] = json!(token(&page.content_md));
        Ok(value)
    }
    pub fn save_scout_preferences(&self, value: Value, expected: Option<&str>) -> Result<Value> {
        let mut document = json!({"schema_version":1});
        for key in [
            "audience",
            "interests",
            "capabilities",
            "constraints",
            "reachable_buyers",
        ] {
            let text = value.get(key).and_then(Value::as_str).unwrap_or("").trim();
            ensure!(text.chars().count() <= 2000, "scout_preferences_too_large");
            document[key] = json!(text);
        }
        self.save_scout_document(
            "scout-personal-preferences",
            "Scout personal preferences",
            document,
            expected,
            "scout-preferences",
        )
    }

    fn save_scout_document(
        &self,
        slug: &str,
        title: &str,
        mut value: Value,
        expected: Option<&str>,
        tag: &str,
    ) -> Result<Value> {
        if let Some(object) = value.as_object_mut() {
            object.remove("token");
            object.remove("slug");
        }
        let body = serde_json::to_string_pretty(&value)?;
        ensure!(body.len() <= 60000, "research_too_large");
        let tx = self.begin_write_transaction()?;
        let existing = self.get_wiki_page(slug)?;
        if let Some(page) = &existing {
            ensure!(
                expected == Some(token(&page.content_md).as_str()),
                "research_changed_reload"
            );
        } else {
            ensure!(expected.is_none(), "research_changed_reload");
        }
        let content = format!("# {title}\n\n```json\n{body}\n```\n");
        if let Some(page) = existing {
            self.save_wiki_page_content(
                slug,
                &content,
                "Scout: explicit user edit",
                Some(&page.updated_at),
            )?;
        } else {
            self.upsert_wiki_page_in_tx(
                &WikiPageDraft {
                    slug: slug.into(),
                    kind: "memo".into(),
                    title: title.into(),
                    summary: title.into(),
                    content_md: content.clone(),
                    tags: vec![tag.into()],
                    source_event_ids: vec![],
                    status: "active".into(),
                    reason: "Scout: explicit user decision".into(),
                    source_url: None,
                },
                ContentPolicy::Always,
            )?;
            let page = self.get_wiki_page(slug)?.unwrap();
            self.save_wiki_page_content(
                slug,
                &content,
                "Scout: human-held",
                Some(&page.updated_at),
            )?;
        }
        if tag == "scout-research" {
            self.connection.execute(
                "DELETE FROM scout_research_drafts WHERE study_key=?1",
                [slug],
            )?;
        }
        if let Some(tx) = tx {
            tx.commit()?;
        }
        value["slug"] = json!(slug);
        value["token"] = json!(token(&self.get_wiki_page(slug)?.unwrap().content_md));
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn research_survives_cache_clear_and_rejects_lost_updates() -> Result<()> {
        let path = std::env::temp_dir().join(format!("scout-research-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path)?;
        let draft = json!({"instance_id":"one","product_id":"a","title":"A","state":"research","question":"Who pays?","evidence":{"observed_at":"2026-11-17","price":29}});
        let saved = store.save_scout_research(draft.clone(), None)?;
        assert!(store.save_scout_research(draft.clone(), None).is_err());
        store.clear_scout_mirror()?;
        let list = store.scout_research_list()?;
        assert_eq!(list.as_array().unwrap().len(), 1);
        let mut edited = draft;
        edited["state"] = json!("rejected");
        store.save_scout_research(edited, saved["token"].as_str())?;
        assert_eq!(store.scout_research_list()?[0]["state"], "rejected");
        assert!(
            store
                .save_scout_research(saved.clone(), saved["token"].as_str())
                .is_err()
        );
        Ok(())
    }
    #[test]
    fn explicit_preferences_can_be_withdrawn_without_erasing_research() -> Result<()> {
        let path =
            std::env::temp_dir().join(format!("scout-preferences-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path)?;
        assert_eq!(store.scout_preferences()?, json!({}));
        let prefs = store.save_scout_preferences(
            json!({"audience":"Designers","constraints":"2 hours/week","secret":"ignored"}),
            None,
        )?;
        assert!(prefs.get("secret").is_none());
        assert!(store.save_scout_preferences(json!({}), None).is_err());
        let research = json!({"instance_id":"one","product_id":"a","title":"A","state":"research","themes":["Designers deliver work", "Freelancers send invoices"],"fit":"I can reach designers","preferences_basis":prefs});
        store.save_scout_research(research, None)?;
        let cleared = store.save_scout_preferences(json!({}), prefs["token"].as_str())?;
        assert_eq!(cleared["audience"], "");
        store.clear_scout_mirror()?;
        assert_eq!(store.scout_preferences()?["constraints"], "");
        let records = store.scout_research_list()?;
        assert_eq!(records.as_array().unwrap().len(), 1);
        assert_eq!(records[0]["preferences_basis"]["audience"], "Designers");
        assert_eq!(records[0]["themes"].as_array().unwrap().len(), 2);
        let mut invalid = records[0].clone();
        invalid["themes"] = json!(["duplicate", "duplicate"]);
        assert!(
            store
                .save_scout_research(invalid, records[0]["token"].as_str())
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn local_drafts_survive_reopen_and_failed_save() -> Result<()> {
        let path = std::env::temp_dir().join(format!("scout-drafts-{}.db", uuid::Uuid::new_v4()));
        let draft = json!({"instance_id":"fixture","product_id":"p","title":"P","state":"research","question":"Unfinished question"});
        {
            let store = Store::open(&path)?;
            store.save_scout_research_draft(draft.clone())?;
        }
        let store = Store::open(&path)?;
        store.clear_scout_mirror()?;
        assert_eq!(
            store.scout_research_draft("fixture", "p")?["question"],
            "Unfinished question"
        );
        assert_eq!(store.scout_research_drafts()?.as_array().unwrap().len(), 1);
        assert!(
            store
                .save_scout_research(draft.clone(), Some("stale"))
                .is_err()
        );
        assert!(!store.scout_research_draft("fixture", "p")?.is_null());
        let saved = store.save_scout_research(draft.clone(), None)?;
        assert!(store.scout_research_draft("fixture", "p")?.is_null());
        let mut edited = draft;
        edited["token"] = saved["token"].clone();
        store.save_scout_research_draft(edited)?;
        store.delete_scout_research_draft("fixture", "p")?;
        assert_eq!(store.scout_research_drafts()?, json!([]));
        Ok(())
    }

    #[test]
    #[ignore = "opt-in synthetic scale baseline, no personal database"]
    fn research_scale_thousand_records() -> Result<()> {
        let path = std::env::temp_dir().join(format!("scout-scale-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path)?;
        {
            let tx = store.begin_write_transaction()?;
            for n in 0..1000 {
                let content = format!(
                    "# Product {n}\n\n```json\n{}\n```",
                    json!({"instance_id":"scale","product_id":n.to_string(),"title":format!("Product {n}"),"state":"research","materials":[{"id":"e1","url":"https://example.test","body":"evidence ".repeat(200)}]})
                );
                store.connection.execute("INSERT INTO wiki_pages(id,slug,kind,title,summary,content_md,tags,source_event_ids,evidence_count,first_seen_at,last_seen_at,status,created_at,updated_at,area) VALUES(?1,?1,'memo','Scale','',?2,'[]','[]',0,'t','t','active','t','t','insight')",rusqlite::params![format!("scout-research-{n}"),content])?;
            }
            if let Some(tx) = tx {
                tx.commit()?;
            }
        }
        let mut durations = Vec::new();
        for n in 0..21 {
            let start = std::time::Instant::now();
            assert_eq!(store.scout_research_list()?.as_array().unwrap().len(), 1000);
            if n > 0 {
                durations.push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        durations.sort_by(f64::total_cmp);
        eprintln!(
            "SCOUT_RESEARCH_SCALE records=1000 runs=20 p50_ms={:.2} p95_ms={:.2}",
            durations[9], durations[18]
        );
        assert!(durations[18] <= 500.0);
        drop(store);
        let _ = std::fs::remove_file(path);
        Ok(())
    }
}
