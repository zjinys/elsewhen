# elsewhen · 别时

> 本地优先的个人认知主循环 —— 记录此刻，复利此生。

Personal Event System 的 Rust 本地核心 + Flutter 跨平台 GUI。

项目当前正从“事件、对话、知识库、待办的功能集合”收敛为个人认知主循环：

```text
随手记录 → AI 理解 → 人物 / 项目 / 认识沉淀 → 每日回顾 / 后续行动
保存内容 → 阅读理解 → 观点 / 灵感 / 创作素材 → 采用或归档
```

实施进度见 [`docs/roadmap/2026-09-17-personal-cognition-main-loop-roadmap.md`](docs/roadmap/2026-09-17-personal-cognition-main-loop-roadmap.md)。

## 架构

- **Rust 核心** (项目根目录)：事件存储、AI 分析、SQLite 数据库、传统 Iced GUI
- **Flutter UI** (`ui/` 目录)：现代化跨平台 GUI，支持 Linux/macOS/Windows 桌面和 Android/iOS 移动端

目前 Flutter UI 已包含对话、知识库、待办、Capture、AI Provider 设置、网页/文本导入、人物关系和知识页 AI 加工。知识页支持标签、来源分区和派生产物关联。

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
cargo run -- analyze-once
```

AI Provider 配置保存在数据库 `ai_provider_configs` 表（唯一配置源），在应用内 **设置页** 填写 Base URL、模型和 API key，保存即生效。未配置 Provider 时，分析任务保留在重试队列中，不影响原始事件。

Provider 不可用、超时或返回非法 JSON 时，Raw Event 不受影响，任务进入指数退避重试状态。

Capture 保存成功后会自动启动一次后台分析，不阻塞窗口关闭。若需要持续处理失败重试和积压任务，可运行：

```bash
cargo run -- worker
```

查看已完成的结构化分析：

```bash
cargo run -- analyses
```

分析队列的 pending / running / retry / succeeded / failed 数量也可在 Flutter **设置 → 数据 → 事件分析队列** 查看。

## LLM Wiki 个人知识库与认知推微

Elsewhen 内置一个 **LLM Wiki 个人知识库**（模式来自 Karpathy 的 [LLM Wiki](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) 思路）：
"编译一次，持续更新"——AI 把随手记录的事件消化成**关于你的、持久复利的知识页**，
而不是每次查询时从原始记录重新推导。人类负责记录与提问，AI 负责整理与维护。

三层结构：

- **原始事件**（`events` 表，不可变）：你的日记，事实来源
- **wiki 页面**（`wiki_pages` 表，AI 维护 + 核心确定性合并）：`recurring_cost` / `capability` / `asset` / `project` / `relationship` / `decision` / `habit` / `constraint` / `insight` 等页面，markdown 正文，带**事件溯源**与**证据计数**
- **schema**（系统提示词 + Rust 合并规则）：约束 AI 成为"守纪律的维护者"，AI 只提议，核心决定入库

常用命令：

```bash
# 消化：最近事件 → 提炼/合并 进 wiki 页（每次写回留 revision，追加操作日志）
cargo run -- wiki digest [--days N] [--dry-run] [--force]

# 认知推微：导航 wiki（知识）+ 最近事件 → 四透镜生成反常识认知，并归档回 wiki
cargo run -- insight [--days N]

# 知识库浏览 / 维护
cargo run -- wiki list [kind]
cargo run -- wiki show <slug>
cargo run -- wiki log
cargo run -- wiki lint
cargo run -- wiki export <目录>    # 只读快照：物化 markdown 树（浏览/备份用，编辑不回写）

# 历史洞察
cargo run -- insights
```

核心机制：

- **证据复利**：同一个事实被 N 条事件支持时 `evidence_count=N`，页面向"更懂你"累积；
- **确定性合并**：AI 建议的页面变更由核心校验（kind/slug 枚举）后以事件 id **并集**合并，AI 不能改数字；
- **好答案写回**：认知推微生成的洞察存为 `insight/*` 页并 `[[wikilink]]` 溯源来源页，探索也在知识库复利；
- **原始事件永不被覆盖**，wiki 全部是 derived data，可随时重新消化重建。

设计细节见 [`docs/llm-wiki.md`](docs/llm-wiki.md)。

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
这些脚本只用于从源码生成安装包，不会被安装到用户系统。安装包本身包含 CLI、桌面入口和正式品牌图标（`assets/brand/elsewhen-icon-v2-256.png`）。

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

Rust API 发生变化后运行根目录的 `./regen.sh`，它会重新生成 Flutter-Rust Bridge 代码并重建 release 动态库。真实 Bridge 测试使用独立临时数据库，测试代码不得依赖个人数据目录已有内容。

## 文档

- [Flutter UI README](ui/README.md) - Flutter GUI 开发指南
- [CLAUDE.md](CLAUDE.md) - Claude Code 开发指南
- [需求文档](docs/requirements/) - 产品需求和架构设计
