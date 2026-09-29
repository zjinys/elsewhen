# Elsewhen 快捷键设置指南

由于 Linux 桌面环境的多样性和 Wayland 的安全限制，Elsewhen 采用**系统级快捷键**方案，而非应用内全局快捷键。

## 🎯 推荐配置

### 主快捷键：Meta+Space (Super+Space)
唤起 Capture 模式（Alfred 风格快速输入）

---

## 📋 各桌面环境配置方法

### GNOME (Wayland/X11)

1. 打开设置：`gnome-control-center`
2. 导航到 **键盘** → **键盘快捷键** → **自定义快捷键**
3. 点击 **+** 添加新快捷键
4. 填写信息：
   - **名称**: `Elsewhen Capture`
   - **命令**: `/home/pp/playground/ai/elsewhen/scripts/elsewhen-capture.sh`
   - **快捷键**: 按下 `Super+Space`

### KDE Plasma

1. 打开系统设置：`systemsettings5`
2. 导航到 **快捷键** → **自定义快捷键**
3. 编辑 → 新建 → 全局快捷键 → 命令/URL
4. 触发器标签页：设置为 `Meta+Space`
5. 动作标签页：命令设置为 `/home/pp/playground/ai/elsewhen/scripts/elsewhen-capture.sh`

### i3 / Sway

在配置文件中添加（`~/.config/i3/config` 或 `~/.config/sway/config`）：

```
bindsym $mod+space exec /home/pp/playground/ai/elsewhen/scripts/elsewhen-capture.sh
```

然后重新加载配置：`$mod+Shift+r`

### Xfce

1. 打开设置：`xfce4-settings-manager`
2. 导航到 **键盘** → **应用程序快捷键**
3. 点击 **添加**
4. 命令：`/home/pp/playground/ai/elsewhen/scripts/elsewhen-capture.sh`
5. 按下 `Super+Space`

### Hyprland

在 `~/.config/hypr/hyprland.conf` 添加：

```
bind = SUPER, Space, exec, /home/pp/playground/ai/elsewhen/scripts/elsewhen-capture.sh
```

重新加载配置：`hyprctl reload`

---

## 🚀 启动脚本

项目提供了两个便捷脚本：

### 1. 主应用

```bash
scripts/elsewhen.sh
```

启动完整的主应用界面（1000x700 窗口）

### 2. Capture 模式

```bash
scripts/elsewhen-capture.sh
```

启动紧凑的快速输入窗口（500x240，始终置顶）

---

## 💡 使用技巧

### 方案 A：纯命令行启动

如果你更喜欢直接命令，可以使用：

```bash
# 主应用
cd ~/playground/ai/elsewhen/ui && fvm flutter run -d linux

# Capture 模式
cd ~/playground/ai/elsewhen/ui && fvm flutter run -d linux --dart-entrypoint-args "--mode=capture"
```

### 方案 B：后台运行

使用 systemd 用户服务让 Elsewhen 开机自启：

1. 创建服务文件 `~/.config/systemd/user/elsewhen.service`：

```ini
[Unit]
Description=Elsewhen Personal Event System
After=graphical-session.target

[Service]
Type=simple
WorkingDirectory=/home/pp/playground/ai/elsewhen/ui
ExecStart=/home/pp/.fvm/versions/3.47.4/bin/flutter run -d linux
Restart=on-failure

[Install]
WantedBy=default.target
```

2. 启用并启动：

```bash
systemctl --user daemon-reload
systemctl --user enable elsewhen.service
systemctl --user start elsewhen.service
```

### 方案 C：.desktop 快捷方式

创建 `~/.local/share/applications/elsewhen-capture.desktop`：

```desktop
[Desktop Entry]
Version=1.0
Type=Application
Name=Elsewhen Capture
Comment=Quick event capture
Exec=/home/pp/playground/ai/elsewhen/scripts/elsewhen-capture.sh
Icon=accessories-text-editor
Terminal=false
Categories=Utility;
```

然后可以从应用启动器搜索 "Elsewhen Capture"。

---

## 🔍 故障排除

### 快捷键不响应

1. **检查冲突**：确保 `Meta+Space` 没有被其他应用占用
2. **使用绝对路径**：确认脚本路径正确
3. **测试脚本**：直接在终端运行脚本，验证是否能启动
4. **查看日志**：运行 `journalctl --user -u elsewhen` 查看服务日志

### Wayland 限制

某些快捷键组合在 Wayland 下可能被保留。推荐的替代方案：

- `Meta+Space` → `Meta+Shift+Space`
- `Ctrl+Space` → `Ctrl+Alt+Space`
- `Alt+Space` → `Alt+Shift+Space`

### 权限问题

确保脚本可执行：

```bash
chmod +x ~/playground/ai/elsewhen/scripts/elsewhen.sh
chmod +x ~/playground/ai/elsewhen/scripts/elsewhen-capture.sh
```

---

## 📌 快速开始

**最快捷的方式（GNOME 用户）：**

```bash
# 1. 打开设置
gnome-control-center keyboard

# 2. 添加自定义快捷键
#    名称: Elsewhen Capture
#    命令: /home/pp/playground/ai/elsewhen/scripts/elsewhen-capture.sh
#    快捷键: Super+Space

# 3. 测试
#    按 Super+Space，应该弹出 Capture 窗口
```

---

## 🎨 工作流建议

1. **主应用常驻**：启动 `scripts/elsewhen.sh`，最小化到后台
2. **快捷键捕获**：随时按 `Super+Space` 快速记录
3. **Capture 提交后自动返回**：记录完成后窗口自动切换回主应用

这样就能实现类似 Alfred/Raycast 的流畅体验！

---

**当前检测到的会话类型**: Wayland  
**项目路径**: /home/pp/playground/ai/elsewhen
