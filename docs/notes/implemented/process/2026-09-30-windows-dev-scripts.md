# Agent Note: Windows 开发脚本

Status: implemented

## Problem

`scripts/` 下原本只有 bash 脚本，唯一的 Windows 入口是 `package-msi.ps1`，而它只负责打包。
Windows 上做日常开发（跑起来、带诊断日志启动、重建 frb bridge、验证集成、打 APK、修旧库）
必须逐个手工翻译命令，而 `regen.sh` 那套 CPATH 与 cc/cxx shim 的前置条件在 Windows 上
并不显然成立。

CLAUDE.md 声明 CI 已在 Windows 上运行（`ci.yml`），说明 Windows 是受支持平台，开发循环
却没有对应的可执行入口——这是支持声明与工具链之间的缺口。

## Decision

为 7 个 `.sh` 各提供一个同名 `.ps1`，保持一一对应：

| `.sh` | `.ps1` | 关键移植点 |
|---|---|---|
| `elsewhen.sh` | `elsewhen.ps1` | `-d linux` → `-d windows`；保留 `--target` 必带的警告 |
| `elsewhen-capture.sh` | `elsewhen-capture.ps1` | 同上 |
| `debug-run.sh` | `debug-run.ps1` | 数据目录改 `%LOCALAPPDATA%`；`stat -c %s` → `(Get-Item).Length`；`tee -a` → `Tee-Object -Append` |
| `test_bridge.sh` | `test_bridge.ps1` | `libelsewhen.so` → `elsewhen.dll`；`ps aux` → `Get-Process`；数据库路径改为遵循 `ELSEWHEN_DATA_DIR` |
| `build-apk.sh` + `build-android-rust.sh` | `build-apk.ps1` | NDK host tag → `windows-x86_64`；NDK 版本排序改用 `[version]`（字符串序会把 `9.0.0` 排到 `25.2.9519653` 前） |
| `stamp-legacy-db.sh` | `stamp-legacy-db.ps1` | 结构指纹比对逻辑一字未改；`diff -u` → `git diff --no-index` |
| `regen.sh` | `regen.ps1` | 见下 |

Flutter 解析顺序统一为 `ui\.fvm\flutter_sdk\bin\flutter.bat` → 全局 `fvm` → 全局
`flutter`，与既有的 `package-msi.ps1` 一致。

### 全部 8 个 ps1 加 UTF-8 BOM

`package-msi.ps1` 及新增的 7 个原先都是无 BOM 的 UTF-8。Windows PowerShell 5.1
（`powershell.exe` 默认版本）把无 BOM 文件按系统 ANSI 代码页读取，中文注释与
`Write-Host` 输出会整体乱码。8 个文件现已统一带 BOM，PowerShell 5.1 与 7 均可正确读取。

这是对既有文件的修复，不是新脚本的约定。

### regen.ps1 与 regen.sh 的两处有意分歧

**clang 头文件改为探测而非硬失败。** `regen.sh` 第 1 步源于一个 Linux 专属问题：
ffigen 找不到 `stdbool.h`，把 cbindgen 生成的 `typedef bool (*DartPostCObjectFnType)(...)`
解析成 `typedef bool = ffi.NativeFunction<...>`，遮蔽 Dart 的 `bool` 类型。
该缺失是 Debian/Ubuntu 的 clang 打包问题，Windows 上通常不复现，所以 `regen.ps1`
在探测不到头文件时打印提示并继续，而不是 `exit 1`。同时**不实现 cc/cxx shim**：
cargo/cc-rs 对非 `.exe` 的 `CC` 有引号处理差异，这层封装在 Windows 上比 POSIX 脆弱得多。
`-ForceClangInclude` 可强制启用探测。

**新增 codegen hash 静态校验。** `regen.sh` 仅靠「脚本最后一步总是重编」来保证
Dart 侧 `rustContentHash` 与 Rust 侧 `FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH` 一致；
若 codegen 因工具版本不匹配而静默失效，会安静地产出一对不匹配的库，直到 app 启动时
`RustLib.init()` 抛 hash mismatch。`regen.ps1` 在收尾处直接正则读取两个源文件比对，
不一致就地抛错并说明处置。

## Alternatives considered

- **给全部 15 个脚本都写 Windows 版**：拒绝。`package-{deb,rpm,pacman,appimage,dmg}.sh`
  与 `install-linux-desktop.sh` 平台绑定，Windows 上本就不该存在；`flutter-wrapper.sh`
  硬编码了 `/home/pp/.fvm/versions/3.47.4` 某一台机器的路径，移植它等于把个人环境
  噪声复制一份。
- **用 Makefile 统一跨平台**：拒绝。根 `Makefile` 声明 `SHELL := /bin/sh`，在 Windows 上
  本就不可用；为它改造等于重写整个构建入口，远超需求。改为在 CLAUDE.md 里给出
  `.sh` / `.ps1` 对照表。
- **把 `regen.ps1` 的 clang 步骤照抄 `regen.sh`（缺就报错）**：拒绝。Windows 上大概率
  探测不到，那是必然失败的假阳性。改成探测到才启用，并在注释里写明该段可删。
- **只写 `regen.ps1` 一个**：拒绝。regen 是最痛的，但日常最高频的是启动和带日志启动。

## 未移植清单（有意为之）

`package-deb.sh` / `package-rpm.sh` / `package-pacman.sh` / `package-appimage.sh` /
`package-dmg.sh` / `install-linux-desktop.sh` / `flutter-wrapper.sh`，
以及根 `Makefile`（`/bin/sh`）。理由见上。

## Verification

**已验证：** 下载 PowerShell 7.4.6 linux-x64，用
`[System.Management.Automation.Language.Parser]::ParseFile` 解析 8 个脚本，全部通过；
以既有的 `package-msi.ps1` 作为对照确认解析器工作正常。PSScriptAnalyzer
（Error + Warning 级）初次运行报出 2 个真问题，均已修：

1. `PSUseBOMForUnicodeEncodedFile` × 8 —— 促成上面的 BOM 修复。
2. `PSUseDeclaredVarsMoreThanAssignments`（`regen.ps1` 的 `$shimDir`）——
   初稿遗留的死变量（最终决定不建 shim），已删。

复检结果：Error 0 findings，Warning 仅剩 `PSAvoidUsingWriteHost`（71 处）。
该规则针对非交互宿主，而这些是交互式构建脚本，且与既有 `package-msi.ps1` 的写法一致，
故保留。

**未验证：** 全部 8 个脚本的运行时行为。这些脚本依赖 Windows 专有工具链
（libclang、`sqlite3` CLI、WiX、Android NDK、`flutter`）与 Windows 路径布局，
在 Linux 上无法执行。语法与静态规则通过不等于运行正确——首次在 Windows 上使用时
应逐个实跑，重点是 `regen.ps1` 的 clang 探测分支和 `build-apk.ps1` 的 NDK 定位。

## Consequences

Windows 上的开发循环不再需要手工翻译命令，且与 bash 版逐个对应，切换平台时心智成本低。

代价是多维护 7 个脚本，且它们**没有 CI 覆盖**——`.github/workflows/ci.yml` 只跑
`cargo test` 与 `cargo build --release`，PowerShell 脚本不在门禁内。同一处逻辑改动
需要在两个文件里各改一次，这是双份脚本的固有成本，接受它换来的是平台可运行性。

`build-apk.ps1` 只编 arm64 这一既有约束在两个平台上一致：Rust 核心仅提供
arm64-v8a 的 `libelsewhen.so`，裁掉多余 ABI 可把 APK 从约 72MB 降到约 31MB。
