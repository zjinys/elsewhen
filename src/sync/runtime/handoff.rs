//! Normal processor handoff (P4d). Every step is explicit and fail-closed:
//! the old processor yields with a signed quiescent proof, the roster owner
//! approves it into a successor roster, and the takeover target confirms only
//! after the local frontier reaches the handoff boundary. This module only
//! loads the unlocked vault identity; all invariants live in the store.

use crate::config::AppConfig;
use crate::storage::Store;
use crate::sync::crypto::DeviceIdentity;
use crate::sync::membership::ProcessingYieldProof;
use anyhow::{Context, Result, ensure};

/// Load the confirmed session's vault and run `f` with the store and the
/// local signing identity. The vault identity never leaves this scope.
fn with_unlocked_vault<T>(
    config: &AppConfig,
    f: impl FnOnce(&Store, &DeviceIdentity) -> Result<T>,
) -> Result<T> {
    let store = Store::open(&config.database_path)?;
    store.require_local_sync_membership()?;
    let (master, revision, installation) = {
        let all = super::sessions().lock().unwrap_or_else(|p| p.into_inner());
        let Some(session) = all.get(&config.data_dir) else {
            anyhow::bail!("sync_runtime_locked");
        };
        ensure!(
            session.joining.is_none()
                && session.database == config.database_path
                && session.confirmed
                && session.master.is_some()
                && !session.enrollment_busy,
            "sync_runtime_locked"
        );
        (
            session.master.unwrap(),
            session.revision,
            session.installation.clone(),
        )
    };
    let vault = store.load_sync_vault(&installation, &master, revision)?;
    ensure!(vault.revision == revision, "sync_runtime_vault_changed");
    f(&store, vault.identity())
}

/// Old active processor yields: quiescent check, signed boundary proof, and a
/// permanent `relinquished` state that hard-blocks local background claims.
/// The successor revision is always the live roster's next revision.
pub(crate) fn yield_processor(
    config: &AppConfig,
    to_device_id: &str,
) -> Result<ProcessingYieldProof> {
    with_unlocked_vault(config, |store, signer| {
        store.require_sync_signer(signer)?;
        let target_revision = store
            .current_signed_roster()?
            .context("sync_authority_not_ready")?
            .roster
            .revision
            .checked_add(1)
            .context("sync_membership_revision_exhausted")?;
        store.yield_processor_role(target_revision, to_device_id, signer)
    })
}

/// Roster owner approves the old processor's yield proof and issues the
/// successor roster. Any unlocked device may call this; approval only passes
/// when the vault identity matches the pinned owner's signing key.
pub(crate) fn approve_handoff(config: &AppConfig, proof: &ProcessingYieldProof) -> Result<()> {
    with_unlocked_vault(config, |store, signer| {
        store.require_sync_signer(signer)?;
        store.approve_processor_handoff(proof, signer)?;
        Ok(())
    })
}

/// Takeover target confirms after the frontier reaches the handoff boundary.
pub(crate) fn confirm_takeover(config: &AppConfig) -> Result<()> {
    let store = Store::open(&config.database_path)?;
    store.require_local_sync_membership()?;
    store.confirm_processor_takeover()
}

/// Owner revokes a roster member. The UI must bind to the revision it
/// displayed; the store fails closed on any concurrent membership change.
pub(crate) fn revoke_member(
    config: &AppConfig,
    device_id: &str,
    expected_revision: u64,
) -> Result<()> {
    with_unlocked_vault(config, |store, signer| {
        store.require_sync_signer(signer)?;
        store.revoke_signed_sync_member(expected_revision, device_id, signer)?;
        Ok(())
    })
}

/// Read-only handoff state for the settings page.
pub(crate) fn state(config: &AppConfig) -> Result<crate::storage::sync::ProcessorHandoffState> {
    let store = Store::open(&config.database_path)?;
    store.get_processor_handoff_state()
}

/// Bridge DTOs carry i64; reject out-of-range values before the u64 cast.
pub(crate) fn proof_from_parts(
    dataset_id: String,
    from_device_id: String,
    to_device_id: String,
    roster_revision: i64,
    final_sequence: i64,
    signature: Vec<u8>,
) -> Result<ProcessingYieldProof> {
    ensure!(
        roster_revision > 0 && final_sequence >= 0,
        "sync_handoff_revision_invalid"
    );
    Ok(ProcessingYieldProof {
        dataset_id,
        from_device_id,
        to_device_id,
        roster_revision: roster_revision as u64,
        final_sequence: final_sequence as u64,
        signature,
    })
}
