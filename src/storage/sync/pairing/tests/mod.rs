use super::*;
use crate::sync::{
    crypto::ContentKey,
    pairing::{InvitationRoute, PairingInvitation, PendingJoin},
    transport_relay::RelayClient,
};

// 公共测试辅助（原文件平移，pub(super) 供分组子文件复用）。
const NOW: i64 = 1_800_000_000_000;
struct Device {
    root: std::path::PathBuf,
    store: Store,
    master: [u8; 32],
    vault: LocalVault,
}

impl Device {
    fn new(owner: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("elsewhen-pair-journal-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let master = ContentKey::generate().unwrap().to_bytes();
        let store = Store::open(&root.join("test.db"))
            .unwrap()
            .with_provider_master(master);
        let vault = store
            .create_sync_vault("install", &master, "synthetic device", "test")
            .unwrap();
        if owner {
            store
                .initialize_sync_replica(&ReplicaIdentity {
                    dataset_id: "dataset".into(),
                    device_id: vault.device().device_id.clone(),
                    stream_id: vault.device().stream_id.clone(),
                    processor_device_id: vault.device().device_id.clone(),
                })
                .unwrap();
            store
                .initialize_sync_authority(vault.device(), vault.identity())
                .unwrap();
            store.enable_provider_key_sealing().unwrap();
        }
        Self {
            root,
            store,
            master,
            vault,
        }
    }
    fn reopen(&self) -> Store {
        Store::open(&self.store.path)
            .unwrap()
            .with_provider_master(self.master)
    }
    fn revision(&self) -> i64 {
        self.store
            .connection
            .query_row("SELECT revision FROM sync_local_vault", [], |r| r.get(0))
            .unwrap()
    }
    fn active(&self, peer: &Device) -> bool {
        self.store
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sync_members WHERE device_id=?1 AND active=1)",
                [&peer.vault.device().device_id],
                |r| r.get(0),
            )
            .unwrap()
    }
    fn request(&self, joiner: &Device, now: i64) -> (PendingApproval, PendingJoin) {
        let (invite, pending) = PairingInvitation::issue_for_relay(
            "dataset",
            self.vault.device().clone(),
            self.vault.identity(),
            now,
            InvitationRoute {
                relay_url: "http://127.0.0.1:9876".into(),
                issuer_mailbox: RelayClient::mailbox_key(self.vault.mailbox_owner()),
                request_sender: "f".repeat(64),
            },
        )
        .unwrap();
        let (join, request) = PendingJoin::begin_for_relay(
            invite,
            joiner.vault.device().clone(),
            joiner.vault.identity(),
            now,
            RelayClient::mailbox_key(joiner.vault.mailbox_owner()),
        )
        .unwrap();
        (pending.receive(&request, now).unwrap(), join)
    }
    fn approve(&self, joiner: &Device, now: i64) -> (PairingCommit, PendingJoin) {
        let (approval, join) = self.request(joiner, now);
        (
            self.store
                .approve_sync_pairing("install", &self.master, self.revision(), approval, now)
                .unwrap(),
            join,
        )
    }
    fn frame(&self, commit: &PairingCommit, now: i64) -> Vec<u8> {
        self.reopen()
            .sync_pairing_frame(
                "install",
                &self.master,
                commit.vault_revision,
                &commit.invitation_id,
                now,
            )
            .unwrap()
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub(super) fn installed(a: &Device, b: &Device) -> (PairingCommit, PairingCommit, Vec<u8>) {
    let (approved, join) = a.approve(b, NOW);
    let pair = join.finish(&a.frame(&approved, NOW), 0, NOW).unwrap();
    let installed = b
        .store
        .install_sync_pairing_join("install", &b.master, 1, pair, NOW)
        .unwrap();
    let proof = b.frame(&installed, NOW);
    (approved, installed, proof)
}

mod approval_lifecycle;

mod join_atomicity;
