use super::*;

use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// 持 config 测试锁，将库路径钉死到临时目录；Drop 时清掉钉死值。
/// 不修改进程环境变量，避免与并行测试中的环境读取发生竞争。
struct DataDirGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}
impl DataDirGuard {
    fn set(dir: &std::path::Path) -> Self {
        let lock = crate::config::test_env_guard();
        crate::config::unpin();
        crate::config::AppConfig::pin(Some(dir.to_str().unwrap())).unwrap();
        Self { _lock: lock }
    }
}
impl Drop for DataDirGuard {
    fn drop(&mut self) {
        crate::config::unpin();
    }
}

#[test]
fn record_event_persists_dto_source_so_badge_renders_consistently() {
    let dir = std::env::temp_dir().join(format!(
        "elsewhen-record-event-{}.db",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let guard = DataDirGuard::set(&dir);

    let dto = record_event("GUI 快录一条事件".to_string()).unwrap();
    assert_eq!(
        dto.source, "flutter_gui",
        "record_event DTO 应声明 GUI 快录路径"
    );
    assert_eq!(dto.status, "pending");

    let rows = list_events().unwrap();
    let row = rows
        .iter()
        .find(|e| e.id == dto.id)
        .expect("应能读回刚插入的事件");
    assert_eq!(
        row.source, "flutter_gui",
        "落库 source 应与 DTO 对齐（list_events 如实读库后 event_card 的 source badge 才能显示 GUI）"
    );

    drop(guard);
    let _ = std::fs::remove_dir_all(&dir);
}
