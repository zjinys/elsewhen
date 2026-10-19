//! 处理器接管协商与更新分发：handoff/yield/takeover、build_seq 与 APK 更新。

use super::*;

/// 本地构建序号（build.rs 注入的 git 提交计数；tarball 降级为 0）。
pub fn build_seq() -> i64 {
    env!("ELSEWHEN_BUILD_SEQ", "build.rs 未注入构建序号")
        .parse()
        .unwrap_or(0)
}

/// 检查应用更新。走与同步相同的 relay（同域名分发，附完整性校验）。
/// relayUrl 由 Dart 传入（设置页已有来源），本函数不依赖 vault 解锁。
pub fn check_app_update(relay_url: String) -> Result<AppUpdateDto> {
    let client = crate::sync::transport_relay::RelayClient::new(relay_url.trim())?;
    let manifest = client.fetch_release_manifest()?;
    let local_seq = build_seq();
    Ok(AppUpdateDto {
        has_update: manifest.build_seq > local_seq,
        latest_hash: manifest.build_hash,
        latest_seq: manifest.build_seq,
        local_hash: crate::api::build_hash(),
        local_seq,
        version: manifest.version,
        changelog: manifest.changelog,
        apk_size: manifest.apk_size,
        apk_sha256: manifest.apk_sha256,
    })
}

/// 下载更新 APK（destDir 由 Dart 用 path_provider 取缓存目录后传入），
/// 完成后本函数内部已通过 sha256 校验并原子改名；返回最终文件路径。
pub fn download_update_apk(
    relay_url: String,
    build_hash: String,
    apk_sha256: String,
    apk_size: i64,
    dest_dir: String,
) -> Result<String> {
    let client = crate::sync::transport_relay::RelayClient::new(relay_url.trim())?;
    let path = client.download_release_apk(
        build_hash.trim(),
        apk_sha256.trim(),
        apk_size,
        Path::new(dest_dir.trim()),
    )?;
    Ok(path.display().to_string())
}
