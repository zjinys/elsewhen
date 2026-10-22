//! 快照载荷工具函数与帧解码器。

use super::*;

pub(crate) fn validate_json_arg(text: &str, code: &'static str) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| anyhow::anyhow!(code))?;
    ensure!(value.is_array() || value.is_object(), code);
    Ok(())
}

pub(crate) fn hex_to_vec(hex: &str) -> Result<Vec<u8>> {
    Ok((0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16))
        .collect::<std::result::Result<Vec<u8>, _>>()
        .map_err(|_| anyhow::anyhow!("sync_snapshot_wiki_asset_invalid"))?)
}

/// head/known_revision 记录的种类必须落在随快照携带的领域表上。
pub(crate) fn domain_table(kind: &str) -> Result<&'static str> {
    Ok(match kind {
        "conversation" => "conversations",
        "wiki_page" => "wiki_pages",
        "todo" => "todos",
        "habit_nudge" => "habit_nudges",
        "goal" => "goals",
        "rule" => "rules",
        _ => return Err(anyhow::anyhow!("sync_snapshot_object_kind_invalid")),
    })
}

#[derive(Default)]
pub(crate) struct FrameDecoder {
    pub(crate) pending: Vec<u8>,
}
impl FrameDecoder {
    pub(crate) fn push(
        &mut self,
        bytes: &[u8],
        mut apply: impl FnMut(Record) -> Result<()>,
    ) -> Result<()> {
        self.pending.extend_from_slice(bytes);
        let mut consumed = 0;
        loop {
            let remaining = &self.pending[consumed..];
            if remaining.len() < 4 {
                break;
            }
            let size = u32::from_be_bytes(remaining[..4].try_into().unwrap()) as usize;
            ensure!(
                size > 0 && size <= MAX_RECORD_BYTES,
                "sync_snapshot_record_size_invalid"
            );
            if remaining.len() < size + 4 {
                break;
            }
            let record: Record = serde_json::from_slice(&remaining[4..size + 4])
                .map_err(|_| anyhow::anyhow!("sync_snapshot_record_invalid"))?;
            apply(record)?;
            consumed += size + 4;
        }
        if consumed > 0 {
            self.pending.drain(..consumed);
        }
        ensure!(
            self.pending.len() <= MAX_RECORD_BYTES + 4,
            "sync_snapshot_record_size_invalid"
        );
        Ok(())
    }
}
