# Elsewhen 下一步工作记录

更新时间：2026-09-12

## 当前产品决策

- GUI 统一使用 Flutter 构建，目标覆盖桌面端和移动端。
- Rust 继续作为本地核心，负责 SQLite、Event、Conversation、Memory、AI Worker 和规则。
- Flutter 与 Rust 通过 `flutter_rust_bridge` 通信。
- 桌面端使用一个 Flutter 应用窗口，在主应用模式和 Capture 模式之间切换；两种模式互斥。
- Capture 界面完全由 Flutter 渲染；平台层只负责全局快捷键和窗口显示/隐藏/尺寸/焦点/置顶/定位。
- 主应用的核心体验是类似 IM 的连续对话。

## 已保存的需求文档

- `docs/requirements/product/FR-PES-001-个人事件记录与状态管理.md`
- `docs/requirements/product/FR-PES-002-记忆系统.md`
- `docs/requirements/product/FR-PES-003-Flutter统一GUI.md`

## 下一步实现顺序

### 1. 创建 Flutter 项目

- 使用 FVM 执行 Flutter 命令。
- 在 `ui/` 下创建 Flutter 项目。
- 固定并记录 Flutter/Dart 版本。
- 先完成桌面端启动，不接 Rust。

### 2. 实现 GUI v0

- 深色生产力工具视觉系统：墨蓝背景、暖橙强调色、紧凑高密度布局。
- 主应用模式：
  - 左侧会话列表
  - 中央消息区
  - 底部多行输入框
  - 新建会话、发送、失败重试的基础交互
- Capture 模式：
  - 同一窗口切换为紧凑输入界面
  - 支持提交和取消
  - 与主应用模式互斥
- 先使用明确隔离的 mock repository，不把 mock 混入最终业务 API。

### 3. 接入 Rust Core

- 抽取稳定 DTO 和面向业务的 service API。
- 评估并接入 `flutter_rust_bridge`。
- 首批接口：`record_event`、`list_conversations`、`list_messages`、`create_conversation`、`send_message`。
- 保证耗时操作异步，UI isolate 不阻塞。

### 4. 桌面平台能力

- Rust/平台层接入全局快捷键。
- 快捷键触发 Flutter 窗口切换到 Capture 模式。
- 完成窗口尺寸、焦点、置顶和恢复主应用状态。

### 5. 验收

- `fvm flutter run -d linux` 可以启动 GUI。
- 可以新建会话并连续发送多条本地 mock 消息。
- Capture 模式与主应用模式互斥切换。
- Capture 提交有效文本后退出 Capture；空白文本不提交。
- GUI 不依赖网络和 AI 也能运行。
- `fvm flutter analyze` 通过。

## 当前阻塞/待确认

- 视觉方向暂定为 Linear/Raycast 风格的深色生产力工具界面；开始完整实现前需要用户确认。
- 需要确认当前环境是否已安装 Linux Flutter desktop 所需依赖。
- `flutter_rust_bridge` 的具体版本和生成流程在 Flutter v0 启动后确定。

## 下一次会话的第一步

读取本文件和 `FR-PES-003-Flutter统一GUI.md`，确认视觉方向后，在 `ui/` 创建 Flutter 项目并实现 GUI v0。
