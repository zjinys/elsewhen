#!/usr/bin/env bash
# 修复 ananicy-cpp 对 Elsewhen / Flutter 工具链的降权（CachyOS 及手装 ananicy
# 的 Arch 系发行版）。未运行 ananicy-cpp 的机器上是空操作，任何平台都可安全执行。
#
# 背景：00-default 的 dart_flutter.rules 把 flutter/dart 全族按 Heavy_CPU 类型
# 钉在 nice 9 左右。debug 运行（JIT + asserts + 增量编译）是最吃 CPU 的进程，
# 被压之后与高优先级进程抢核，表现为 debug 启动/使用明显卡顿。
# 详见 docs/development/build-environment.md §2.4。
#
# 做两件事：
#   1) 写入 /etc/ananicy.d/99-elsewhen/elsewhen.rules（99- 目录比 00-default
#      晚加载，覆盖默认规则；按二进制名匹配，debug/release 通用）；
#   2) renice 当前已在运行的 elsewhen / flutter / dart 进程（免重启立即生效）。
#
# 需 root（写 /etc）；重复执行幂等（规则文件整体覆盖）。
#
# 用法：sudo ./scripts/fix-ananicy-priority.sh [--remove]
#   --remove  删除豁免规则（恢复发行版默认行为）。

set -euo pipefail

RULES_DIR="/etc/ananicy.d/99-elsewhen"
RULES_FILE="$RULES_DIR/elsewhen.rules"

if [ "$(id -u)" -ne 0 ]; then
    echo "需要 root：sudo $0 $*" >&2
    exit 1
fi

if [ "${1:-}" = "--remove" ]; then
    if [ -f "$RULES_FILE" ]; then
        rm -f "$RULES_FILE"
        rmdir "$RULES_DIR" 2>/dev/null || true
        echo "已删除 $RULES_FILE"
    else
        echo "未发现 $RULES_FILE，无需删除"
    fi
    if systemctl is-active --quiet ananicy-cpp 2>/dev/null; then
        systemctl restart ananicy-cpp
        echo "已重启 ananicy-cpp"
    fi
    exit 0
fi

if ! systemctl is-active --quiet ananicy-cpp 2>/dev/null; then
    echo "本机没有在运行 ananicy-cpp，无需修复。"
    echo "（若确认装了但没起：systemctl status ananicy-cpp 查看原因）"
    exit 0
fi

mkdir -p "$RULES_DIR"
cat > "$RULES_FILE" <<'EOF'
# Elsewhen 桌面应用 + Flutter 工具链：豁免 Heavy_CPU 降权。
# 00-default 的 dart_flutter.rules 把 flutter/dart 全族压到 nice 9，
# 而 debug 运行（JIT+asserts+增量编译）最吃 CPU，被压后帧调度饿死——表现为
# debug 启动/使用明显卡顿。99- 目录数字更大、加载更晚，覆盖默认规则。
# flutter_tester 是 flutter test 的宿主进程，一并豁免。
{ "name": "elsewhen_ui", "nice": -5 }
{ "name": "elsewhen",    "nice": -5 }
{ "name": "flutter",     "nice": 0 }
{ "name": "dart",        "nice": 0 }
{ "name": "dartvm",      "nice": 0 }
{ "name": "dartaotruntime", "nice": 0 }
{ "name": "flutter_tester", "nice": 0 }
EOF
echo "已写入 $RULES_FILE"

systemctl restart ananicy-cpp
echo "已重启 ananicy-cpp"

# 即时纠正已在运行的进程，免得还要重启应用/开发会话。
renice_one() {
    local pgrep_pat="$1" nice_val="$2"
    local pids
    pids=$(pgrep -x "$pgrep_pat" 2>/dev/null || true)
    [ -n "$pids" ] || return 0
    # shellcheck disable=SC2086 —— 多 PID 逐个传给 renice
    renice -n "$nice_val" -p $pids >/dev/null
    echo "已调整 $pgrep_pat -> nice $nice_val"
}
renice_one elsewhen_ui -5
renice_one elsewhen -5
renice_one flutter 0
renice_one dart 0
renice_one dartvm 0
renice_one dartaotruntime 0
# dart 二进制以解释器名出现在进程表（如 frontend_server 的 dartaotruntime），
# pgrep -x 覆盖不到的残留用 -f 补一轮（仅限本用户，避免碰到系统进程）。
for pid in $(pgrep -u "${SUDO_USER:-$USER}" -f 'frontend_server|flutter_tools' 2>/dev/null || true); do
    renice -n 0 -p "$pid" >/dev/null
done

echo
echo "完成。验证：ps -o pid,ni,cmd -p \$(pgrep -x elsewhen_ui) 的 NI 列应为 -5。"
