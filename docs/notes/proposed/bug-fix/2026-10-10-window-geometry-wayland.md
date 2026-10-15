# Agent Note: Linux/Wayland 下窗口位置记忆失效

Status: 待解决

> 2026-10-10：已用「关闭时补存」做了一层规避（见文末「规避落地记录」），但用户实测**仍不能正确恢复**，
> 故重新打开为待解决。需要进一步实机排查根因。

## Problem

桌面端「记住上次窗口位置和大小」（存到 `window.json`，下次启动恢复）在用户的机器上失效：
每次启动窗口都回到默认落位，而不是上次关闭时的位置/大小。

环境：**KDE Plasma + Wayland**（`XDG_SESSION_TYPE=wayland`，`XDG_CURRENT_DESKTOP=KDE`），
`window_manager 0.5.2`，Flutter 3.47.6，Linux x64 debug。

## 已查证的事实

实证（诊断日志 `[windiag]`，见 `agent-debug.log` 历史）：

1. **保存侧断在事件派发**：`onWindowMoved` / `onWindowResized` 在 Wayland 下**完全不派发**
   （`_onMovedOrResized` 一次都没 fire）。这是 Wayland 协议限制——compositor 不向应用暴露
   窗口实时位置，window_manager 无从转发。因此「移动/缩放时防抖保存」这条路径在 Wayland 下是死的。
   证据：`window.json` 修改时间停留在 2026-10-01（约是切到 Wayland 之前），此后移动窗口不再更新。

2. **`onWindowClose` 会派发，且 `getBounds()` 在 Wayland 能拿到正确几何**。
   实测关闭时拿到 `Rect.fromLTRB(45, 45, 1511, 1125)`（即 45,45 / 1466×1080），`window.json`
   成功写入（修改时间更新为当天）。

3. 存储/读取本身没问题：`window_geometry_store.dart` 的 `save`/`load`/`computeRestoreBounds`
   逻辑完整；`restoreMainBounds()` 在 `applyMainChrome()` 里有被调用；`initialize()`（挂 listener）
   在 `appInitializationProvider`（`main.dart:84` watch）里被执行。

## 已做的规避（不充分）

`window_service_desktop.dart` 的 `_onClose()` 里加了 `_saveGeometryNow()`：关闭那一刻立即保存
当前几何（不防抖）。**已验证这一步能写对 window.json**（事实 2）。

## 但用户实测：还是不行

关闭时明明写入了正确的 `window.json`，下次启动**窗口仍不在该位置**。说明问题不止保存侧——
**恢复侧在 Wayland 下也没生效**。待查的疑点（按可能性排序）：

- **`setBounds()` 在 Wayland 下被 compositor 忽略**：Wayland 下应用**无法自设窗口位置**
  （xdg-shell 没有 set_position 请求，位置由 compositor 决定）。`restoreMainBounds()` 调
  `windowManager.setBounds(...)` 设了 size 但 **position 大概率被忽略**。这能解释「大小似乎对、
  位置总是回默认」。需实机验证：恢复时打日志看 `setBounds` 后的实际 `getBounds`。
- **`computeRestoreBounds` 拒绝了恢复**：保存的中心点若被判为「不在任何可见区」（多屏/缩放/
  `screenRetriever` 在 Wayland 下拿到的 display 几何异常），会返回 null → 退回默认居中。
  需打日志确认 `restoreMainBounds` 走到了哪一分支（load null? displays 异常? compute null? setBounds?）。
- **时序**：`setBounds` 在 `waitUntilReadyToShow` 之前调用，Wayland 下此时窗口尚未 map，
  位置请求可能被丢弃。

## Proposed Fix（方向，未定）

- **先补恢复侧诊断**：`restoreMainBounds()` 各分支加日志（load 结果 / displays 数量与可见区 /
  compute 结果 / setBounds 后回读），实机跑一次定位断在哪。
- 若确诊是「Wayland 无法设位置」：这是协议级限制，**位置记忆在 Wayland 下本质不可行**，
  只能记住**窗口大小**（size 可以设）。退而求其次：Wayland 下只恢复 size，位置交给 compositor；
  或文档注明该限制。
- 若用户接受，可提供 `GDK_BACKEND=x11` 启动选项走 XWayland，恢复完整的几何记忆（位置+大小）。
  入口脚本 `scripts/elsewhen.sh` 加环境变量即可，但失去 Wayland 原生（高分屏缩放可能略糊）。

## 涉及文件

- `ui/lib/utils/window_service_desktop.dart`（`_onClose` / `_saveGeometryNow` / `restoreMainBounds` / `_scheduleGeometrySave`）
- `ui/lib/utils/window_geometry_store.dart`（save / load / computeRestoreBounds / resolveDataDir）
- `ui/lib/main_desktop.dart`、`ui/lib/providers/app_provider.dart`（启动编排 / initialize 触发）
- 数据文件：`~/.local/share/elsewhen/window.json`
