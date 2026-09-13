# elsewhen

Personal Event System 的 Rust 本地核心 + Flutter 跨平台 GUI。

## 架构

- **Rust 核心** (项目根目录)：事件存储、AI 分析、SQLite 数据库、传统 Iced GUI
- **Flutter UI** (`ui/` 目录)：现代化跨平台 GUI，支持 Linux/macOS/Windows 桌面和 Android/iOS 移动端

## Flutter GUI (推荐)

Flutter UI 提供更现代的体验，支持两种模式：

```bash
cd ui/

# 主应用模式：完整的事件时间线
fvm flutter run -d linux

# Capture 模式：快速录入浮动窗口
fvm flutter run -d linux --dart-entrypoint-args "--mode=capture"
```

详细文档见 [`ui/README.md`](ui/README.md)。

## Rust 核心 CLI

### 运行

```bash
cargo run -- record "今天完成了 F429 的 USART 修改"
cargo run -- list
```

测试或便携运行时可设置 `ELSEWHEN_DATA_DIR=/path/to/data` 覆盖默认平台数据目录。

## Capture 与快捷键

```bash
cargo run -- capture
cargo run -- daemon
```

`daemon` 默认监听双击 Left Ctrl，并启动 Capture 窗口。Linux 原始全局键盘监听当前依赖 X11；Wayland 会话可能阻止该能力。

## 后台事件分析

事件保存时会在同一 SQLite 事务中创建分析任务。配置任意 OpenAI-compatible Provider 后可处理一个待分析事件：

```bash
export ELSEWHEN_AI_API_KEY="..."
export ELSEWHEN_AI_MODEL="gpt-4.1-mini"
# 可选，默认 https://api.openai.com/v1
export ELSEWHEN_AI_BASE_URL="https://api.openai.com/v1"

cargo run -- analyze-once
```

数据库尚无 Provider 时，Elsewhen 会从当前目录的 `.env` 首次导入 Base URL、模型和 API key。导入后数据库成为唯一配置源，后续不再由 `.env` 覆盖。

Provider 不可用、超时或返回非法 JSON 时，Raw Event 不受影响，任务进入指数退避重试状态。

Capture 保存成功后会自动启动一次后台分析，不阻塞窗口关闭。若需要持续处理失败重试和积压任务，可运行：

```bash
cargo run -- worker
```

查看已完成的结构化分析：

```bash
cargo run -- analyses
```

## 构建

```bash
cargo build --release
```

二进制为 `target/release/elsewhen`。三个平台由 `.github/workflows/ci.yml` 编译和测试。

Linux 上安装一次桌面元数据，使窗口合成器能解析应用 ID 到图标：

```bash
./scripts/install-linux-desktop.sh
```

发行版安装包：

```sh
./scripts/package-deb.sh       # Debian/Ubuntu，生成 dist/*.deb
./scripts/package-rpm.sh       # Fedora/RHEL，生成 dist/*.rpm
./scripts/package-pacman.sh    # Arch，生成 dist/*.pkg.tar.*
```

三个脚本都会先构建 release 版本；分别需要系统提供 `dpkg-deb`、`rpmbuild` 或 `makepkg`。
这些脚本只用于从源码生成安装包，不会被安装到用户系统。安装包本身包含 CLI、桌面入口和应用图标（`assets/icons/elsewhen-256.png` 落地后自动打入包）。

也可以通过 Makefile 调用：

```sh
make release          # 构建 release 二进制
make deb              # Debian/Ubuntu
make rpm              # Fedora/RHEL
make pacman           # Arch Linux
make package          # 构建全部格式（要求三种打包工具都已安装）
```

Capture 窗口、全局快捷键、AI 分析和规则引擎将在后续阶段接入；它们不能绕过 `Store::insert_event` 的本地提交边界。

## 开发

### Flutter UI 开发

```bash
cd ui/
fvm install                    # 安装 Flutter 3.47.4
fvm flutter pub get            # 安装依赖
fvm flutter analyze            # 代码分析
fvm flutter test               # 运行测试
fvm flutter run -d linux       # 运行应用
```

### Rust 核心开发

```bash
cargo test                     # 运行测试
cargo build --release          # 构建 release
cargo run -- record "事件"     # CLI 记录事件
```

## 文档

- [Flutter UI README](ui/README.md) - Flutter GUI 开发指南
- [CLAUDE.md](CLAUDE.md) - Claude Code 开发指南
- [需求文档](docs/requirements/) - 产品需求和架构设计
