# bootstrap.ps1 - 新环境一键准备并启动（Windows）
#
# 与 bootstrap.sh 等价：预检 → 解析 Flutter SDK → pub get → 编 Rust →
# 校验桥接 hash → 启动。参数用 PowerShell 写法（-SetupOnly / -Regen /
# -Device），其余参数原样透传给 flutter run。
#
# 解决的问题：新 clone 出来的仓库直接跑 scripts\elsewhen.ps1 一定失败，
# 因为它假定三件事已经就绪，而这三样都不在 git 里：
#   1. target\release\elsewhen.dll —— target\ 被 .gitignore 排除。缺失时
#      Dart 侧 RustLib.init() 抛的是一句 dlopen 失败，看不出「该先编 Rust」。
#   2. ui\.fvm\flutter_sdk —— .fvm\ 同样被 gitignore，库里只有 ui\.fvmrc。
#   3. ui\.dart_tool\package_config.json —— pub get 从没跑过。
#
# 前提：系统依赖与工具链已按 docs\development\build-environment.md 装好
# （MSVC Build Tools + Flutter + FVM）。本脚本**不装任何东西** —— 那要么要
# 管理员权限，要么要交互式安装器（msiexec），代劳只会把「环境没配好」变成
# 「脚本坏了」。它只做装包做不到的事：在动手之前把缺的东西指出来。
#
# 用法：
#   powershell -File scripts\bootstrap.ps1                  # 准备 + 启动（桌面）
#   powershell -File scripts\bootstrap.ps1 -SetupOnly      # 只准备，不启动
#   powershell -File scripts\bootstrap.ps1 -Regen           # 先重生成桥接（改了 Rust 公开 API）
#   powershell -File scripts\bootstrap.ps1 -Device android  # 换设备
#
# ⚠ 参数绑定不是「原样透传」：PowerShell 会把单横线/双横线开头的 token 当参数名
#   做前缀匹配，所以 `-d android`、`--device android` 都会被绑到 -Device，而不是
#   交给 flutter run（实测确认）。因此设备一律用 -Device 指定；其余落不到的参数
#   才进 $FlutterArgs 透传，例如 --dart-define=X。
param(
    # 只把环境准备好，不启动（首次验证 / 无人值守）
    [switch]$SetupOnly,

    # 先跑 scripts\regen.ps1 重新生成桥接代码（改了 Rust 侧公开 API 时用）
    [switch]$Regen,

    # flutter run 的设备 id（默认 windows，可用环境变量 ELSEWHEN_DEVICE 覆盖）
    [string]$Device,

    # 其余参数原样透传给 flutter run
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$FlutterArgs
)

$ErrorActionPreference = "Stop"

# 多行提示不能用 throw 直接抛：PowerShell 7 的 ConciseView 会折叠连续空白，
# 把排版好的处置说明压成一堵字墙（PS 5.1 不折叠，所以这个坑只在 7+ 暴露）。
# 与 regen.ps1 同一个处置：Write-Host 逐行输出，throw 只带单行摘要。
function Stop-WithGuidance([string]$summary, [string[]]$guidance) {
    Write-Host ""
    foreach ($line in $guidance) { Write-Host $line }
    Write-Host ""
    throw $summary
}

function Write-Note([string]$text) { Write-Host "      $text" }

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$ui = Join-Path $root "ui"

