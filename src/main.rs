mod config;
mod event;
mod storage;
mod capture;

use anyhow::Result;
use event::NewEvent;
use std::env;
use storage::Store;

fn main() -> Result<()> {
    let config = config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    match env::args().nth(1).as_deref() {
        Some("capture") => capture::run(store)?,
        Some("record") => {
            let text = env::args().skip(2).collect::<Vec<_>>().join(" ");
            let text = text.trim();
            if text.is_empty() {
                anyhow::bail!("record text must not be empty");
            }
            let id = store.insert_event(NewEvent::now(text))?;
            println!("saved event {id}");
        }
        Some("list") => {
            for event in store.list_events()? {
                println!("{} {}", event.recorded_at, event.raw_text);
            }
        }
        _ => print_usage(),
    }

    Ok(())
}

fn print_usage() {
    println!("elsewhen record <text>");
    println!("elsewhen list");
    println!("elsewhen capture");
}
