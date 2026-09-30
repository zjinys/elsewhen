# test_bridge.ps1 - 验证 Rust bridge 集成（Windows）
#
# 与 test_bridge.sh 等价，改动点：
#   - 桥接库产物：Linux 的 target/release/libelsewhen.so → Windows 的
#     target/release/elsewhen.dll（package-msi.ps1 里 frb 按裸名 LoadLibrary）。
#   - 数据库路径改为遵循 ELSEWHEN_DATA_DIR / %LOCALAPPDATA%，与
#     src/config.rs 的 resolve_data_dir 对齐（.sh 里是硬编码的 Linux 路径）。
#   - ps aux | grep → Get-Process。
param()

$ErrorActionPreference = "Continue"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Push-Location $root

try {
    Write-Host "Testing Rust bridge integration..."
    Write-Host ""

    # Test 1: 检查 Rust 库是否存在
    Write-Host "1. Checking Rust library..."
    $dll = "target\release\elsewhen.dll"
    if (Test-Path $dll) {
        Write-Host "   ✓ elsewhen.dll found"
    } else {
        Write-Host "   ✗ elsewhen.dll not found"
        exit 1
    }

    # Test 2: 检查生成的 bridge 代码是否存在
    Write-Host ""
    Write-Host "2. Checking generated bridge code..."
    if (Test-Path "ui\lib\bridge\generated.dart\api.dart") {
        Write-Host "   ✓ api.dart generated"
    } else {
        Write-Host "   ✗ api.dart not generated"
        exit 1
    }

    if (Test-Path "ui\lib\bridge\generated.dart\frb_generated.dart") {
        Write-Host "   ✓ frb_generated.dart generated"
    } else {
        Write-Host "   ✗ frb_generated.dart not generated"
        exit 1
    }

    # Test 3: 检查数据库（信息性检查，缺失不判失败）
    Write-Host ""
    Write-Host "3. Checking database..."
    $dataDir = if ($env:ELSEWHEN_DATA_DIR) { $env:ELSEWHEN_DATA_DIR } else { Join-Path $env:LOCALAPPDATA "elsewhen" }
    $db = Join-Path $dataDir "elsewhen.db"
    if (Test-Path $db) {
        Write-Host "   ✓ Database exists at $db"

        $sqlite = (Get-Command sqlite3 -ErrorAction SilentlyContinue).Source
        if ($sqlite) {
            $tables = & $sqlite $db ".tables" 2>&1
            Write-Host "   Tables: $tables"
            $count = & $sqlite $db "SELECT COUNT(*) FROM events;" 2>&1
            Write-Host "   Events count: $count"
        } else {
            Write-Host "   (跳过表/行数检查：未找到 sqlite3 命令行工具)"
        }
    } else {
        Write-Host "   ✗ Database not found at $db"
    }

    # Test 4: 检查运行中的进程（信息性检查）
    Write-Host ""
    Write-Host "4. Checking running processes..."
    $procs = @(Get-Process -Name "elsewhen_ui" -ErrorAction SilentlyContinue)
    Write-Host "   Running instances: $($procs.Count)"

    Write-Host ""
    Write-Host "Bridge integration test complete!"
} finally {
    Pop-Location
}
