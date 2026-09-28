# package-msi.ps1 - 把 Flutter UI 桌面端打成 Windows MSI（需在 Windows 上运行）
#
# 产物：dist\Elsewhen-<version>-x64.msi
#
# 依赖：cargo、flutter（ui\.fvm 优先）、WiX Toolset v3.11（heat/candle/light，
#        通过 %WIX%\bin 或 PATH 查找；安装：choco install wixtoolset）。
#
# 用法：powershell -File scripts\package-msi.ps1 [输出目录，默认 dist]
param(
    [string]$OutDir = "dist"
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Push-Location $root
try {
    # 版本号与 Cargo.toml 保持一致（MSI 要求四段以内数字版本）。
    $rawVersion = (Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"' |
        Select-Object -First 1).Matches[0].Groups[1].Value
    $version = if ($env:ELSEWHEN_VERSION) { $env:ELSEWHEN_VERSION } else { $rawVersion }
    if ($version -notmatch '^\d+\.\d+\.\d+') {
        throw "版本号 '$version' 不符合 MSI 要求（需形如 1.2.3）；可用 ELSEWHEN_VERSION 覆盖"
    }
    if (-not [System.IO.Path]::IsPathRooted($OutDir)) { $OutDir = Join-Path $root $OutDir }
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

    $flutter = if (Test-Path "ui\.fvm\flutter_sdk\bin\flutter.bat") {
        "ui\.fvm\flutter_sdk\bin\flutter.bat"
    } else {
        (Get-Command flutter -ErrorAction Stop).Source
    }

    Write-Host "[1/4] 构建 Rust 桥接库 (cargo build --release) ..."
    cargo build --release

    Write-Host "[2/4] 构建 Flutter Windows release ..."
    & $flutter build windows --release --target=lib/main_desktop.dart
    if ($LASTEXITCODE -ne 0) { throw "flutter build windows 失败" }

    $releaseDir = Join-Path $root "ui\build\windows\x64\runner\Release"
    if (-not (Test-Path (Join-Path $releaseDir "elsewhen_ui.exe"))) {
        throw "未找到 $releaseDir\elsewhen_ui.exe"
    }

    # frb 在 Windows 按裸名 LoadLibrary('elsewhen.dll')，Windows 加载器
    # 首先搜索 exe 所在目录——把桥接库放到 exe 旁边即可。
    Copy-Item (Join-Path $root "target\release\elsewhen.dll") $releaseDir

    # WiX v3.11 工具定位
    $wixBin = if ($env:WIX) { Join-Path $env:WIX "bin" } else { "" }
    $heat = if ($wixBin -and (Test-Path "$wixBin\heat.exe")) { "$wixBin\heat.exe" }
            else { (Get-Command heat.exe -ErrorAction SilentlyContinue).Source }
    if (-not $heat) {
        throw "未找到 WiX Toolset v3.11（heat/candle/light）。安装：choco install wixtoolset"
    }
    $wixDir = Split-Path -Parent $heat
    $candle = Join-Path $wixDir "candle.exe"
    $light  = Join-Path $wixDir "light.exe"

    Write-Host "[3/4] heat 收集文件 + candle 编译 ..."
    $stage = Join-Path $env:TEMP ("elsewhen-msi-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $stage | Out-Null
    try {
        # WixUI_Minimal 需要一个许可页 RTF；项目未分发单独许可证文本，
        # 生成一页占位 RTF。
        $licenseRtf = Join-Path $stage "License.rtf"
        Set-Content -Path $licenseRtf -Value '{\rtf1\ansi Elsewhen - local-first personal event recorder.}'
        $harvested = Join-Path $stage "harvested.wxs"

        & $heat dir $releaseDir -cg ProductComponents -dr INSTALLFOLDER `
            -srd -sreg -scom -gg -ag -var var.SourceDir -out $harvested
        if ($LASTEXITCODE -ne 0) { throw "heat 失败" }

        $iconPath = Join-Path $root "ui\windows\runner\resources\app_icon.ico"
        & $candle -nologo `
            "-dSourceDir=$releaseDir" `
            "-dProductVersion=$version" `
            "-dIconPath=$iconPath" `
            "-dLicenseRtf=$licenseRtf" `
            -arch x64 `
            -out "$stage\" `
            (Join-Path $root "packaging\windows\elsewhen.wxs") $harvested
        if ($LASTEXITCODE -ne 0) { throw "candle 失败" }

        Write-Host "[4/4] light 链接 MSI ..."
        $out = Join-Path $OutDir "Elsewhen-$version-x64.msi"
        Remove-Item $out -Force -ErrorAction SilentlyContinue
        & $light -nologo -ext WixUIExtension `
            -out $out "$stage\elsewhen.wixobj" "$stage\harvested.wixobj"
        if ($LASTEXITCODE -ne 0) { throw "light 失败" }
    } finally {
        Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
    }

    Write-Host "Created $out"
} finally {
    Pop-Location
}
