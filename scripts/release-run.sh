#!/usr/bin/env bash
# 以 release 模式启动应用，其余行为与 debug-run.sh 完全一致。
#
# 与 debug-run.sh 的唯一差别：
#   传给 elsewhen.sh 的参数多一个 --release（AOT 编译、无 debug 断言/热重载）。
#   用途：贴近真实使用手感跑长会话、验收排版渲染、复现那些「只在 release
#   出现/消失」的问题——比如 UI 的 debug 断言（如 ListTile 水波纹）在 release
#   根本不触发，反过来 release 专属的裁剪/优化问题 debug 也看不见。两边都要
#   能跑才叫复现。
#
# 为什么日志捕获原样保留（而不是「release 就不用记日志了」）：
#   ELSEWHEN_DEBUG 是运行时 env gate（std::env::var，src/ai/conversation.rs），
#   release 二进制同样响应。真实负载下的性能与行为差异恰恰是 release 才看得到
#   的，此时留在文件里的诊断价值更高。
#
# ⚠ 注意：release 启动需要一次 AOT 编译，首启比 debug 慢（后续增量快）；
#   不支持热重载（flutter run 的 r/R 无效），改了代码要退出重跑。
#
# 轮转与脱敏约定与 debug-run.sh 相同（同一份 LOG 文件）：
#   ⚠ 日志含个人数据，贴给外部前先自己过一遍。
#
# 用法：
#   scripts/release-run.sh                 # release 启动
#   scripts/release-run.sh --dart-define=x # 透传参数给 elsewhen.sh
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


# relay 出站走代理：relay 是 Cloudflare Workers，直连优选 IP 的 TLS 握手
# 抖动可达数秒，配对链路（多个串行往返）会因此失败。reqwest 不读系统
# 代理，这里探测本机常见代理端口并导出标准变量（src/http_proxy.rs 消费）。
# 已显式设置 HTTPS_PROXY/ALL_PROXY 时完全尊重用户；全没探测到则直连。
if [ -z "${HTTPS_PROXY:-}" ] && [ -z "${https_proxy:-}" ] && [ -z "${ALL_PROXY:-}" ] && [ -z "${all_proxy:-}" ]; then
    for port in 7897 7890 7891 1080; do
        if (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then
            exec 3>&- 3<&- || true
            export HTTPS_PROXY="http://127.0.0.1:$port"
            export ALL_PROXY="$HTTPS_PROXY"
            echo "已探测到本机代理端口 $port，relay/AI 出站将走该代理（ALL_PROXY=$ALL_PROXY）"
            break
        fi
    done
fi

rotate_if_oversized

echo "Release 模式（AOT，无 debug 断言/热重载）"
echo "  日志：$LOG"
echo "  ⚠ 含个人数据（知识库正文片段、对话内容），贴给外部前先自查或只截取需要的行。"
echo

set +e
ELSEWHEN_DEBUG=1 "$ROOT/scripts/elsewhen.sh" --release "$@" 2>&1 | tee -a "$LOG"
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
