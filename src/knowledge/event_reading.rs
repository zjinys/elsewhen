//! One bounded event-reading node per tick. The digest scheduler owns final
//! publication; completing leaves never acknowledges the original event.
use crate::ai::{
    memory::ContextMessage,
    provider::{AiProvider, RequestFit},
};
use crate::storage::prompts::render::TextResources;
use crate::storage::{
    Store,
    event_readings::{MAX_OUTPUT_BYTES, ReadingNode, ReadingOutput, ReadingTask},
};
use anyhow::{Context, Result, ensure};
use serde_json::json;

const MAX_LEAF_CHARS: usize = 6000;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReadingTick {
    Waiting,
    Advanced { start: usize, end: usize },
    LeavesComplete,
    Ready,
    Failed { error: String },
}

/// Older digest bundles have no event resources. Use an independently frozen
/// v1 pack in the bundle's original background locale, never today's settings.
pub(crate) fn resources(store: &Store) -> Result<TextResources> {
    let body = store.prompt_for_run("task.processing.fixed_contract")?;
    let parsed = TextResources::parse(&body)?;
    if parsed.literal("event_reading_contract").is_ok() {
        return Ok(parsed);
    }
    TextResources::parse(
        if store
            .run_language()?
            .background_instruction_locale
            .starts_with("en")
        {
            include_str!("../../resources/prompts/event-reading.v1.en.json")
        } else {
            include_str!("../../resources/prompts/event-reading.v1.zh-Hans.json")
        },
    )
}

fn leaf_messages(
    store: &Store,
    task: &ReadingTask,
    start: usize,
    end: usize,
    text: &str,
) -> Result<Vec<ContextMessage>> {
    let templates = resources(store)?;
    let system = format!(
        "{}\n{}\n{}\n{}",
        store.prompt_for_run("reading.policy")?,
        store.prompt_for_run("reading.fixed_contract")?,
        templates.literal("event_reading_contract")?,
        store.output_language_directive(false)?
    );
    let input = json!({"event_id":task.event_id,"start":start,"end":end,"total_chars":task.total_chars,"text":text}).to_string();
    Ok(vec![
        ContextMessage::new("system", system),
        ContextMessage::new(
            "user",
            templates.render("event_reading_leaf", &[("input", input)])?,
        ),
    ])
}

fn plan_leaf(
    store: &Store,
    provider: &dyn AiProvider,
    task: &ReadingTask,
    start: usize,
    chars: &[char],
) -> Result<ReadingNode> {
    let mut admitted: Option<usize> = None;
    let mut fixed_fit = false;
    let mut budget_available = false;
    let mut invalid_budget = false;
    // Choose a range that fits every usable candidate, before pinning it. A
    // channel unable to fit even one scalar is excluded by the send preflight.
    provider.visit_request_budgets(&mut |fits: &RequestFit<'_>| {
        budget_available = true;
        match fits(&leaf_messages(store, task, start, start, "")?) {
            Ok(true) => {}
            Ok(false) => return Ok(()),
            Err(_) => {
                invalid_budget = true;
                return Ok(());
            }
        }
        fixed_fit = true;
        let max = (chars.len() - start).min(MAX_LEAF_CHARS);
        let mut low = 0;
        let mut high = max;
        while low < high {
            let size = low + (high - low).div_ceil(2);
            let body: String = chars[start..start + size].iter().collect();
            if fits(&leaf_messages(store, task, start, start + size, &body)?)? {
                low = size;
            } else {
                high = size - 1;
            }
        }
        if low > 0 {
            admitted = Some(admitted.map_or(low, |n| n.min(low)));
        }
        Ok(())
    })?;
    ensure!(
        budget_available,
        "event_reading_provider_budget_unavailable"
    );
    if !fixed_fit && invalid_budget {
        anyhow::bail!("event_reading_candidate_budget_invalid");
    }
    ensure!(fixed_fit, "event_reading_fixed_instructions_exceed_budget");
    let size = admitted.context("event_reading_no_material_fits_budget")?;
    store.prepare_event_reading_node(&task.id, start, start + size, &[])
}

