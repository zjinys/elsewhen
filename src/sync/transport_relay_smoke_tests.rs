//! 端到端冒烟测试：Rust RelayClient → 本地 wrangler dev 真实 Worker。
//!
//! 运行前提：`cd relay && npx wrangler dev --port 8787`（测试会跳过不可用的中继）。
//! 这个测试验证的不是协议单元逻辑（那是 relay/test/crypto.test.mjs 的事），
//! 而是 Rust 客户端与 TS Worker 的**签名互通**——两端独立实现 HMAC 规范，
//! 只有真实 HTTP 往返才能暴露拼接顺序/编码的不一致。

#[cfg(test)]
mod smoke {
    use crate::sync::transport_relay::RelayClient;

    fn relay_available() -> Option<RelayClient> {
        let client = RelayClient::new("http://localhost:8787").ok()?;
        match client.health() {
            Ok(true) => Some(client),
            _ => None,
        }
    }

    #[test]
    fn end_to_end_send_fetch_ack_against_real_worker() {
        let Some(client) = relay_available() else {
            eprintln!(
                "skip: relay not running on :8787 (start with `cd relay && npx wrangler dev`)"
            );
            return;
        };

        // 模拟绑定产物：共享密钥 → relay_cred → mailbox_key
        // 用随机 shared_key 保证每次运行是“新设备对”，避免与上次运行残留在 dev 实例
        // 里的凭证撞 409（真实场景中新设备绑定本来就是新密钥对）。
        use ring::rand::{SecureRandom, SystemRandom};
        let mut shared_key = [0u8; 32];
        SystemRandom::new()
            .fill(&mut shared_key)
            .expect("system rng");
        let cred = RelayClient::derive_relay_cred(&shared_key);
        let sender_mb = RelayClient::mailbox_key(&cred);
        // 收件人邮箱用同一 cred 派生（真实场景是双方各自派生自己的邮箱；
        // 冒烟测试里发件人=收件人=自己，自发自收）
        let recipient_mb = sender_mb.clone();

        // 1. 注册凭证（仅首次成功，重复注册应 409 —— 但 dev 实例每次重启清零，故直接注册）
        let expires_at = chrono::Utc::now().timestamp_millis() + 86_400_000;
        client
            .register_credential(&recipient_mb, &sender_mb, &cred, expires_at)
            .expect("register credential against real worker");

        // 2. 发送端到端"密文"（冒烟测试用明文 base64 占位；真实加密由 binding 层完成）
        let plaintext = "elsewhen sync increment batch #1";
        let ciphertext_b64 = base64_encode(plaintext.as_bytes());
        let exp = chrono::Utc::now().timestamp_millis() + 3600_000;
        let id = client
            .send(&recipient_mb, &sender_mb, &cred, 1, &ciphertext_b64, exp)
            .expect("send envelope to real worker");
        assert!(!id.is_empty(), "worker must return an envelope id");

        // 3. 取件：游标 0 之后应拿到刚发的信封
        let envelopes = client
            .fetch(&recipient_mb, &cred, "0", 10)
            .expect("fetch from real worker");
        assert_eq!(envelopes.len(), 1, "should fetch exactly the one we sent");
        let got = &envelopes[0];
        assert_eq!(got.sender_mb, sender_mb);
        assert_eq!(got.seq_hint, 1);
        // 密文原样返回（中继不可读，但也不篡改）
        assert_eq!(got.ciphertext, ciphertext_b64);
        // 解出明文验证端到端完整性
        let decoded = base64_decode(&got.ciphertext).expect("decode our own ciphertext");
        assert_eq!(String::from_utf8(decoded).unwrap(), plaintext);

        // 4. 确认删除
        let deleted = client
            .ack(&recipient_mb, &cred, std::slice::from_ref(&got.id))
            .expect("ack against real worker");
        assert_eq!(deleted, 1);

        // 5. 再取应为空
        let envelopes = client
            .fetch(&recipient_mb, &cred, "0", 10)
            .expect("fetch after ack");
        assert!(envelopes.is_empty(), "acked envelope must be gone");
    }

    // base64（标准字母表，与 worker.ts 的 atob/btoa 对齐）
    fn base64_encode(data: &[u8]) -> String {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
            out.push(T[(n >> 18) as usize & 63] as char);
            out.push(T[(n >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 {
                T[(n >> 6) as usize & 63] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                T[n as usize & 63] as char
            } else {
                '='
            });
        }
        out
    }

    fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
        let val = |c: u8| -> Result<u8, String> {
            match c {
                b'A'..=b'Z' => Ok(c - b'A'),
                b'a'..=b'z' => Ok(c - b'a' + 26),
                b'0'..=b'9' => Ok(c - b'0' + 52),
                b'+' => Ok(62),
                b'/' => Ok(63),
                _ => Err(format!("bad base64 char: {c}")),
            }
        };
        let s = s.trim_end_matches('=');
        let bytes = s.as_bytes();
        let mut out = Vec::new();
        for chunk in bytes.chunks(4) {
            let mut n = 0u32;
            for (i, &c) in chunk.iter().enumerate() {
                n |= (val(c)? as u32) << (18 - 6 * i);
            }
            out.push((n >> 16) as u8);
            if chunk.len() > 2 {
                out.push((n >> 8) as u8);
            }
            if chunk.len() > 3 {
                out.push(n as u8);
            }
        }
        Ok(out)
    }
}
