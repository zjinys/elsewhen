use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::path::PathBuf;

pub struct AppConfig {
    pub database_path: PathBuf,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        if let Ok(data_dir) = std::env::var("PERSONALD_DATA_DIR") {
            let data_dir = PathBuf::from(data_dir);
            std::fs::create_dir_all(&data_dir)
                .with_context(|| format!("create data directory {}", data_dir.display()))?;
            return Ok(Self {
                database_path: data_dir.join("events.db"),
            });
        }
        let dirs = ProjectDirs::from("dev", "elsewhen", "elsewhen")
            .context("unable to determine platform data directory")?;
        let data_dir = dirs.data_local_dir();
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("create data directory {}", data_dir.display()))?;
        Ok(Self {
            database_path: data_dir.join("events.db"),
        })
    }
}
