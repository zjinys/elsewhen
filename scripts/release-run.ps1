# release-run.ps1 - release 模式启动应用（Windows），日志捕获与轮转同 debug-run.ps1。
#
# 与 debug-run.ps1 完全同构（坑位说明也一致，那边写得更全），唯二差别：
#   1) 传给 elsewhen.ps1 的参数多一个 --release（AOT 编译、无 debug 断言/热重载）；
#   2) 提示语说明 release 的取舍。
#
# 为什么叫「release 模式」还是开 ELSEWHEN_DEBUG（而不是「release 就不用记日志」）：
#   ELSEWHEN_DEBUG 是运行时 env gate（std::env::var，src/ai/conversation.rs），
#   release 二进制同样响应。真实负载下的性能与行为差异恰恰是 release 才看得到
#   的，此时留在文件里的诊断价值更高。
#
# 用途：贴近真实手感的验收（排版渲染、长会话）与复现那些「只在 release 出现
#   /消失」的问题——UI 的 debug 断言（如 ListTile 水波纹）在 release 根本不
#   触发，反过来 release 专属的裁剪/优化问题 debug 也看不见。两边都要能跑
#   才叫复现。
#
# ⚠ 注意：
#   - release 首启需要一次 AOT 编译，比 debug 慢（后续增量快）；不支持热重载
#     （flutter run 的 r/R 无效），改了代码要退出重跑。
#   - 日志含个人数据（知识库正文片段、对话内容），贴给别人前先自己过一遍。
#
# 用法：
#   scripts\release-run.ps1                  # release 启动
#   scripts\release-run.ps1 --dart-define=x  # 透传参数给 elsewhen.ps1
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$FlutterArgs
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$app = Join-Path $root "scripts\elsewhen.ps1"

# 与 src/config.rs 的 resolve_data_dir 对齐：ELSEWHEN_DATA_DIR 优先，否则平台默认。
# Windows 上 directories::data_local_dir() 即 %LOCALAPPDATA%。
$dataDir = if ($env:ELSEWHEN_DATA_DIR) { $env:ELSEWHEN_DATA_DIR } else { Join-Path $env:LOCALAPPDATA "elsewhen" }
New-Item -ItemType Directory -Force -Path $dataDir | Out-Null
$log = Join-Path $dataDir "agent-debug.log"

$capBytes = 4MB
$keep = 3

function Get-FileSize([string]$Path) {
    if (-not (Test-Path $Path)) { return 0 }
    return (Get-Item $Path).Length
}

function Format-Size([long]$Bytes) {
    if ($Bytes -ge 1MB) { return "{0}MB" -f [math]::Floor($Bytes / 1MB) }
    if ($Bytes -ge 1KB) { return "{0}KB" -f [math]::Floor($Bytes / 1KB) }
    return "$Bytes B"
}

# 超限时把旧代往后挪一代再腾出 .1。KEEP 代之后的最老文件直接丢。
function Invoke-RotateIfOversized {
    if (-not (Test-Path $log)) { return }
    if ((Get-FileSize $log) -le $capBytes) { return }

    for ($i = $keep - 1; $i -ge 1; $i--) {
        $src = "$log.$i"
        if (Test-Path $src) { Move-Item -Force $src "$log.$($i + 1)" }
    }
    Move-Item -Force $log "$log.1"
    Write-Host "已轮转：上一份 $(Format-Size (Get-FileSize "$log.1")) → $log.1"
}

Invoke-RotateIfOversized

Write-Host "Release 模式（AOT，无 debug 断言/热重载）"
Write-Host "  日志：$log"
Write-Host "  ⚠ 含个人数据（知识库正文片段、对话内容），贴给外部前先自查或只截取需要的行。"
Write-Host ""

$env:ELSEWHEN_DEBUG = "1"

# --release 放在透传参数之前（后传可被同名参数覆盖，顺序即语义）。
# 2>&1 把 flutter 的 stderr 并进管道，镜像 release-run.sh 的 `2>&1 | tee`。
# Tee-Object 是 PowerShell cmdlet，管道结束后 $LASTEXITCODE 仍是上游原生命令的退出码。
& $app --release @FlutterArgs 2>&1 | Tee-Object -FilePath $log -Append
$appStatus = $LASTEXITCODE

Write-Host ""
if (Test-Path $log) {
    Write-Host "本次日志累计：$log（$(Format-Size (Get-FileSize $log))）"
    Invoke-RotateIfOversized
    Write-Host "  日志文件："
    Get-ChildItem -Path $log -Filter "agent-debug.log.*" -ErrorAction SilentlyContinue |
        Sort-Object Name | ForEach-Object { Write-Host "    $($_.FullName)" }
}

if ($appStatus -ne 0) {
    Write-Host "应用退出码：$appStatus（已记入上面的日志尾部）"
}
exit $appStatus
