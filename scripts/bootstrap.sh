#!/usr/bin/env bash
# bootstrap.sh - 新环境一键准备并启动（Linux / macOS）
#
# 解决的问题：新 clone 出来的仓库直接跑 scripts/elsewhen.sh 一定失败，因为
# elsewhen.sh 假定三件事已经就绪，而这三样都不在 git 里：
#
#   1. target/release/libelsewhen.so —— target/ 被 .gitignore 排除。Dart 侧
#      RustLib.init() 走 frb 默认加载链（ioDirectory = ../target/release/，
#      见 ui/lib/bridge/rust_bridge_repository.dart 的注释），文件不在时它抛的是
#      一句 dlopen 失败，看不出「该先编 Rust」。
#   2. ui/.fvm/flutter_sdk —— .fvm/ 同样被 gitignore，库里只有 ui/.fvmrc（钉住
#      Flutter 版本）。fvm flutter 会直接报没有配置 SDK。
#   3. ui/.dart_tool/package_config.json —— pub get 从没跑过。
#
# 所以本脚本 = 预检 + 装 SDK + pub get + 编 Rust + 校验桥接 hash + 启动。
# 之后日常开发仍然用 scripts/elsewhen.sh（更快，它不做这些）。
#
# 前提：系统依赖与工具链已按 docs/development/build-environment.md 装好。
# 本脚本**不装任何系统包、不装 Flutter/Rust** —— 那些要么要 sudo，要么要交互式
# 安装器（msiexec / xcode-select），代劳只会把「环境没配好」变成「脚本坏了」。
# 它只做一件装包做不到的事：在动手之前把缺的东西指出来，并指向文档对应小节。
#
# 用法：
#   scripts/bootstrap.sh                     # 准备 + 启动（桌面）
#   scripts/bootstrap.sh --setup-only        # 只准备，不启动
#   scripts/bootstrap.sh --regen             # 先重生成桥接（改了 Rust 公开 API）
#   scripts/bootstrap.sh --device android    # 换设备；其余参数透传给 flutter run
#   scripts/bootstrap.sh -- --dart-define=X  # 需要显式 -- 才是分隔符，但直接传也行

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# ---------------------------------------------------------------------------
# 参数
# ---------------------------------------------------------------------------
SETUP_ONLY=0
REGEN=0
DEVICE=""
RUN_ARGS=()

# 打印文件头那段注释（到第一个非注释行为止），改头部时不必同步改行号。
usage() {
    awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"
}

while [ $# -gt 0 ]; do
    case "$1" in
        --setup-only)
            SETUP_ONLY=1
            shift
            ;;
        --regen)
            REGEN=1
            shift
            ;;
        --device)
            DEVICE="${2:?--device 需要一个设备 id（flutter devices 可列全部）}"
            shift 2
            ;;
        --device=*)
            DEVICE="${1#*=}"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        --)
            shift
            RUN_ARGS+=("$@")
            break
            ;;
        *)
            RUN_ARGS+=("$1")
            shift
            ;;
    esac
done

# 透传参数里已经带了 -d/--device 就不加默认值，否则会同时出现两个 -d。
DEVICE_IN_ARGS=0
for a in ${RUN_ARGS+"${RUN_ARGS[@]}"}; do
    case "$a" in
        -d|--device) DEVICE_IN_ARGS=1 ;;
        --device=*) DEVICE_IN_ARGS=1 ;;
    esac
done

# ---------------------------------------------------------------------------
# 输出helpers
# ---------------------------------------------------------------------------
step() { echo ""; echo "[$1/5] $2"; }
note() { echo "      $1"; }

# 报错时把「装什么、去哪看」一起打出来。单独 exit 1 只会让用户自己去猜缺什么。
die() {
    echo "" >&2
    echo "✗ $1" >&2
    shift
    for line in "$@"; do echo "  $line" >&2; done
    echo "" >&2
    exit 1
}

require_cmd() {
    command -v "$1" >/dev/null 2>&1 && return 0
    local hint="$2"
    die "找不到命令：$1" "$hint"
}

# ---------------------------------------------------------------------------
# [1/5] 预检
#
# 检查项都对着 CMake 的真实查找逻辑挑，不是「常见工具清单」：
# ui/linux/CMakeLists.txt:57-58 要 pkg-config + gtk+-3.0，
# cnativeapi-0.4.0/cxx_impl/src/CMakeLists.txt:34-37（仅 Linux 分支）要
# pkg-config + gtk+-3.0 + x11 + xi。缺任何一个的真实症状都是 CMake 在
# configure 阶段报错，而那已经在几分钟的 cargo build 之后了。
# ---------------------------------------------------------------------------
step 1 "预检工具链与系统依赖"

require_cmd cargo "装法：https://rustup.rs —— 仓库没有 rust-toolchain.toml，跟随 stable"
# fvm 不在这里要求：ui/.fvm 已经解析出钉住的 SDK 时根本用不到它（见 [2/5]）。

