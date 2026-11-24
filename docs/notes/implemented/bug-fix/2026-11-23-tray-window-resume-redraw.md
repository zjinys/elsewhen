# Agent Note: 托盘唤回复用主窗口并恢复绘制

Status: implemented

## Problem

用户反馈从托盘唤回主窗口时只显示黑色窗口。检查发现 `switchToMainMode` 仍保留已经删除的 capture 小窗切换流程：先 hide，再改最小尺寸、尺寸、居中及窗口属性，最后 show/focus。它也用于托盘菜单和单实例唤回，因而在并无模式切换需求时重复隐藏、映射窗口，并改变用户当前几何。

另发现本机 SDK 与产物不一致：项目 `ui/.fvmrc` 已声明 3.50.1，本地 `.fvm/flutter_sdk` 却链接 3.50.0；重建前 release 包的 `libflutter_linux_gtk.so` 校验值与本机 3.47.9 完全相同，既不是 3.50.0 也不是 3.50.1。这证明本地产物陈旧，不证明用户正在运行的进程一定使用该包，也不足以独立判定此次黑屏的唯一根因。

用户随后反馈仍然黑屏。现场只读检查确认当前进程运行 debug 包，其引擎校验值已经与 3.50.1 一致。独立 GTK/Flutter 测试窗在同一 Wayland 桌面复现：隐藏时 GTK 销毁主 `wl_surface`；恢复时 Flutter renderer 的 map 先于 GTK 主表面创建，旧子表面被释放，新子表面却未建立。没有尺寸变化时也不会自动重新分配 renderer 的布局，持续请求 Dart 帧仍不能修复表面归属。

## Decision

主窗口只有一种模式。`switchToMainMode` 直接复用 `showWindow`，不再 hide、resize、center 或重设装饰。最小化恢复继续由 `window_manager.show` 已有的 isMinimized/restore 流程处理，不重复调用。成功唤回将 `currentMode` 恢复为 main，使后续几何保存继续生效；`closeWindow` 统一走隐藏状态维护。

合并并发唤回请求，失败后释放在途状态以允许重试。移除上一轮新增的 Dart 强制重绘和 show/restore/focus 事件刷新；这些处理未解决用户黑屏。

Linux runner 在首次 show 完成后，仅对 Wayland 注册主窗口 map 的 after 回调。在 GTK 已重新建立主表面之后，对 FlView 原生控件树调用 `gtk_widget_queue_allocate`，让 Flutter renderer 在新主表面上重新建立子表面。仅给 FlView 根控件排队不足以触发未改尺寸的 renderer，故逐个标记其原生子控件；它不是 Flutter 业务组件树。辅助代码放在 `window_restore.h`，由正式 runner 和原生测试共用。不销毁窗口或引擎、不修改尺寸或装饰、不加延时、轮询或 Dart 定时重绘；X11/macOS/Windows 不增加此回调。

将本地 FVM 链接对齐项目已声明且已缓存的 3.50.1，并使用桌面入口重建 debug/release；没有另行提高项目声明的 SDK 版本，也没有修改 Flutter SDK 源码。

## Alternatives considered

- 保留 hide/resize/show 并加延时：旧小窗模式已删除，额外重映射与尺寸操作没有产品用途，延时也无法证明渲染表面已就绪。
- Dart 强制刷新：上一轮已尝试且用户复测失败。独立测试即使每秒强制提交 Dart 帧，恢复后也没有新子表面；删除这些补偿逻辑。
- 只对 FlView 根控件 queue_allocate：独立测试未重新建立子表面。实际 renderer 的原生布局也需标记，才会运行引擎已有的父表面检测与重挂逻辑。
- 强制 X11、软件绘制或关闭 Impeller：未通过现场证据定位到这些后端问题，不改变既有平台选择来掩盖症状。
- 只改 Dart 后热重载旧 release：不能更新正在运行进程的原生引擎，需构建并完全退出旧进程后复测。

## Verification

