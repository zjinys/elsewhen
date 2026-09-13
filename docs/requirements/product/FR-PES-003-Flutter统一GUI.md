# FR-PES-003: Flutter 统一 GUI

**版本**: v0.1  
**最后更新**: 2026-09-12  
**变更类型**: FEATURE  
**状态**: Draft  
**关联**: [FR-PES-001](./FR-PES-001-个人事件记录与状态管理.md)、[FR-PES-002](./FR-PES-002-记忆系统.md)

## 1. 产品目标

Elsewhen 使用 Flutter 统一承载桌面端和移动端 GUI。Rust 保留本地核心能力，通过 `flutter_rust_bridge` 向 Flutter 提供稳定的业务 API。桌面端的主应用和快速录入 Capture 使用同一个 Flutter 应用窗口，通过模式切换实现互斥显示。

## 2. 界面范围

### 2.1 主应用模式

主应用提供类似 IM 的连续对话体验，并可访问事件、记忆和设置：

- 左侧会话列表：新建、切换和查看历史会话。
- 中央消息区：按时间顺序显示用户消息和助手回复。
- 底部输入区：支持连续发送、发送中状态、失败重试和多行文本。
- 辅助导航：事件记录、Memory、设置等入口。

### 2.2 Capture 模式

- Capture 是同一个 Flutter 应用窗口中的紧凑模式，不是独立原生 UI 窗口。
- Capture 与主应用模式互斥，不同时显示。
- 全局快捷键触发后，平台层将窗口切换为 Capture 模式、调整尺寸、显示、置顶并聚焦。
- Flutter 完成文本输入、提交和取消；平台层不负责绘制界面。
- 提交成功或取消后恢复主应用模式。
- Capture 提交不得等待网络或 AI。

## 3. 平台与职责

### 3.1 Flutter

Flutter 负责所有界面绘制、交互状态、导航、消息列表、输入框、加载/错误状态和响应式布局。桌面与移动端尽量复用同一套页面和状态模型。

### 3.2 Rust

Rust 负责 SQLite、事件、会话、消息、记忆、AI worker、规则和数据校验。桌面平台层额外负责全局快捷键及窗口显示、隐藏、尺寸、焦点、置顶和位置控制。

### 3.3 Bridge API

Flutter 不直接访问 SQLite。通过 `flutter_rust_bridge` 暴露面向业务的 API，例如：

```text
record_event(raw_text)
list_events()
create_conversation()
list_conversations()
list_messages(conversation_id)
send_message(conversation_id, text)
search_memories(query)
set_capture_mode(enabled)
```

接口应返回稳定 DTO，不暴露 rusqlite 类型或内部数据库结构。耗时操作必须异步；AI 回复可通过流或状态事件增量通知 Flutter。

## 4. MVP 验收标准

- 使用 Flutter 项目启动桌面窗口，主应用模式可展示会话列表、消息区和输入区。
- 可以在同一界面连续输入多条消息，并按顺序显示本地消息气泡。
- Capture 模式与主应用模式互斥，能够在两种模式间切换。
- Capture 提交空白文本时不创建事件；提交有效文本后退出 Capture 模式。
- 主应用和 Capture 共享会话/事件状态，不各自维护一套数据。
- 当前 Rust Core 尚未完全桥接时，GUI 使用明确的本地 mock/repository，不把临时 mock API 混入最终业务接口。
- Flutter 项目可通过 FVM 固定版本运行和构建。

## 5. 非目标

本阶段不实现完整 AI 流式协议、移动端发布配置、复杂动画、主题市场、系统级通知适配，以及最终的全局快捷键平台插件；这些能力保留稳定的接口边界，后续逐步接入。

## 6. 设计约束

- GUI 不得阻塞 UI isolate。
- 主应用和 Capture 必须共享单一应用状态源。
- Capture 是快速路径，视觉上保持紧凑、可立即输入和提交。
- 对话是主要工作区，不能被 Capture 的交互模型反向限制。
