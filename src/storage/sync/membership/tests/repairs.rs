use super::enroll;
use super::owner_with_vault;
use super::*;

#[test]
fn rename_local_device_updates_vault_roster_and_anchor() {
    let owner = owner_with_vault();
    let key = owner.vault_key.as_ref().unwrap();
    let local_id = owner.device.device_id.clone();

    let revision = owner
        .store
        .rename_sync_vault_device("owner-install", key, 1, &local_id, "主机甲")
        .unwrap();
    assert_eq!(revision, 2);
    let vault = owner
        .store
        .load_sync_vault("owner-install", key, 2)
        .unwrap();
    assert_eq!(vault.device().name, "主机甲");
    let roster = owner.store.current_signed_roster().unwrap().unwrap();
    assert_eq!(roster.roster.revision, 2);
    assert_eq!(roster.roster.owner.name, "主机甲");
    assert!(
        roster
            .roster
            .members
            .iter()
            .any(|m| m.device.device_id == local_id && m.device.name == "主机甲")
    );
    // 锚同步更新，本机后续 decode/apply 不因名字失配而炸。
    let anchor = owner.store.sync_roster_anchor().unwrap();
    assert_eq!(anchor.owner.name, "主机甲");
}

#[test]
fn rename_member_only_touches_roster_name() {
    let owner = owner_with_vault();
    let joiner = Db::new("b");
    align_joiner_replica(&joiner, &owner.device.device_id);
    let (approved, _joining, roster) = enroll(&owner, &joiner);
    assert_eq!(roster.roster.revision, 2);
    let key = owner.vault_key.as_ref().unwrap();
    let b_id = joiner.device.device_id.clone();
    let revision = owner
        .store
        .rename_sync_vault_device("owner-install", key, 1, &b_id, "手机改名")
        .unwrap();
    assert_eq!(revision, 3);
    // 本机 vault 名不变（member 改名不触发 vault 重存，revision 仍为 1）。
    let vault = owner
        .store
        .load_sync_vault("owner-install", key, 1)
        .unwrap();
    assert_eq!(vault.device().name, "owner");
    let current = owner.store.current_signed_roster().unwrap().unwrap();
    assert!(
        current
            .roster
            .members
            .iter()
            .any(|m| m.device.device_id == b_id && m.device.name == "手机改名")
    );
    assert_eq!(current.roster.owner.name, "owner");
}

#[test]
fn rename_rejects_non_owner_invalid_name_and_unknown_device() {
    let owner = owner_with_vault();
    let joiner = Db::new("b");
    align_joiner_replica(&joiner, &owner.device.device_id);
    let (_approved, _joining, _roster) = enroll(&owner, &joiner);
    let key = owner.vault_key.as_ref().unwrap();
    let local_id = owner.device.device_id.clone();
    // 空白名。
    assert!(
        owner
            .store
            .rename_sync_vault_device("owner-install", key, 1, &local_id, "   ")
            .is_err()
    );
    // 未知设备。
    assert!(
        owner
            .store
            .rename_sync_vault_device("owner-install", key, 1, "zz", "新名")
            .is_err()
    );
    // 修订被并发推进后拒绝盲写。
    owner
        .store
        .rename_sync_vault_device("owner-install", key, 1, &local_id, "第一版")
        .unwrap();
    assert!(
        owner
            .store
            .rename_sync_vault_device("owner-install", key, 1, &local_id, "第二版")
            .is_err()
    );
}

#[test]
fn revoke_purges_pending_outbox_but_keeps_acknowledged_history() {
    let a = Db::owner();
    let b = Db::new("b");
    let (_, _, _grant_b) = enroll(&a, &b);
    // 给 b 准备一个待发送（悬空重试源）和一个已回执的历史信封；
    // 另给 owner 自己一个在途信封，验证 revoke 不误伤其他设备。
    for (recipient, id, state, hash) in [
        ("b", "message:p", "", "h1"),
        ("b", "message:a", "acknowledged", "h2"),
        ("a", "message:o", "", "h3"),
    ] {
        let sql = if state.is_empty() {
            format!(
                "INSERT INTO sync_wire_outbox(recipient_device_id,message_id,payload_hash,key_version,encrypted_envelope) VALUES('{recipient}','{id}','{hash}',1,x'01')"
            )
        } else {
            format!(
                "INSERT INTO sync_wire_outbox(recipient_device_id,message_id,payload_hash,key_version,encrypted_envelope,state) VALUES('{recipient}','{id}','{hash}',1,x'01','{state}')"
            )
        };
        a.store.connection.execute(&sql, []).unwrap();
    }
    let revision = a
        .store
        .current_signed_roster()
        .unwrap()
        .unwrap()
        .roster
        .revision;
    a.store
        .revoke_signed_sync_member(revision, "b", &a.signer)
        .unwrap();
    let pending: i64 = a
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sync_wire_outbox WHERE recipient_device_id='b' AND state='pending'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let acknowledged: i64 = a
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sync_wire_outbox WHERE recipient_device_id='b' AND state='acknowledged'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pending, 0, "pending 信封必须随 revoke 清除");
    assert_eq!(acknowledged, 1, "已回执的历史记录不清理");
    // 其他设备（owner 自己）的在途信封不受影响。
    let owner_pending: i64 = a
        .store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sync_wire_outbox WHERE recipient_device_id='a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(owner_pending, 1);
}
