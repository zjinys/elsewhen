# 2026-09-24 window_manager + screen_retriever → nativeapi 迁移（完成）

## 结论
**已迁移**：window_manager 0.4.3 + screen_retriever 0.2.2 → nativeapi 0.3.0（leanflutter 统一原生 API 继任者）。
本文件前身为「迁移评估：暂缓」笔记（上午），下午用户拍板迁移，复核原生源码后落地。

## 关键转折：Linux 窗口拖拽并非硬阻塞
- 缓评结论的第 2 条（README「Moving via DragToMoveArea is not yet implemented on Linux」
  被误读为低层 `startDragging()` 在 Linux 不可用）经源码复核推翻。
- 原生库 src/platform/linux/window_linux.cpp:1584 `Window::StartDragging()` 完整实现，
  走 `gdk_window_begin_move_drag_for_device`，注释明确 **Wayland 同样可用**
  （WM 接管移动，只要求调用时的鼠标按下时间戳）。README 警告仅指 widget 包装层。
- Linux 真正的 TODO 只有 `SetWindowControlButtonsVisible`（本项目不用，自绘按钮）。
- 迁移时把 `custom_title_bar.dart` 的拖拽从 widget 层改为直接调 `Window.startDragging()`，
  正好避开 README 警告的范围。

## 迁移清单
- `ui/pubspec.yaml`：删 window_manager/screen_retriever，加 `nativeapi: ^0.3.0`（是 ffiPlugin，
  linux/macos/windows 目录齐全，flutter 构建自动打包原生库，无需 CMake 改动）。
- `ui/lib/utils/window_service.dart` 重写：
  - `WindowListener` mixin → `WindowManager.instance.addListener(sealed WindowEvent)` + ListenerId；
  - 显示器信息 `screenRetriever` → `DisplayManager.instance.getPrimary()/getAll()`（`Display.workArea`
    等价旧 visiblePosition+visibleSize）；`clampedMainSize()` 由 `static Future<Size>` 改同步 `static Size`；
  - 启动：`applyMainChrome()` / `applyCaptureChrome()`（取代 WindowOptions+waitUntilReadyToShow）；
  - 事件里 `WindowMovedEvent/WindowResizedEvent → _scheduleGeometrySave()`（防抖 400ms 不变，
    最大化/全屏跳过）；`WindowClosedEvent → hide()` 双保险（isClosable=false 时不会发生）；
  - `switchToMainMode()` 现在优先 `restoreMainBounds()`（与启动一致，无历史才默认尺寸+居中）。
- `ui/lib/main.dart`：去掉 ensureInitialized/WindowOptions/waitUntilReadyToShow/setPreventClose，
  main() 里按 mode 调 applyMainChrome/applyCaptureChrome。setPreventClose 语义 → `isClosable=false`。
- `ui/lib/widgets/custom_title_bar.dart`：`_window` 顶层 getter；startDragging/minimize/
  isMaximized/unmaximize/maximize 同步调用；**关闭按钮从 `close()` 改为 `hide()`**
  （nativeapi 无 close API；isClosable=false 已拦截原生关闭）。
- `ui/lib/main_timeline_example.dart`：死代码示例，最小迁移保持 analyze 干净。

## 行为差异与取舍
| 项 | window_manager | nativeapi |
|----|----------------|-----------|
| 启动显示 | waitUntilReadyToShow 决定 show 时机 | runner 首帧自动 show；main() 提前配置规避原生标题栏/默认尺寸闪现 |
| 窗口就绪 | 回调保证 | `GetCurrent()` 遍历 GTK toplevel，兜底 first_hidden（源码注释确认 Flutter runner 首帧才可见，窗口先于 Dart main 创建） |
| 关闭 | close() → onWindowClose → hide() | isClosable=false + X 按钮 hide()；Alt+F4/WM 关闭被直接拦截 |
| 关闭动画 | window_manager 有 | 无（直接 hide） |
| 捕获窗启动尺寸 | 650x180（分裂） | captureWindowSize 500x240（与 toggle 归一） |
| 事件 | mixin 多回调 | 密封类 switch（编译期穷尽） |

## 验证
- 4 个改动文件 `flutter analyze` 0 issue；无残留 windowManager/screenRetriever 引用。
- `window_geometry_store_test.dart` 14/14 通过；无测试 import 本次改动文件。
- 全量 `flutter test` 当前 29 个失败**全部**为并行会话字体 WIP 的编译错误所致
  （system_fonts.dart `library;` 指令错位 + settings_screen.dart/font_picker_dialog.dart
  半迁引用），日志中 nativeapi/window_service/window_manager 相关错误为 **0**。

## 残留风险（待并行树修复后复验）
1. **Linux 真机目测**：`flutter build/run linux` 目前被 system_fonts.dart 编译错误阻断；
   恢复后需实测标题栏拖拽、几何恢复（window.json）、捕获模式、ALT+F4 拦截。
2. **多平台未实测**：macOS/Windows 的 GetCurrent() 时序（理论上 runner 同样先建窗后跑 Dart）。
3. nativeapi 0.3.0 发布 4 天，仍标 WIP；若上游大改，回滚方式：git 恢复上述 4 文件 + pubspec。

## 附：实测 API 映射（本次核对过 setter 名）
ensureInitialized → 无需（FFI 即用）；waitUntilReadyToShow/WindowOptions → applyMainChrome/applyCaptureChrome；
setSize/getSize → setSize(Size,bool)/size；getBounds/setBounds → bounds 属性；getPosition/setPosition → position 属性；
center → center()；setMinimumSize → minimumSize 属性；setAlwaysOnTop → isAlwaysOnTop；
setTitleBarStyle → titleBarStyle（TitleBarStyle.hidden/normal）；setTitle → title；
show/hide/focus → 同名；isMaximized → isMaximized 属性；maximize/unmaximize/minimize/restore → 同名；
isFullScreen → isFullScreen 属性；startDragging → startDragging()（Linux 已实现）；close → 无（hide/Application.quit）；
setPreventClose → isClosable=false（+ WindowClosedEvent 兜底）；destroy → dispose；skipTaskbar → isVisibleInTaskbar；
监听器 → addListener(WindowEvent) 密封类 switch（ListenerId 记账，removeListener 移除）；
screen_retriever → DisplayManager（Display.size/position/workArea/scaleFactor）。