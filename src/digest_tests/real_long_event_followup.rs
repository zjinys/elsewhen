//! Finite follow-up acceptance of an existing synthetic complete root. Every
//! network step is a new process; old trials and their cost remain immutable.
use super::real_long_event::{Observed, config};
use super::*;
use crate::ai::provider::{FailoverEntry, FailoverProvider, OpenAiCompatibleProvider};
use serde_json::json;
use std::{
    io::{BufRead, Read, Write},
    process::Command,
};
const TEST: &str = "digest_tests::real_long_event_followup::real_model_cached_root_followup";

fn failing_channel() -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut socket = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "injected failing channel was not attempted"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(e) => panic!("{e}"),
            }
        };
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut reader = std::io::BufReader::new(socket.try_clone().unwrap());
        let mut size = 0;
        loop {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).unwrap() > 0);
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                size = value.trim().parse::<usize>().unwrap();
            }
        }
        assert!(size < 131072);
        let mut body = vec![0; size];
        reader.read_exact(&mut body).unwrap();
        let body = r#"{"error":{"message":"injected channel outage"}}"#;
        write!(socket,"HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    (url, worker)
}

#[test]
#[ignore = "explicit follow-up acceptance, at most four real synthetic requests"]
fn real_model_cached_root_followup() {
    if let Ok(dir) = std::env::var("ELSEWHEN_FOLLOWUP_CHILD_DIR") {
        let dir = PathBuf::from(dir);
        let store = Store::open(&dir.join("synthetic.db")).unwrap();
        store.recover_interrupted_digest_jobs().unwrap();
        let config = config();
        let model = config.model.clone();
        let observed = Observed {
            inner: OpenAiCompatibleProvider::new(config.clone()).unwrap(),
            trace: dir.join("requests.jsonl"),
        };
        let phase = std::env::var("ELSEWHEN_FOLLOWUP_PHASE").unwrap();
        let (provider, fixture): (Box<dyn AiProvider>, _) = if phase == "final" {
            let (url, worker) = failing_channel();
            let failed =
                OpenAiCompatibleProvider::new(crate::ai::provider::OpenAiCompatibleConfig {
                    base_url: url,
                    api_key: "synthetic-no-secrets".into(),
                    ..config
                })
                .unwrap();
            (
                Box::new(FailoverProvider::new(vec![
                    FailoverEntry::new("injected-outage", "injected local 503", Box::new(failed)),
                    FailoverEntry::new("real-model", "configured real model", Box::new(observed)),
                ])),
                Some(worker),
            )
        } else {
            (Box::new(observed), None)
        };
        let now = std::time::Instant::now();
        let tick = process_digest_queue(&store, provider.as_ref(), Some(&model)).unwrap();
        if let Some(worker) = fixture {
            worker.join().unwrap();
        }
        let result = json!({"phase":phase,"pid":std::process::id(),"tick":format!("{tick:?}"),"elapsed_ms":now.elapsed().as_millis()});
        std::fs::write(dir.join("tick.json"), result.to_string()).unwrap();
        return;
    }
    let source = PathBuf::from(
        std::env::var("ELSEWHEN_FOLLOWUP_SOURCE_DIR")
            .expect("explicit synthetic source directory required"),
    );
    assert!(
        source.starts_with(std::env::temp_dir())
            && source
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("elsewhen-real-long-event-")
    );
    let origin = Store::open(&source.join("synthetic.db")).unwrap();
    let events = origin.list_events().unwrap();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert!(
        event
            .raw_text
            .starts_with("START FACT: Project Aster has exactly 17 prototype units")
    );
    let task_id: String = origin
        .connection
        .query_row(
            "SELECT id FROM event_reading_tasks WHERE event_id=?1 AND status='ready'",
            [&event.id],
            |r| r.get(0),
        )
        .unwrap();
    let task = origin.event_reading_task(&task_id).unwrap();
    let root = origin.event_reading_root(&task).unwrap();
    let baseline = origin
        .event_reading_nodes(&task_id)
        .unwrap()
        .into_iter()
        .map(|n| (n.id, n.output))
        .collect::<Vec<_>>();
    let dir = std::env::temp_dir().join(format!("elsewhen-real-followup-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    origin
        .connection
        .execute(
            "VACUUM INTO ?1",
            [dir.join("synthetic.db").to_str().unwrap()],
        )
        .unwrap();
    let executable = dir.join("worker");
    std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let store = Store::open(&dir.join("synthetic.db")).unwrap();
    // Only an isolated acceptance clock is advanced. Node results and original
    // attempt history are preserved, while normal cooldown recovery runs.
    store.connection.execute_batch("UPDATE event_reading_tasks SET available_at=0;UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00Z';").unwrap();
    let mut ticks = vec![];
    let mut final_complete = false;
    for _ in 0..3 {
        let output = Command::new(&executable)
            .args(["--exact", TEST, "--ignored", "--nocapture"])
            .env("ELSEWHEN_FOLLOWUP_CHILD_DIR", &dir)
            .env("ELSEWHEN_FOLLOWUP_PHASE", "final")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "follow-up child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        ticks.push(
            serde_json::from_str::<serde_json::Value>(
                &std::fs::read_to_string(dir.join("tick.json")).unwrap(),
            )
            .unwrap(),
        );
        if store.digest_job_stats().unwrap().succeeded == 1 {
            final_complete = true;
            break;
        }
        store.connection.execute_batch("UPDATE event_reading_tasks SET available_at=0;UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00Z';").unwrap();
    }
    let mut short_complete = false;
    if final_complete {
        let short=store.insert_event(NewEvent::now("PUBLIC SYNTHETIC EVENT: Project Boreal has 5 test notebooks. Review is scheduled for 2026-12-08. No purchase is authorized.")).unwrap();
        store
            .set_event_recordability(&short, true, "synthetic latency comparison")
            .unwrap();
        store.connection.execute("UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00Z' WHERE event_id=?1",[&short]).unwrap();
        let output = Command::new(&executable)
            .args(["--exact", TEST, "--ignored", "--nocapture"])
            .env("ELSEWHEN_FOLLOWUP_CHILD_DIR", &dir)
            .env("ELSEWHEN_FOLLOWUP_PHASE", "short")
            .output()
            .unwrap();
        assert!(output.status.success());
        ticks.push(
            serde_json::from_str(&std::fs::read_to_string(dir.join("tick.json")).unwrap()).unwrap(),
        );
        short_complete = store.digest_job_stats().unwrap().succeeded == 2;
        assert!(store.event_reading_progress(&short).unwrap().is_none());
    }
    let rows = std::fs::read_to_string(dir.join("requests.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert!(rows.len() <= 4);
    let same_nodes = store
        .event_reading_nodes(&task_id)
        .unwrap()
        .into_iter()
        .map(|n| (n.id, n.output))
        .collect::<Vec<_>>()
        == baseline;
    let pages=store.list_wiki_pages(None,None).unwrap().iter().map(|p|json!({"slug":p.slug,"content":p.content_md,"sources":p.source_event_ids,"evidence_count":p.evidence_count})).collect::<Vec<_>>();
    let recheck = process_digest_queue(
        &store,
        &Observed {
            inner: OpenAiCompatibleProvider::new(config()).unwrap(),
            trace: dir.join("requests.jsonl"),
        },
        None,
    )
    .unwrap();
    assert!(
        matches!(recheck, DigestTick::Idle),
        "completed jobs should not re-run"
    );
    let report = json!({"parent_trial":source.file_name().unwrap().to_string_lossy(),"real_call_limit":4,"real_calls":rows.len(),"final_complete":final_complete,"short_complete":short_complete,"root_id":root.id,"unchanged_reading_nodes":same_nodes,"original_event_id":event.id,"original_chars":event.raw_text.chars().count(),"ticks":ticks,"requests":rows,"pages":pages,"dataset_output_policy":store.knowledge_language_policy().unwrap().output_locale});
    std::fs::write(
        dir.join("report.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("followup_artifacts={}", dir.display());
    assert!(same_nodes);
    assert_eq!(
        store
            .list_events()
            .unwrap()
            .iter()
            .find(|e| e.id == event.id)
            .unwrap()
            .raw_text,
        event.raw_text
    );
    assert!(
        final_complete && short_complete,
        "follow-up incomplete; inspect {}",
        dir.display()
    );
    assert_eq!(
        store.event_reading_task(&task_id).unwrap().status,
        "succeeded"
    );
}
