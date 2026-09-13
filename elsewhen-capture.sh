#!/bin/bash
# Elsewhen - 启动 Capture 模式（Alfred 风格快速输入）

cd "$(dirname "$0")/ui"
fvm flutter run -d linux --dart-entrypoint-args "--mode=capture"
