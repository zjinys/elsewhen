use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

// 公共测试辅助（原 mod tests 内嵌段平移，pub(super) 供分组子文件复用）。
pub(super) fn temporary_database() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "elsewhen-memory-test-{}.db",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

pub(super) fn setup_store() -> (Store, String, std::path::PathBuf) {
    let path = temporary_database();
    let store = Store::open(&path).unwrap();
    let conversation_id = store
        .create_conversation(Some("今日记录"), Some("discussion"))
        .unwrap();
    store
        .send_message(
            &conversation_id,
            "user",
            "今天用 opencode2 讨论了一些需求",
            None,
        )
        .unwrap();
    (store, conversation_id, path)
}

// ── 目标注入（FR-PES-005-02）──

fn system_text(context: &[ContextMessage]) -> String {
    context
        .iter()
        .filter(|message| message.role == "system")
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn system_prompt_of(store: &Store, conversation_id: &str) -> String {
    system_text(&build_system_context(store, conversation_id).unwrap())
}

mod memory_part1;
mod memory_part2;
mod memory_part3;
mod memory_part4;
