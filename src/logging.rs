//! 统一日志出口。
//!
//! 背景（issue 2026-10-31）：Android 上非 debuggable 的 app 进程，stderr 被
//! 指到 /dev/null，`eprintln!` 全部丢失 —— release 版在手机上的生成失败
//! 无法从 logcat 取证。本模块在 Android 上改走 `__android_log_print`
//! （tag = `elsewhen`，`adb logcat -s elsewhen` 可见），桌面平台仍走 stderr。
//!
//! 两档：
//! - [`log_always`]：渠道级生命周期日志（渠道名/错误类别/状态），不含用户内容，
//!   始终输出。
//! - [`log_debug`]：替代 `ELSEWHEN_DEBUG` 门控的 `eprintln!`，可能带个人数据，
//!   仅在开启调试时输出。

use std::sync::Once;

#[cfg(target_os = "android")]
const LOG_PRIORITY_INFO: i32 = 4;

#[cfg(target_os = "android")]
const LOG_TAG: &[u8] = b"elsewhen\0";

#[cfg(target_os = "android")]
unsafe extern "C" {
    fn __android_log_print(prio: i32, tag: *const u8, fmt: *const u8, ...) -> i32;
}

#[cfg(target_os = "android")]
fn android_log(msg: &str) {
    // __android_log_print 的 %s 占位：消息里可能出现 %，替换掉避免被当格式符。
    let sanitized = msg.replace('%', "%%");
    let Ok(cmsg) = std::ffi::CString::new(sanitized) else {
        return; // 内部 NUL 极罕见；丢弃好过 panic 在日志路径上
    };
    unsafe {
        __android_log_print(
            LOG_PRIORITY_INFO,
            LOG_TAG.as_ptr(),
            b"%s\0".as_ptr(),
            cmsg.as_ptr(),
        );
    }
}

/// 首次调用时装 panic 钩子：Android 上 panic 信息原本也进 stderr 黑洞，
/// 挂到 logcat 后崩溃才可取证。
fn install_panic_hook_once() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        #[cfg(target_os = "android")]
        {
            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                android_log(&format!("[panic] {info}"));
                default_hook(info);
            }));
        }
    });
}

fn write_line(msg: &str) {
    #[cfg(target_os = "android")]
    android_log(msg);
    #[cfg(not(target_os = "android"))]
    eprintln!("{msg}");
}

/// 渠道级生命周期日志：不含用户内容，始终输出（Android → logcat tag=elsewhen）。
pub fn log_always(msg: &str) {
    install_panic_hook_once();
    write_line(msg);
}

/// 调试日志：等价原 `ELSEWHEN_DEBUG` 门控的 `eprintln!`。
pub fn log_debug(msg: &str) {
    if !debug_enabled() {
        return;
    }
    install_panic_hook_once();
    write_line(msg);
}

/// 调试开关：环境变量（桌面常用）。Android 上进程环境无法注入，
/// 额外认数据目录里的标记文件（app 内可创建）。
pub fn debug_enabled() -> bool {
    static CACHE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| {
        if std::env::var_os("ELSEWHEN_DEBUG").is_some() {
            return true;
        }
        #[cfg(target_os = "android")]
        {
            // Android 无环境变量注入；退回平台默认数据目录查标记文件。
            if let Some(dirs) = directories::ProjectDirs::from("dev", "elsewhen", "elsewhen") {
                return dirs.data_local_dir().join("ELSEWHEN_DEBUG").exists();
            }
        }
        false
    })
}