- Flutter 3.50.1 下定向 **25 passed、0 failed**：6 项恢复测试，加既有托盘语言、窗口几何、桌面偏好回归。覆盖不隐藏/改几何、隐藏状态回主窗、最小化恢复顺序、并发合并、失败重试，以及事件和恢复不再强制安排 Dart 帧、不伪造生命周期。
- 原生对照测试使用 `ui/test/linux/window_restore_probe.cc` 和静止的 `window_restore_probe.dart`，在当前 KDE/Wayland 上各连续隐藏/恢复五次。旧流程首次启动有内容帧，五次恢复均没有新子表面和内容帧；接入正式代码共用的 map 回调后，首次启动和五次恢复均重新建立子表面并 attach 内容 buffer。测试不启动 Rust、不读个人数据库、不依靠定时 Dart 重绘。`scripts/check-window-restore-trace.mjs` 逐次检查协议日志，基线退出码为 1，修正后为 0。
- 限定范围 `flutter analyze` 无问题，`dart format` 通过。Linux debug/release 均使用 `--target=lib/main_desktop.dart` 构建通过；已有 appindicator 弃用警告未改。
- 新 release 包的引擎 SHA-256 与本机 3.50.1 缓存相同：`8fc5c4ee6a20215f54c332c7b8910c482434247e451f24cfd7d3225f0a28066b`。
- 验证覆盖真实 Wayland 表面的创建及内容 buffer 提交，但不等于已观察用户完整业务窗口的视觉结果。未操控用户正在使用的主窗口、未读取个人数据库。原生代码需完全退出旧进程后重启，热重载无法加载此修正。

```sh
cd ui
fvm flutter test --no-pub test/tray_window_restore_test.dart test/tray_locale_test.dart test/window_geometry_store_test.dart test/desktop_prefs_test.dart
fvm flutter analyze --no-pub lib/utils/window_service_desktop.dart test/tray_window_restore_test.dart test/tray_locale_test.dart
fvm flutter build linux --release --no-pub --target=lib/main_desktop.dart
```

原生回归在 Wayland 桌面执行，会短暂打开无个人内容的独立绿色测试窗：

```sh
cd ui
probe_dir=$(mktemp -d /tmp/elsewhen-window-restore.XXXXXX)
engine_dir=$(readlink -f .fvm/flutter_sdk)/bin/cache/artifacts/engine/linux-x64
fvm flutter build bundle --debug --no-pub --target-platform=linux-x64 --target=test/linux/window_restore_probe.dart --asset-dir="$probe_dir/assets"
g++ -Wall -Wextra -Werror test/linux/window_restore_probe.cc -Ilinux/runner -Ilinux/flutter/ephemeral -L"$engine_dir" -Wl,-rpath,"$engine_dir" -lflutter_linux_gtk $(pkg-config --cflags --libs gtk+-3.0) -o "$probe_dir/probe"
WAYLAND_DEBUG=1 "$probe_dir/probe" "$probe_dir/assets" "$engine_dir/icudtl.dat" --baseline > "$probe_dir/baseline.log" 2>&1
node ../scripts/check-window-restore-trace.mjs "$probe_dir/baseline.log" # 预期退出码 1
WAYLAND_DEBUG=1 "$probe_dir/probe" "$probe_dir/assets" "$engine_dir/icudtl.dat" > "$probe_dir/fixed.log" 2>&1
node ../scripts/check-window-restore-trace.mjs "$probe_dir/fixed.log" # 预期退出码 0
```

## Consequences

托盘和单实例入口使用相同、可重试的 show/focus 流程，保留当前窗口几何；GTK 在重新映射完成后让 Flutter 恢复表面归属。代价是 Wayland 每次重新映射会安排一次内容原生控件树的布局，无常驻定时器或额外 Dart 事件刷新。

工程修正、本地构建和独立原生对照测试完成，用户完整主窗口的最终视觉验收仍待重启后确认。上次关闭位置恢复的独立问题继续保留在[Wayland 几何问题](../../proposed/bug-fix/2026-10-10-window-geometry-wayland.md)，本次不宣称解决它。
