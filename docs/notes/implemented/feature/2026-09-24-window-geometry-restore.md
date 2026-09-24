# 2026-09-24 主窗口大小/位置恢复（window.json，不落库）

## 需求
- 重新打开 App 时恢复上次的主窗口大小/位置
- 用户明确：这个「应该不用保存数据库」→ 落一个本地配置文件即可

## 方案
- 新增 `ui/lib/utils/window_geometry_store.dart`：
  - `WindowGeometry`（x/y/width/height）+ `tryParse` 容错解析（字段缺失/类型错/尺寸非法 → null）
  - `WindowGeometryStore`：读写应用数据目录下的 `window.json`，与 elsewhen.db 同目录但**不进库**
  - `resolveDataDir()` 规则与 Rust 侧 `src/config.rs` 的 `AppConfig::load` 保持一致：
    `ELSEWHEN_DATA_DIR` 环境变量优先 → Linux `$XDG_DATA_HOME|~/.local/share/elsewhen` →
    macOS `~/Library/Application Support/dev.elsewhen.elsewhen` → Windows
    `%LOCALAPPDATA%\dev\elsewhen\elsewhen`
  - `computeRestoreBounds()` 纯函数夹取逻辑（可单测，见下）
- `window_service.dart`：
  - `restoreMainBounds()`：启动时调用；多显示器用 `screenRetriever.getAllDisplays()`
    的 visible 区参与判定；失败（非桌面/无历史/显示器布局变了）返回 false → 调用方退回居中
  - `_scheduleGeometrySave()`：`onWindowMoved` / `onWindowResized` 防抖 400ms 落盘；
    跳过最大化/全屏事件（记录铺满状态没意义）
- `main.dart`：`waitUntilReadyToShow` 里 `show()` → `restoreMainBounds()` → 失败才 `center()`

## 关键取舍
- 恢复失败一律退回默认居中，**绝不把窗口摆到屏幕外**或带着坏数据 setBounds
- 目标屏以保存的窗口「中心」落在哪个可见区判定（多屏下恢复的目标屏未必是主屏；
  拔了外接屏中心落在空区 → 放弃恢复）：这是两次测试失败的根因，非实现缺陷
- 尺寸夹 `[mainWindowMinSize, 目标可见区]`；位置夹到完全可见
- 不存库、不加 Rust bridge（`frb_generated`/`api.rs` 正被并行会话改动，避免碰编译产物）
- capture→main 的热键切换（`switchToMainMode`）**不改**：临时小窗流程仍 setSize+center，
  只有进程级重启才恢复几何——避免改变既有切换行为（潜在 follow-up）

## 验证
- `ui/test/window_geometry_store_test.dart` 14 项：
  JSON 解析容错 / 文件存取往返与损坏兜底 / 父目录自动创建 / computeRestoreBounds
  边界（小于最小尺寸、超可见区、右缘/上缘出屏推回、多屏落副屏、拔屏返回 null、无可见区兜底）
- `flutter analyze` 4 个相关文件 0 issue；`dart format` 通过
- 窗口真实行为需构建后目验：拖动/缩放 → 重启 → 位置尺寸复现