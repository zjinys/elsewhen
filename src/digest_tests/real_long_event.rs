//! Explicit opt-in acceptance: public synthetic text, isolated SQLite, finite
//! paid calls. Each tick runs in a fresh OS process to exercise restart reuse.
use super::*;
use crate::ai::provider::{OpenAiCompatibleConfig, OpenAiCompatibleProvider, RequestFit};
use serde_json::json;
use std::{collections::HashMap, io::Write, process::Command};
const TEST: &str = "digest_tests::real_long_event::real_model_long_event_acceptance";
const WINDOW: u32 = 4096;
const RESERVE: u32 = 1536;

pub(super) fn config() -> OpenAiCompatibleConfig {
    let text =
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".env"))
            .unwrap_or_default();
    let values: HashMap<_, _> = text
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim(), v.trim().trim_matches(['\'', '"'])))
        .collect();
    let value = |key: &str| {
        std::env::var(key)
            .ok()
            .or_else(|| values.get(key).map(|v| v.to_string()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| panic!("missing acceptance configuration {key}"))
    };
    OpenAiCompatibleConfig {
        api_key: value("ELSEWHEN_AI_API_KEY"),
        model: value("ELSEWHEN_AI_MODEL"),
        base_url: value("ELSEWHEN_AI_BASE_URL"),
        temperature: 0.1,
        max_tokens: Some(RESERVE),
        context_window: Some(WINDOW),
    }
}
pub(super) struct Observed {
    pub(super) inner: OpenAiCompatibleProvider,
    pub(super) trace: PathBuf,
}
impl AiProvider for Observed {
    fn fits_request(&self, m: &[ContextMessage]) -> Result<bool> {
        self.inner.fits_request(m)
    }
    fn visit_request_budgets(
        &self,
        v: &mut dyn FnMut(&RequestFit<'_>) -> Result<()>,
    ) -> Result<()> {
        self.inner.visit_request_budgets(v)
    }
    fn generate_reply_with_tools(
        &self,
        m: Vec<ContextMessage>,
        t: Option<&[ToolSpec]>,
    ) -> Result<AiReply> {
        anyhow::ensure!(
            self.fits_request(&m)?,
            "acceptance actual request exceeds budget"
        );
        let start = std::time::Instant::now();
        let reply = self.inner.generate_reply_with_tools(m.clone(), t);
        let row = match &reply {
            Ok(r) => {
                json!({"model":r.model,"elapsed_ms":start.elapsed().as_millis(),"messages":m.iter().map(|m|json!({"role":m.role,"content":m.content})).collect::<Vec<_>>(),"output":r.content,"finish_reason":r.finish_reason,"usage":r.usage.as_ref().map(|u|json!({"prompt":u.prompt_tokens,"completion":u.completion_tokens,"total":u.total_tokens}))})
            }
            Err(_) => json!({"elapsed_ms":start.elapsed().as_millis(),"transport_failed":true}),
        };
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.trace)?;
        writeln!(file, "{row}")?;
        reply
    }
}

