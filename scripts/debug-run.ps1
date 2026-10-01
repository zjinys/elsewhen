# debug-run.ps1 - 开 ELSEWHEN_DEBUG 启动应用，并把 agent loop 诊断同时留在文件里。
#
# 与 debug-run.sh 等价。原始设计理由（保留，因为都是踩过的坑）：
#
# 为什么需要这个脚本：
#   agent 循环的诊断（round / protocol / content_len / tool_calls / model /
#   工具派发结果）本来只走 debug_eprintln!，也就是 eprintln! + ELSEWHEN_DEBUG gate。
#   应用是 Flutter 桌面应用，Rust 侧走 FFI 与 Dart 同进程，所以 eprintln!
#   会打到该进程的 stderr。用本脚本从终端启动，stderr 被 flutter run 捕获并
#   打印到终端 —— 也就是说「终端里已经能看到」，前提是环境变量被设上。
#   这个脚本就是把那一步固化下来，免得每次去记变量名和日志位置。
#
# 为什么不改 Rust 侧加文件 sink：
#   1) 边界更清楚。日志只在「你主动用这个脚本启动」时产生，打包版正常启动不会有
#      任何文件；Rust 内建 sink 则要把「开不开日志」这件事塞进库和包体。
#   2) 脱敏责任清晰。stderr 本来就只在你主动开时可见，脚本不改变这个前提。
#   3) 无新依赖。文件 sink 要自己实现轮转，还要在 FFI 库里维护 I/O 失败路径。
#
# ⚠ 日志含个人数据：`[agent] dispatch tool=... -> <工具结果前 80 字>` 会原样带出
#   知识库正文片段、对话内容等。定位问题需要看到真实值（空回复那次就是靠
#   content_len=0 与 tool_calls=0 定的性），故不做截断或脱敏。
#   **贴给别人前先自己过一遍**，或只截取需要的几行。
#
# 轮转：按次轮转，不是写入中轮转。启动前检查一次、退出后再检查一次。
#   单次会话的量级是 KB 级（一轮约 100 字符，一次会话几十轮），为它拉一个后台
#   轮转进程不值得 —— 那样还得处理进程回收，收益却是零。
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

Write-Host "调试模式已开启（ELSEWHEN_DEBUG=1）"
Write-Host "  日志：$log"
Write-Host "  ⚠ 含个人数据（知识库正文片段、对话内容），贴给外部前先自查或只截取需要的行。"
Write-Host ""

$env:ELSEWHEN_DEBUG = "1"

# 2>&1 把 flutter 的 stderr 并进管道，镜像 debug-run.sh 的 `2>&1 | tee`。
# Tee-Object 是 PowerShell cmdlet，管道结束后 $LASTEXITCODE 仍是上游原生命令的退出码。
& $app @FlutterArgs 2>&1 | Tee-Object -FilePath $log -Append
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
