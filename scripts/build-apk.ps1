# build-apk.ps1 - 一键编译 Android APK（先编 Rust 核心 arm64，再 flutter build apk）
#
# 与 build-apk.sh + build-android-rust.sh 等价；两者在 .sh 里本来就是
# 前者调后者，这里合成一个 PowerShell 脚本。
#
# 注意：本脚本不跑 regen.ps1。第 0 步会比对两侧桥接 hash，改了 Rust 公开 API
# 却漏跑 regen 时直接报错，避免 APK 装上去在 RustLib.init() 才炸。
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
    # ---------- [0/3] 桥接代码 hash 预检 ----------
    # 改了 Rust 公开 API 却只跑本脚本时，Dart 侧桥接代码还是旧的：编译能过，
    # 装到手机上要到 RustLib.init() 才报 hash mismatch（且报错现象与根因相距
    # 很远）。这里提前拦下，指向 regen.ps1。
    $dartFrb = Join-Path $root "ui\lib\bridge\generated.dart\frb_generated.dart"
    $rustFrb = Join-Path $root "src\frb_generated.rs"

    $dartHash = $null
    $rustHash = $null
    try {
        # `-?`：content hash 是 i32，负数很常见（当前 -854730017），
        # 不带符号时预检会直接判定「读不到桥接 hash」而拦下每一次构建。
        $m = Select-String -Path $dartFrb -Pattern 'rustContentHash\s*=>\s*(-?\d+)' |
            Select-Object -First 1
        if ($m) { $dartHash = $m.Matches[0].Groups[1].Value }
        $m = Select-String -Path $rustFrb -Pattern 'FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH:\s*i32\s*=\s*(-?\d+)' |
            Select-Object -First 1
        if ($m) { $rustHash = $m.Matches[0].Groups[1].Value }
    } catch {
        $dartHash = $null
        $rustHash = $null
    }

    if (-not $dartHash -or -not $rustHash) {
        throw ("读不到桥接 hash（Dart='$dartHash' Rust='$rustHash'）。" +
            "先跑 scripts\regen.ps1 重新生成两侧桥接代码。")
    }
    if ($dartHash -ne $rustHash) {
        throw ("桥接代码与 Rust 不一致（Dart rustContentHash=$dartHash，Rust=$rustHash）。" +
            "改了 Rust 公开 API 后必须先跑 scripts\regen.ps1 重生成两侧桥接，再重跑本脚本；" +
            "否则 APK 装上去会在 RustLib.init() 报 hash mismatch。")
    }
    Write-Host "==> [0/3] 桥接 hash 一致（$rustHash）"

    # ---------- [1/3] 编译 Rust 核心（android arm64）----------
    Write-Host "==> [1/3] 编译 Rust 核心 (android arm64)"

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

    # ---------- [2/3] flutter build apk ----------
    Write-Host "==> [2/3] flutter build apk (仅 android-arm64)"

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
