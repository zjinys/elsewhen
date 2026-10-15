# Agent Note: 快速捕获（tray 菜单 / 单实例转发）切不到 CaptureScreen UI

Status: 待解决

## Problem

tray 图标菜单里的「快速捕获」点出来的窗口是**错的**：出现一个 500×240、置顶、居中的小窗口，
但里面塞的仍是**完整主界面（MainScreen）**，而不是快速捕获输入框（CaptureScreen）。

用户判断：快速捕获本质是**全局快捷键**场景——在任何 app 里按快捷键弹出小输入框、记一条、
回车即走。放在 tray 菜单里意义不大（点 tray 菜单时本就能直接开主窗，多一步）。记录待后续决策。

## Root Cause（已定位，架构层）

capture 模式的 **UI（CaptureScreen）** 和 **窗口外观（小尺寸/置顶/居中）** 是两套，且只有外观能运行期切换：

1. **UI 渲染只认启动时的 `config.mode`**：`ui/lib/main.dart:149`
   `widget.config.mode == AppMode.capture && captureBuilder != null ? captureBuilder() : MainScreen()`。
   `config` 来自 `AppConfig.fromArgs(args)`（`main_desktop.dart:31`），**运行期不可变**。

2. **正常启动 `./scripts/elsewhen.sh` → `config.mode = main` → 永远渲染 MainScreen**。

3. tray「快速捕获」→ `switchToCaptureMode()`（`window_service_desktop.dart:306`）只做了
   `setSize(500,240) + setAlwaysOnTop + center + show + focus`，**改不了 `config.mode`**，
   所以 UI 还是 MainScreen → 「小窗口装主界面」。

4. **单实例转发同样无效**：第二个进程 `--mode=capture` 启动 → `SingleInstance.ensurePrimary`
   把 `capture` 命令转发给主进程（`main_desktop.dart:41-49`），主进程 `onCommand('capture')`
   也是调 `switchToCaptureMode()`——同样只改外观，UI 不切。

5. **`WindowService.currentMode` 只是窗口外观状态**（main/capture/hidden），
   **没有任何 UI / provider 读它来切换 CaptureScreen**。全局搜证：
   `main.dart:149` 是唯一决定渲染哪个 Screen 的地方，只认 `config.mode`。

结论：单进程内**根本切不到 CaptureScreen**，这是设计缺口，不是某个函数写错。

## Proposed Fix（方向，未定，待与用户确认）

**方案 1（推荐）= 引入运行期 mode provider**：
- 新增一个 `windowModeProvider`（StateProvider/Riverpod），初始值取 `config.mode`。
- `main.dart:149` 的渲染判断从「只看 `config.mode`」改成「watch 该 provider」。
- `switchToCaptureMode()` / `switchToMainMode()` 除了改窗口外观，同时 set 该 provider → UI 真正切换。
- 这样三条入口全通：tray 菜单、二次启动单实例转发、（未来）全局快捷键。

**方案 2（配套，待确认）**：
- **全局快捷键**（如 `Ctrl+Shift+E`）触发快速捕获——这才是它的主场。需要快捷键插件
  （如 `hotkey_manager`）或平台通道，跨 Linux/macOS/Windows。
- tray 菜单的「快速捕获」**保留 or 删除**由用户定（用户认为意义不大）。

## 涉及文件

- `ui/lib/main.dart:149`（渲染哪个 Screen 的唯一判断点）
- `ui/lib/main_desktop.dart`（`config.mode` 解析、单实例 `onCommand` 转发）
- `ui/lib/utils/window_service_desktop.dart`（`switchToCaptureMode` / `switchToMainMode` / tray 菜单 `_initTray`）
- `ui/lib/utils/window_service.dart` / `window_service_stub.dart`（`currentMode` 抽象）
- `ui/lib/screens/capture_screen.dart`（快速捕获 UI 本体）
- 待新增：运行期 mode provider（可放 `ui/lib/providers/`）
