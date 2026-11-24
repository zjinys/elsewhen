# Agent Note: Android 构建适配固定 Flutter 3.50

Status: implemented

## Problem

固定的 Flutter 3.50.0 在 Android 打包时拒绝仓库的 AGP 9.1.0，最低要求 9.1.1，导致手机更新无法生成安装包。

## Decision

`ui/android/settings.gradle.kts` 将 Android Gradle Plugin 升至 9.1.1，保留 Gradle 9.3.1。该组合满足本机固定 Flutter SDK 的最低检查，不跳过依赖版本验证。

## Alternatives considered

使用 `--android-skip-build-dependency-validation`：会掩盖固定工具链和项目配置之间的差异，不作为构建修复。

一次升级所有 Android 工具链：超出此次手机更新需要，当前只补齐导致失败的 AGP 补丁版本。

## Consequences

当前源码成功生成 0.8.0+80 的 ARM64 release APK，包含三处对话的阅读位置修复。工具链未来升级提示及 keyboard_height_plugin 的 KGP 兼容警告仍存在，不代表已经解决未来版本兼容问题。

首次下载 Flutter 运行库直连缓慢；本机使用代理获取官方文件并校验服务端 MD5 `7d7aa285abdd01f7978aba894270fa63`，用 `/tmp` 临时 Maven 源提供相同运行库，不改变仓库依赖来源。临时源仅限定当前 Android 工程，避免影响 Flutter Gradle 插件的 included build。

## Verification

桥接 hash 1282260005 两侧一致；Android Rust release 编译通过。Gradle `assembleRelease` 成功，505 tasks（390 executed / 115 up-to-date）。APK 证书 SHA-256 与 `android-app-update/config.env` 一致，aapt2 核验 versionName 0.8.0、versionCode 80。ZIP 完整性通过，包内 Rust 库与当前构建产物逐字节一致。

手机重连后的首次覆盖尝试由签名检查中止：旧版 0.7.2+72 证书为 `b30bf370112f05d4ec71c078cc9ab425109debc5acd09f3ffddf9a91e8ef68bf`，与本机不同。用户随后明确授权卸载，旧版卸载成功并清除手机应用数据，再安装本机 0.8.0+80。安装脚本回读的 Rust 库与迁移清单一致；独立回读整包、Dart AOT、Rust SHA-256 均与构建产物一致，冷启动后进程存活。

原 APK、Rust 和 Dart AOT 校验值、设备安装记录保存在 `/tmp/elsewhen-android-update-20261119-verification.json`。后续[固定发布签名](2026-11-19-android-fixed-release-signing.md)保留当前证书，消除 CI 每次生成 debug 证书造成的覆盖升级问题。
