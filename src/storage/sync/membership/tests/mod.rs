use super::*;
use crate::sync::{
    crypto::{self, ContentKey, EncryptedEnvelope, EnvelopeHeader},
    engine,
    membership::ProcessingYieldProof,
    pairing::{PairingInvitation, PendingJoin},
};
use base64::{Engine, engine::general_purpose::STANDARD};

// 公共测试辅助（原文件平移，pub(super) 供分组子文件复用）。
struct Db {
    store: Store,
    signer: DeviceIdentity,
    device: PairingDevice,
    dir: std::path::PathBuf,
    /// 改名测试用：owner vault 的内容密钥字节。
    vault_key: Option<[u8; 32]>,
}

impl Db {
    fn new(id: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("elsewhen-authority-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        let signer = DeviceIdentity::generate().unwrap();
        let device = PairingDevice {
            device_id: id.into(),
            stream_id: format!("stream-{id}"),
            name: id.into(),
            platform: "test".into(),
            signing_key: signer.public_key(),
        };
        store
            .initialize_sync_replica(&crate::storage::sync::ReplicaIdentity {
                dataset_id: "dataset".into(),
                device_id: id.into(),
                stream_id: device.stream_id.clone(),
                processor_device_id: "a".into(),
            })
            .unwrap();
        Self {
            store,
            signer,
            device,
            dir,
            vault_key: None,
        }
    }
    fn owner() -> Self {
        let a = Self::new("a");
        a.store
            .initialize_sync_authority(&a.device, &a.signer)
            .unwrap();
        a
    }
    fn active(&self, id: &str) -> bool {
        self.store
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
                [id],
                |r| r.get(0),
            )
            .unwrap()
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub(super) fn pairing(a: &Db, b: &Db) -> (ApprovedPair, ApprovedPair) {
    let now = 1_800_000_000_000;
    let anchor = a.store.sync_roster_anchor().unwrap();
    let (invite, pending) =
        PairingInvitation::issue("dataset", a.device.clone(), &a.signer, now).unwrap();
    let (join, request) = PendingJoin::begin(invite, b.device.clone(), &b.signer, now).unwrap();
    let (completion, reply) = pending
        .receive(&request, now)
        .unwrap()
        .approve_for_dataset(&a.signer, 1, now, anchor.clone())
        .unwrap();
    let joining = join.finish(&reply, 0, now).unwrap();
    assert_eq!(joining.authority.as_ref(), Some(&anchor));
    b.store.pin_sync_authority_from_pair(&joining).unwrap();
    let confirmed = completion
        .confirm(&joining.completion_proof(&b.signer).unwrap(), now)
        .unwrap();
    (confirmed, joining)
}

pub(super) fn enroll(a: &Db, b: &Db) -> (ApprovedPair, ApprovedPair, SignedRoster) {
    let (approved, joining) = pairing(a, b);
    let version = a.store.sync_roster_anchor().unwrap().revision;
    let roster = a
        .store
        .grant_sync_pair(version, &approved, &a.signer)
        .unwrap();
    b.store
        .apply_signed_roster(&roster.encode().unwrap())
        .unwrap();
    (approved, joining, roster)
}

// ---------- P4d processor handoff defense lines ----------

fn handoff_fixture() -> (Db, Db) {
    let a = Db::owner();
    let b = Db::new("b");
    enroll(&a, &b);
    (a, b)
}

pub(super) fn signed_handoff_roster(
    a: &Db,
    proof: ProcessingYieldProof,
    target_revision: u64,
) -> SignedRoster {
    let mut roster = a.store.current_signed_roster().unwrap().unwrap().roster;
    roster.revision = target_revision;
    roster.processor_device_id = proof.to_device_id.clone();
    roster.handoff = Some(proof);
    SignedRoster::sign(roster, &a.signer).unwrap()
}

pub(super) fn processor_of(store: &Store) -> String {
    store.sync_identity().unwrap().unwrap().processor_device_id
}

// ---------------------------------------------------------------------------
// owner 改设备显示名：vault + 签名 roster + 锚三处原子更新。
// ---------------------------------------------------------------------------

/// joiner 侧 replica 的 processor 指向真实 owner 设备（等价于配对确认后的
/// 视角）；vault 化 owner 的 device_id 是随机 UUID，固定值断言不再适用。
fn align_joiner_replica(joiner: &Db, owner_device_id: &str) {
    joiner
        .store
        .connection
        .execute(
            "UPDATE sync_replica SET processor_device_id=?1 WHERE singleton=1",
            [owner_device_id],
        )
        .unwrap();
}

pub(super) fn owner_with_vault() -> Db {
    // 顺序与 runtime init 相同：先 vault，再 replica，再 authority；设备身份
    // 直接取 vault 生成的 identity，保证 signer/device 与 vault 三方一致。
    let dir = std::env::temp_dir().join(format!("elsewhen-authority-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("test.db")).unwrap();
    let key = ContentKey::generate().unwrap().to_bytes();
    let vault = store
        .create_sync_vault("owner-install", &key, "owner", "test")
        .unwrap();
    store
        .initialize_sync_replica(&crate::storage::sync::ReplicaIdentity {
            dataset_id: "dataset".into(),
            device_id: vault.device().device_id.clone(),
            stream_id: vault.device().stream_id.clone(),
            processor_device_id: vault.device().device_id.clone(),
        })
        .unwrap();
    store
        .initialize_sync_authority(vault.device(), vault.identity())
        .unwrap();
    Db {
        store,
        // DeviceIdentity 无 Clone（防私钥外泄）：经 pkcs8 往返恢复测试副本。
        signer: DeviceIdentity::from_pkcs8(&vault.identity().to_pkcs8()).unwrap(),
        device: vault.device().clone(),
        dir,
        vault_key: Some(key),
    }
}

mod handoff;

mod enrollment_security;

mod quiescence;

mod bootstrap;

mod repairs;
