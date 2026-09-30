# build-apk.ps1 - 一键编译 Android APK（先编 Rust 核心 arm64，再 flutter build apk）
#
# 与 build-apk.sh + build-android-rust.sh 等价；两者在 .sh 里本来就是
# 前者调后者，这里合成一个 PowerShell 脚本。
#
# 用法：
#   powershell -File scripts\build-apk.ps1              # release APK
#   powershell -File scripts\build-apk.ps1 --debug      # debug APK
#   powershell -File scripts\build-apk.ps1 --split-per-abi   # flutter 参数原样透传
#
# 依赖：
#   rustup target add aarch64-linux-android
#   Android SDK/NDK（ANDROID_HOME 或 ANDROID_SDK_ROOT；ndk 在 $ANDROID_HOME\ndk\<ver>\）
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$FlutterArgs
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Push-Location $root

try {
    # ---------- [1/2] 编译 Rust 核心（android arm64）----------
    Write-Host "==> [1/2] 编译 Rust 核心 (android arm64)"

    $sdkRoot = if ($env:ANDROID_HOME) { $env:ANDROID_HOME }
               elseif ($env:ANDROID_SDK_ROOT) { $env:ANDROID_SDK_ROOT }
               else { Join-Path $env:LOCALAPPDATA "Android\Sdk" }

    if (-not $env:ANDROID_NDK_HOME) {
        $ndkBase = Join-Path $sdkRoot "ndk"
        if (-not (Test-Path $ndkBase)) {
            throw "找不到 Android NDK（$ndkBase\）。请用 SDK Manager 安装 NDK。"
        }
        # 取已安装的最新 NDK。按 [version] 排序，不能用字符串序：
        # "9.0.0" 会排在 "25.2.9519653" 前面。解析失败的目录名退到末尾。
        $ndkDirs = Get-ChildItem -Path $ndkBase -Directory |
            Sort-Object -Property @{ Expression = {
                try { [version]$_.Name } catch { [version]"0.0" } } }, @{ Expression = { $_.Name } } -Descending
        if (-not $ndkDirs) { throw "找不到 Android NDK（$ndkBase\）。请用 SDK Manager 安装 NDK。" }
        $ndkHome = $ndkDirs[0].FullName
    } else {
        $ndkHome = $env:ANDROID_NDK_HOME
    }

    # 与 build-android-rust.sh 的差异点：NDK 预编译目录在 Windows 上是
    # windows-x86_64（Linux 为 linux-x86_64、macOS 为 darwin-x86_64）。
    # NDK 不发 windows-aarch64 工具链，ARM 版 Windows 也用这个目录（走模拟）。
    $hostTag = "windows-x86_64"
    $toolchainBin = Join-Path $ndkHome "toolchains\llvm\prebuilt\$hostTag\bin"
    if (-not (Test-Path $toolchainBin)) {
        throw "找不到 NDK toolchain：$toolchainBin"
    }

    # minSdk 21（flutter.minSdkVersion）；NDK r25+ 的 clang 前缀直接用 API level
    $api = 21
    $triple = "aarch64-linux-android"

    $env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = Join-Path $toolchainBin "${triple}${api}-clang"
    $env:AR_aarch64_linux_android = Join-Path $toolchainBin "llvm-ar.exe"
    $env:CC_aarch64_linux_android = Join-Path $toolchainBin "${triple}${api}-clang.exe"

    cargo build --release --target $triple
    if ($LASTEXITCODE -ne 0) { throw "cargo build --target $triple 失败" }

    # 注意：产物始终是 libelsewhen.so —— 目标平台是 Android，与宿主无关。
    $rustLibDir = Join-Path $root "ui\rustLibs\arm64-v8a"
    New-Item -ItemType Directory -Force -Path $rustLibDir | Out-Null
    $artifact = Join-Path $root "target\$triple\release\libelsewhen.so"
    if (-not (Test-Path $artifact)) { throw "未找到编译产物 $artifact" }
    Copy-Item $artifact $rustLibDir -Force
    & (Join-Path $toolchainBin "llvm-strip.exe") (Join-Path $rustLibDir "libelsewhen.so")
    if ($LASTEXITCODE -ne 0) { throw "llvm-strip 失败" }

    # ---------- [2/2] flutter build apk ----------
    Write-Host "==> [2/2] flutter build apk (仅 android-arm64)"

    $flutter = if (Test-Path "ui\.fvm\flutter_sdk\bin\flutter.bat") {
        "ui\.fvm\flutter_sdk\bin\flutter.bat"
    } elseif (Get-Command fvm -ErrorAction SilentlyContinue) {
        "fvm"
    } else {
        (Get-Command flutter -ErrorAction Stop).Source
    }

    Push-Location "ui"
    try {
        & $flutter pub get
        if ($LASTEXITCODE -ne 0) { throw "flutter pub get 失败" }

        # 只编 arm64：Rust 核心只提供 arm64-v8a 的 libelsewhen.so，且裁掉多余 ABI
        # 可把 APK 从 ~72MB 降到 ~31MB。main.dart 是默认目标（无 nativeapi，
        # 避免 Android release AOT 崩溃），这里无需 --target。
        if ($FlutterArgs -and $FlutterArgs.Count -gt 0) {
            & $flutter build apk --target-platform android-arm64 @FlutterArgs
        } else {
            & $flutter build apk --release --target-platform android-arm64
        }
        if ($LASTEXITCODE -ne 0) { throw "flutter build apk 失败" }
    } finally {
        Pop-Location
    }

    Write-Host ""
    Write-Host "==> 完成，APK 位置："
    Get-ChildItem -Path "ui\build\app\outputs\flutter-apk" -Filter "*.apk" -ErrorAction SilentlyContinue |
        ForEach-Object { Write-Host ("  {0,10}  {1}" -f $_.Length, $_.Name) }
} finally {
    Pop-Location
}
