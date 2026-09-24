mod ai;
mod api;
mod capture;
mod config;
mod event;
mod fonts;
mod hotkey;
mod settings;
mod storage;
mod wiki;

use anyhow::{Context, Result};
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
        Some("insight") => {
            let args = env::args().skip(2).collect::<Vec<_>>();
            let mut days = 14_i64;
            let mut max_events = 60_usize;
            let mut i = 0;
            while i < args.len() {
                match args[i].as_str() {
                    "--days" => {
                        let val = args
                            .get(i + 1)
                            .and_then(|v| v.parse().ok())
                            .context("--days 需要一个整数")?;
                        days = val;
                        i += 1;
                    }
                    "--max-events" => {
                        let val = args
                            .get(i + 1)
                            .and_then(|v| v.parse().ok())
                            .context("--max-events 需要一个整数")?;
                        max_events = val;
                        i += 1;
                    }
                    other => anyhow::bail!("未知参数: {other}"),
                }
                i += 1;
            }
            let options = ai::insight::InsightOptions { days, max_events };
            let insights = ai::insight::generate_insights(&store, &options)?;
            if insights.is_empty() {
                println!("本次没有生成洞察（AI 认为近期事件里没有值得说的内容）");
            }
            for (idx, ins) in insights.iter().enumerate() {
                println!("\n── 洞察 {}（透镜 {}）──", idx + 1, ins.lens);
                println!("💡 {}", ins.title);
                println!("{}", ins.observation);
                if let Some(action) = &ins.action {
                    if !action.is_empty() {
                        println!("\n→ 建议行动: {}", action);
                    }
                }
                if !ins.related_events.is_empty() {
                    println!("\n相关事件:");
                    for r in &ins.related_events {
                        println!("  · {}", r);
                    }
                }
            }
        }
        Some("insights") => {
            let list = store.list_insights()?;
            if list.is_empty() {
                println!("暂无洞察。运行 `elsewhen insight` 基于最近事件生成。");
            }
            for ins in list {
                println!("[{}] 透镜{} | {}", ins.created_at, ins.lens, ins.title);
            }
        }
        Some("wiki") => {
            let args = env::args().skip(2).collect::<Vec<_>>();
            match args.first().map(String::as_str) {
                Some("list") => {
                    let kind = args.get(1).map(String::as_str);
                    let pages = store.list_wiki_pages(kind, None)?;
                    if pages.is_empty() {
                        println!(
                            "wiki 还没有页面。运行 `elsewhen wiki digest` 把事件消化进知识库。"
                        );
                    }
                    for p in pages {
                        println!(
                            "[{}] {} | {} | 证据{} | {}",
                            p.kind, p.slug, p.title, p.evidence_count, p.summary
                        );
                    }
                }
                Some("show") => {
                    let slug = args.get(1).context("usage: elsewhen wiki show <slug>")?;
                    match store.get_wiki_page(slug)? {
                        Some(p) => {
                            println!("# {} ({})\n", p.title, p.kind);
                            println!(
                                "证据数: {} | 状态: {} | 更新: {}",
                                p.evidence_count, p.status, p.updated_at
                            );
                            if !p.source_event_ids.is_empty() {
                                println!("溯源事件: {}", p.source_event_ids.join(", "));
                            }
                            println!("\n{}", p.content_md);
                        }
                        None => println!("没有找到页面: {}", slug),
                    }
                }
                Some("digest") => {
                    let mut opts = wiki::DigestOptions::default();
                    let mut i = 1;
                    while i < args.len() {
                        match args[i].as_str() {
                            "--days" => {
                                opts.days = args
                                    .get(i + 1)
                                    .and_then(|v| v.parse().ok())
                                    .context("--days 需要一个整数")?;
                                i += 1;
                            }
                            "--max-events" => {
                                opts.max_events = args
                                    .get(i + 1)
                                    .and_then(|v| v.parse().ok())
                                    .context("--max-events 需要一个整数")?;
                                i += 1;
                            }
                            "--dry-run" => opts.dry_run = true,
                            "--force" => opts.force = true,
                            other => anyhow::bail!("未知参数: {other}"),
                        }
                        i += 1;
                    }
                    let result = wiki::generate_digest(&store, &opts)?;
                    if result.created.iter().any(|s| s == "__no_new_events__") {
                        println!("没有自上次 digest 以来的新事件。加 --force 强制重新消化。");
                    } else {
                        println!(
                            "digest 完成: 新建 {} 页, 更新 {} 页, 跳过 {} 条",
                            result.created.len(),
                            result.updated.len(),
                            result.skipped.len()
                        );
                        for s in &result.created {
                            println!("  + {}", s);
                        }
                        for s in &result.updated {
                            println!("  ~ {}", s);
                        }
                        for s in &result.skipped {
                            println!("  ! {}", s);
                        }
                    }
                }
                Some("export") => {
                    let dir = args.get(1).context("usage: elsewhen wiki export <目录>")?;
                    let report = wiki::export_wiki(&store, std::path::Path::new(dir))?;
                    println!("已导出 {} 个文件到 {}", report.files.len(), report.dir);
                    println!("注意：这是只读快照，真源在数据库；在快照里的修改不会被写回。");
                    for f in &report.files {
                        println!("  {}", f);
                    }
                }
                Some("log") => {
                    let log = store.list_wiki_log(20)?;
                    if log.is_empty() {
                        println!("wiki 尚无操作日志");
                    }
                    for (ts, entry) in log {
                        println!("{} {}", &ts[..19.min(ts.len())], entry);
                    }
                }
                Some("lint") => {
                    let issues = wiki::lint_wiki(&store)?;
                    if issues.is_empty() {
                        println!("wiki 健康，无问题。");
                    }
                    for i in &issues {
                        println!("⚠ {}", i);
                    }
                }
                _ => {
                    println!("usage:");
                    println!("  elsewhen wiki list [kind]");
                    println!("  elsewhen wiki show <slug>");
                    println!("  elsewhen wiki digest [--days N] [--dry-run] [--force]");
                    println!("  elsewhen wiki export <目录>");
                    println!("  elsewhen wiki log");
                    println!("  elsewhen wiki lint");
                }
            }
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
    println!("elsewhen insight [--days N] [--max-events N]");
    println!("elsewhen insights");
    println!("elsewhen wiki list|show|digest|export|log|lint");
    println!("elsewhen analyses");
    println!("elsewhen capture");
    println!("elsewhen daemon");
    println!("elsewhen settings");
    println!("elsewhen analyze-once");
    println!("elsewhen worker");
    println!("elsewhen providers");
}
