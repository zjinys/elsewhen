#!/bin/bash
# Elsewhen - 启动 Capture 模式（Alfred 风格快速输入）

cd "$(dirname "$0")/ui"
# capture 模式同样是桌面入口（CaptureScreen 依赖 window_service → nativeapi）。
fvm flutter run -d linux --target=lib/main_desktop.dart --dart-entrypoint-args "--mode=capture"
