//! Bounded enrollment ticks under the existing workspace session. This is not a
//! general synchronization worker and never selects a different application DB.
use super::*;
use crate::{
    storage::sync::PairingCommit,
    sync::{
        engine::{self, enrollment as transfer},
        enrollment::{EnrollmentRecord, EnrollmentRole, completion_id, offer_id},
        pairing::InvitationRoute,
        transport_relay::RelayClient,
        vault::LocalVault,
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};

mod flow;
mod view;

#[cfg(test)]
use flow::{EnrollmentLoan, accept_proof_page, begin, grant_failure, prepare_snapshot};
pub(crate) use flow::{advance, confirm};
pub(crate) use view::EnrollmentView;
pub(crate) use view::list;
use view::{scope, view};

#[cfg(test)]
mod tests;
