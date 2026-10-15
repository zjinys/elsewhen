use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// 数据库统一文件名（曾用 events.db，改名后首次启动自动迁移）
const DB_FILE_NAME: &str = "elsewhen.db";
const LEGACY_DB_FILE_NAME: &str = "events.db";

/// 进程内钉死的数据目录，由 `AppConfig::pin` 在 `init_bridge` 时写入一次。
///
/// 此前库路径是「每个 API 调用重读 `ELSEWHEN_DATA_DIR`」的进程级可变全局
/// （`init_bridge` 用 `std::env::set_var` 写、每个 API 函数用 `AppConfig::load`
/// 重读），带来两个问题：
///
/// 1. **运行时可改写数据源**。`set_var` 是进程全局，任何代码设一下这个环境变量就能把
///    整个应用的数据目录悄悄指到别处，且下一个调用立刻生效。
/// 2. **集成测试互相串库**。Flutter 集成测试靠「每个 suite 传自己的临时目录」隔离，
///    但 `flutter test` 把所有测试文件作为 isolate 跑在**同一个 `flutter_tester`
///    进程**里，于是并发 suite 互相覆盖这个全局变量、打开同一个 SQLite 文件，表现为
///    随机的 `SQLITE_BUSY`（"database is locked"）且每次失败的用例都不同。
///
/// 钉死之后库路径在进程内唯一且不可改写：问题 1 消失；问题 2 变成 `pin` 里的**显式
/// 报错**，把「一个进程内无法隔离多个 suite」这个真实约束摆到台面上，而不是随机 BUSY。
static PINNED: RwLock<Option<AppConfig>> = RwLock::new(None);

fn read_pinned() -> Option<AppConfig> {
    // 锁中毒（持有期间 panic）不阻断后续调用，取回内层值继续即可。
    PINNED.read().unwrap_or_else(|e| e.into_inner()).clone()
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    /// 本机工作区根目录，用于进程钉死与数据集目录定位；不能在进程内切换。
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
}

impl AppConfig {
    /// 数据目录优先级：显式环境变量 > 平台默认目录。
    fn resolve_data_dir() -> Result<PathBuf> {
        if let Ok(data_dir) = std::env::var("ELSEWHEN_DATA_DIR") {
            return Ok(PathBuf::from(data_dir));
        }
        let dirs = ProjectDirs::from("dev", "elsewhen", "elsewhen")
            .context("unable to determine platform data directory")?;
        Ok(dirs.data_local_dir().to_path_buf())
    }

    /// 建目录 + 收紧权限 + 旧库文件迁移。钉死后只在 `pin` 里跑一次。
    fn build(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("create data directory {}", data_dir.display()))?;
        secure_directory(data_dir)?;
        let database_path = match crate::sync::datasets::selected_database_path(data_dir)? {
            Some(path) => path,
            None => migrate_db_file(data_dir)?,
        };
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            database_path,
        })
    }

    pub fn load() -> Result<Self> {
        match read_pinned() {
            Some(cfg) => Ok(cfg),
            // 未钉死时保持旧行为：按当前环境变量解析，且**不**写入全局
            None => Self::build(&Self::resolve_data_dir()?),
        }
    }

    /// `init_bridge` 用：把数据目录钉死在进程里，之后所有调用复用同一份。
    ///
    /// 语义是**先到先得 + 冲突即报错**：
    /// - 未钉死 → 解析（显式路径优先于环境变量）并钉死；
    /// - 已钉死且与显式路径一致 → 幂等返回；
    /// - 已钉死且与显式路径不同 → 报错。库路径是进程级唯一资源，半途改指向会让已
    ///   打开的连接与后续调用指向不同文件（SQLite 报 "database is locked"）；
    ///   与其静默共库或随机 BUSY，不如在这里把话说清楚。
    pub fn pin(explicit_data_dir: Option<&str>) -> Result<Self> {
        if let Some(cfg) = read_pinned() {
            if let Some(dir) = explicit_data_dir {
                let want = PathBuf::from(dir);
                if cfg.data_dir != want {
                    anyhow::bail!(
                        "数据目录已在本进程初始化为 {}，不能再改指向 {}",
                        cfg.data_dir.display(),
                        want.display()
                    );
                }
            }
            return Ok(cfg);
        }

        let data_dir = match explicit_data_dir {
            Some(dir) => PathBuf::from(dir),
            None => Self::resolve_data_dir()?,
        };
        let cfg = Self::build(&data_dir)?;
        *PINNED.write().unwrap_or_else(|e| e.into_inner()) = Some(cfg.clone());
        Ok(cfg)
    }
}

/// 仅测试内用：清掉钉死状态，让下一个数据目录作用域重新解析。
///
/// 必需：测试里 `DataDirGuard` 按用例钉死临时目录；不清掉钉死值，后跑的用例
/// 会被拒绝切换目录。环境变量行为另由独立进程测试覆盖。
#[cfg(test)]
pub fn unpin() {
    *PINNED.write().unwrap_or_else(|e| e.into_inner()) = None;
}

/// 测试内串行化「会改动进程级数据目录」的用例。
///
/// 钉死值是进程级状态，而 Rust 测试默认并行跑在同一进程。凡是碰钉死值的用例
/// 都必须持这把锁；它不能保证修改进程环境变量的线程安全。
#[cfg(test)]
pub fn test_env_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// 数据库文件名统一为 elsewhen.db。首次启动时若发现旧文件 events.db，
/// 自动重命名迁过去（含 -wal / -shm 副产物），保证已有数据不丢。
fn migrate_db_file(data_dir: &Path) -> Result<PathBuf> {
    let new_path = data_dir.join(DB_FILE_NAME);
    let legacy_path = data_dir.join(LEGACY_DB_FILE_NAME);
    if !new_path.exists() && legacy_path.exists() {
        // 主文件迁移失败必须上抛：否则调用方会继续用「不存在的新路径」建出
        // 一个空库，legacy 数据从此滞留丢失。
        std::fs::rename(&legacy_path, &new_path).with_context(|| {
            format!(
                "migrate database {} -> {}",
                legacy_path.display(),
                new_path.display()
            )
        })?;
        for suffix in ["-wal", "-shm"] {
            let legacy_side = data_dir.join(format!("{LEGACY_DB_FILE_NAME}{suffix}"));
            if legacy_side.exists() {
                // WAL 里可能还有未 checkpoint 的已提交事务，副产物迁移失败同样上抛
                std::fs::rename(
                    &legacy_side,
                    data_dir.join(format!("{DB_FILE_NAME}{suffix}")),
                )
                .with_context(|| {
                    format!(
                        "migrate database sidecar {suffix} {}",
                        legacy_side.display()
                    )
                })?;
            }
        }
        eprintln!(
            "migrated database {} -> {}",
            legacy_path.display(),
            new_path.display()
        );
    }
    Ok(new_path)
}

/// Unix 权限收紧的统一入口：Android 应用沙箱目录天然只允许本应用访问，
/// 且对外部存储路径 chmod 会 EPERM 直接让 init_bridge 失败，因此 Android
/// 一律跳过；桌面/服务器平台才需要显式收紧。
#[cfg(all(unix, not(target_os = "android")))]
pub(crate) fn set_unix_permissions(path: &std::path::Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(target_os = "android")]
pub(crate) fn set_unix_permissions(_path: &std::path::Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

fn secure_directory(path: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    set_unix_permissions(path, 0o700)?;
    Ok(())
}
