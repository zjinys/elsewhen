#!/bin/bash
# Elsewhen - 启动主应用

cd "$(dirname "$0")/ui"
fvm flutter run -d linux
