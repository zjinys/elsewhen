use elsewhen::event::NewEvent;
use elsewhen::storage::{StorageAdapter, Store};

fn main() -> anyhow::Result<()> {
    let db_path = std::env::var("HOME").unwrap() + "/.local/share/elsewhen/elsewhen.db";
    let store = Store::open(&std::path::PathBuf::from(db_path))?;

    // Test through StorageAdapter trait
    let adapter: &dyn StorageAdapter = &store;

    println!("✓ Adapter created");

    // Insert event through adapter
    let new_event = NewEvent::now("测试 StorageAdapter - 这是通过 trait 插入的事件");
    let id = adapter.insert_event(new_event)?;
    println!("✓ Event inserted through adapter: {}", id);

    // List events through adapter
    let events = adapter.list_events()?;
    println!("✓ Found {} events through adapter", events.len());

    // Show last event
    if let Some(last) = events.last() {
        println!("  Last event: {}", last.raw_text);
    }

    println!("\n✅ StorageAdapter 工作正常！");

    Ok(())
}