Push-Location $root
try {
    # ---------- 1. 预检 ----------
    # 只查 cargo：MSVC / CMake / Ninja 由 Visual Studio 与 Flutter 自带或自动发现，
    # 没有可靠的「一句话探测」方式，硬猜反而会误报。真的缺时后面对应的步骤会
    # 带着原始报错失败，那时的信息已经足够定位。
    Write-Host ""
    Write-Host "[1/5] 预检工具链"
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Stop-WithGuidance -summary "找不到 cargo" -guidance @(
            "装法：https://rustup.rs —— 仓库没有 rust-toolchain.toml，跟随 stable。",
            "装完重开一个 PowerShell 窗口（PATH 不会自动刷新）再跑本脚本。"
        )
    }
    # 桌面入口：main_desktop.dart 含 nativeapi 的窗口 chrome 初始化；main.dart 不含
    # （Android 也用它，因为 nativeapi 的 FFI union 会把 Android release AOT 编译器打崩）。
    # flutter run 不带 --target 时默认构建 lib/main.dart，所以必须显式带上。
    if (-not (Test-Path (Join-Path $ui "lib\main_desktop.dart"))) {
        Stop-WithGuidance -summary "找不到 ui\lib\main_desktop.dart" -guidance @("仓库不完整？")
    }
    Write-Note "cargo OK"

    if (-not $Device) { $Device = if ($env:ELSEWHEN_DEVICE) { $env:ELSEWHEN_DEVICE } else { "windows" } }

    # ---------- 2. 解析 Flutter SDK ----------
    # 顺序与 elsewhen.ps1 / package-msi.ps1 一致：ui\.fvm 里钉住的版本 →
    # 全局 fvm → 全局 flutter。已经从 .fvmrc 解析出来时不再调 fvm install，
    # 这样「装了 Flutter 但没装 fvm」的机器也能一条命令跑起来。
    Write-Host ""
    Write-Host "[2/5] 解析 Flutter SDK"
    $localFlutter = Join-Path $ui ".fvm\flutter_sdk\bin\flutter.bat"
    if (Test-Path $localFlutter) {
        $flutter = $localFlutter
        Write-Note "用已解析的 ui\.fvm（无需 fvm）"
    }
    elseif (Get-Command fvm -ErrorAction SilentlyContinue) {
        Write-Note "fvm install（读 ui\.fvmrc）"
        Push-Location $ui
        try { & fvm install } finally { Pop-Location }
        # 用 [System.IO.File]::Exists 而非 Test-Path：后者遇到不存在的驱动器会**抛**
        # 「Cannot find drive」而不是返回 false，在 $ErrorActionPreference = "Stop"
        # 下会终止脚本，用户看到的是一句无关的驱动错误（与 regen.ps1 同一个坑）。
        if (-not [System.IO.File]::Exists($localFlutter)) {
            Stop-WithGuidance -summary "fvm install 之后仍没有 ui\.fvm\flutter_sdk" -guidance @(
                "先手工看一眼 ui\.fvmrc 的钉住版本是否可安装：",
                "  cd ui; fvm install"
            )
        }
        $flutter = $localFlutter
    }
    elseif (Get-Command flutter -ErrorAction SilentlyContinue) {
        $flutter = (Get-Command flutter).Source
        Write-Note "⚠ 没有 ui\.fvm 也没有 fvm，改用全局 flutter —— 版本不再由 ui\.fvmrc 钉住。"
        Write-Note "  正常做法：dart pub global activate fvm"
    }
    else {
        Stop-WithGuidance -summary "找不到 Flutter" -guidance @(
            "三条路任选其一：",
            "  1) dart pub global activate fvm（PATH 里需有 %APPDATA%\npm 或 pub-cache\bin），本脚本会自动 fvm install",
            "  2) 自己装好 Flutter 并在 ui\ 下跑一次 fvm install",
            "  3) 只做校验不启动：装全局 flutter 后加 -SetupOnly"
        )
    }

    # ---------- 3. Dart 依赖 ----------
    Write-Host ""
    Write-Host "[3/5] 拉取 Dart 依赖（pub get）"
    Push-Location $ui
    try { & $flutter pub get } finally { Pop-Location }
    if ($LASTEXITCODE -ne 0) { throw "pub get 失败（退出码 $LASTEXITCODE）" }

    # ---------- 4. Rust 桥接库 ----------
    # 必须是 --release：Dart 侧 frb 默认从 ..\target\release\ 加载（不是 debug），
    # 打成 debug 会出现 `Content hash on Dart side ... is different from Rust side`。
    # 每次都跑而不是「DLL 不存在才跑」：cargo 自己的 fingerprint 会让无改动的
    # 重复构建耗时接近 0，而跳过它意味着改了 Rust 代码就静默跑旧库。
    Write-Host ""
    Write-Host "[4/5] 编译 Rust 桥接库（cargo build --release --lib）"
    if ($Regen) {
        Write-Note "-Regen：先重新生成 Dart/Rust 桥接代码"
        & powershell -ExecutionPolicy Bypass -File (Join-Path $root "scripts\regen.ps1")
        if ($LASTEXITCODE -ne 0) { throw "regen.ps1 失败（退出码 $LASTEXITCODE）" }
    }
    else {
        & cargo build --release --lib
        if ($LASTEXITCODE -ne 0) { throw "cargo build --release 失败（退出码 $LASTEXITCODE）" }
    }

    # ---------- 5. 桥接 hash 校验 ----------
    # 比对 Dart 侧 rustContentHash 与 src\frb_generated.rs 里的常量。刚编完的
    # DLL 里烧的就是后者，所以源码层面相等 ⟺ 运行时相等。这是不一致时**唯一**
    # 能提前抓到它的时机：否则要等到 app 启动时 RustLib.init() 抛 hash mismatch，
    # 而那句话完全看不出该跑 regen。
    Write-Host ""
    Write-Host "[5/5] 校验桥接 codegen hash"

    $dartHashFile = Join-Path $ui "lib\bridge\generated.dart\frb_generated.dart"
    $rustHashFile = Join-Path $root "src\frb_generated.rs"
    $dartHash = $null
    $rustHash = $null

    # Select-String 命中为空时 .Matches[0] 会抛（Stop 下终止脚本），所以先自己判。
    # -Pattern 只取第一个命中：frb_generated.dart 里 rustContentHash 只出现一次，
    # 但同一文件可能有多个 getter 变体，取首个足够且不会因空匹配炸掉。
    if (Test-Path $dartHashFile) {
        $m = Select-String -Path $dartHashFile -Pattern 'int get rustContentHash => (-?\d+);' |
            Select-Object -First 1
        if ($m) { $dartHash = $m.Matches[0].Groups[1].Value }
    }
    if (Test-Path $rustHashFile) {
        $m = Select-String -Path $rustHashFile -Pattern 'FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH: i32 = (-?\d+);' |
            Select-Object -First 1
        if ($m) { $rustHash = $m.Matches[0].Groups[1].Value }
    }

    if (-not $dartHash) {
        Stop-WithGuidance -summary "未能在 $dartHashFile 中找到 rustContentHash" -guidance @(
            "桥接生成代码不在库里？正常 clone 应该带着它。",
            "若 frb 版本或生成布局变了，这里需要跟着改匹配式。"
        )
    }
    if (-not $rustHash) {
        Stop-WithGuidance -summary "未能在 $rustHashFile 中找到 FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH" -guidance @(
            "同上。"
        )
    }
    if ($dartHash -ne $rustHash) {
        Stop-WithGuidance -summary "桥接 codegen hash 不一致：Dart=$dartHash Rust=$rustHash" -guidance @(
            "Rust 侧公开 API 变了但桥接代码没重新生成。此时启动会在 RustLib.init()",
            "抛 'Content hash on Dart side ... is different from Rust side'。",
            "",
            "两条出路：",
            "  1) powershell -File scripts\bootstrap.ps1 -Regen（= regen.ps1 + 重编）",
            "  2) 直接跑 powershell -File scripts\regen.ps1"
        )
    }
    Write-Note "两侧一致（$dartHash）"

    # ---------- 启动 ----------
    if ($SetupOnly) {
        Write-Host ""
        Write-Host "✔ 环境就绪，未启动。"
        Write-Host "  下一步：scripts\elsewhen.ps1   # 日常启动入口"
        exit 0
    }

    Write-Host ""
    Write-Host "启动（设备：$Device）..."
    Write-Host "  ⚠ 排查时务必带 --target：elsewhen.ps1 与本脚本都已固定用 main_desktop.dart。"
    Write-Host ""

    Push-Location $ui
    try {
        # 设备恒由 -Device / ELSEWHEN_DEVICE 决定（见头注释：-d 会被绑到 -Device），
        # 所以这里总是显式传 -d，不存在「两个 -d」的情况。
        & $flutter run "-d" $Device "--target=lib/main_desktop.dart" @FlutterArgs
        exit $LASTEXITCODE
    }
    finally {
        Pop-Location
    }
}
finally {
    Pop-Location
}