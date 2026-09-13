mod ai;
mod capture;
mod config;
mod event;
mod hotkey;
mod settings;
mod storage;

use anyhow::Result;
use event::NewEvent;
use std::env;
use storage::Store;

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let config = config::AppConfig::load()?;
    let store = Store::open(&config.database_path)?;

    match env::args().nth(1).as_deref() {
        Some("capture") => capture::run(store)?,
        Some("daemon") => hotkey::run_daemon()?,
        Some("settings") => settings::run(config.database_path)?,
        Some("analyze-once") => ai::run_once(&store)?,
        Some("worker") => ai::run_worker(&store)?,
        Some("providers") => {
            if let Some(provider) = store.active_ai_provider_config()? {
                println!(
                    "{} {} {} key={}",
                    provider.provider_type,
                    provider.base_url,
                    provider.model,
                    provider.api_key_source
                );
            } else {
                println!("no active AI provider");
            }
        }
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
        Some("analyses") => {
            for analysis in store.list_analyses()? {
                println!(
                    "[{} {:.2}] {}\n  clarifications: {}",
                    analysis.event_type,
                    analysis.confidence,
                    analysis.raw_text,
                    analysis.clarifications
                );
            }
        }
        _ => print_usage(),
    }

    Ok(())
}

fn print_usage() {
    println!("elsewhen record <text>");
    println!("elsewhen list");
    println!("elsewhen analyses");
    println!("elsewhen capture");
    println!("elsewhen daemon");
    println!("elsewhen settings");
    println!("elsewhen analyze-once");
    println!("elsewhen worker");
    println!("elsewhen providers");
}
