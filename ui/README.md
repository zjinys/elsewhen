# Elsewhen Flutter UI

Elsewhen 的现代化跨平台图形界面，使用 Flutter 构建。

## 特性

- **主应用模式**：完整的事件时间线视图
  - 类似 IM 的消息流界面
  - 事件卡片显示时间、来源和内容
  - 底部输入框快速记录新事件
  
- **Capture 模式**：快速录入浮动窗口
  - 紧凑的浮动窗口设计
  - 自动聚焦到输入框
  - Enter 提交，Escape 关闭
  - 始终置顶

- **深色生产力主题**
  - 墨蓝色 (Ink Blue) 背景
  - 暖橙色 (Warm Orange) 强调色
  - 高密度信息布局
  - Inter 字体

## 开发环境

### 前置要求

- Flutter 3.47.4 (通过 FVM 管理)
- Linux 桌面依赖：`libgtk-3-dev`, `pkg-config`, `cmake`, `ninja-build`

### 安装依赖

```bash
# 安装 Flutter SDK (通过 FVM)
fvm install

# 安装 Dart 依赖
fvm flutter pub get
```

### 运行

```bash
# 主应用模式 (默认)
fvm flutter run -d linux

# Capture 模式
fvm flutter run -d linux --dart-entrypoint-args "--mode=capture"
```

Capture 模式是一个独立的快速录入窗口。想用全局快捷键随时唤起它，需要在**系统层面**绑定
快捷键指向下面这条命令（应用内不再注册全局热键：Linux 上 `hotkey_manager` 一类插件在
X11/Wayland 下有编译与抢键问题，因此没有内置实现）：

| 桌面环境 | 做法 |
| --- | --- |
| GNOME / KDE | 系统设置 → 键盘 → 自定义快捷键 → 命令填 `elsewhen --mode=capture` |
| 独立使用 | 终端常驻跑 `fvm flutter run -d linux --dart-entrypoint-args "--mode=capture"`，或用 `scripts/elsewhen-capture.sh` |

### 开发

```bash
# 代码分析
fvm flutter analyze

# 运行测试
fvm flutter test

# 热重载
# 在运行时按 'r' 热重载，按 'R' 热重启
```

### 构建

```bash
# Debug 构建
fvm flutter build linux

# Release 构建
fvm flutter build linux --release
```

## 项目结构

```
lib/
├── main.dart                 # 应用入口
├── models/                   # 数据模型
│   ├── app_config.dart      # 应用配置
│   └── event.dart           # 事件模型
├── providers/               # Riverpod 状态管理
│   └── event_provider.dart  # 事件状态
├── screens/                 # 页面
│   ├── main_screen.dart     # 主应用界面
│   └── capture_screen.dart  # Capture 界面
├── theme/                   # 主题配置
│   └── app_theme.dart       # 设计系统
└── widgets/                 # 可复用组件
    ├── event_card.dart      # 事件卡片
    └── event_input.dart     # 输入框
```

## 架构说明

### 状态管理

使用 **Riverpod** 进行状态管理：
- `eventRepositoryProvider`: 事件存储层 (当前为 mock，将来桥接到 Rust)
- `eventsProvider`: 事件列表异步状态
- `eventInputProvider`: 输入框状态

### 模式切换

应用通过 `AppConfig` 在启动时决定运行模式：
- `AppMode.main`: 完整应用
- `AppMode.capture`: 快速录入

窗口配置由 `window_manager` 包管理。

### Rust 桥接 (待实现)

当前使用 mock repository，将来会通过 `flutter_rust_bridge` 桥接到 Rust 核心：

```dart
// 待实现的 FFI 接口
- record_event(raw_text) -> Event
- list_events() -> List<Event>
- create_conversation() -> Conversation
- list_conversations() -> List<Conversation>
- send_message(conversation_id, text) -> Message
```

## 设计规范

### 颜色系统

```dart
surface0: #05070C  // 最深背景
surface1: #0A0D12  // 主背景
surface2: #0F131C  // 卡片背景
surface3: #161D2B  // 悬浮元素
surface4: #1E2636  // 高亮区域

accentPrimary: #E9A568  // 暖橙主色
accentMuted: #7A5C3D    // 暖橙暗色

textPrimary: #E8E9EC    // 主文字
textSecondary: #9BA1AB  // 次级文字
textTertiary: #5F6570   // 辅助文字
```

### 间距系统

```dart
space1: 4px
space2: 8px
space3: 12px
space4: 16px
space6: 24px
space8: 32px
space12: 48px
```

### 圆角

```dart
radiusSmall: 6px
radiusMedium: 12px
radiusLarge: 16px
radiusFull: 999px
```

## 待办事项

- [ ] 桥接 Rust 核心 (flutter_rust_bridge)
- [ ] 实现会话系统
- [ ] 实现 AI 分析状态显示
- [ ] 全局快捷键集成 (hotkey_manager) — 见上文「Capture 模式」，当前需系统级配置
- [ ] 窗口显示/隐藏动画
- [ ] Android/iOS 支持
- [ ] 设置界面
- [ ] 搜索和过滤

## License

MIT