case "$(uname -s)" in
    Linux)
        require_cmd pkg-config "Debian/Ubuntu: sudo apt install pkg-config"
        require_cmd cmake "Debian/Ubuntu: sudo apt install cmake"
        # 二进制名不统一：Debian/Ubuntu 的包是 ninja-build，Fedora/Arch 是 ninja，
        # 所以两个名字都查（cmake 两个都认）。
        command -v ninja >/dev/null 2>&1 || command -v ninja-build >/dev/null 2>&1 \
            || die "找不到 ninja 或 ninja-build" "Debian/Ubuntu: sudo apt install ninja-build"
        for m in gtk+-3.0 x11 xi; do
            pkg-config --exists "$m" 2>/dev/null || die "pkg-config 找不到 $m" \
                "Debian/Ubuntu: sudo apt install libgtk-3-dev libx11-dev libxi-dev" \
                "Fedora: sudo dnf install gtk3-devel libX11-devel libXi-devel" \
                "Arch: sudo pacman -S --needed gtk3 libx11 libxi" \
                "完整清单见 docs/development/build-environment.md §2.1"
        done
        note "Linux 系统依赖 OK（gtk+-3.0 / x11 / xi / cmake / ninja）"
        DEFAULT_DEVICE="linux"
        ;;
    Darwin)
        xcode-select -p >/dev/null 2>&1 \
            || die "缺 Xcode 命令行工具" "装法：xcode-select --install"
        note "macOS 系统依赖 OK（Xcode CLT）"
        DEFAULT_DEVICE="macos"
        ;;
    *)
        note "未识别的平台 $(uname -s)，跳过系统依赖检查"
        DEFAULT_DEVICE=""
        ;;
esac

# 桌面入口：main_desktop.dart 含 nativeapi 的窗口 chrome 初始化；main.dart 不含
# （Android 也用它，因为 nativeapi 的 FFI union 会把 Android release AOT 编译器打崩）。
# flutter run 不带 --target 时默认构建 lib/main.dart，所以这里必须显式带上。
[ -f ui/lib/main_desktop.dart ] || die "找不到 ui/lib/main_desktop.dart" "仓库不完整？"

if [ -z "$DEVICE" ] && [ "$DEVICE_IN_ARGS" -eq 0 ]; then
    DEVICE="$DEFAULT_DEVICE"
fi

# ---------------------------------------------------------------------------
# [2/5] Flutter SDK
#
# 解析顺序与 elsewhen.ps1 / package-msi.ps1 一致：ui/.fvm 里钉住的版本 →
# 全局 fvm → 全局 flutter。已经从 .fvmrc 解析出来时不再调 fvm install，
# 这样「装了 Flutter 但没装 fvm」的机器也能一条命令跑起来。
#
# 只在 ui/ 下解析 .fvmrc —— 应用从 ui/ 构建，库根的 .fvmrc（3.47.4）与
# ui/.fvmrc（3.47.5）目前不一致，这个漂移记录在 build-environment.md §1.2。
# 这里把实际解析到的版本打出来，漂移就在第一次运行时暴露，而不是靠猜。
# ---------------------------------------------------------------------------
step 2 "解析 Flutter SDK"

FLUTTER_SDK="$ROOT/ui/.fvm/flutter_sdk"
if [ -x "$FLUTTER_SDK/bin/flutter" ]; then
    FLUTTER=("$FLUTTER_SDK/bin/flutter")
    note "用已解析的 ui/.fvm（无需 fvm）"
elif command -v fvm >/dev/null 2>&1; then
    note "fvm install（读 ui/.fvmrc）"
    (cd ui && fvm install)
    FLUTTER=(fvm flutter)
    if [ ! -x "$FLUTTER_SDK/bin/flutter" ]; then
        die "fvm install 之后仍没有 ui/.fvm/flutter_sdk" \
            "先手工看一眼 ui/.fvmrc 的钉住版本是否可安装：cd ui && fvm install"
    fi
elif command -v flutter >/dev/null 2>&1; then
    FLUTTER=(flutter)
    note "⚠ 没有 ui/.fvm 也没有 fvm，改用全局 flutter —— 版本不再由 ui/.fvmrc 钉住。"
    note "  正常做法：dart pub global activate fvm"
else
    die "找不到 Flutter" \
        "三条路任选其一：" \
        "  1) dart pub global activate fvm（PATH 里需有 ~/.pub-cache/bin），本脚本会自动 fvm install" \
        "  2) 自己装好 Flutter 并在 ui/ 下跑一次 fvm install" \
        "  3) 只做校验不启动：装全局 flutter 后加 --setup-only"
fi

note "版本：$(cd ui && "${FLUTTER[@]}" --version 2>/dev/null | head -1)"

