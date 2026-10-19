use super::super::*;
use super::*;

#[test]
fn proof_commit_is_native_fenced_and_old_role_or_token_cannot_confirm_it() {
    let f = Pair::new();
    let pending = f.accept_proof();
    assert!(pending.confirmation_token.is_some());
    assert!(
        begin(
            &f.owner.config,
            "owner",
            &pending.operation_id,
            &f.invitation
        )
        .is_err()
    );
    assert!(
        confirm(
            &f.owner.config,
            "joiner",
            &pending.operation_id,
            &f.invitation,
            pending.confirmation_token.as_ref().unwrap(),
            pending.revision
        )
        .is_err()
    );
    assert!(
        confirm(
            &f.owner.config,
            "owner",
            &pending.operation_id,
            "other-invitation",
            pending.confirmation_token.as_ref().unwrap(),
            pending.revision
        )
        .is_err()
    );
    assert!(
        confirm(
            &f.owner.config,
            "owner",
            &pending.operation_id,
            &f.invitation,
            "stale",
            pending.revision
        )
        .is_err()
    );
    f.owner
        .store
        .submit_input_as_event("capture during native confirmation", "capture", None)
        .unwrap();
    let token = pending.confirmation_token.clone().unwrap();
    let operation = pending.operation_id.clone();
    let revision = pending.revision;
    assert!(super::super::confirm_key(&f.owner.config, &token, revision).is_err());
    let ready = f.confirm(pending);
    assert_eq!(ready.phase, "preparing_channel");
    assert!(
        confirm(
            &f.owner.config,
            "owner",
            &operation,
            &f.invitation,
            &token,
            revision
        )
        .is_ok()
    );
    assert!(
        confirm(
            &f.owner.config,
            "owner",
            &operation,
            "other-invitation",
            &token,
            revision
        )
        .is_err()
    );
    assert_eq!(f.owner.store.list_events().unwrap().len(), 2);
    assert!(f.joiner.store.sync_identity().unwrap().is_none());
}

#[test]
fn grant_failure_rolls_back_and_reopen_preserves_the_exact_intent() {
    let f = Pair::new();
    f.confirm(f.accept_proof());
    let loan = f.loan("owner");
    let original = loan.record.as_ref().unwrap().receive.clone();
    assert_eq!(original.expected_revision, 0);
    assert!(
        loan.mutate(|store| store.confirm_enrollment_grant(
            &loan.installation,
            &loan.master,
            loan.revision,
            &loan.invitation,
            2
        ))
        .is_err()
    );
    let store = loan.store().unwrap();
    store.connection.execute_batch("CREATE TRIGGER fail_enrollment BEFORE UPDATE ON sync_local_vault BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    assert!(
        loan.mutate(|store| store.confirm_enrollment_grant(
            &loan.installation,
            &loan.master,
            loan.revision,
            &loan.invitation,
            1
        ))
        .is_err()
    );
    store
        .connection
        .execute_batch("DROP TRIGGER fail_enrollment")
        .unwrap();
    let revision = loan.revision;
    drop(loan);
    super::super::lock(&f.owner.config);
    let request = super::super::prepare_key(&f.owner.config, None, None).unwrap();
    let key = super::super::install_key(
        &f.owner.config,
        &request.token,
        f.owner_key.to_vec(),
        revision,
    )
    .unwrap();
    super::super::confirm_key(&f.owner.config, &key.token, key.revision).unwrap();
    let restored = f.loan("owner");
    assert_eq!(restored.record.as_ref().unwrap().receive, original);
    assert!(!restored.record.as_ref().unwrap().receive_ready);
    drop(restored);
    f.confirm(f.mutate("owner", |loan, store| {
        store.confirm_enrollment_grant(
            &loan.installation,
            &loan.master,
            loan.revision,
            &loan.invitation,
            1,
        )
    }));
}
