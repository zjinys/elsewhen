//! transport_relay 内嵌测试平移（URL 校验/签名域/整数界/本地 mock relay 集成）。

use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_validation_rejects_prefix_tricks_and_credentials() {
        for url in [
            "http://localhost.evil.invalid",
            "http://127.0.0.1.evil.invalid",
            "http://localhost@evil.invalid",
            // 私网段之外的明文 http 仍然拒绝。
            "http://8.8.8.8:8787",
            "http://example.com:8787",
            "https://user:secret@relay.invalid",
            "https://relay.invalid?key=secret",
            "https://relay.invalid#fragment",
        ] {
            assert!(RelayClient::new(url).is_err(), "{url}");
        }
        for url in [
            "https://relay.invalid",
            "http://localhost:8787",
            "http://127.0.0.1:8787",
            "http://[::1]:8787",
            // 自建局域网中继（NAS）：私网明文地址放行。
            "http://192.168.1.10:8787",
            "http://10.0.0.2",
            "http://172.16.5.5:8787",
            "http://[fd00::5]:8787",
        ] {
            assert!(RelayClient::new(url).is_ok(), "{url}");
        }
    }

    #[test]
    fn sender_capability_is_direction_and_recipient_bound() {
        let a = RelayClient::mailbox_key(&[1; 32]);
        let b = RelayClient::mailbox_key(&[2; 32]);
        let c = RelayClient::mailbox_key(&[3; 32]);
        assert_ne!(a, b);
        let ab = RelayClient::derive_sender_credential(&[9; 32], &a, &b).unwrap();
        assert_ne!(
            ab,
            RelayClient::derive_sender_credential(&[9; 32], &b, &a).unwrap()
        );
        assert_ne!(
            ab,
            RelayClient::derive_sender_credential(&[9; 32], &a, &c).unwrap()
        );
        assert!(RelayClient::derive_sender_credential(&[9; 32], &a, &a).is_err());
    }

    #[test]
    fn wire_numbers_and_cursors_are_bounded() {
        assert!(validate_integer(MAX_SAFE_INTEGER).is_ok());
        assert!(validate_integer(MAX_SAFE_INTEGER + 1).is_err());
        assert!(validate_cursor("0").is_ok());
        assert!(validate_cursor("0000000000000001").is_ok());
        for value in ["1", "-1", "1|nonce", "9999999999999999", ""] {
            assert!(validate_cursor(value).is_err());
        }
    }

    #[test]
    fn signatures_are_domain_separated_from_v1() {
        assert_ne!(
            hmac_sign_hex(&[7; 32], &["relay-send", "a"]),
            hmac_sign_hex(&[7; 32], &["relay-v2-send", "a"])
        );
        assert_eq!(
            sign(&[7; 32], "status", &["mailbox"], 123, "nonce"),
            hmac_sign_hex(&[7; 32], &["relay-v2-status", "mailbox", "123", "nonce"])
        );
    }
}