pub(crate) fn advance_leaves(
    store: &Store,
    provider: &dyn AiProvider,
    task_id: &str,
) -> Result<ReadingTick> {
    if !store.background_processing_allowed()? {
        return Ok(ReadingTick::Waiting);
    }
    let task = store.event_reading_task(task_id)?;
    let _scope = store.use_prompt_bundle(&task.bundle_id);
    let original = store.event_reading_text(&task)?;
    let chars: Vec<char> = original.chars().collect();
    let nodes = store.event_reading_nodes(task_id)?;
    if nodes.iter().all(|n| n.output.is_some()) {
        let start = nodes
            .iter()
            .filter(|n| n.kind == "leaf")
            .map(|n| n.end)
            .max()
            .unwrap_or(0);
        if start == chars.len() {
            return Ok(ReadingTick::LeavesComplete);
        }
        plan_leaf(store, provider, &task, start, &chars)?;
    }
    execute_node(store, provider, &task, &chars)
}

fn execute_node(
    store: &Store,
    provider: &dyn AiProvider,
    task: &ReadingTask,
    chars: &[char],
) -> Result<ReadingTick> {
    let task_id = &task.id;
    let now = chrono::Utc::now().timestamp();
    let Some(lease) = store.claim_event_reading_node(task_id, now)? else {
        return Ok(ReadingTick::Waiting);
    };
    let result = (|| -> Result<()> {
        let messages = if lease.node.kind == "leaf" {
            let body: String = chars[lease.node.start..lease.node.end].iter().collect();
            leaf_messages(store, task, lease.node.start, lease.node.end, &body)?
        } else {
            merge_messages(store, task, &store.event_reading_children(&lease.node)?)?
        };
        let reply = provider.generate_budgeted_prepared(&|fits| {
            if !fits(&messages)? {
                let fixed = if lease.node.kind == "leaf" {
                    leaf_messages(store, task, lease.node.start, lease.node.start, "")?
                } else {
                    merge_messages(store, task, &[])?
                };
                return Err(crate::ai::budget::RequestBudgetError(
                    if fits(&fixed)? {
                        "event_reading_pinned_range_exceeds_budget"
                    } else {
                        "event_reading_fixed_instructions_exceed_budget"
                    }
                    .into(),
                )
                .into());
            }
            store.renew_event_reading_lease(&lease, chrono::Utc::now().timestamp())?;
            Ok(messages.clone())
        })?;
        if let Some(usage) = &reply.usage {
            store.record_token_usage(
                None,
                usage.prompt_tokens as i64,
                usage.completion_tokens as i64,
                usage.total_tokens as i64,
                reply.model.as_deref(),
            )?;
        }
        let text = super::structured_text(&reply, "event reading")?;
        ensure!(
            matches!(
                reply.finish_reason.as_deref(),
                Some("stop" | "eos" | "end_turn")
            ),
            "event_reading_finish_reason_invalid"
        );
        ensure!(text.len() <= MAX_OUTPUT_BYTES, "event_reading_output_limit");
        let mut output: ReadingOutput =
            serde_json::from_str(text).context("event_reading_invalid_json")?;
        if lease.node.kind == "leaf" {
            locate_leaf_quotes(&mut output, &lease.node, chars)?;
        }
        store.complete_event_reading_node(&lease, &output, chrono::Utc::now().timestamp())
    })();
    match result {
        Ok(()) => Ok(ReadingTick::Advanced {
            start: lease.node.start,
            end: lease.node.end,
        }),
        Err(error) => {
            // Avoid retaining model output or source text in queue diagnostics.
            let code = format!("{error:#}").chars().take(300).collect::<String>();
            store.fail_event_reading_node(&lease, &code, chrono::Utc::now().timestamp())?;
            Ok(ReadingTick::Failed { error: code })
        }
    }
}

#[cfg(test)]
mod tests;