# ---------------------------------------------------------------------------
# [3/5] Dart 依赖
# ---------------------------------------------------------------------------
step 3 "拉取 Dart 依赖（pub get）"
(
    cd ui
    "${FLUTTER[@]}" pub get
)

# ---------------------------------------------------------------------------
# [4/5] Rust 桥接库
#
# 必须是 --release：Dart 侧 frb 默认从 ../target/release/ 加载（不是 debug），
# 打成 debug 会出现 `Content hash on Dart side ... is different from Rust side`。
# 每次都跑而不是「.so 不存在才跑」：cargo 自己的 fingerprint 会让无改动的重复
# 构建耗时接近 0，而跳过它意味着改了 Rust 代码就静默跑旧库。
# ---------------------------------------------------------------------------
step 4 "编译 Rust 桥接库（cargo build --release --lib）"
if [ "$REGEN" -eq 1 ]; then
    note "--regen：先重新生成 Dart/Rust 桥接代码"
    ./scripts/regen.sh
else
    cargo build --release --lib
fi

# ---------------------------------------------------------------------------
# [5/5] 桥接 hash 校验
#
# 比对 Dart 侧 rustContentHash 与 src/frb_generated.rs 里的常量。刚编完的 .so
# 里烧的就是后者，所以源码层面相等 ⟺ 运行时相等。这是不一致时**唯一**能提前
# 抓到它的时机：否则要等到 app 启动时 RustLib.init() 抛 hash mismatch，
# 而那句话完全看不出该跑 regen。
# ---------------------------------------------------------------------------
step 5 "校验桥接 codegen hash"

DART_HASH_FILE="ui/lib/bridge/generated.dart/frb_generated.dart"
RUST_HASH_FILE="src/frb_generated.rs"
[ -f "$DART_HASH_FILE" ] || die "找不到 $DART_HASH_FILE" \
    "桥接生成代码不在库里？正常 clone 应该带着它。"
[ -f "$RUST_HASH_FILE" ] || die "找不到 $RUST_HASH_FILE" "仓库不完整？"

# 第二次 grep 必须带 `$` 锚：`i32 = -692400254` 里 `i32` 的 `32` 也是一个
# 「数字」，不带锚会抽出 32\n-692400254 两行，变量变成多行字符串。
dart_hash=$(grep -oE 'rustContentHash => -?[0-9]+' "$DART_HASH_FILE" | head -1 | grep -oE -- '-?[0-9]+$' || true)
rust_hash=$(grep -oE 'FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH: i32 = -?[0-9]+' "$RUST_HASH_FILE" | head -1 | grep -oE -- '-?[0-9]+$' || true)

[ -n "$dart_hash" ] || die "在 $DART_HASH_FILE 里找不到 rustContentHash" "frb 版本或生成布局变了？"
[ -n "$rust_hash" ] || die "在 $RUST_HASH_FILE 里找不到 FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH" "同上"

if [ "$dart_hash" != "$rust_hash" ]; then
    die "桥接 codegen hash 不一致：Dart=$dart_hash Rust=$rust_hash" \
        "Rust 侧公开 API 变了但桥接代码没重新生成。此时启动会在 RustLib.init()" \
        "抛 'Content hash on Dart side ... is different from Rust side'。" \
        "" \
        "两条出路：" \
        "  1) 跑 scripts/bootstrap.sh --regen（= regen.sh + 重编）" \
        "  2) 直接跑 ./scripts/regen.sh"
fi
note "两侧一致（$dart_hash）"

# ---------------------------------------------------------------------------
# 启动
# ---------------------------------------------------------------------------
if [ "$SETUP_ONLY" -eq 1 ]; then
    echo ""
    echo "✔ 环境就绪，未启动。"
    echo "  下一步：scripts/elsewhen.sh${DEVICE:+ -d $DEVICE}   # 日常启动入口"
    exit 0
fi

echo ""
if [ -z "$DEVICE" ] && [ "$DEVICE_IN_ARGS" -eq 0 ]; then
    die "无法确定运行设备" \
        "本平台没有默认设备（当前系统：$(uname -s)）。" \
        "用 --device <id> 指定，或直接传 -d <id>：flutter devices 可列出全部。"
fi

echo "启动（设备：${DEVICE:-由透传参数指定}）..."
echo "  ⚠ 排查时务必带 --target：elsewhen.sh 与本脚本都已固定用 main_desktop.dart。"
echo ""

cd ui
if [ "$DEVICE_IN_ARGS" -eq 0 ]; then
    "${FLUTTER[@]}" run -d "$DEVICE" --target=lib/main_desktop.dart ${RUN_ARGS+"${RUN_ARGS[@]}"}
else
    "${FLUTTER[@]}" run --target=lib/main_desktop.dart ${RUN_ARGS+"${RUN_ARGS[@]}"}
fi