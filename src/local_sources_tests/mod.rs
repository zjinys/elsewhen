use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

// 公共测试辅助（原 mod tests 内嵌段平移，pub(super) 供分组子文件复用）。
pub(super) fn temp_path(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("elsewhen-{label}-{nonce}"))
}

mod local_sources_part1;
mod local_sources_part2;
mod local_sources_part3;
mod local_sources_part4;
