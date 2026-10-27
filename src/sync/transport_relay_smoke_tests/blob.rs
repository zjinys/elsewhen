//! blob 数据面的真实 relay 契约测试（R4-1）。
//!
//! 客户端这一侧的分片流式上传/窗口拉取/逐窗确认只有在真实 relay 上跑过才算数：
//! 签名域的 parts 顺序、`created`/`have`/`present` 的语义、裸二进制帧，任何一处
//! 对不上都只能在真机上暴露。与 `roundtrip.rs` 一样走 `ELSEWHEN_RELAY_TEST_URL`。

use super::key;
use crate::sync::transport_relay::{RelayClient, WINDOW_CHUNKS};

fn chunk(index: usize) -> Vec<u8> {
    // 256 KiB 一片（与客户端 `CHUNK_BYTES` 同量级），内容可复算：任何一片错位
    // 都会在字节比较里立刻暴露，而不是靠长度碰巧相等。
    let mut bytes = vec![0u8; 256 * 1024];
    let seed = (index as u32).wrapping_mul(2_654_435_761);
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = (seed ^ (offset as u32).rotate_left(7)) as u8;
    }
    bytes
}

pub(super) fn blob_roundtrip(
    client: &RelayClient,
    owner: &[u8; 32],
    mailbox: &str,
    sender_mb: &str,
    credential: &[u8; 32],
    blob_id: &str,
) {
    // 开户：新 blob 必须回报 `created:true`，否则续传会把「relay 上早被收掉」
    // 误判成「全部已送达」，跳过的那段分片永远到不了对端。
    let opened = client
        .blob_open(mailbox, sender_mb, credential, blob_id)
        .unwrap();
    assert!(opened.created, "首次 open 必须创建新 blob");
    assert!(opened.have.is_empty());

    // 分片上传：写到超过一窗（40 > 32），模拟 122 MB 快照的跨窗行为。
    let total = 40;
    for index in 0..total {
        client
            .blob_put(
                mailbox,
                sender_mb,
                credential,
                blob_id,
                index,
                &chunk(index),
            )
            .unwrap();
    }
    // 结构去重：同一片重投只占一份空间（丢响应重试的常态）。
    client
        .blob_put(mailbox, sender_mb, credential, blob_id, 7, &chunk(7))
        .unwrap();

    // 上传方视角的进度（sender 凭据，窗口流控就靠它）。
    let sender_view = client
        .blob_state(mailbox, blob_id, credential, Some(sender_mb))
        .unwrap()
        .expect("sender 必须能读自己上传的 blob 进度");
    assert_eq!(sender_view.highest, Some(total - 1));
    assert_eq!(sender_view.present.len(), total);
    assert_eq!(sender_view.acked_upto(), 0);

    // 第二把凭据也能读（owner 收件侧），且看不到「已 ack」以外的东西。
    let owner_view = client
        .blob_state(mailbox, blob_id, owner, None)
        .unwrap()
        .expect("owner 必须能读自己信箱里的 blob");
    assert_eq!(owner_view.highest, sender_view.highest);
    assert_eq!(
        owner_view.stored_bytes, sender_view.stored_bytes,
        "两把凭据看到的是同一份占盘"
    );

    // 一窗下载：从第 0 片起连续一段，帧是裸的 `[4B 大端长度][密文]`。
    let window = client
        .blob_get_window(mailbox, owner, blob_id, 0, WINDOW_CHUNKS)
        .unwrap();
    assert_eq!(window.from, Some(0));
    assert_eq!(window.chunks.len(), WINDOW_CHUNKS);
    for (offset, bytes) in window.chunks.iter().enumerate() {
        assert!(bytes == &chunk(offset), "第 {offset} 片字节不一致");
    }

    // 逐窗确认：确认到第 31 片，relay 立刻删掉这些，只剩尾巴。
    assert_eq!(
        client
            .blob_ack(mailbox, owner, blob_id, WINDOW_CHUNKS - 1)
            .unwrap(),
        WINDOW_CHUNKS
    );
    let after_ack = client
        .blob_state(mailbox, blob_id, owner, None)
        .unwrap()
        .expect("ack 之后 blob 仍然存在");
    assert_eq!(after_ack.present.len(), total - WINDOW_CHUNKS);
    assert_eq!(after_ack.acked_upto(), WINDOW_CHUNKS);
    assert_eq!(
        after_ack.stored_bytes,
        ((total - WINDOW_CHUNKS) * (256 * 1024)) as i64
    );

    // 重复 open 幂等：不新建、不丢进度，并把还剩下的片报回来（续传用）。
    let reopened = client
        .blob_open(mailbox, sender_mb, credential, blob_id)
        .unwrap();
    assert!(!reopened.created, "已存在的 blob 不能回报 created");
    assert_eq!(reopened.have, (WINDOW_CHUNKS..total).collect::<Vec<_>>());

    // 从下半段开始拉剩余窗口，证明 `from` 是绝对片号；一次 GET 最多一窗，
    // 所以剩下的片要一窗一窗拉完（这正是窗口流控该有的形状）。
    let mut from = WINDOW_CHUNKS;
    while from < total {
        let page = client
            .blob_get_window(mailbox, owner, blob_id, from, WINDOW_CHUNKS)
            .unwrap();
        assert_eq!(page.from, Some(from));
        assert_eq!(page.chunks.len(), (total - from).min(WINDOW_CHUNKS));
        for (offset, bytes) in page.chunks.iter().enumerate() {
            assert!(
                bytes == &chunk(from + offset),
                "第 {} 片字节不一致",
                from + offset
            );
        }
        from += page.chunks.len();
    }
    assert_eq!(from, total, "必须能一窗一窗拉到尾部");

    // 结束并精确回收：close 之后这一块占盘必须归零（「close 后残留 0」）。
    // relay 的 freed 单位是字节（实际回收了多少落盘），不是片数。
    assert_eq!(
        client
            .blob_close(mailbox, owner, blob_id, "done", None)
            .unwrap(),
        (total - WINDOW_CHUNKS) * (256 * 1024)
    );
    // close 之后 relay 的 state 仍是 200，但 present 必须为空（「残留 0」看的是
    // blob_bytes 归零 + 拉取窗为空，这两条在下面分别断言）。
    let gone = client
        .blob_state(mailbox, blob_id, owner, None)
        .unwrap()
        .expect("close 后 state 仍是 200，present 为空");
    assert!(gone.present.is_empty());
    assert_eq!(gone.highest, None);
    assert_eq!(gone.stored_bytes, 0);
    assert!(
        client
            .blob_get_window(mailbox, owner, blob_id, 0, WINDOW_CHUNKS)
            .unwrap()
            .chunks
            .is_empty()
    );

    // 另一个合法 sender 不能替开户方写/撤回：blob 记的是开户人 `sender_mb`，
    // 任何别的 sender（哪怕是同一信箱里被授权的）都只能吃 403。
    // 注意：`mailbox_key()` 由凭据确定性导出，intruder 必须用**第三个**密钥，
    // 否则 create_mailbox 会回到开户 sender 自己的信箱（等于重复注册同一个 sender）。
    let intruder_key = key();
    let intruder_mb = client.create_mailbox(&intruder_key).unwrap();
    let intruder_cred = crate::sync::transport_relay::RelayClient::derive_sender_credential(
        &intruder_key,
        &intruder_mb,
        mailbox,
    )
    .unwrap();
    let expiry = chrono::Utc::now().timestamp_millis() + 3_600_000;
    client
        .register_sender(mailbox, owner, &intruder_mb, &intruder_cred, expiry, 0)
        .unwrap();
    assert!(
        client
            .blob_put(mailbox, &intruder_mb, &intruder_cred, blob_id, 0, &chunk(0))
            .is_err(),
        "非开户 sender 不能向别人的 blob 写片"
    );
    // 非开户 sender 读 state 统一 403（连「存在/不存在」都不泄露：404 只给
    // 拥有该信箱 owner 凭据或开户 sender 自己的人）。
    assert!(
        client
            .blob_state(mailbox, blob_id, &intruder_cred, Some(&intruder_mb))
            .is_err(),
        "非开户 sender 读 state 必须吃 403"
    );
}
