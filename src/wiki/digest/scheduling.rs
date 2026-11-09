use super::*;

#[derive(Debug)]
pub(super) struct NeedsEventReading;
impl std::fmt::Display for NeedsEventReading {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("digest_single_event_requires_reading")
    }
}
impl std::error::Error for NeedsEventReading {}

pub(super) fn reading_tick(
    store: &Store,
    provider: &dyn AiProvider,
    model: Option<&str>,
) -> Result<Option<DigestTick>> {
    let Some(task) = store.next_event_reading()? else {
        return Ok(None);
    };
    store.set_digest_lane(true)?;
    let event_id = task.event_id.clone();
    let outcome = if task.status == "ready" {
        let Some(batch) = store.claim_event_reading_batch(&task, model)? else {
            return Ok(Some(DigestTick::Reading {
                event_id,
                state: "waiting".into(),
                error: None,
            }));
        };
        match finalize_event_reading(store, provider, &batch, &task.id) {
            Ok(Some(outcome)) => return Ok(Some(DigestTick::Processed { events: 1, outcome })),
            Ok(None) => {
                store.park_digest_for_reading(&batch)?;
                Ok("waiting")
            }
            Err(error) => {
                store.fail_digest_batch(&batch, &format!("{error:#}"))?;
                Err(error)
            }
        }
    } else {
        use crate::knowledge::event_reading::ReadingTick;
        match crate::knowledge::event_reading::advance(store, provider, &task.id) {
            Ok(ReadingTick::Advanced { .. } | ReadingTick::Ready | ReadingTick::LeavesComplete) => {
                Ok("advanced")
            }
            Ok(ReadingTick::Waiting) => Ok("waiting"),
            Ok(ReadingTick::Failed { error }) => Err(anyhow::anyhow!(error)),
            Err(error) => Err(error),
        }
    };
    let (state, error) = match outcome {
        Ok(state) => (state, None),
        Err(error) => ("failed", Some(format!("{error:#}"))),
    };
    store.record_event_reading_tick(&task.id, error.as_deref(), state == "waiting")?;
    Ok(Some(DigestTick::Reading {
        event_id,
        state: state.into(),
        error,
    }))
}
