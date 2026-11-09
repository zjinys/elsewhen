use super::*;

/// Final model call is a separate tick after a verified complete reading tree.
/// On failure the immutable root remains reusable; publication and both task
/// acknowledgements either all commit, or all roll back.
pub(crate) fn finalize_event_reading(
    store: &Store,
    provider: &dyn AiProvider,
    batch: &crate::storage::DigestBatch,
    task_id: &str,
) -> Result<Option<crate::storage::DigestBatchOutcome>> {
    let task = store.event_reading_task(task_id)?;
    let _scope = store.use_prompt_bundle(&task.bundle_id);
    let Some(lease) =
        store.claim_event_reading_final(task_id, batch, chrono::Utc::now().timestamp())?
    else {
        return Ok(None);
    };
    let result = (|| {
        let root = store.event_reading_root(&task)?;
        let output: crate::storage::event_readings::ReadingOutput = serde_json::from_str(
            root.output
                .as_ref()
                .context("event_reading_root_incomplete")?,
        )?;
        let leaf_count = store
            .event_reading_nodes(task_id)?
            .iter()
            .filter(|n| n.kind == "leaf")
            .count();
        let input=serde_json::json!({"event_id":task.event_id,"covered_start":0,"covered_end":task.total_chars,"total_chars":task.total_chars,"verified_leaf_count":leaf_count,"root_node":root.id,"generated_summary":output.summary,"verified_original_quotes":output.quotes}).to_string();
        let material = crate::knowledge::event_reading::resources(store)?
            .render("event_reading_context", &[("input", input)])?;
        let (drafts, count) = digest_batch_drafts(
            store,
            provider,
            &batch.events,
            Some(&material),
            Some((&lease, batch)),
            &std::cell::Cell::new(0usize),
        )?;
        anyhow::ensure!(count == 1, "event_reading_final_batch_invalid");
        store.apply_digest_batch_with_reading(batch, &drafts, Some(&lease))
    })();
    match result {
        Ok(outcome) => Ok(Some(outcome)),
        Err(error) => {
            // Revocation/lease loss intentionally prevents even a cleanup write.
            let _ =
                store.release_event_reading_final(&lease, batch, chrono::Utc::now().timestamp());
            Err(error)
        }
    }
}
