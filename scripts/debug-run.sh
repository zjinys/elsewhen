#!/usr/bin/env bash
# 开 ELSEWHEN_DEBUG 启动应用，并把 agent loop 诊断同时留在文件里。
#
# 为什么需要这个脚本：
#   agent 循环的诊断（round / protocol / content_len / tool_calls / model /
#   工具派发结果）本来只走 debug_eprintln!，也就是 eprintln! + ELSEWHEN_DEBUG gate。
#   应用是 Flutter Linux 桌面应用，Rust 侧走 FFI 与 Dart 同进程，所以 eprintln!
#   会打到该进程的 stderr。开发时用 ./elsewhen.sh 从终端启动，stderr 被 flutter run
#   捕获并打印到终端 —— 也就是说「终端里已经能看到」，前提是环境变量被设上。
#   这个脚本就是把那一步固化下来，免得每次去记变量名和日志位置。
#
# 为什么不改 Rust 侧加文件 sink：
#   1) 边界更清楚。日志只在「你主动用这个脚本启动」时产生，打包版正常启动不会有
#      任何文件；Rust 内建 sink 则要把「开不开日志」这件事塞进库和包体。
#   2) 脱敏责任清晰。stderr 本来就只在你主动开时可见，脚本不改变这个前提。
#   3) 无新依赖。文件 sink 要自己实现轮转，还要在 FFI 库里维护 I/O 失败路径。
#
# ⚠ 日志含个人数据：`[agent] dispatch tool=... -> <工具结果前 80 字>` 会原样带出
#   知识库正文片段、对话内容等。定位问题需要看到真实值（本次断链就是靠
#   content_len=0 与 tool_calls=0 定的性），故不做截断或脱敏。
#   **贴给别人前先自己过一遍**，或只截取需要的几行。
#
# 轮转：按次轮转，不是写入中轮转。启动前检查一次、退出后再检查一次。
#   单次会话的量级是 KB 级（一轮约 100 字符，一次会话几十轮），为它拉一个后台
#   轮转进程不值得 —— 那样还得处理进程回收，收益却是零。
#
# 用法：
#   scripts/debug-run.sh                 # 正常启动
#   scripts/debug-run.sh --dart-define=x # 透传参数给 elsewhen.sh
#
# 依赖：bash、tee、flutter（经 elsewhen.sh）

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# 与 src/config.rs 的 resolve_data_dir 对齐：ELSEWHEN_DATA_DIR 优先，否则平台默认。
# Linux 上 directories::data_local_dir() 即 ~/.local/share。
DATA_DIR="${ELSEWHEN_DATA_DIR:-$HOME/.local/share/elsewhen}"
mkdir -p "$DATA_DIR"
LOG="$DATA_DIR/agent-debug.log"

CAP_MB=4
KEEP=3

human_size() {
    local bytes
    bytes=$(stat -c %s "$1" 2>/dev/null || echo 0)
    if [ "$bytes" -ge 1048576 ]; then
        echo "$((bytes / 1048576))MB"
    elif [ "$bytes" -ge 1024 ]; then
        echo "$((bytes / 1024))KB"
    else
        echo "${bytes}B"
    fi
}

# 超限时把旧代往后挪一代再腾出 .1。KEEP 代之后的最老文件直接丢。
rotate_if_oversized() {
    [ -f "$LOG" ] || return 0
    local bytes
    bytes=$(stat -c %s "$LOG" 2>/dev/null || echo 0)
    [ "$bytes" -gt $((CAP_MB * 1024 * 1024)) ] || return 0

    local i
    for ((i = KEEP - 1; i >= 1; i--)); do
        if [ -f "$LOG.$i" ]; then
            mv -f "$LOG.$i" "$LOG.$((i + 1))"
        fi
    done
    mv -f "$LOG" "$LOG.1"
    echo "已轮转：上一份 $(human_size "$LOG.1") → $LOG.1"
}

rotate_if_oversized

echo "调试模式已开启（ELSEWHEN_DEBUG=1）"
echo "  日志：$LOG"
echo "  ⚠ 含个人数据（知识库正文片段、对话内容），贴给外部前先自查或只截取需要的行。"
echo

set +e
ELSEWHEN_DEBUG=1 "$ROOT/scripts/elsewhen.sh" "$@" 2>&1 | tee -a "$LOG"
APP_STATUS=${PIPESTATUS[0]}
set -e

echo
if [ -f "$LOG" ]; then
    echo "本次日志累计：$LOG（$(human_size "$LOG")）"
    rotate_if_oversized
    echo "  日志文件："
    echo "    $LOG"
    # 没有历史代次时 glob 不展开，ls 会以 2 退出；必须显式吞掉，
    # 否则 pipefail + set -e 会在正常路径上把脚本打断（退出码 2）。
    ls -1 "$LOG".[0-9]* 2>/dev/null | sed 's/^/    /' || true
fi

if [ "$APP_STATUS" -ne 0 ]; then
    echo "应用退出码：$APP_STATUS（已记入上面的日志尾部）"
fi
exit "$APP_STATUS"
