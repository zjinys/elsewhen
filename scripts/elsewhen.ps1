# elsewhen.ps1 - 启动主应用（Windows）
#
# 与 elsewhen.sh 等价，只把设备从 linux 换成 windows。
#
# ⚠ 排查时务必带 --target：flutter build/run 不带时默认构建 lib/main.dart
# （不含窗口 chrome），用它验证会得到「全部正常」的假结论。
#
# 桌面入口用 main_desktop.dart（含 nativeapi 窗口 chrome 初始化）；
# main.dart 是移动端/通用入口，不带窗口管理 —— Android 也用它，因为
# nativeapi 的 FFI union 会把 Android release AOT 编译器打崩。
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$FlutterArgs
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$ui = Join-Path $root "ui"

# Flutter 解析顺序与 package-msi.ps1 一致：ui\.fvm 优先，再退到全局 fvm / flutter。
$flutter = if (Test-Path (Join-Path $ui ".fvm\flutter_sdk\bin\flutter.bat")) {
    Join-Path $ui ".fvm\flutter_sdk\bin\flutter.bat"
} elseif (Get-Command fvm -ErrorAction SilentlyContinue) {
    "fvm"
} else {
    (Get-Command flutter -ErrorAction Stop).Source
}

$device = if ($env:ELSEWHEN_DEVICE) { $env:ELSEWHEN_DEVICE } else { "windows" }

Push-Location $ui
try {
    & $flutter run "-d" $device "--target=lib/main_desktop.dart" @FlutterArgs
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
