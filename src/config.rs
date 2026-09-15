use anyhow::{Context, Result};
use directories::ProjectDirs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// 数据库统一文件名（曾用 events.db，改名后首次启动自动迁移）
const DB_FILE_NAME: &str = "elsewhen.db";
const LEGACY_DB_FILE_NAME: &str = "events.db";

pub struct AppConfig {
    pub database_path: PathBuf,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        if let Ok(data_dir) = std::env::var("ELSEWHEN_DATA_DIR") {
            let data_dir = PathBuf::from(data_dir);
            std::fs::create_dir_all(&data_dir)
                .with_context(|| format!("create data directory {}", data_dir.display()))?;
            secure_directory(&data_dir)?;
            return Ok(Self {
                database_path: migrate_db_file(&data_dir),
            });
        }
        let dirs = ProjectDirs::from("dev", "elsewhen", "elsewhen")
            .context("unable to determine platform data directory")?;
        let data_dir = dirs.data_local_dir();
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("create data directory {}", data_dir.display()))?;
        secure_directory(data_dir)?;
        Ok(Self {
            database_path: migrate_db_file(data_dir),
        })
    }
}

/// 数据库文件名统一为 elsewhen.db。首次启动时若发现旧文件 events.db，
/// 自动重命名迁过去（含 -wal / -shm 副产物），保证已有数据不丢。
fn migrate_db_file(data_dir: &Path) -> PathBuf {
    let new_path = data_dir.join(DB_FILE_NAME);
    let legacy_path = data_dir.join(LEGACY_DB_FILE_NAME);
    if !new_path.exists() && legacy_path.exists() {
        match std::fs::rename(&legacy_path, &new_path) {
            Ok(()) => {
                for suffix in ["-wal", "-shm"] {
                    let legacy_side = data_dir.join(format!("{LEGACY_DB_FILE_NAME}{suffix}"));
                    if legacy_side.exists() {
                        let _ = std::fs::rename(
                            &legacy_side,
                            data_dir.join(format!("{DB_FILE_NAME}{suffix}")),
                        );
                    }
                }
                eprintln!(
                    "migrated database {} -> {}",
                    legacy_path.display(),
                    new_path.display()
                );
            }
            Err(e) => {
                eprintln!(
                    "failed to migrate {} to {}: {e}",
                    legacy_path.display(),
                    new_path.display()
                );
            }
        }
    }
    new_path
}

fn secure_directory(path: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}