fn merge_messages(
    store: &Store,
    task: &ReadingTask,
    children: &[ReadingNode],
) -> Result<Vec<ContextMessage>> {
    let templates = resources(store)?;
    let system = format!(
        "{}\n{}\n{}\n{}",
        store.prompt_for_run("reading.policy")?,
        store.prompt_for_run("reading.fixed_contract")?,
        templates.literal("event_reading_contract")?,
        store.output_language_directive(false)?
    );
    let readings=children.iter().map(|n|Ok(json!({"node_id":n.id,"start":n.start,"end":n.end,"reading":serde_json::from_str::<ReadingOutput>(n.output.as_ref().context("event_reading_dependency_unfinished")?)?}))).collect::<Result<Vec<_>>>()?;
    let input =
        json!({"event_id":task.event_id,"total_chars":task.total_chars,"children":readings})
            .to_string();
    Ok(vec![
        ContextMessage::new("system", system),
        ContextMessage::new(
            "user",
            templates.render("event_reading_merge", &[("input", input)])?,
        ),
    ])
}

/// Full reader: no more than one leaf or merge call, and never final digest in
/// the same tick. Merge the lowest adjacent levels to keep depth bounded.
pub(crate) fn advance(
    store: &Store,
    provider: &dyn AiProvider,
    task_id: &str,
) -> Result<ReadingTick> {
    if !store.background_processing_allowed()? {
        return Ok(ReadingTick::Waiting);
    }
    let task = store.event_reading_task(task_id)?;
    let _scope = store.use_prompt_bundle(&task.bundle_id);
    if task.status == "ready" {
        store.event_reading_root(&task)?;
        return Ok(ReadingTick::Ready);
    }
    let leaf = advance_leaves(store, provider, task_id)?;
    if leaf != ReadingTick::LeavesComplete {
        return Ok(leaf);
    }
    let frontier = store.event_reading_frontier(&task)?;
    if frontier.len() == 1 {
        store.mark_event_reading_ready(task_id)?;
        return Ok(ReadingTick::Ready);
    }
    let index = frontier
        .windows(2)
        .enumerate()
        .min_by_key(|(_, pair)| pair[0].level.max(pair[1].level))
        .context("event_reading_root_incomplete")?
        .0;
    let level = frontier[index].level.max(frontier[index + 1].level);
    let group: Vec<_> = frontier[index..]
        .iter()
        .take(8)
        .take_while(|n| n.level <= level)
        .cloned()
        .collect();
    let mut count: Option<usize> = None;
    provider.visit_request_budgets(&mut |fits| {
        let mut best = 0;
        for size in 2..=group.len() {
            match fits(&merge_messages(store, &task, &group[..size])?) {
                Ok(true) => best = size,
                _ => break,
            }
        }
        if best >= 2 {
            count = Some(count.map_or(best, |n| n.min(best)));
        }
        Ok(())
    })?;
    let count = count.context("event_reading_merge_input_exceeds_budget")?;
    let children: Vec<_> = group[..count].iter().map(|n| n.id.clone()).collect();
    store.prepare_event_reading_node(task_id, group[0].start, group[count - 1].end, &children)?;
    let original = store.event_reading_text(&task)?;
    execute_node(
        store,
        provider,
        &task,
        &original.chars().collect::<Vec<_>>(),
    )
}

/// Models copy text more reliably than counting Unicode positions. A correct
/// supplied span is retained. Otherwise only an exact match inside the
/// input leaf may determine canonical source coordinates. For repeated text,
/// use the earliest exact occurrence as a reproducible source locator.
fn locate_leaf_quotes(
    output: &mut ReadingOutput,
    node: &ReadingNode,
    chars: &[char],
) -> Result<()> {
    ensure!(output.quotes.len() <= 8, "event_reading_output_limit");
    for quote in &mut output.quotes {
        let needle: Vec<char> = quote.text.chars().collect();
        ensure!(
            !needle.is_empty() && needle.len() <= 400,
            "event_reading_quote_range"
        );
        let valid = quote.start >= node.start
            && quote.end <= node.end
            && quote.end <= chars.len()
            && quote.start < quote.end
            && chars[quote.start..quote.end] == needle;
        if valid {
            continue;
        }
        let mut matches = chars[node.start..node.end]
            .windows(needle.len())
            .enumerate()
            .filter(|(_, text)| *text == needle.as_slice());
        let (position, _) = matches.next().context("event_reading_quote_forged")?;
        quote.start = node.start + position;
        quote.end = quote.start + needle.len();
    }
    Ok(())
}
