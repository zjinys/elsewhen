#!/bin/bash
# Elsewhen - 启动主应用
#
# 不要再加「关 Impeller」或「强制软件渲染」这类规避：
# 空窗口的根因是 nativeapi 的 `w.titleBarStyle = hidden` 在 Flutter 建好
# GL 上下文之后才执行，GTK 为此重建窗口 GdkVisual，首帧即
# `Could not determine GL version` → 窗口只有边框没有内容。无边框现改由
# linux/runner/my_application.cc 在 fl_view_new 之前用
# gtk_window_set_decorated(FALSE) 完成，视觉等价且时序安全。
# Impeller 本身在本机完全正常。
#
# 三条已排除的假设（留着以免再走一遍）：
# - 不是环境问题：同机器全新 flutter create 的最小项目用同样 Impeller 后端正常。
# - 不是 Skia 的锅：Skia 后端在这台机上同样失败（gpu_surface_gl_skia.cc
#   "Could not make the context current"），别想着切后端来绕。
# - 不是透明背景的锅：逐项二分显示跳过 backgroundColor 无效，只有 titleBarStyle
#   有效。Linux 上设透明背景实测也正常（圆角效果保留）。
#
# ⚠ 排查时务必带 --target：flutter build/run 不带时默认构建 lib/main.dart
# （不含窗口 chrome），用它验证会得到「全部正常」的假结论。
#
# 桌面入口用 main_desktop.dart（含 nativeapi 窗口 chrome 初始化）；
# main.dart 是移动端/通用入口，不带窗口管理 —— Android 也用它，因为
# nativeapi 的 FFI union 会把 Android release AOT 编译器打崩。
cd "$(dirname "$0")/../ui"
fvm flutter run -d linux --target=lib/main_desktop.dart "$@"
