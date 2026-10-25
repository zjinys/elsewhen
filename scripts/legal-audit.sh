#!/usr/bin/env bash
# legal-audit.sh - 隐私/合规红线回归检查（建议进 CI 或 Makefile）
#
# 背景：2026-10-25 六项法律风险审计的结论是 elsewhen 当前六个风险全部不成立：
# 本地优先、无注册、无遥测、无订阅、无营销邮件、字体零远程加载。
# 靠的是一个不变量：「用户数据与外部内容都在本地闭环；出网的只有用户显式
# 触发的 AI 请求、DoH 查询、relay 端到端加密信箱」。
#
# 本脚本把不变量变成回归测试：一旦有人引入 SDK 依赖或远程资源，CI 立即报错，
# 而不是等发布前人工审计才发现。审计记录与未来功能的自查清单见 docs/legal/。

set -uo pipefail
cd "$(dirname "$0")/.."

fail=0

# 检查目录下是否存在不该存在的模式（排除第三方与生成物目录）。
# 命中即报 ❌：这些类别的任何引入都要先过合规评审（docs/legal/）。
forbidden() { # $1=说明 $2=模式 $3=目录...
    local desc="$1" pattern="$2"; shift 2
    local hits
    hits=$( { for d in "$@"; do grep -rEinl "$pattern" "$d" 2>/dev/null; done; } |
        grep -vE 'third_party|node_modules|\.fvm/|google-libs|/target/|\.claude/' |
        sort -u | head -5 )
    if [ -n "$hits" ]; then
        echo "❌ $desc"
        echo "$hits" | sed 's/^/    /'
        fail=1
    else
        echo "✅ $desc"
    fi
}

# 1. 字体 self-host：不引 google_fonts 包、不远程加载任何字体/CDN 资源
forbidden "字体：未引入 google_fonts 包" \
    'package:google_fonts|GoogleFonts\.' ui/lib ui/pubspec.yaml
forbidden "字体：无远程字体 URL / CDN @import" \
    'fonts\.googleapis\.com|fonts\.gstatic\.com|@import[[:space:]]+url\(' ui/ui/web ui/lib ui/web

# 2. 会话重放/遥测：不允许任何采集型 SDK（诊断只走本地 stderr，ELSEWHEN_DEBUG gate）
forbidden "遥测：无 sentry/firebase/crashlytics 等采集 SDK（Dart）" \
    'package:(sentry|firebase|crashlytics|amplitude|smartlook|logrocket|fullstory|datadog)' ui/lib ui/pubspec.yaml
forbidden "遥测：Rust 侧无上报依赖" \
    'sentry|amplitude_sdk|datadog' src/

# 3. 订阅/计费：引入任何支付封装时必须同步交付续约条款展示（ROSCA/应用商店要求）
forbidden "计费：无 in_app_purchase/RevenueCat 等支付依赖" \
    'package:in_app_purchase|package:google_mobile_ads|package:purchases_flutter' ui/lib ui/pubspec.yaml

# 4. 营销邮件：本 app 不发邮件；引入邮件能力时必须带退订链接 + 邮政地址（CAN-SPAM）
forbidden "邮件：relay 无 SMTP/nodemailer 发信能力" \
    'smtp|nodemailer|sendmail' relay/standalone relay/test

echo
if [ "$fail" = 1 ]; then
    echo "审计失败：发现违反隐私不变量的引入。处理办法见 docs/legal/。"
    exit 1
fi
echo "审计通过：字体 / 遥测 / 计费 / 邮件 四类红线当前均未引入。"
