//! Local synchronization setup, protected-key handshake and owner invitations.
//! Invitations require an unlocked owner and explicit Provider-key disclosure;
//! ordinary synchronization is not started by these pairing-only operations.

use crate::{
    api::provider_reachability::{ReachabilityHint, assess_base_url},
    config::AppConfig,
    storage::{
        Store,
        sync::{
            page_conflicts::WikiPageConflictReview, provider_conflicts::ProviderConflictResolution,
        },
    },
    sync::runtime,
};
use anyhow::{Context, Result, ensure};
use rusqlite::OptionalExtension;

pub mod conflicts;
pub mod datasets;
pub mod invite;
pub mod join;
pub mod repair;

pub use conflicts::{
    ObjectConflictDto, list_object_conflicts, list_wiki_page_conflicts,
    resolve_object_conflict, resolve_wiki_page_conflict,
};
pub use datasets::{
    ProviderConflictProfileDto, ProviderSyncConflictDto, SyncRosterDto, SyncRosterMemberDto,
    WikiPageConflictDto, WikiPageConflictSideDto, approve_processor_handoff,
    confirm_processor_takeover, get_processor_handoff_state, get_sync_roster, list_sync_datasets,
    list_sync_provider_conflicts, rename_sync_device, resolve_sync_provider_conflict,
    revoke_sync_member, select_default_sync_dataset, select_sync_dataset, yield_sync_processor,
};
pub use invite::{
    LocalSyncStateDto, ProcessingYieldProofDto, ProcessorHandoffStateDto, SyncInvitationOfferDto,
    SyncInvitationPreviewDto, SyncKeyCommitDto, SyncKeyRequestDto, approve_owner_sync_invitation,
    cancel_owner_sync_invitation, confirm_sync_key, create_owner_sync_invitation,
    get_local_sync_state, inspect_owner_sync_invitation, install_sync_key, lock_local_sync,
    prepare_sync_key_request, release_sync_key_request, send_owner_sync_approval,
};
pub use repair::clear_local_sync_identity;
pub use join::{
    SyncDatasetEntryDto, SyncEnrollmentStateDto, SyncJoinEntryDto, SyncJoinStateDto,
    SyncTickResultDto, advance_sync_enrollment, confirm_sync_enrollment, confirm_sync_join_key,
    discard_sync_join_staging, get_sync_join_state, inspect_sync_join_approval,
    install_sync_join_key, list_pending_sync_joins, list_sync_enrollments, prepare_sync_join_key,
    prepare_sync_join_resume, preview_sync_join, release_sync_join, send_sync_join_proof,
    send_sync_join_request, trigger_network_sync,
};
