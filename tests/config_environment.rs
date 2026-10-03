//! Real environment tests run in single-threaded child processes, not libtest workers.
use anyhow::{Context, Result, bail};
use elsewhen::config::AppConfig;
use std::path::{Path, PathBuf};
use std::process::Command;

const CASES: &[(&str, fn(&Path))] = &[
    (
        "pin_prefers_explicit_dir_over_env",
        pin_prefers_explicit_dir_over_env,
    ),
    (
        "load_keeps_pinned_dir_after_env_changes",
        load_keeps_pinned_dir_after_env_changes,
    ),
    (
        "pin_is_idempotent_for_same_dir",
        pin_is_idempotent_for_same_dir,
    ),
    (
        "pin_rejects_switching_to_another_dir",
        pin_rejects_switching_to_another_dir,
    ),
    (
        "load_without_pin_follows_env_each_time",
        load_without_pin_follows_env_each_time,
    ),
];

struct TestDir(PathBuf);

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--case") {
        let name = args.get(1).context("missing test case")?;
        let root = Path::new(args.get(2).context("missing temporary directory")?);
        let (_, run) = CASES
            .iter()
            .find(|(case, _)| case == name)
            .context("unknown test case")?;
        // Refuse to run against an inherited personal data directory.
        assert_eq!(
            PathBuf::from(std::env::var_os("ELSEWHEN_DATA_DIR").unwrap()),
            root.join("from-env")
        );
        run(root);
        return Ok(());
    }

    // Keep the common cargo test filters usable with this harness-free target.
    let mut filter = None;
    let mut exact = false;
    let mut list = false;
    let mut ignored_only = false;
    let mut skipped = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--exact" => exact = true,
            "--list" => list = true,
            "--ignored" => ignored_only = true,
            "--skip" => skipped.push(args.next().context("missing skip filter")?.as_str()),
            "--test-threads" => {
                args.next().context("missing thread count")?;
            }
            "--nocapture" | "--no-capture" | "--show-output" | "--include-ignored" | "--quiet"
            | "-q" => {}
            value if value.starts_with("--test-threads=") => {}
            value if !value.starts_with('-') && filter.is_none() => filter = Some(value),
            value => bail!("unsupported config_environment test argument: {value}"),
        }
    }

    let exe = std::env::current_exe()?;
    let mut passed = 0;
    for (name, _) in CASES {
        if ignored_only
            || filter.is_some_and(|filter| {
                if exact {
                    *name != filter
                } else {
                    !name.contains(filter)
                }
            })
            || skipped.iter().any(|skip| name.contains(skip))
        {
            continue;
        }
        if list {
            println!("{name}: test");
            continue;
        }
        let root =
            TestDir(std::env::temp_dir().join(format!("elsewhen-config-{}", uuid::Uuid::new_v4())));
        std::fs::create_dir(&root.0)?;
        let output = Command::new(&exe)
            .arg("--case")
            .arg(name)
            .arg(&root.0)
            .env("ELSEWHEN_DATA_DIR", root.0.join("from-env"))
            .output()
            .with_context(|| format!("run {name}"))?;
        if !output.status.success() {
            bail!(
                "{name} failed ({}):\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        println!("test {name} ... ok");
        passed += 1;
    }
    if !list {
        println!("config environment tests: {passed} passed");
    }
    Ok(())
}

fn pin_prefers_explicit_dir_over_env(root: &Path) {
    let explicit = root.join("explicit");
    let cfg = AppConfig::pin(Some(explicit.to_str().unwrap())).unwrap();
    assert_eq!(cfg.data_dir, explicit);
    assert_eq!(cfg.database_path, explicit.join("elsewhen.db"));
}

fn load_keeps_pinned_dir_after_env_changes(root: &Path) {
    let pinned = root.join("from-env");
    let cfg = AppConfig::pin(None).unwrap();
    assert_eq!(cfg.data_dir, pinned);

    // SAFETY: harness=false runs this case on the child process's main thread.
    // This process never starts other threads; only synchronous config/filesystem calls run.
    unsafe { std::env::set_var("ELSEWHEN_DATA_DIR", root.join("after-pinned")) };
    assert_eq!(AppConfig::load().unwrap().data_dir, pinned);
}

fn pin_is_idempotent_for_same_dir(root: &Path) {
    let dir = root.join("from-env");
    let first = AppConfig::pin(Some(dir.to_str().unwrap())).unwrap();
    let second = AppConfig::pin(Some(dir.to_str().unwrap())).unwrap();
    assert_eq!(first.data_dir, second.data_dir);
    assert_eq!(first.database_path, second.database_path);
}

fn pin_rejects_switching_to_another_dir(root: &Path) {
    let first = root.join("from-env");
    AppConfig::pin(None).unwrap();

    let other = root.join("switch-b");
    let err = AppConfig::pin(Some(other.to_str().unwrap())).unwrap_err();
    assert!(
        err.to_string().contains("不能再改指向"),
        "错误信息应说明冲突，实际: {err}"
    );
    assert_eq!(AppConfig::load().unwrap().data_dir, first);
}

fn load_without_pin_follows_env_each_time(root: &Path) {
    let first = root.join("from-env");
    assert_eq!(AppConfig::load().unwrap().data_dir, first);

    let second = root.join("unpinned-b");
    // SAFETY: as above, this isolated harness-free child never starts other threads.
    unsafe { std::env::set_var("ELSEWHEN_DATA_DIR", &second) };
    assert_eq!(AppConfig::load().unwrap().data_dir, second);
}
