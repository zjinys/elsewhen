//! Pinned owner authority and cumulative signed rosters. Unsigned snapshot or
//! content metadata cannot grant membership once this boundary is installed.

use super::Store;
use crate::sync::{
    crypto::DeviceIdentity,
    membership::{MembershipRoster, RosterAnchor, RosterMember, SignedRoster, same_identity},
    pairing::{ApprovedPair, PairingDevice},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

/// 加工移交状态查询结果（存储层内部表示，API 层会映射为 DTO）。
#[derive(Clone, Debug)]
pub(crate) struct ProcessorHandoffState {
    pub state: String,
    pub from_device_id: Option<String>,
    pub to_device_id: Option<String>,
    pub final_sequence: Option<i64>,
    pub can_confirm: bool,
}

mod authority;
mod members;

#[cfg(test)]
mod tests;
