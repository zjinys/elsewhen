//! One bounded semantic assessment per background tick, with strict citations.
use super::{
    memory::ContextMessage,
    provider::{
        AiProvider, AiReply, FailoverEntry, FailoverProvider, OpenAiCompatibleConfig,
        OpenAiCompatibleProvider,
    },
};
use crate::storage::{
    GoalCompassNudge, Store,
    compass::{CompassLease, CompassVerdict},
};
use anyhow::{Result, ensure};

fn provider(store: &Store) -> Result<Option<Box<dyn AiProvider>>> {
    let entries = store
        .list_ai_provider_configs_for_runtime()?
        .into_iter()
        .filter(|c| c.provider_type == "openai-compatible" && !c.api_key.trim().is_empty())
        .take(3)
        .map(|c| {
            let provider = OpenAiCompatibleProvider::new(OpenAiCompatibleConfig {
                base_url: c.base_url,
                api_key: c.api_key,
                model: c.model,
                temperature: 0.1,
                max_tokens: Some(c.max_tokens.filter(|t| *t > 0).unwrap_or(2048).min(4096) as u32),
                context_window: c.context_window.map(|t| t as u32),
            })?;
            Ok(FailoverEntry::new(c.id, c.name, Box::new(provider)))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(if entries.is_empty() {
        None
    } else {
        Some(Box::new(FailoverProvider::new(entries)))
    })
}

pub(crate) fn tick(store: &Store) -> Result<Option<GoalCompassNudge>> {
    // Ownership guards paid requests as well as durable writes.
    if !store.background_processing_allowed()? {
        return Ok(None);
    }
    if let Some(existing) = store.get_active_compass_nudge()? {
        return Ok(Some(existing));
    }
    let Some(provider) = provider(store)? else {
        return Ok(None);
    };
    tick_with_provider(store, provider.as_ref())
}

pub(crate) fn tick_with_provider(
    store: &Store,
    provider: &dyn AiProvider,
) -> Result<Option<GoalCompassNudge>> {
    let Some(lease) = store.claim_compass_assessment(chrono::Utc::now())? else {
        return store.get_active_compass_nudge();
    };
    let result = (|| -> Result<()> {
        let _scope = store.use_prompt_bundle(&lease.bundle_id);
        let reply = evaluate(store, provider, &lease)?;
        let usage=reply.usage.as_ref().map(|u|serde_json::json!({"prompt_tokens":u.prompt_tokens,"completion_tokens":u.completion_tokens,"total_tokens":u.total_tokens,"prompt_cached_tokens":u.prompt_cached_tokens}).to_string());
        // Account for a paid response even when its JSON or verdict is invalid.
        if let Some(tokens) = &reply.usage {
            store.record_token_usage(
                None,
                tokens.prompt_tokens as i64,
                tokens.completion_tokens as i64,
                tokens.total_tokens as i64,
                reply.model.as_deref(),
            )?;
        }
        store.record_compass_response_usage(&lease, reply.model.as_deref(), usage.as_deref())?;
        ensure!(
            reply.tool_calls.is_empty() && reply.finish_reason.as_deref() != Some("length"),
            "compass_reply_incomplete"
        );
        let verdict = parse(&reply.content)?;
        verdict.validate(&lease.input)?;
        store.complete_compass_assessment(
            &lease,
            &verdict,
            reply.model.as_deref(),
            usage.as_deref(),
            chrono::Utc::now().timestamp(),
        )
    })();
    if let Err(error) = result {
        // Error records contain stable diagnostics, never provider bodies or
        // raw user evidence. Other background tasks remain independent.
        let code = if error
            .chain()
            .any(|e| e.is::<crate::ai::budget::RequestBudgetError>())
        {
            "compass_request_over_budget"
        } else {
            "compass_evaluation_rejected"
        };
        store.fail_compass_assessment(&lease, code, chrono::Utc::now().timestamp())?;
        return Err(anyhow::anyhow!(code));
    }
    store.get_active_compass_nudge()
}

fn evaluate(store: &Store, provider: &dyn AiProvider, lease: &CompassLease) -> Result<AiReply> {
    let messages = vec![
        ContextMessage::new(
            "system",
            format!(
                "{}\n{}",
                store.prompt_for_run("compass.fixed_contract")?,
                store.output_language_directive(false)?
            ),
        ),
        ContextMessage::new("user", serde_json::to_string(&lease.input)?),
    ];
    provider.generate_budgeted_prepared(&|fits| {
        if !fits(&messages)? {
            return Err(crate::ai::budget::RequestBudgetError(
                "compass_request_over_budget".into(),
            )
            .into());
        }
        // The original snapshot is sent in full. We never shrink away a
        // counterexample to get a drifting verdict to fit a smaller provider.
        Ok(messages.clone())
    })
}

fn parse(text: &str) -> Result<CompassVerdict> {
    ensure!(text.len() <= 16_384, "compass_output_limit");
    let text = text.trim();
    let text = if let Some(inner) = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
    {
        inner.trim().strip_suffix("```").unwrap_or(inner).trim()
    } else {
        text
    };
    Ok(serde_json::from_str(text)?)
}

#[cfg(test)]
mod tests;
