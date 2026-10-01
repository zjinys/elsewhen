# elsewhen-capture.ps1 - 启动 Capture 模式（Windows）
#
# Capture 模式同样是桌面入口（CaptureScreen 依赖 window_service → nativeapi），
# 所以必须带 --target=lib/main_desktop.dart。
param()

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$ui = Join-Path $root "ui"

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
    & $flutter run "-d" $device "--target=lib/main_desktop.dart" `
        "--dart-entrypoint-args" "--mode=capture"
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
