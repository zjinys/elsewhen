# Elsewhen 快速开始指南

## 🚀 立即开始

### 运行应用

```bash
cd ui/
fvm flutter run -d linux
```

应用将启动并显示主界面。

### 基本使用

#### 1. 记录事件

**方式一：在主应用中**
- 在底部输入框输入文本
- 点击"记录"按钮或按 Enter
- 事件立即出现在时间线中

**方式二：全局快捷键（推荐）**
- 在任何地方按 **Ctrl+Space**
- 弹出 Capture 窗口（始终置顶）
- 输入文本后按 Enter 提交
- 自动返回主应用

#### 2. 查看事件

- 主界面显示所有事件的时间线
- 每个事件卡片显示：
  - 记录时间
  - 来源标签
  - 事件内容
  - AI 分析（如果有）

#### 3. 窗口操作

- **Escape** - 从 Capture 模式返回主应用
- **关闭按钮** - 最小化到后台
- Capture 窗口始终置顶，不会被其他窗口遮挡

## 🎯 快捷键

| 快捷键 | 功能 |
|--------|------|
| **Ctrl+Space** | 从任何地方唤起 Capture 窗口 |
| **Enter** | 提交事件（在输入框中） |
| **Escape** | 关闭 Capture，返回主应用 |

## 🛠️ 开发

### 安装依赖

```bash
cd ui/
fvm install           # 安装 Flutter 3.47.4
fvm flutter pub get   # 安装 Dart 依赖
```

### Linux 系统依赖

**Arch/CachyOS/Manjaro:**
```bash
sudo pacman -S libkeybinder3
```

**Ubuntu/Debian:**
```bash
sudo apt-get install libkeybinder-3.0-dev
```

**Fedora/RHEL:**
```bash
sudo dnf install keybinder3-devel
```

### 运行测试

```bash
cd ui/
fvm flutter test      # 运行单元测试
fvm flutter analyze   # 代码分析
```

### 构建

```bash
cd ui/
fvm flutter build linux --release
```

构建产物在 `build/linux/x64/release/bundle/`

## 📁 数据位置

事件数据存储在：
- **Linux**: `~/.local/share/elsewhen/events.db`
- **macOS**: `~/Library/Application Support/elsewhen/events.db`
- **Windows**: `%LOCALAPPDATA%\elsewhen\events.db`

可以通过环境变量覆盖：
```bash
export ELSEWHEN_DATA_DIR=/custom/path
```

## 🎨 界面说明

### 主应用模式 (1000x700)

```
┌────────────────────────────────────────┐
│ 🕐 Elsewhen                        ⚙️  │ ← 顶部导航
├────────────────────────────────────────┤
│                                        │
│  ┌──────────────────────────────────┐ │
│  │ 15:23  2026-09-13     [GUI]     │ │
│  │ 完成了 Flutter GUI 的实现        │ │ ← 事件卡片
│  │ 🤖 AI: 工作进展                 │ │
│  └──────────────────────────────────┘ │
│                                        │
│  ┌──────────────────────────────────┐ │
│  │ 14:10  2026-09-13     [Hotkey]  │ │
│  │ 测试全局快捷键功能               │ │
│  └──────────────────────────────────┘ │
│                                        │
├────────────────────────────────────────┤
│ [输入框：记录此刻发生的事情...]  [记录]│ ← 底部输入
└────────────────────────────────────────┘
```

### Capture 模式 (500x240)

```
┌──────────────────────────────┐
│ 📝 快速记录              ✕  │ ← 标题栏
├──────────────────────────────┤
│                              │
│  ┌────────────────────────┐ │
│  │ [输入框]               │ │ ← 自动聚焦
│  │                        │ │
│  └────────────────────────┘ │
│                              │
│ Enter 提交 • Esc 关闭  [记录]│ ← 操作提示
└──────────────────────────────┘
```

## 🔧 故障排除

### 问题：全局快捷键不工作

**Linux (X11)**:
- 正常工作，无需额外配置

**Linux (Wayland)**:
- Wayland 安全限制可能阻止全局快捷键
- 需要在系统设置中手动绑定
- 或使用 X11 会话

**检查方法**:
```bash
echo $XDG_SESSION_TYPE  # 显示 x11 或 wayland
```

### 问题：窗口显示异常

1. 检查窗口管理器兼容性
2. 尝试重启应用
3. 查看控制台输出：
   ```bash
   fvm flutter run -d linux -v
   ```

### 问题：编译失败

确保安装了所有依赖：
```bash
# Linux
sudo pacman -S libkeybinder3  # Arch
sudo apt install libkeybinder-3.0-dev  # Ubuntu
```

### 问题：数据库错误

删除数据库重新开始：
```bash
rm ~/.local/share/elsewhen/events.db
```

## 📚 更多文档

- [CLAUDE.md](../CLAUDE.md) - 开发指南
- [ui/README.md](README.md) - Flutter UI 详细文档
- [PROGRESS_REPORT.md](../PROGRESS_REPORT.md) - 功能完成情况

## 💡 提示

1. **快速录入工作流**
   - 保持主应用在后台运行
   - 随时按 Ctrl+Space 快速记录
   - 提交后自动返回之前的工作

2. **事件分类**
   - GUI - 通过主应用输入
   - Hotkey - 通过快捷键 Capture
   - CLI - 通过命令行工具

3. **最佳实践**
   - 简短描述即可，AI 会自动分析
   - 及时记录，不要等到忘记
   - 定期查看时间线回顾

## 🎉 享受使用！

Elsewhen 帮助你轻松记录生活和工作中的每一刻。

有问题？查看文档或提交 Issue。
