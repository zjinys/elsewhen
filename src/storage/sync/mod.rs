//! Transactional replication storage. Transport must authenticate a member before
//! invoking receive; these methods are not a Flutter/network entry point.

mod analysis;
mod delivery;
pub(crate) use delivery::BeginExport;
mod enrollment;
mod fragments;
mod gc;
mod membership;
pub(crate) use membership::ProcessorHandoffState;
mod mutable;
mod pairing;
pub(crate) use pairing::{PairingCommit, PairingDispatch};
pub(crate) mod object_conflicts;
pub(crate) mod page_conflicts;
mod processing;
pub(crate) mod provider_conflicts;
mod providers;
mod resnapshot;
pub(crate) mod snapshot;
mod singleton;
mod vault;
mod wiki;
mod wire;

use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

use super::Store;
use crate::sync::protocol::{
    ConversationRecord, EventRecord, InputRecord, MAX_BATCH_OPERATIONS, MAX_OPERATION_BYTES,
    MessageRecord, PROTOCOL_VERSION, SyncMutation, SyncOperation, validate_id,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplicaIdentity {
    pub dataset_id: String,
    pub device_id: String,
    pub stream_id: String,
    pub processor_device_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReceiveReport {
    pub applied: usize,
    pub conflicts: usize,
    pub waiting: usize,
    pub rejected: usize,
}

pub(crate) enum Apply {
    Applied,
    Waiting,
    Conflict {
        kind: &'static str,
        id: String,
        revision: Option<String>,
        reason: &'static str,
    },
}

mod capture;
mod receipt;
mod receive;

pub(crate) fn hash(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod chat_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "gc/tests.rs"]
mod gc_tests;

#[cfg(test)]
#[path = "resnapshot/tests.rs"]
mod resnapshot_tests;
