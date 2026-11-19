//! Optional, bounded and version-bound analysis; never writes the user's conclusions.
use super::research::{decode, token};
use crate::storage::Store;
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

pub(super) fn evidence(study: &Value) -> Vec<Value> {
    let mut remaining = 12000usize;
    study["materials"]
        .as_array()
        .into_iter()
        .flatten()
        .take(20)
        .filter_map(|source| {
            if remaining < 8 {
                return None;
            }
            let mut source = source.clone();
            let body = source["body"].as_str().unwrap_or("");
            let count = body.chars().count();
            let excerpt = body.chars().take(remaining.min(4000)).collect::<String>();
            remaining = remaining.saturating_sub(excerpt.chars().count());
            source["body"] = json!(excerpt);
            let truncated = count > source["body"].as_str().unwrap().chars().count();
            source["truncated"] = json!(truncated);
            Some(source)
        })
        .collect()
}
pub(super) fn basis(store: &Store, study: &Value, prompt: &str) -> Result<String> {
    let providers = store
        .list_ai_provider_configs()?
        .iter()
        .map(|p| {
            json!({
                "id":p.id,"model":p.model,"type":p.provider_type,"url":p.base_url,
                "active":p.is_active,"temperature":p.temperature,
            })
        })
        .collect::<Vec<_>>();
    Ok(token(
        &json!({"strategy":"scout-evidence-v2","providers":providers,"materials":study["materials"],"question":study["question"],"prompt":prompt})
            .to_string(),
    ))
}
pub(crate) fn validate(result: &Value, sources: &[Value]) -> Result<()> {
    ensure!(
        result["claims"].is_array()
            && result["hypotheses"].is_array()
            && result["coverage"].is_string(),
        "scout_analysis_invalid_shape"
    );
    ensure!(
        result["claims"].as_array().unwrap().len() <= 6
            && result["hypotheses"].as_array().unwrap().len() <= 3,
        "scout_analysis_limit"
    );
    let has = |id: &str| sources.iter().any(|s| s["id"] == id);
    for claim in result["claims"].as_array().unwrap() {
        let id = claim["evidence_id"].as_str().unwrap_or("");
        let quote = claim["quote"].as_str().unwrap_or("");
        let source = sources
            .iter()
            .find(|s| s["id"] == id)
            .ok_or_else(|| anyhow::anyhow!("scout_analysis_unknown_evidence"))?;
        ensure!(
            (8..=400).contains(&quote.chars().count())
                && source["body"].as_str().unwrap_or("").contains(quote),
            "scout_analysis_fabricated_quote"
        );
        ensure!(
            claim["text"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.chars().count() <= 1000),
            "scout_analysis_invalid_claim"
        );
        // Reject new numeric assertions even if the model attached a real but
        // unrelated quotation. Market numbers must be visible in that quote.
        let digits = |text: &str| {
            text.split(|c: char| !c.is_ascii_digit())
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        let quoted = digits(quote);
        ensure!(
            digits(claim["text"].as_str().unwrap())
                .iter()
                .all(|n| quoted.contains(n)),
            "scout_analysis_unsupported_number"
        );
    }
    for hypothesis in result["hypotheses"].as_array().unwrap() {
        let support = hypothesis["support"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("scout_analysis_no_support"))?;
        ensure!(
            !support.is_empty() && support.iter().all(|id| id.as_str().is_some_and(has)),
            "scout_analysis_no_support"
        );
        ensure!(
            hypothesis["text"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.chars().count() <= 2000),
            "scout_analysis_invalid_hypothesis"
        );
        ensure!(
            hypothesis["unknowns"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
                && hypothesis["experiment"].is_string(),
            "scout_analysis_missing_unknowns"
        );
        ensure!(
            hypothesis["counter"]
                .as_array()
                .is_some_and(|v| v.iter().all(|id| id.as_str().is_some_and(has))),
            "scout_analysis_unknown_counter"
        );
    }
    Ok(())
}
pub(crate) fn tick(store: &Store) -> Result<bool> {
    let language = store
        .prompt_language_preferences()?
        .instruction_locale
        .unwrap_or_else(|| "zh-Hans".into());
    let prompt = store
        .resolve_prompt("scout.fixed_contract", &language)?
        .revision
        .body;
    let Some(provider) = crate::ai::provider::active_first_provider_chain(store)? else {
        return Ok(false);
    };
    tick_with(store, provider.as_ref(), &prompt, &language)
}
fn tick_with(
    store: &Store,
    provider: &dyn crate::ai::provider::AiProvider,
    prompt: &str,
    language: &str,
) -> Result<bool> {
    let studies = store.scout_research_list()?;
    for study in studies.as_array().unwrap() {
        if study["analysis_enabled"] != true
            || evidence(study).is_empty()
            || matches!(study["state"].as_str(), Some("paused" | "rejected"))
        {
            continue;
        }
        let slug = study["slug"].as_str().unwrap();
        let key = basis(store, study, prompt)?;
        let owner = uuid::Uuid::new_v4().to_string();
        let id = token(&format!("{slug}:{key}"));
        let claimed=store.connection.execute("INSERT INTO scout_analysis_jobs(id,study_slug,basis_token,status,owner,lease_until) VALUES(?1,?2,?3,'running',?4,unixepoch()+300)
    ON CONFLICT(id) DO UPDATE SET status='running',owner=excluded.owner,lease_until=excluded.lease_until,attempts=attempts+1,updated_at=datetime('now')
    WHERE (scout_analysis_jobs.status='cancelled' OR (scout_analysis_jobs.status IN ('running','failed') AND scout_analysis_jobs.lease_until<unixepoch())) AND attempts<3",params![id,slug,key,owner])?;
        if claimed == 0 {
            continue;
        }
        let sources = evidence(study);
        let outcome = (|| -> Result<(Value, Option<String>, Option<crate::ai::provider::TokenUsage>)> {
            let mut checked_at = std::time::Instant::now();
            let reply = provider.generate_observed(vec![
                crate::ai::memory::ContextMessage::new("system", prompt),
                crate::ai::memory::ContextMessage::new("user", json!({
                    "question":study["question"],"evidence":sources,"language":language,
                    "coverage":"Only user-supplied or imported excerpts; not a market-wide sample.",
                }).to_string()),
            ], None, &mut |notification| {
                if matches!(notification, crate::ai::provider::ProviderEvent::Poll)
                    && checked_at.elapsed().as_secs() >= 2 {
                    checked_at = std::time::Instant::now();
                    let current = store.get_wiki_page(slug)?.and_then(|p|decode(&p.content_md).ok());
                    ensure!(current.as_ref().is_some_and(|s|s["analysis_enabled"]==true
                        && !matches!(s["state"].as_str(),Some("paused"|"rejected"))
                        && basis(store,s,prompt).is_ok_and(|current|current==key)), "scout_analysis_cancelled");
                    ensure!(store.connection.execute(
                        "UPDATE scout_analysis_jobs SET lease_until=unixepoch()+300 WHERE id=?1 AND owner=?2 AND status='running' AND lease_until>unixepoch()",
                        params![id,owner])?==1,"scout_analysis_lease_lost");
                }
                Ok(())
            })?;
            ensure!(reply.tool_calls.is_empty(), "scout_analysis_tools_forbidden");
            let result: Value = serde_json::from_str(reply.content.trim().trim_start_matches("```json").trim_end_matches("```").trim())?;
            validate(&result, &sources)?;
            Ok((result, reply.model, reply.usage))
        })();
        let current = store
            .get_wiki_page(slug)?
            .and_then(|p| decode(&p.content_md).ok());
        if !current.as_ref().is_some_and(|s| {
            s["analysis_enabled"] == true
                && !matches!(s["state"].as_str(), Some("paused" | "rejected"))
                && basis(store, s, prompt).is_ok_and(|current| current == key)
        }) {
            store.connection.execute("UPDATE scout_analysis_jobs SET status='cancelled',updated_at=datetime('now') WHERE id=?1 AND owner=?2",params![id,owner])?;
            return Ok(true);
        }
        match outcome {
            Ok((result, model, usage)) => {
                store.connection.execute("UPDATE scout_analysis_jobs SET status='succeeded',result_json=?3,model=?4,prompt_tokens=?5,completion_tokens=?6,updated_at=datetime('now') WHERE id=?1 AND owner=?2 AND lease_until>unixepoch()",params![id,owner,result.to_string(),model,usage.as_ref().map(|u|u.prompt_tokens as i64),usage.as_ref().map(|u|u.completion_tokens as i64)])?;
            }
            Err(e) => {
                store.connection.execute("UPDATE scout_analysis_jobs SET status='failed',error=?3,lease_until=unixepoch()+1800,updated_at=datetime('now') WHERE id=?1 AND owner=?2",params![id,owner,e.to_string()])?;
            }
        }
        return Ok(true);
    }
    Ok(false)
}
pub(crate) fn result(store: &Store, slug: &str) -> Result<Value> {
    let row:Option<(String,String,Option<String>,Option<String>)>=store.connection.query_row("SELECT basis_token,status,result_json,error FROM scout_analysis_jobs WHERE study_slug=?1 ORDER BY updated_at DESC,rowid DESC LIMIT 1",[slug],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    Ok(match row {
        None => json!({"status":"waiting"}),
        Some((key, status, body, error)) => {
            let current = store
                .get_wiki_page(slug)?
                .and_then(|p| decode(&p.content_md).ok());
            let language = store
                .prompt_language_preferences()?
                .instruction_locale
                .unwrap_or_else(|| "zh-Hans".into());
            let prompt = store
                .resolve_prompt("scout.fixed_contract", &language)?
                .revision
                .body;
            let stale = !current
                .as_ref()
                .is_some_and(|s| basis(store, s, &prompt).is_ok_and(|current| current == key));
            json!({"status":status,"stale":stale,"basis":key,"result":body.map(|b|serde_json::from_str::<Value>(&b)).transpose()?,"error":error})
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invented_quotes_and_missing_support_are_rejected() {
        let e = vec![json!({"id":"e1","body":"I would pay for team collaboration."})];
        let valid = json!({"claims":[{"text":"A reviewer mentions collaboration","evidence_id":"e1","quote":"pay for team collaboration"}],"hypotheses":[],"coverage":"One reviewer only"});
        assert!(validate(&valid, &e).is_ok());
        let mut bad = valid.clone();
        bad["claims"][0]["text"] = json!("There are 500 paying customers");
        assert!(validate(&bad, &e).is_err());
        let mut bad = valid.clone();
        bad["claims"][0]["quote"] = json!("This has thousands of paying customers");
        assert!(validate(&bad, &e).is_err());
        let mut bad = valid;
        bad["hypotheses"] =
            json!([{"support":["fake"],"unknowns":["Who pays?"],"experiment":"Interview"}]);
        assert!(validate(&bad, &e).is_err());
    }
    #[test]
    fn candidate_generation_is_deduplicated_and_manual_edits_remain_separate() -> Result<()> {
        use crate::ai::{
            memory::ContextMessage,
            provider::{AiProvider, AiReply},
            tool::ToolSpec,
        };
        struct Fake(std::sync::atomic::AtomicUsize);
        impl AiProvider for Fake {
            fn generate_reply_with_tools(
                &self,
                _: Vec<ContextMessage>,
                tools: Option<&[ToolSpec]>,
            ) -> Result<AiReply> {
                ensure!(tools.is_none(), "tools forbidden");
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(AiReply::text(
                    r#"{"claims":[{"text":"One reviewer mentions collaboration","evidence_id":"e1","quote":"pay for team collaboration"}],"hypotheses":[{"text":"Explore a team workflow","support":["e1"],"counter":[],"unknowns":["Who would pay?"],"experiment":"Interview one buyer"}],"coverage":"One user statement only"}"#,
                ))
            }
        }
        let path = std::env::temp_dir().join(format!("scout-analysis-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path)?;
        store.register_prompts()?;
        let document = json!({"instance_id":"fixture","product_id":"p","title":"P","state":"research","question":"Who pays?","analysis_enabled":true,"reason":"My original decision","materials":[{"id":"e1","url":"https://example.test/review","body":"I would pay for team collaboration."}]});
        let saved = store.save_scout_research(document, None)?;
        let prompt = store
            .resolve_prompt("scout.fixed_contract", "zh-Hans")?
            .revision
            .body;
        let fake = Fake(std::sync::atomic::AtomicUsize::new(0));
        assert!(tick_with(&store, &fake, &prompt, "zh-Hans")?);
        assert!(!tick_with(&store, &fake, &prompt, "zh-Hans")?);
        assert_eq!(fake.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        let slug = saved["slug"].as_str().unwrap();
        assert_eq!(result(&store, slug)?["status"], "succeeded");
        assert_eq!(
            store.scout_research_list()?[0]["reason"],
            "My original decision"
        );
        let mut accepted = saved.clone();
        accepted["analysis_selected_basis"] = result(&store, slug)?["basis"].clone();
        let accepted = store.save_scout_research(accepted, saved["token"].as_str())?;
        assert_eq!(
            accepted["accepted_analysis"]["claims"][0]["evidence_id"],
            "e1"
        );
        assert_eq!(
            accepted["accepted_analysis_evidence"][0]["body"],
            "I would pay for team collaboration."
        );
        let mut changed = accepted.clone();
        changed["question"] = json!("A new question");
        store.save_scout_research(changed, accepted["token"].as_str())?;
        assert_eq!(result(&store, slug)?["stale"], true);
        Ok(())
    }
}