#[test]
#[ignore = "explicit real-model acceptance, uses finite paid synthetic requests"]
fn real_model_long_event_acceptance() {
    if let Ok(dir) = std::env::var("ELSEWHEN_ACCEPTANCE_CHILD_DIR") {
        let path = PathBuf::from(dir);
        let store = Store::open(&path.join("synthetic.db")).unwrap();
        store.recover_interrupted_digest_jobs().unwrap();
        let config = config();
        let model = config.model.clone();
        let provider = Observed {
            inner: OpenAiCompatibleProvider::new(config).unwrap(),
            trace: path.join("requests.jsonl"),
        };
        let before: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM event_reading_nodes WHERE status='succeeded'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let tick = process_digest_queue(&store, &provider, Some(&model)).unwrap();
        let after: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM event_reading_nodes WHERE status='succeeded'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(after - before <= 1);
        std::fs::write(
            path.join("tick.json"),
            json!({"tick":format!("{tick:?}"),"completed_before":before,"completed_after":after})
                .to_string(),
        )
        .unwrap();
        return;
    }
    let config = config();
    let resume = std::env::var("ELSEWHEN_ACCEPTANCE_RESUME_DIR").ok();
    let dir = resume.as_ref().map(PathBuf::from).unwrap_or_else(|| {
        std::env::temp_dir().join(format!("elsewhen-real-long-event-{}", uuid::Uuid::new_v4()))
    });
    assert!(
        dir.starts_with(std::env::temp_dir())
            && dir
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("elsewhen-real-long-event-")
    );
    std::fs::create_dir_all(&dir).unwrap();
    // Keep a stable worker binary while independent cargo checks replace the
    // usual test executable. Each child is still a fresh OS process.
    let executable = dir.join("acceptance-worker");
    std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let store = Store::open(&dir.join("synthetic.db")).unwrap();
    if resume.is_none() {
        let policy = store.knowledge_language_policy().unwrap();
        store
            .set_knowledge_language("en", policy.generation)
            .unwrap();
        store
            .save_prompt_language_preferences(Some("en"), "auto", None, 0)
            .unwrap();
    }
    let filler=(0..240).map(|i|format!("Archive line {i:03}: neutral background about reviewing public documentation, organizing notes, and checking mechanical details. This paragraph reports no decision, expense, purchase, deadline, or authorization.\n")).collect::<String>();
    let original = format!(
        "START FACT: Project Aster has exactly 17 prototype units as of 2026-11-09. The project is synthetic.\n{filler}MIDDLE FACT: On 2026-11-15, the Aster team did NOT authorize purchase of 42 sensors. Budget approval remains pending.\n{filler}TAIL FACT: The Aster review deadline is 2026-12-01. Only 3 validation trials are scheduled; no shipment has been authorized."
    );
    let event = if resume.is_some() {
        let events = store.list_events().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].raw_text, original);
        events[0].id.clone()
    } else {
        let id = store.insert_event(NewEvent::now(&original)).unwrap();
        store
            .set_event_recordability(&id, true, "public synthetic acceptance")
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00Z'",
                [],
            )
            .unwrap();
        std::fs::write(dir.join("original.txt"), &original).unwrap();
        id
    };
    if resume.is_some() {
        store.connection.execute_batch("UPDATE event_reading_tasks SET available_at=0; UPDATE event_reading_nodes SET available_at=0 WHERE status='pending'; UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00Z' WHERE status='retry';").unwrap();
    }
    let mut ticks = vec![];
    let mut complete = false;
    let mut changed = resume.is_some();
    let mut failures = 0;
    for index in 0..32 {
        let used = std::fs::read_to_string(dir.join("requests.jsonl"))
            .unwrap_or_default()
            .lines()
            .count();
        if used >= 32 {
            break;
        }
        let output = Command::new(&executable)
            .args(["--exact", TEST, "--ignored", "--nocapture"])
            .env("ELSEWHEN_ACCEPTANCE_CHILD_DIR", &dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "acceptance child failed; artifacts at {}",
            dir.display()
        );
        let tick: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("tick.json")).unwrap()).unwrap();
        ticks.push(tick);
        let request_count = std::fs::read_to_string(dir.join("requests.jsonl"))
            .unwrap_or_default()
            .lines()
            .count();
        assert!(request_count <= 32);
        let completed: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM event_reading_nodes WHERE status='succeeded'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        if completed > 0 && !changed {
            let pref = store.prompt_language_preferences().unwrap();
            store
                .save_prompt_language_preferences(
                    Some("zh-Hans"),
                    "fixed",
                    Some("zh-Hans"),
                    pref.generation,
                )
                .unwrap();
            changed = true;
        }
        if store.digest_job_stats().unwrap().succeeded == 1 {
            complete = true;
            break;
        }
        let text = ticks.last().unwrap()["tick"].as_str().unwrap();
        if text.contains("failed") || text.starts_with("Failed") {
            failures += 1;
            if failures >= 3 {
                break;
            }
            // The synthetic acceptance advances only its isolated retry clock.
            // A new OS process still performs each bounded model node attempt.
            store.connection.execute_batch("UPDATE event_reading_tasks SET available_at=0; UPDATE event_reading_nodes SET available_at=0 WHERE status='pending'; UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00Z' WHERE status='retry';").unwrap();
        } else if text == "Idle" {
            break;
        }
        assert!(
            index < 31,
            "finite acceptance tick limit reached; {}",
            dir.display()
        );
    }
    let pages = store.list_wiki_pages(None, None).unwrap();
    let artifacts = pages
        .iter()
        .map(|p| json!({"slug":p.slug,"content":p.content_md,"sources":p.source_event_ids}))
        .collect::<Vec<_>>();
    let output = json!({"provider_host":reqwest::Url::parse(&config.base_url).unwrap().host_str(),"model":config.model,"context_window":WINDOW,"output_reserve":RESERVE,"actual_request_limit":32,"complete":complete,"settings_changed_after_first_node":changed,"ticks":ticks,"pages":artifacts});
    std::fs::write(
        dir.join("report.json"),
        serde_json::to_string_pretty(&output).unwrap(),
    )
    .unwrap();
    println!("acceptance_artifacts={}", dir.display());
    assert_eq!(store.list_events().unwrap()[0].raw_text, original);
    assert!(
        complete,
        "real model did not complete; inspect report.json, no automatic retry loop"
    );
    let combined = pages
        .iter()
        .map(|p| p.content_md.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for expected in ["17", "42", "2026-12-01", "3"] {
        assert!(
            combined.contains(expected),
            "real model omitted expected fact {expected}"
        );
    }
    assert!(
        pages
            .iter()
            .all(|p| p.source_event_ids == vec![event.clone()])
    );
    let quoted:i64=store.connection.query_row("SELECT COUNT(*) FROM event_reading_nodes WHERE json_array_length(json_extract(output_json,'$.quotes'))>0",[],|r|r.get(0)).unwrap();
    assert!(quoted > 0, "real model provided no verifiable quotes");
}
