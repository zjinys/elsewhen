use super::pairing;
use super::*;

#[test]
fn roots_and_approval_do_not_overwrite_existing_independent_data() {
    let a = Db::owner();
    let b = Db::new("b");
    b.store
        .submit_input_as_event("independent original", "capture", None)
        .unwrap();
    let now = 1_800_000_000_000;
    let (invite, pending) =
        PairingInvitation::issue("dataset", a.device.clone(), &a.signer, now).unwrap();
    let (join, request) = PendingJoin::begin(invite, b.device.clone(), &b.signer, now).unwrap();
    let (_, reply) = pending
        .receive(&request, now)
        .unwrap()
        .approve_for_dataset(&a.signer, 1, now, a.store.sync_roster_anchor().unwrap())
        .unwrap();
    let pair = join.finish(&reply, 0, now).unwrap();
    assert!(b.store.pin_sync_authority_from_pair(&pair).is_err());
    assert!(!b.store.has_sync_authority().unwrap());
    assert_eq!(
        b.store.list_events().unwrap()[0].raw_text,
        "independent original"
    );
}

#[test]
fn protocol_version_mismatch_rejects_as_version_mismatch() {
    let a = Db::owner();
    let b = Db::new("b");
    let (approved, _joining) = pairing(&a, &b);
    let v1 = a.store.sync_roster_anchor().unwrap().revision;
    let roster = a.store.grant_sync_pair(v1, &approved, &a.signer).unwrap();
    b.store
        .apply_signed_roster(&roster.encode().unwrap())
        .unwrap();
    a.store
        .submit_input_as_event("version tolerance probe", "capture", None)
        .unwrap();
    let mut op = a.store.pending_sync_operations("b", 10).unwrap().remove(0);
    let mut raw: serde_json::Value =
        serde_json::from_slice(&serde_json::to_vec(&op).unwrap()).unwrap();
    raw["protocol_version"] = serde_json::json!(999);
    let forged: crate::sync::protocol::SyncOperation = serde_json::from_value(raw).unwrap();
    // 版本不匹配按隔离语义处理：不应用、不阻塞流、可诊断。
    let report = b
        .store
        .receive_sync_operation("a", &serde_json::to_vec(&forged).unwrap())
        .unwrap();
    assert_eq!(report.applied, 0);
    assert_eq!(report.rejected, 0);
    let op_id = forged.op_id.clone();
    let (state, error_code): (String, Option<String>) = b
        .store
        .connection
        .query_row(
            "SELECT state,error_code FROM sync_operations WHERE op_id=?1",
            params![op_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "isolated");
    assert_eq!(error_code.as_deref(), Some("sync_protocol_unsupported"));
}
