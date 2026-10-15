//! Bounded operation fragmentation. Pieces are not domain operations and do not
//! advance origin progress; only reassembly and the domain transaction can do so.

use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

use super::protocol::{MAX_OPERATION_BYTES, PROTOCOL_VERSION};

pub(crate) const CHUNK_BYTES: usize = 256 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OperationFragment {
    pub version: u32,
    pub op_id: String,
    pub payload_hash: String,
    pub total_bytes: usize,
    pub chunk_count: usize,
    pub index: usize,
    pub data: String,
}

impl OperationFragment {
    pub(crate) fn message_id(&self) -> String {
        format!(
            "op-part-{}-{}",
            crate::storage::sync::hash(self.op_id.as_bytes()),
            self.index
        )
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<(Self, Vec<u8>)> {
        ensure!(
            bytes.len() <= CHUNK_BYTES.div_ceil(3) * 4 + 2048,
            "sync_fragment_too_large"
        );
        let fragment: Self =
            serde_json::from_slice(bytes).map_err(|_| anyhow::anyhow!("sync_fragment_invalid"))?;
        ensure!(
            fragment.version == PROTOCOL_VERSION,
            "sync_protocol_unsupported"
        );
        let (stream, seq) = fragment
            .op_id
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("sync_fragment_identity_invalid"))?;
        super::protocol::validate_id(stream)?;
        let seq: i64 = seq
            .parse()
            .map_err(|_| anyhow::anyhow!("sync_fragment_identity_invalid"))?;
        ensure!(
            seq > 0 && fragment.op_id == format!("{stream}:{seq}"),
            "sync_fragment_identity_invalid"
        );
        ensure!(
            (1..=MAX_OPERATION_BYTES).contains(&fragment.total_bytes)
                && fragment.chunk_count == fragment.total_bytes.div_ceil(CHUNK_BYTES)
                && fragment.index < fragment.chunk_count
                && fragment.data.len() <= CHUNK_BYTES.div_ceil(3) * 4
                && fragment.payload_hash.len() == 64
                && fragment
                    .payload_hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "sync_fragment_limit_invalid"
        );
        let data = STANDARD
            .decode(&fragment.data)
            .map_err(|_| anyhow::anyhow!("sync_fragment_invalid"))?;
        let expected = (fragment.total_bytes - fragment.index * CHUNK_BYTES).min(CHUNK_BYTES);
        ensure!(data.len() == expected, "sync_fragment_size_invalid");
        Ok((fragment, data))
    }
}
